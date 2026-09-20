## 1. Thread one adapter through the daemon

- [x] 1.1 In `govee-ble/src/main.rs`, change `daemon_loop` to accept an adapter param (`c: &btleplug::platform::Adapter`) — daemon arm creates one adapter, passes `&c`; per-cycle `adapter()` calls removed from the daemon path
- [x] 1.2 Change `read_sensor` and `try_plug_inner` (and `plug_on`/`plug_off`/`plug_status` pass-throughs) to take `c: &btleplug::platform::Adapter` — `cargo build --release` clean, `drop(c)` internal calls removed
- [x] 1.3 `daemon` CLI arm creates one adapter once; one-shot arms (`read`/`on`/`off`/`status`) create their own — verified one-shot works: `govee-ble read` → `22.3C 52% 86%`
- [x] 1.4 `cargo test` → 5/5 passed

## 2. Deploy and verify leak is gone

- [x] 2.1 Pushed + pulled on the Pi, `cargo build --release`, restarted `humidity-daemon` — service active, status page responds
- [x] 2.2 Leak check: baseline 11 fds at 01:02, re-checked 46 min / 4 cycles later = 11 fds — **flat, no per-cycle growth** (pre-fix would have been ~46 by 8.7 h; +4 by now). Note: 2/4 cycles still showed `H5179 not found` in an ok/fail/ok/fail pattern — investigation shows this tracks **sensor RSSI at the BLE range edge** (−82 dBm, sometimes reported as +0/unset by btleplug), not the adapter; one-shot reads succeed 6/6 at the same moments. Leak verdict: **fixed**. Sensor range is a pre-existing, separate concern.
- [ ] 2.3 Next day: confirm reads still succeeding and fds remain flat; treat further `H5179 not found` cycles as a **sensor range** matter (possible fixes if it matters: move sensor closer, add a second scan pass per cycle, or check battery) — do NOT re-investigate the daemon unless fds grow

## 3. Docs

- [ ] 3.1 Add a line to `HANDOVER.md` under the daemon section: daemon reuses one BlueZ session (was leaking ~1 fd/cycle, fixed via `fix-daemon-ble-session-leak`) — verify the doc reads true after the fd check in 2.2
- [ ] 3.2 Commit and push; sync the Pi (`git pull`) — verify `git status` clean on both