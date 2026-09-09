## Context

The Python BLE controller (`h5080_controller.py`) works and validates the protocol. This design ports the same protocol logic to Rust on the Raspberry Pi (armv7l, Debian 12). The Python scripts remain as reference; the Rust binary replaces them in production.

Key constraints from the Pi environment:
- **Architecture**: armv7l (32-bit), no cross-compilation needed — builds native on the Pi
- **BLE stack**: BlueZ via DBus — `btleplug` wraps this in async Rust
- **Crypto**: AES-128-ECB + RC4 — no external crypto crate needed for RC4 (trivial), use `aes` crate for AES
- **Deployment**: systemd service, single binary at `/usr/local/bin/govee-ble`

## Goals / Non-Goals

**Goals:**
- Single Rust crate (`govee-ble`) with library + CLI binary
- Zero Python dependency for the running service
- Same protocol behavior as `h5080_controller.py` + `govee_ble_protocol.py`
- Daemon mode matching `main.py`'s humidity-threshold loop
- ~8MB stripped binary, minimal runtime dependencies

**Non-Goals:**
- Not porting the BTSnoop analysis tools (keep Python for dev)
- Not porting the OpenSpec planning machinery
- No cross-compilation toolchain setup (build on the Pi itself)
- No octx integration yet (deferred)

## Decisions

### Decision: btleplug over bluez-async

btleplug is the most battle-tested Rust BLE library with BlueZ DBus backend. It handles adapter discovery, scanning, and GATT connect/write/notify. `bluez-async` is a thinner wrapper but requires more manual DBus handling.

**Alternatives considered:**
- `bluez-async`: lower level, more control — but requires managing DBus objects directly for notifications
- Raw DBus via `zbus`: too much boilerplate for what btleplug abstracts

### Decision: aes crate + manual RC4

RC4 is trivially implementable (~15 lines). `aes` crate is pure Rust, works on armv7l without any C dependency. Combining them matches the Python `pycryptodome AES.ECB + rc4()` pattern exactly.

**Alternatives considered:**
- `openssl` crate: ARM build would need `libssl-dev`, adds a C dependency to an otherwise pure-Rust crate
- Single 20-byte unkeyed CRC: the protocol uses RC4 on the last 4 bytes specifically — not replacable

### Decision: CLI subcommands via clap derive

The ROADMAP specifies `govee-ble read`, `govee-ble on`, `govee-ble off`, `govee-ble status`, `govee-ble daemon`. clap derive maps directly to this with `#[derive(Parser)]` enum dispatch.

### Decision: Single crate, not workspace

The library (`lib.rs`) and CLI (`main.rs`) live in one crate. The daemon is a subcommand within `main.rs`. No need for a workspace until/unless an octx arm is added.

## Architecture

```
govee-ble/
├── Cargo.toml
└── src/
    ├── main.rs              # CLI entry: enum dispatch to subcommands
    ├── lib.rs               # Re-export public API
    ├── crypto.rs            # AES-ECB + RC4 encrypt/decrypt
    ├── protocol.rs          # Frame construction, checksum, session key
    ├── h5080.rs             # H5080 connect + handshake + init + toggle
    ├── h5179.rs             # H5179 advertisement scanner + parse
    └── daemon.rs            # Continuous humidity loop
```

### Module responsibilities

- `crypto.rs`: pure functions `encrypt(frame, key) -> [u8; 20]` and `decrypt(payload, key) -> [u8; 20]`, constants `KEY_COMM`
- `protocol.rs`: `frame_from(cmd, sub, &data) -> [u8; 20]`, `verify(frame) -> bool`
- `h5080.rs`: `H5080` struct with `connect`, `handshake`, `init`, `turn_on`, `turn_off`, `get_state`, `disconnect`. Uses notify callback to capture device responses.
- `h5179.rs`: `H5179Reading::from_manufacturer_data(&[u8]) -> Option<Self>`. Scanner via btleplug `scan_for_device_by_address`.
- `daemon.rs`: `run(interval, threshold)` — infinite loop with tokio::time::sleep, state tracking, error logging
- `main.rs`: clap `enum Subcommand { Read, On, Off, Status, Daemon { interval, threshold } }`

### Flow (daemon mode)

```
loop {
    read H5179 (btleplug scan for address, timeout 10s)
    match result:
        Ok(reading) => {
            if reading.humidity > threshold && last_state != ON:
                connect H5080 → handshake → init → turn_on → disconnect
                last_state = ON
            elif reading.humidity <= threshold && last_state != OFF:
                connect H5080 → handshake → init → turn_off → disconnect
                last_state = OFF
            else:
                // no change needed
        }
        Err(e) => log error, continue loop
    sleep(interval)
}
```

Key difference from Python: Rust keeps the `last_state` in a struct field (not a global or file), making the daemon stateless across restarts. On first run, it always reads and reports current state before deciding.

## Risks / Trade-offs

- **[Risk] btleplug API stability**: btleplug has churned across versions. Lock to a specific version in Cargo.toml and test upgrades explicitly.
- **[Risk] First build time**: Compiling btleplug + its deps on an armv7l Pi takes 20-30 minutes. Use `--release` only after verifying debug builds work.
- **[Risk] Notification timing**: btleplug's notification callback is async; the Python controller uses `asyncio.sleep` to wait for device responses. Rust must handle the same race: send command, wait for notification with a timeout, return error if no response.
- **[Trade-off] No hot-reload**: Unlike Python, you can't edit the running Rust code and see changes instantly. Each fix requires recompile + systemd restart.
- **[Risk] RC4 is deterministic**: RC4's keystream is the same for a given key every time. This matches the protocol's design (validated against captures). No security concern here — the key is static and known.
- **[Trade-off] PI startup time**: Python cold start ~0.3s; Rust cold start ~1ms. The daemon runs continuously so this only matters on system boot.

## Migration Plan

1. Copy `h5080_controller.py` and `scripts/govee_ble_protocol.py` reference files to the Pi (`~/govee/`)
2. Create the Rust crate on the Pi: `cargo new govee-ble && cd govee-ble`
3. Add dependencies to Cargo.toml and implement `crypto.rs` + `protocol.rs` first (no BLE dependency, testable standalone)
4. Implement `h5179.rs` — test with `cargo run -- read`
5. Implement `h5080.rs` — test with `cargo run -- on` / `cargo run -- off`
6. Implement `daemon.rs` — test with `cargo run -- daemon --interval 60` (short interval for testing)
7. Build release: `cargo build --release`
8. Deploy: `sudo cp target/release/govee-ble /usr/local/bin/`
9. Update systemd: `ExecStart=/usr/local/bin/govee-ble daemon`
10. Rollback: `ExecStart=/usr/bin/python3 /home/pi/Github/govee-humidity-control/main.py`

## Open Questions

- Should the daemon support multiple H5080 plugs (for users with >1 plug)? Could be added later as a `--mac` flag without changing the architecture.
- Naming convention: `govee-ble` vs `govee_ble` — clap subcommands prefer kebab-case binary names.