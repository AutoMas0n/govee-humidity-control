# Fix Daemon BLE Session Leak

## Why

The daemon (`govee-ble daemon`) leaks one BlueZ D-Bus socket per 15-minute poll
cycle. Every `read_sensor`/`plug_*` call opens a fresh `BluetoothSession`
(`dbus_tokio::connection::new_system_sync()`), and bluez-async/dbus-tokio never
close that connection on drop — the spawned connection task runs forever. After
~8 hours the process holds 40+ sockets; reads then start failing
(`H5179 not found`) with failures accelerating as sockets accumulate, even
though one-shot `govee-ble read` processes succeed instantly at the same moment.
Symptom: status page goes stale with `error=H5179 not found` for hours.

## What Changes

- `adapter()` is called **once per daemon lifetime** instead of once per cycle;
  the adapter is threaded into `read_sensor` and `plug_on`/`plug_off`/
  `plug_status` (via `try_plug_inner`) so all cycle work reuses one session.
- CLI one-shots (`read`, `on`, `off`, `status`, `scan`, `pair`) keep their own
  single `adapter()` call — the process exits, so one session is fine.
- `daemon_loop` gains an adapter parameter instead of opening one per loop
  iteration.

## Capabilities

### Modified Capabilities

- `daemon/humidity-loop`: the daemon contract gains a requirement that it keeps
  its BLE session usage bounded over its lifetime (no per-cycle session growth),
  and that a failed sensor read alone never leaves the process with a growing
  resource footprint.

## Impact

- `govee-ble/src/main.rs` — `daemon_loop` signature + the sensor/plug call sites
  (≈10 lines)
- Rebuild + redeploy on the Pi; restart the unit
- Verify with `ls /proc/$PID/fd | wc -l` staying flat across poll cycles
- No CLI/protocol/band behaviour changes