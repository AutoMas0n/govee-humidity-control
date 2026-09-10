# Humidity Loop Daemon Specification

## Purpose

Runs a continuous loop that reads H5179 humidity and toggles the H5080 plug based on a configurable threshold. Replaces the current Python main.py as a self-contained Rust binary for 24/7 systemd deployment.

## Requirements

### Requirement: Continuous humidity-threshold loop
The SHALL run an infinite loop that reads H5179 humidity on a configurable interval and toggles the H5080 plug when humidity crosses a threshold, with state-change tracking to avoid redundant writes.

#### Scenario: Humidity exceeds threshold
- **WHEN** the H5179 reading exceeds the configured threshold (default: 45% RH)
- **THEN** the daemon turns the H5080 plug ON, if it was OFF

#### Scenario: Humidity returns below threshold
- **WHEN** the H5179 reading drops to or below the configured threshold
- **THEN** the daemon turns the H5080 plug OFF, if it was ON

#### Scenario: No state change needed
- **WHEN** the plug is already ON and humidity is still above threshold
- **THEN** the daemon does NOT write to the plug (tracks last known state)

#### Scenario: Configurable interval
- **WHEN** the daemon starts with a poll interval flag (e.g., `--interval 300`)
- **THEN** it reads H5179 every N seconds (default: 900 / 15 minutes)

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