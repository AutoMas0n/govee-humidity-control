## MODIFIED Requirements

### Requirement: Continuous humidity-threshold loop
The SHALL run an infinite loop that reads H5179 humidity on a configurable
interval and toggles the H5080 plug according to a hysteresis band (hi/lo),
with state-change tracking to avoid redundant writes. While a dry-mode timer
is active, the plug SHALL be forced ON (see Dry clothes force mode).

#### Scenario: Humidity exceeds threshold
- **WHEN** the H5179 reading is greater than or equal to the high setpoint
  (`--hi`, default 55% RH) and no dry-mode timer is active
- **THEN** the daemon turns the H5080 plug ON, if it was OFF

#### Scenario: Humidity returns below threshold
- **WHEN** the H5179 reading drops to or below the low setpoint
  (`--lo`, default 45% RH) and no dry-mode timer is active
- **THEN** the daemon turns the H5080 plug OFF, if it was ON

#### Scenario: Humidity within the dead band (hold)
- **WHEN** the H5179 reading is between the low and high setpoints
  (`lo < humidity < hi`) and no dry-mode timer is active
- **THEN** the daemon does NOT change the plug state — it holds the last
  known state (no writes while inside the band)

#### Scenario: No state change needed
- **WHEN** the plug is already ON and humidity is still ≥ hi and no dry-mode
  timer is active
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

### Requirement: Dry clothes force mode
The daemon SHALL support a timer-based force mode that keeps the H5080 plug ON for a configured duration regardless of humidity. The bandwidth is suspended while the timer is active; on expiry the loop resumes the hysteresis band.

#### Scenario: Force ON starts
- **WHEN** a force request (e.g. via the status page `POST /dry`) sets a duration
- **THEN** the daemon turns the plug ON if it was OFF, and keeps it ON for the requested duration regardless of humidity

#### Scenario: Force ON holds through the band
- **WHEN** dry mode is active and humidity drops below the low setpoint
- **THEN** the plug remains ON (the band must not turn it OFF)

#### Scenario: Timer expiry resumes band
- **WHEN** the dry-mode timer expires
- **THEN** the daemon stops forcing and the next poll uses the hysteresis band, treating the read as a fresh first read (start OFF unless humidity ≥ high setpoint)

#### Scenario: Force mode cancelled
- **WHEN** a cancel request (e.g. `POST /dry-off`) arrives while a timer is active
- **THEN** the timer is cleared and the band resumes on the next poll

#### Scenario: Force mode survives daemon restart
- **WHEN** the daemon restarts while a dry-mode timer is still running (e.g. the daily 06:00 reboot)
- **THEN** the remaining force duration is restored and the plug continues to be held ON until expiry

#### Scenario: Stale force state after restart
- **WHEN** the daemon restarts and the persisted force deadline is already in the past
- **THEN** the force is treated as expired (no hold) and the band resumes normally

#### Scenario: Force mode does not conflict with status page
- **WHEN** dry mode is active and the status page is requested
- **THEN** the page shows the force mode state and remaining time
