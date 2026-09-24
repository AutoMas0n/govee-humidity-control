## 1. State model & RSSI capture

- [x] 1.1 Define the `Status` struct (temp, humidity, battery, rssi: Option<i16>, plug, hi, lo, interval, last_ok_ts, last_attempt_ts, last_error, force_until) and a shared `Arc<tokio::sync::Mutex<Status>>`; verify `cargo build` clean
- [x] 1.2 Extend `read_sensor` to also capture `pr.rssi` from the same scan advertisement and return it; verify by running `govee-ble read` on the Pi and confirming the RSSI value prints (e.g. `-48 dBm`)

## 2. Force timer in the daemon loop

- [x] 2.1 Add force state (`force_until: Option<Instant>` in the shared state) with setters used by the server and readers in the loop; verify `cargo build` clean
- [x] 2.2 Wire the loop decision: `need_on = if force_active() { true } else { band_need_on(...) }`, and on active→expired transition reset `last_on = None` (fresh first-read handoff); verify `cargo test` passes including a new unit test for the handoff
- [x] 2.3 Add a pure helper (or inline test) covering `force_active` + expiry reset; verify the new unit tests pass in `cargo test`
- [x] 2.4 Persist `force_until` to the state file (path per design.md 3b): write on `POST /dry` and remove on `POST /dry-off`; at daemon startup load a future deadline back into `force_until` and ignore/unlink stale past ones; verify with a unit test of the read/write helpers plus a manual `systemctl restart` test mid-session in 5.2

## 3. HTTP server: routes & JSON

- [x] 3.1 Replace the watch-channel snapshot in `status_server` with the shared-mutex `Status`; implement `GET /state.json` serializing it (serde_json) and `GET /` returning the HTML page; verify `curl http://localhost:8843/state.json` on the Pi returns JSON with all fields
- [x] 3.2 Implement `POST /dry?mins=N` (clamped 1..1440) and `POST /dry-off` mutating force state and returning a JSON ack, plus a 404 branch for unknown paths; verify with `curl -X POST 'http://localhost:8843/dry?mins=60'` then `state.json` shows `force_until` set, and `POST /dry-off` clears it
- [x] 3.3 (added during apply) Implement `POST /poll` — signals the loop for an immediate sensor read, rate-limited to 1/30s returning 429+retry_after; split the page JS so auto-refresh stays cache-only while the manual Refresh button calls `/poll` then re-fetches; verify live: first poll ok (fresh state.json), immediate second poll → HTTP 429 retry_after

## 4. HTML dashboard (lila.lan style)

- [x] 4.1 Build the inline HTML page: max-480px CSS, stat boxes (temp/humidity/battery/plug), band setpoints, RSSI meter with green/amber/red/gray classes, last-reading age ("N minutes ago"), miss indicator, force countdown; verify the server responds with complete HTML containing the elements
- [x] 4.2 Add the fetch-on-load + 30s `setInterval` refresh of `/state.json` and the manual refresh button; verify in a browser the page updates automatically
- [x] 4.3 Add preset buttons (30m/1h/2h/4h), custom-minutes input, and a prominent red STOP/cancel button (visible whenever dry mode is active) wired to POST endpoints; verify in a browser: tapping a preset shows confirmation + countdown, tapping STOP clears it, and picking a new preset while active replaces the existing timer
- [x] 4.4 Verify no external assets are referenced (no CDN links) by grepping the HTML string; confirm the page renders with network fully offline except LAN

## 5. Deployment on the Pi

- [ ] 5.1 Commit + push; on the Pi `git pull`, `cargo build --release`, restart `humidity-daemon`; verify `systemctl is-active` and that both `GET /` (HTML) and `GET /state.json` respond on port 8843
- [x] 5.2 Live test: run a force session (`POST /dry?mins=10`), confirm the status page shows dry mode + countdown, confirm `journalctl` shows plug ON held through a band-off condition, and confirm expiry returns control to the band (humidity below lo turns plug OFF)
- [x] 5.2b Persistence live test: while a dry session is active, `sudo systemctl restart humidity-daemon`; confirm the countdown/force_until survives the restart (spec: "Force mode survives daemon restart"), and that a stale past deadline from the file is ignored on boot
- [x] 5.3 Verify miss-retention live: sensor moved out of range ~22:36Z Sep 24 — state.json retained last good values with last_attempt_ts advanced + last_error set, page served 200 with banner JS; after restoring the sensor a live poll (`POST /poll`) recovered: fresh last_ok_ts, last_error null, banner clear, RSSI green — **both parts verified live 2026-09-24**
- [ ] 5.4 Update HANDOVER.md status-page + dry-mode sections; commit + push; sync the Pi; confirm `git status` clean on both
