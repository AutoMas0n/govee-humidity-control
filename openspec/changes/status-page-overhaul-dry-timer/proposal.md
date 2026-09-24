## Why

The status page is a plain-text key/value dump (`text/plain`) that **erases the last good reading when a poll misses** — exactly when the user needs to see *what's known*. There is no signal-health indication (RSSI is measured in the scanner but never surfaced), no readable last-updated cue, and no way to manually override the hysteresis loop (e.g. run the dehumidifier for an afternoon to dry clothes). A functional control — a "dry clothes" timer that forces the plug ON for a set duration — plus a lila.lan-style mobile dashboard fix this in one pass.

## What Changes

- **Status server overhaul** — replaces the `text/plain` snapshot with a lila.lan-style mobile web page (max-480px CSS, stat boxes/cards, no external CDNs, no chart.js yet). Served by the same embedded HTTP listener.
- **Structured status state** — snapshot becomes a struct (`temp, humidity, battery, rssi, plug, hi, lo, interval, last_ok_ts, last_attempt_ts, last_error, force_until`) exposed as `/state.json` for the page to poll every 30 s.
- **Last reading survives misses** — on a failed poll the daemon keeps the last good snapshot and only updates `last_attempt_ts`/`last_error`; the page shows "Last reading … ago" plus a clear "sensor missed" indicator. No more blank/error-only page.
- **RSSI / dBm meter** — `read_sensor` captures advertisement RSSI from the existing scan pass (free: receiver-side measurement, no extra sensor activity, no battery cost) and the page renders it as a colored bar: green ≥ −70 dBm, amber −70…−85, red < −85, gray = unknown.
- **"Dry clothes" force mode** — `POST /dry?mins=N` and `POST /dry-off` on the status server. While active, the hysteresis band is suspended and the plug stays ON until the timer expires; on expiry the loop treats the next read as a fresh first read (start OFF unless humidity ≥ hi) — no surprise long-running compressor.
- **UI for force mode** — big tappable preset buttons (30 min / 1 h / 2 h / 4 h) plus a custom-minutes field, an active countdown, and a cancel; colored "dry mode active" state on the page.
- **Non-goals (phase 2, out of scope)**: chart.js history graphs, auth, external access/TLS. Page stays LAN-only like today.

## Capabilities

### New Capabilities
- `daemon/status-page`: the local dashboard — HTML rendering, `/state.json` contract, RSSI meter colors, last-reading retention + miss indication, 30 s auto-refresh, and the dry-mode control surface (buttons/countdown/cancel). Backend state lives in the daemon loop; this capability covers the page's externally observable behavior and its HTTP/JSON contract.

### Modified Capabilities
- `daemon/humidity-loop`: the dry-clothes force mode (timer overrides the hysteresis band; expiry resets to fresh first-read) and the retirement of the old "status page shows the band" text-page scenario (superseded by `daemon/status-page`).

## Impact

- `govee-ble/src/main.rs`: `status_server` → route handler (`GET /`, `GET /state.json`, `POST /dry`, `POST /dry-off`) sharing a state mutex; `daemon_loop` gains force-timer state and the suspension logic; `read_sensor` returns RSSI alongside the payload; snapshot rendering moves off the hot path into the server task.
- Unit tests: `band_need_on` unaffected; new pure function for force-timer decision + expiry (fresh first-read) tested.
- No new dependencies (embedded page is static HTML/JS; hand-rolled HTTP like today).
- Docs: HANDOVER.md status-page + dry-mode sections; systemd unit unchanged.
- **BREAKING (minor)**: `GET /` content changes from `text/plain` key/value to HTML — any script consuming the old text format (none known; the status page is human-facing) must switch to `/state.json`.
