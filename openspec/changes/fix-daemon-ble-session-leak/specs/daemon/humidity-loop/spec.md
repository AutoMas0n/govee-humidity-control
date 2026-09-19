# Humidity Loop Daemon Specification — Delta

## MODIFIED Requirements

### Requirement: Error resilience
The SHALL handle BLE scan/connection failures gracefully and continue the
loop, with exponential backoff on repeated failures.

#### Scenario: BLE scan fails
- **WHEN** an H5179 scan times out or fails
- **THEN** the daemon logs the error, sleeps the standard poll interval, and retries

#### Scenario: H5080 connection fails
- **WHEN** the daemon cannot connect to the H5080 plug
- **THEN** it logs the error and retries on the next poll cycle (no cascade failure)

#### Scenario: BLE session count stays bounded over the daemon lifetime
- **WHEN** the daemon has run for an extended period (hours to days) across many
  poll cycles
- **THEN** the number of open BLE/D-Bus session file descriptors does not grow
  with each cycle — a single adapter is reused for the process lifetime