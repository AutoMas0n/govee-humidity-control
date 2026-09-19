# Humidity Loop Daemon Specification

## Purpose

Runs a continuous loop that reads H5179 humidity and toggles the H5080 plug
based on a configurable hysteresis band (hi/lo) — ON above the high setpoint,
OFF below the low setpoint, hold in between. Replaces the current Python
main.py as a self-contained Rust binary for 24/7 systemd deployment.

## Requirements

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

### Requirement: Configurable band setpoints via CLI
The daemon SHALL accept `--hi` and `--lo` flags on the `daemon` subcommand,
with `--threshold N` retained as a compatibility alias for band N/N.

#### Scenario: Defaults when no setpoint flags given
- **WHEN** the daemon is started without `--hi`/`--lo`/`--threshold`
- **THEN** the high setpoint defaults to 55% RH and the low setpoint to 45% RH

#### Scenario: Partial specification
- **WHEN** only one of `--hi` or `--lo` is given
- **THEN** the missing setpoint takes its default (hi 55 / lo 45)

### Requirement: CLI interface
The SHALL provide a CLI with subcommands: `read` (one-shot H5179), `on`/`off` (toggle plug), `status` (query plug), and `daemon` (continuous loop).

#### Scenario: CLI read
- **WHEN** `govee-ble read` is run
- **THEN** it scans for H5179 once and prints temperature, humidity, battery

#### Scenario: CLI on
- **WHEN** `govee-ble on` is run
- **THEN** it connects to H5080, handshakes, initializes, and turns the plug ON

#### Scenario: CLI off
- **WHEN** `govee-ble off` is run
- **THEN** it connects to H5080, handshakes, initializes, and turns the plug OFF

#### Scenario: CLI status
- **WHEN** `govee-ble status` is run
- **THEN** it connects to H5080, handshakes, queries state, and prints ON/OFF

#### Scenario: CLI daemon
- **WHEN** `govee-ble daemon --interval 300 --threshold 50` is run
- **THEN** it runs the continuous humidity-threshold loop with those parameters

### Requirement: Systemd integration
The binary SHALL log to stdout/stderr in a format suitable for systemd journald capture, and SHALL handle SIGTERM/SIGINT for graceful shutdown.

#### Scenario: Graceful shutdown
- **WHEN** the daemon receives SIGTERM while idle between poll cycles
- **THEN** it logs "shutting down" and exits cleanly within 2 seconds

### Requirement: Error resilience
The SHALL handle BLE scan/connection failures gracefully and continue the loop, with exponential backoff on repeated failures.

#### Scenario: BLE scan fails
- **WHEN** an H5179 scan times out or fails
- **THEN** the daemon logs the error, sleeps the standard poll interval, and retries

#### Scenario: H5080 connection fails
- **WHEN** the daemon cannot connect to the H5080 plug
- **THEN** it logs the error and retries on the next poll cycle (no cascade failure)