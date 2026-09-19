## 1. Thread one adapter through the daemon

- [ ] 1.1 In `govee-ble/src/main.rs`, change `daemon_loop` to accept an adapter param (`c: &btleplug::platform::Adapter`) and remove its per-cycle `adapter()` calls — verify `grep -c "adapter()" main.rs` drops by exactly the number of daemon-path call sites (read_sensor, plug_on, plug_off, plug_status each lose one)
- [ ] 1.2 Change `read_sensor` and `try_plug_inner` (and `plug_on`/`plug_off`/`plug_status` pass-throughs) to take `c: &btleplug::platform::Adapter` and use it instead of calling `adapter()`/`drop(c)` — verify `cargo build` clean (no unused `drop(c)`, no warnings)
- [ ] 1.3 Update the `daemon` CLI arm to create one adapter once and pass it in; leave all one-shot arms (`read`/`on`/`off`/`status`/`scan`/`pair`) calling `adapter()` themselves — verify a one-shot still works after the change (`sudo ./target/release/govee-ble read --mac E3:32:81:12:40:A4`)
- [ ] 1.4 Verify unit tests still pass: `cargo test` → 5/5 (band + threshold alias tests unaffected)

## 2. Deploy and verify leak is gone

- [ ] 2.1 Push + pull on the Pi, `cargo build --release`, restart `humidity-daemon` — verify service active, status page responds
- [ ] 2.2 Leak check: record `ls /proc/$PID/fd | wc -l` at restart, then again after 2–3 poll cycles (≈30–45 min) — verify the count does not grow per cycle (pre-fix: +1 socket/cycle); note the baseline in the change summary
- [ ] 2.3 (Optional, next day) confirm reads are still succeeding and fds remain flat — verification via `journalctl -u humidity-daemon | grep -cE "ERROR.*sensor"` trending flat

## 3. Docs

- [ ] 3.1 Add a line to `HANDOVER.md` under the daemon section: daemon reuses one BlueZ session (was leaking ~1 fd/cycle, fixed via `fix-daemon-ble-session-leak`) — verify the doc reads true after the fd check in 2.2
- [ ] 3.2 Commit and push; sync the Pi (`git pull`) — verify `git status` clean on both