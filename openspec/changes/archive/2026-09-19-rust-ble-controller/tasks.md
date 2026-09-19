## 1. Project Setup (on Pi)

- [ ] 1.1 `ssh pi@192.168.2.21` and verify Rust toolchain installed (`rustc --version`, `cargo --version`); install via `rustup` if missing
- [ ] 1.2 Create crate: `cargo new govee-ble` under `/home/pi/Github/govee-humidity-control/`
- [ ] 1.3 Add dependencies to `Cargo.toml`: `btleplug`, `tokio`, `clap` (derive), `aes`, `thiserror`, `anyhow`, `hex`, `log`, `env_logger` and verify `cargo check` passes

## 2. Crypto & Protocol (pure functions, testable without BLE)

- [ ] 2.1 Implement `crypto.rs`: `KEY_COMM` constant, `encrypt(frame, key) -> [u8; 20]` (AES-128-ECB on first 16B + RC4 on last 4B), `decrypt(payload, key) -> [u8; 20]` and verify round-trip: `decrypt(encrypt(plain, k), k) == plain` for test vectors from Python implementation
- [ ] 2.2 Implement `protocol.rs`: `frame_from(cmd, sub, &[u8]) -> [u8; 20]` with zero padding and XOR checksum, `verify(&[u8; 20]) -> bool` and verify `frame_from(0x33, 0x01, &[0x11])` produces expected hex `33 01 11 00... 23`

## 3. H5179 Reader

- [ ] 3.1 Implement `h5179.rs`: scan for H5179 by MAC address via btleplug, extract manufacturer data (ID 0xEC88) and verify `cargo run -- read` outputs temperature, humidity, battery matching the Python `adv_check.py` reference

## 4. H5080 Controller

- [ ] 4.1 Implement `h5080.rs`: `H5080` struct with `connect()`, `handshake()`, `init()`, `turn_on()`, `turn_off()`, `get_state()`, `disconnect()` using btleplug GATT and notify callbacks and verify `cargo run -- status` returns plug ON/OFF matching `python3 h5080_controller.py status` on Pi
- [ ] 4.2 Verify `cargo run -- on` clicks plug ON and `cargo run -- off` clicks plug OFF (manual audible/heater-pilot-light verification)

## 5. Daemon & CLI

- [ ] 5.1 Implement `main.rs` with clap derive subcommands: `Read`, `On`, `Off`, `Status`, `Daemon { interval: u64, threshold: u8 }` and verify `cargo run -- --help` shows all subcommands
- [ ] 5.2 Implement `daemon.rs`: infinite `loop { read_h5179() → cmp threshold → h5080_toggle_if_changed → sleep(interval) }` with tokio::time::sleep, log::error on failures, graceful shutdown on SIGTERM and verify runs 3 cycles without crashing: `timeout 30 cargo run -- daemon --interval 10 --threshold 50`

## 6. Deploy

- [ ] 6.1 Build release: `cargo build --release` on Pi; binary at `target/release/govee-ble` sized ~8MB stripped
- [ ] 6.2 Deploy: `sudo cp target/release/govee-ble /usr/local/bin/` and verify `govee-ble status` works from any directory
- [ ] 6.3 Update systemd: edit `ExecStart` in `/etc/systemd/system/myscript.service` to `/usr/local/bin/govee-ble daemon`, `sudo systemctl daemon-reload && sudo systemctl restart myscript.service` and verify `systemctl status myscript.service` shows running and `journalctl -u myscript.service -n 10` shows normal log output
- [ ] 6.4 Verify rollback: restore `ExecStart` to Python version and confirm `main.py` service still works

## 7. Repo Housekeeping

- [ ] 7.1 Add `govee-ble/` to the root gitignore or track as part of the repo
- [ ] 7.2 Commit Rust crate and updated systemd docs to `git@github.com:AutoMas0n/govee-humidity-control.git`
- [ ] 7.3 Update `ROADMAP.md` Phase 3 checkbox to complete, note any deviations from the original plan