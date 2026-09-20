## 1. Thread one adapter through the daemon

- [x] 1.1 In `govee-ble/src/main.rs`, change `daemon_loop` to accept an adapter param (`c: &btleplug::platform::Adapter`) — daemon arm creates one adapter, passes `&c`; per-cycle `adapter()` calls removed from the daemon path
- [x] 1.2 Change `read_sensor` and `try_plug_inner` (and `plug_on`/`plug_off`/`plug_status` pass-throughs) to take `c: &btleplug::platform::Adapter` — `cargo build --release` clean, `drop(c)` internal calls removed
- [x] 1.3 `daemon` CLI arm creates one adapter once; one-shot arms (`read`/`on`/`off`/`status`) create their own — verified one-shot works: `govee-ble read` → `22.3C 52% 86%`
- [x] 1.4 `cargo test` → 5/5 passed

## 2. Deploy and verify leak is gone

- [x] 2.1 Pushed + pulled on the Pi, `cargo build --release`, restarted `humidity-daemon` — service active, status page responds
- [x] 2.2 Leak check: baseline 11 fds at 01:02, re-checked 46 min / 4 cycles later = 11 fds — **flat, no per-cycle growth** (pre-fix would have been ~46 by 8.7 h; +4 by now). Note: 2/4 cycles still showed `H5179 not found` in an ok/fail/ok/fail pattern — investigation shows this tracks **sensor RSSI at the BLE range edge** (−82 dBm, sometimes reported as +0/unset by btleplug), not the adapter; one-shot reads succeed 6/6 at the same moments. Leak verdict: **fixed**. Sensor range is a pre-existing, separate concern.
- [x] 2.3 Next-day confirm (2026-09-20, ~07:36 uptime): fds still **11** (flat, no growth; pre-fix 46 at 8.7h); NRestarts=1 (06:00 restart, unrelated); 131 ok / 31 fail of 162 cycles ≈ 19% — isolated misses, no acceleration (pre-fix failures clustered into consecutive runs as fds grew). Sensor range remains the residual cause; follow up later by comparing new exported data (see HANDOVER note).

## 3. Docs

- [x] 3.1 HANDOVER note added (commit 97c7f62): "BLE session reuse (2026-09-20)" entry documenting the leak + fix — verified accurate against 2.2/2.3 fd results
- [x] 3.2 Committed + pushed (97c7f62, b9bc888, + docs), Pi synced — `git status` clean on both