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

### Decision: No clap — manual argv

clap adds ~30s to armv7l compilation and pulls in a dependency tree. Manual `args.iter().position()` is 3 lines, zero compile cost, and just as readable for 5 subcommands.

### Decision: Single file, not multi-module

The entire crate is `src/main.rs`. Crypto (3 functions), protocol (2 functions), BLE scan, BLE control, daemon loop. Splitting into 6 files adds `mod` declarations, pub visibility decisions, and cross-module import boilerplate with zero runtime benefit. All functions are private or local to the single file.

### Decision: No thiserror/anyhow

`String` error type with `map_err(|e| format!("{e}"))` is 10 chars. thiserror adds derive macros and another compile cost. For a binary with ~10 fallible operations, String errors are the lazy choice — they print what happened without any type-level tax.

### Decision: Plain TCP GET for healthcheck, not reqwest

`reqwest` pulls in hyper, h2, rustls — megabytes of compile on armv7l. Healthcheck services (healthchecks.io, uptimerobot) support plain HTTP. A raw TCP GET via `tokio::net::TcpStream` is 8 lines and needs TLS only if the user specifies https://. If they need HTTPS, they can run a local relay.

### Decision: 3 retries with exponential backoff, inline

A trait-based retry framework (backoff, governor, or custom) adds abstractions for a single call site. Three inline attempts with `2u64.pow(attempt)` sleep is 6 lines. Add when there are >1 callers with different retry policies

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
- **[Risk] Notification timing**: btleplug's notification callback is sync (blocking_lock). If the device sends notifications faster than the main loop drains them, the buffer grows. Mitigation: drain after every write, and the plug sends at most 1 notification per command.
- **[Risk] blocking_lock in notification callback**: The `n.blocking_lock()` inside the btleplug notification closure can stall if the main task holds the lock long. Mitigation: the main task holds the lock only briefly (drain to local vec, release immediately). Swap to a `tokio::sync::mpsc` channel if throughput ever becomes an issue.
- **[Trade-off] No hot-reload**: Unlike Python, you can't edit the running Rust code and see changes instantly. Each fix requires recompile + systemd restart.
- **[Risk] RC4 is deterministic**: RC4's keystream is the same for a given key every time. This matches the protocol's design (validated against captures). No security concern here — the key is static and known.
- **[Trade-off] PI startup time**: Python cold start ~0.3s; Rust cold start ~1ms. The daemon runs continuously so this only matters on system boot.

## Migration Plan

1. Pull the crate from GitHub on the Pi: `cd ~/Github/govee-humidity-control && git pull`
2. Install a current Rust toolchain on the Pi: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y` (Debian's rustc 1.63 is too old for btleplug's dependencies)
3. Build debug: `cd govee-ble && cargo build`
4. Test CLI: `./target/debug/govee-ble read`, `./target/debug/govee-ble on`, `./target/debug/govee-ble off`, `./target/debug/govee-ble status`
5. Build release: `cargo build --release`
6. Deploy: `sudo cp target/release/govee-ble /usr/local/bin/`
7. Test daemon: `govee-ble daemon --interval 60 --threshold 45 --hc-url http://your-id.healthchecks.io`
8. Update systemd: `ExecStart=/usr/local/bin/govee-ble daemon`

Rollback:
```bash
sudo sed -i 's|/usr/local/bin/govee-ble daemon|/usr/bin/python3 /home/pi/Github/govee-humidity-control/main.py|' /etc/systemd/system/myscript.service
sudo systemctl daemon-reload && sudo systemctl restart myscript.service
```

## Open Questions

None — all deferred decisions are marked with `ponytail:` comments in the source code.
- Session key nonce: using `[0u8; 16]` instead of random bytes for E7 handshake. Add urandom if device ever rejects. Not needed — frame is encrypted anyway.
- HTTPS for healthcheck: raw TCP only. Add TLS if using healthchecks.io with https.
- Multiple plugs: `--mac` flag can be added later without architecture change.