# Humidity Hysteresis Band

## Why

The daemon toggles the plug at a single 45% threshold, but a full year of
1-minute H5179 data (2025-09 → 2026-09, ~490k samples) shows this
**chatters**: ~6,042 toggle cycles/year, ~27 plug clicks/day in summer,
because basement humidity hovers right around the setpoint. Hysteresis —
ON above a high setpoint, OFF below a low setpoint, hold in between —
eliminates the chatter with the same dehumidifier effect.

## What Changes

- Daemon `--threshold` becomes a **band**: add `--hysteresis` (or `--hi/--lo`)
  flags; ON when humidity ≥ high, OFF when ≤ low, hold otherwise (default
  high 55, low 45 — chosen from the year of data: 55/45 keeps the basement
  below mold risk with 1/10 the runtime, 318 vs 6,042 cycles/yr)
- `--threshold` (single number) stays supported as `--threshold N` = band
  `N`/`N` (backward compatible) — or is reinterpreted as the low setpoint
  with a default hysteresis width
- Defaults in `humidity-daemon.service` updated to the band values
- Status page gains `hi`/`lo` fields
- **Not a breaking change** for the CLI contract; only the daemon default
  behaviour changes

## Capabilities

### Modified Capabilities

- `daemon/humidity-loop`: the continuous-loop requirement changes from a
  single threshold crossing to a hysteresis band with hold behaviour

## Impact

- `govee-ble/src/main.rs` daemon logic (one function: the `need_on` decision)
- `govee-ble/humidity-daemon.service` (default hi/lo values)
- Status page output (add hi/lo)
- No changes to protocol, BLE, or other subcommands
- Data-backed justification in `scripts/humidity_analysis.py` (reproducible)