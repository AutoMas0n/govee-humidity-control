# Humidity Loop Daemon Specification — Delta

## MODIFIED Requirements

### Requirement: Continuous humidity-threshold loop
The SHALL run an infinite loop that reads H5179 humidity on a configurable
interval and toggles the H5080 plug according to a hysteresis band (hi/lo),
with state-change tracking to avoid redundant writes.

#### Scenario: Humidity exceeds threshold
- **WHEN** the H5179 reading is greater than or equal to the high setpoint
  (`--hi`, default 55% RH)
- **THEN** the daemon turns the H5080 plug ON, if it was OFF

#### Scenario: Humidity returns below threshold
- **WHEN** the H5179 reading drops to or below the low setpoint
  (`--lo`, default 45% RH)
- **THEN** the daemon turns the H5080 plug OFF, if it was ON

#### Scenario: Humidity within the dead band (hold)
- **WHEN** the H5179 reading is between the low and high setpoints
  (`lo < humidity < hi`)
- **THEN** the daemon does NOT change the plug state — it holds the last
  known state (no writes while inside the band)

#### Scenario: No state change needed
- **WHEN** the plug is already ON and humidity is still ≥ hi
- **THEN** the daemon does NOT write to the plug (tracks last known state)

#### Scenario: Single-threshold compatibility
- **WHEN** the daemon is started with `--threshold N` (no band flags)
- **THEN** it behaves as a band where both setpoints equal N (ON above N,
  OFF at or below N) — the previous single-threshold contract is preserved

#### Scenario: Configurable interval
- **WHEN** the daemon starts with a poll interval flag (e.g., `--interval 300`)
- **THEN** it reads H5179 every N seconds (default: 900 / 15 minutes)

#### Scenario: CLI daemon with band
- **WHEN** `govee-ble daemon --hi 55 --lo 45` is run
- **THEN** it runs the continuous humidity-band loop with those setpoints

#### Scenario: Status page shows the band
- **WHEN** the daemon serves its status page (`--status-port`)
- **THEN** the page includes the `hi` and `lo` setpoints alongside the
  current reading, plug state, interval, and timestamp

## ADDED Requirements

### Requirement: Configurable band setpoints via CLI
The daemon SHALL accept `--hi` and `--lo` flags on the `daemon` subcommand,
with `--threshold N` retained as a compatibility alias for band N/N.

#### Scenario: Defaults when no setpoint flags given
- **WHEN** the daemon is started without `--hi`/`--lo`/`--threshold`
- **THEN** the high setpoint defaults to 55% RH and the low setpoint to 45% RH

#### Scenario: Partial specification
- **WHEN** only one of `--hi` or `--lo` is given
- **THEN** the missing setpoint takes its default (hi 55 / lo 45)