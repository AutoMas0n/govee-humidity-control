## Context

Today's status server (`status_server` in `govee-ble/src/main.rs`) accepts one connection per request, borrows a `tokio::sync::watch::Receiver<String>`, and writes a `text/plain` key/value snapshot. On a failed poll the daemon *replaces* the snapshot with `error=…` — the previous good values vanish. RSSI is measured in the `scan` CLI arm (`pr.rssi`) but never carried into `read_sensor`'s result, so the daemon's status has no signal health. The daemon loop is the single writer of both the reading and the plug decision, which makes a force timer a naturally local change. See proposal.md for motivation; see `specs/daemon/status-page/spec.md` and `specs/daemon/humidity-loop/spec.md` for the behavior contract.

## Goals / Non-Goals

**Goals:**
- One embedded HTTP server serving a lila.lan-style mobile dashboard, a `/state.json` contract, and two mutation endpoints (`/dry`, `/dry-off`)
- State model that preserves last good reading across misses and exposes signal health (RSSI)
- Force-on timer integrated with the existing hysteresis decision with a clear handoff on expiry
- No new Rust dependencies; page is self-contained static HTML/JS
- All changes fit the existing one-adapter daemon architecture

**Non-Goals:**
- Chart.js history graphs (phase 2)
- Toggle of *voltage/other* sensor fields; only temp/humidity/battery/RSSI
- Any change to the one-shot CLI semantics (`read`/`scan` keep returning their current formats)

## Decisions

