# Design: Fix Daemon BLE Session Leak

## Context

The daemon loop calls `read_sensor` → `adapter()` → `Manager::new()` →
`BluetoothSession::new()` once per 15-min cycle. Each `BluetoothSession::new()`
opens a new system-bus D-Bus connection (`dbus_tokio::connection::new_system_sync()`).
bluez-async stores the connection in the session; neither bluez-async nor
dbus-tokio implements `Drop`/`close` on the connection, and the spawned
connection task runs forever, so every cycle leaks one socket. Measured: 12 fds
at start → 46 fds after 8.7 h (≈1 per cycle), with read failures accelerating
from ~1 in 6 to 4-in-a-row. See proposal.md for the symptom story.

## Goals / Non-Goals

**Goals:**
- Keep the daemon's open BlueZ/D-Bus session count flat over its lifetime
- Zero behaviour change to CLI, protocol, band logic, status page

**Non-Goals:**
- No changes to the one-shot CLI commands (their single session dies with the
  process — not a leak)
- Not patching btleplug/bluez-async/dbus-tokio themselves
- No systemd "restart daily" band-aid (symptom patch, not root cause)

## Decisions

### Decision: One adapter created in `daemon_loop`, threaded into workers

`daemon_loop` creates `let c = adapter().await` once before the loop and passes
it as a parameter to `read_sensor` and `plug_on`/`plug_off`/`plug_status`, which
forward it to `try_plug_inner`. The CLI one-shot arms keep calling `adapter()`
themselves as today (their process exits, so one session is fine).

**Alternatives considered:**
- A cached singleton inside `adapter()`: touches every caller anyway for the
  lifetime-scope question, and a global mutable in this single-file binary is
  more magic than a parameter. Parameter threading is explicit and testable.
- Daily systemd restart: would mask the symptom but leave the leak; a restart
  also resets the plug state memory mid-cycle. Rejected — root cause fix only.

### Decision: `try_plug_inner` no longer calls `adapter()`

It already receives the adapter via the plug helpers; the only change is removing
its internal `adapter()`/`drop(c)` pair and using the passed session. Retry logic
(3 attempts) is unchanged and reuses the same session per attempt.

## Risks / Trade-offs

- **[Risk] Sharing one adapter across scan + connect work in the same task** →
  bluez-async sessions are designed for this (a session is the process's
  connection to the bus, not a single operation); CPU/async state is task-local.
  Verified: one-shot commands already reuse `c` for scan-then-connect patterns
  (e.g., `pair`).
- **[Risk] `drop(c)` removal hides a session we still need** → `daemon_loop`
  owns the adapter for the loop's life; nothing drops it until process exit,
  which is the correct lifetime.

## Migration Plan

1. Implement: thread `c` through `daemon_loop` → `read_sensor`/`plug_*` →
   `try_plug_inner`; drop the internal `adapter()`/`drop(c)` calls.
2. `cargo build --release` on the Pi; `cargo test` still 5/5.
3. Restart `humidity-daemon`.
4. Verify fd count stays flat: `ls /proc/$PID/fd | wc -l` sampled across 2–3
   cycles (was +1/cycle before), and `govee-ble status`/status page respond.

Rollback: revert the threading commit and reinstall (unit unchanged — the
rollback is a binary swap, no config change).

## Open Questions

None.