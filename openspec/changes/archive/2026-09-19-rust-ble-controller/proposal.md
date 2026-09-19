## Why

The Python BLE controller works but has inherent limitations for a 24/7 systemd service on a resource-constrained Raspberry Pi Zero/4:

- **Fragile runtime**: Python's GC, asyncio event loop, and bleak's BlueZ binding introduce latency and crash modes that are hard to debug on an unattended headless Pi
- **Dependency burden**: Requires `bleak`, `pycryptodomex` — Python packages that break across system Python upgrades and sd card corruption
- **No compile-time safety**: The crypto protocol (AES-ECB + RC4, frame checksums, session keys) has no type-level guarantees; one wrong byte and the plug ignores the command silently
- **Energy inefficiency**: Python startup (~0.3s) + import overhead for every daemon poll cycle adds up on battery-backed Pi setups

A Rust rewrite eliminates these: single statically-linked binary, zero runtime overhead, compile-time protocol safety, and <1MB deployed footprint.

## What Changes

- New Rust crate `govee-ble` that replaces `h5080_controller.py` and the planned `main.py` humidity loop
- Keep the Python scripts (`h5080_controller.py`, `scripts/`) as reference and for development/debugging
- `main.py` will eventually be replaced by the Rust binary as the systemd service
- The Rust binary provides a CLI identical to the current Python controller + a daemon mode

## Capabilities

### New Capabilities
- `ble/h5080-controller`: Rust implementation of the H5080 BLE control protocol — E7 session handshake, AES-ECB+RC4 encryption, init sequence, ON/OFF toggle, status query
- `ble/h5179-reader`: Rust implementation of H5179 BLE advertisement scanning — manufacturer data parse (temperature, humidity, battery)
- `daemon/humidity-loop`: Rust daemon that combines H5179 reading + H5080 control in a continuous humidity-threshold loop (same logic as the Python main.py, but as a self-contained binary)

## Impact

- `Cargo.toml` revived with new dependencies (btleplug, tokio, clap)
- `src/` directory created with Rust source
- systemd service `myscript.service` updated to point to the Rust binary
- Python `main.py` retired (kept in git history for reference)
- No changes to `openspec/`, `PROTOCOL.md`, or `scripts/` (they remain as documentation and dev tools)