### 1. State model: a `Status` struct behind a shared mutex, not a watch channel of strings
The current `watch::Receiver<String>` is replaced by a `Status` struct (all fields in the spec's `/state.json` contract) stored in `Arc<Mutex<Status>>`. The daemon loop updates it each cycle; the server task locks it to render `/` and `/state.json`.

- *Why not keep watch<String>?* The page needs structured fields (numbers, timestamps, rssi, force remaining) to render a dashboard and to *retain last-good on miss* — a string snapshot can't do that cleanly, and watch channels are fire-and-forget (late renderers can miss the latest state).
- *Why a mutex over a channel at all?* Writes are rare (every 15 min + force changes) and renders are rare (every 30 s + manual refresh); contention is negligible. `tokio::sync::Mutex` avoids async-in-lock issues if the server task ever awaits while holding.

**Exact shape** (mirrors `/state.json`):
```
temp: f32, humidity: u8, battery: u8, rssi: Option<i16>,
plug: bool, hi: u8, lo: u8, interval: u64,
last_ok_ts: u64, last_attempt_ts: u64, last_error: Option<String>,
force_until: Option<u64>   // unix seconds
```

### 2. RSSI acquisition reuses the poll scan — no extra BLE work
`read_sensor` already scans for H5179 advertisements; add capture of `pr.rssi` (`Option<i16>` from btleplug) to its return tuple. This is receiver-side measurement of a packet the sensor already sends — zero extra sensor activity, zero battery cost on the H5179 (advertising runs 24/7 regardless; the Pi just listens). Keeps the spec's "never forces extra sensor contact" scenario true by construction.

- **Design note**: on a *failed* poll the daemon has no fresh RSSI; `Status.rssi` keeps the last known value (with `last_attempt_ts` showing staleness via the page's age display). The page's gray state is reserved for "never measured".

### 3. Force timer: shared `Arc<Mutex<Option<Instant>>>`-style state read by the loop, written by the server
A `force_until: Option<std::time::Instant>` (wrapped in the same shared state or its own mutex) is the single source of truth:

- `POST /dry?mins=N` (N = 1..1440, clamped) sets `force_until = now + N minutes`
- `POST /dry-off` sets it to `None`
- The loop's plug decision becomes:
  ```
  need_on = if force_active() { true } else { band_need_on(h, hi, lo, last_on) }
  ```
  where `force_active()` = `force_until.map(|t| t > now).unwrap_or(false)`. When it flips from active→expired, `last_on` is reset to `None` so the next poll is a *fresh first read* (start OFF unless h ≥ hi) — the agreed handoff semantics.

**Implementation note (found in live testing):** the loop must act *immediately* on force changes, not at the next 15-min poll — a 1-minute timer otherwise expires between polls and never fires. A `tokio::sync::Notify` in the shared state is signaled by `POST /dry` and `POST /dry-off`; the loop's sleep becomes `select!(sleep(min(interval, time-to-deadline)) | notify)`, so short timers fire within ~2 s and natural expiry wakes the loop at the deadline. `force_active()` compares the deadline against `Instant::now()` — never `Option::is_some()` alone, or the expired deadline would keep the plug ON forever (a bug caught and fixed in the 1-minute test).

- *Why instant over UNIX ts in Status?* The loop does wall-clock comparisons; `Instant` is monotonic and immune to NTP jumps. `Status.force_until` (epoch seconds) is derived for the page's countdown.
- *Why a separate mutex vs reusing Status?* Keep them separate (or a small `Control` struct alongside `Status`) so the server's write of `force_until` can't block on a status update. Simpler alternative considered: one mutex holding both — acceptable but slightly more coupling; decide in code, both satisfy the spec.

### 3b. Force persistence across the daily 06:00 reboot
The Pi reboots every day at 06:00 (`0 6 * * * /sbin/shutdown -r now` in root crontab — confirmed Sep 19–23). An in-memory-only timer would silently die mid-run. Persist the deadline to a small state file so dry mode survives restarts:

- One file, fixed path (e.g. `/var/lib/humidity/force_until`), format: a single decimal epoch-seconds value (or absent/empty = no force). Daemon runs as root; create the directory if missing at startup (`std::fs::create_dir_all`).
- Write on `POST /dry` and on `POST /dry-off` (write removes the file). On daemon startup, read it: if the value is in the future, `force_until = Some(epoch)` (restores the hold); if absent or in the past, ignore and unlink stale files.
- The daemon loop may also opportunistically unlink the file when the timer naturally expires, but the startup read already handles the stale case — the unlink is cosmetic, skip it unless trivial (avoid a write per expired poll).
- Testing: `sudo systemctl restart humidity-daemon` mid-session must restore the countdown; delete-when-past must clear it.
- *Why a file and not a config/flag?* A file is written by the running daemon itself (the systemd unit stays unchanged — no new CLI flag, no unit edit, no re-copy to /etc/systemd/system). State that must survive a reboot belongs on disk, and this is the smallest disk model that works.

### 4. HTTP server: minimal router in the existing task
One `status_server` task binds the port and routes by request line:
- `GET /` → HTML page (static string with inline CSS/JS)
- `GET /state.json` → `serde_json`-serialized Status
- `POST /dry?mins=N` / `POST /dry-off` → mutate force state, respond `200` with a small JSON ack
- else → `404`

Requests are read with `tokio::io::AsyncReadExt` (read until the double-CRLF), handled per-connection like today. No route table (three branches), no new dependency; `serde`/`serde_json` are already in the tree (used by BLE payload parsing helpers).

- *Why not a framework (axum/warp)?* The server is 4 branches; a framework adds build time and surface for zero behavioral gain. ponytail: the current hand-rolled listener already works; extend it.

### 4b. JSON serialization: hand-rolled, no new dependency
`/state.json` is a flat 13-field object with one free-text field (`last_error`). Serde/serde_json are **not** in the project's dependency tree (Cargo.toml: btleplug, tokio, aes, log, env_logger, futures, hex) and the BLE payload code is already byte-level with no serde derives. Rather than add serde_json for one flat struct, serialize with a small hand-rolled `format!`-based function: numbers via direct field output, `rssi`/`force_until`/`last_error` as `null` when absent, and the error string escaped only for `"`/`\\`/newline. Escapes are trivial to unit test. If the state ever grows nested structures, switch to serde_json then — YAGNI now.

### 5. HTML page: single static string, lila.lan visual language
Inline `<style>` mirroring lila's look (max-width 480px, `-apple-system` stack, stat-box grid, rounded cards, `#1a1a2e`/`#6b7280` accents). One `<script>` block does:

- `fetch('/state.json')` on load and every 30 s (`setInterval`), re-rendering stat boxes (temp/humidity/battery/plug), band setpoints, RSSI meter (green/amber/red/gray class), last-reading age ("N minutes ago" from `last_ok_ts`), miss indicator (`last_error` present + `last_attempt_ts > last_ok_ts`), and force countdown
- Preset buttons (30 m/1 h/2 h/4 h) + custom minutes input → `fetch('/dry?mins=…', {method:'POST'})`
- Cancel button (visible only while active) → `POST /dry-off`
- Manual refresh button

Everything inline — no CDN, satisfying the "no external assets" scenario.

### 6. `band_need_on` stays untouched; force wraps it
The pure function has 5 tests and a proven contract. The force logic lives in the loop's decision site (one `if force_active() { true } else { band_need_on(...) }`), and a **new pure helper** `force_handoff(force_active, last_on) -> Option<bool>`-ish decides when to reset `last_on` on expiry. Unit tests cover it without BLE.

## Risks / Trade-offs

- [Broken scripts hitting `GET /` text format] → **BREAKING** noted in proposal; only consumer is a human browser; provide `/state.json` as the structured replacement.
- [Long dry mode (hours) with no user attention] → countdown + cancel on page; band resumes automatically at expiry by design.
- [Force request races a poll write to Status] → both writes go through the shared mutex; single-threaded loop means no torn snapshots; worst case the poll skips one cycle of the visual only.
- [Mutex held across render makes the page slow under many clients] → renders are small (a few KB) and rare; if ever needed, serve a cached last-rendered snapshot instead (watch-style). Not now.
- [RSSI deadband of ±1 dBm flapping color near thresholds] → colors are derived in JS from the DB; hysteresis not needed for a 3-color meter; acceptable.

## Migration Plan

- Deploy as an in-place replacement of the current unit: rebuild binary, `sudo cp` is only needed if the unit file changes (it does not — same flags, same port 8843), `systemctl restart humidity-daemon`.
- Old `text/plain` status is gone; no data migration (state is ephemeral).
- Rollback: `git revert` the binary change and restart the service (unit unchanged).

## Open Questions

None — force handoff semantics, duration controls, refresh cadence, and chart scope were settled with the user during exploration.
