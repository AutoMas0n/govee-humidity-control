## Purpose

Serve the humidity controller's local status as a mobile-friendly dashboard — current readings, band setpoints, BLE signal health, last-updated age with miss indication — and expose manual dry-mode controls over HTTP. LAN-only, no external services.

## ADDED Requirements

### Requirement: Status page renders current system state
The status server SHALL serve an HTML dashboard at `GET /` that displays the latest good readings, band setpoints, plug state, poll interval, battery, and BLE signal health, on every request.

#### Scenario: Page loads
- **WHEN** a client requests `GET /` from the status port
- **THEN** the server responds HTTP 200 with an HTML page

#### Scenario: Readings and setpoints shown
- **WHEN** the page is rendered after at least one successful sensor read
- **THEN** it displays temperature, humidity, battery percent, plug state, `hi`/`lo` setpoints, and poll interval

#### Scenario: No external assets required
- **WHEN** the page is served
- **THEN** it depends only on the daemon's own response (no external CDN/JS/CSS must be fetched)

### Requirement: JSON state endpoint
The status server SHALL expose the same state as structured data at `GET /state.json` (temp, humidity, battery, rssi, plug, hi, lo, interval, last-ok timestamp, last-attempt timestamp, last error, force-mode remaining time).

#### Scenario: State fetched
- **WHEN** a client requests `GET /state.json`
- **THEN** the server responds HTTP 200 with a JSON object containing the current state fields

#### Scenario: State updates after each poll
- **WHEN** the daemon completes a poll cycle
- **THEN** `/state.json` reflects the new reading or the updated miss/last-error fields

### Requirement: Signal health indicator
The dashboard SHALL show measured BLE advertisement RSSI as a colored meter: green for strong (≥ −70 dBm), amber for marginal (−70…−85 dBm), red for weak (< −85 dBm), gray when unknown.

#### Scenario: Strong signal
- **WHEN** the last sensor advertisement was received at ≥ −70 dBm
- **THEN** the meter renders green

#### Scenario: Marginal signal
- **WHEN** the last sensor advertisement was received between −70 and −85 dBm
- **THEN** the meter renders amber

#### Scenario: Weak signal
- **WHEN** the last sensor advertisement was received below −85 dBm
- **THEN** the meter renders red

#### Scenario: Unknown signal
- **WHEN** no RSSI is available for the last sensor contact
- **THEN** the meter renders gray and does not claim a signal level

#### Scenario: RSSI capture never forces extra sensor contact
- **WHEN** the daemon collects the RSSI value
- **THEN** it comes from the same advertisement/scan pass used for the reading (no additional scan, connect, or sensor-side activity)

### Requirement: Last reading survives a missed poll
When a poll fails to read the sensor, the dashboard SHALL keep displaying the last successful reading, clearly labeled with how long ago it was taken, and indicate that the latest poll missed.

#### Scenario: Miss after a good reading
- **WHEN** the daemon fails to read the sensor but has a previous success
- **THEN** the dashboard keeps the last good values and shows a visible "missed" indicator plus the age of the last reading ("last reading N minutes ago")

#### Scenario: Consecutive misses
- **WHEN** the daemon misses several polls in a row
- **THEN** the dashboard continues showing the last good reading and the last-updated age grows accordingly (no blank or error-only page)

#### Scenario: Recovery
- **WHEN** a poll succeeds after one or more misses
- **THEN** the dashboard clears the miss indicator and shows the fresh reading and timestamp

### Requirement: Automatic refresh
The dashboard SHALL refresh its state automatically by polling `/state.json`.

#### Scenario: Page polls on an interval
- **WHEN** the dashboard is open in a browser
- **THEN** it re-fetches `/state.json` on a fixed interval (30 seconds) and updates in place

#### Scenario: Manual refresh
- **WHEN** a user taps the refresh control
- **THEN** the dashboard requests a live sensor read (`POST /poll`) and then re-fetches `/state.json`, showing the fresh data

#### Scenario: Manual refresh is rate-limited
- **WHEN** a user taps refresh more often than once every 30 seconds
- **THEN** the server responds 429 with a retry-after hint and the button shows a short wait before the next attempt

### Requirement: Dry-mode control via page
The dashboard SHALL provide controls to start and cancel dry mode: preset duration buttons (30 min, 1 h, 2 h, 4 h), a custom-minutes input, and a cancel control, all sending the corresponding request to the status server.

#### Scenario: Start dry mode with a preset
- **WHEN** a user taps a preset button (e.g. "2 h")
- **THEN** the dashboard sends `POST /dry?mins=120`, shows confirmation, and displays the active countdown

#### Scenario: Start dry mode with custom minutes
- **WHEN** a user enters a custom duration and confirms
- **THEN** the dashboard sends `POST /dry?mins=<N>` with the entered value

#### Scenario: Cancel dry mode
- **WHEN** a user taps cancel while dry mode is active
- **THEN** the dashboard sends `POST /dry-off` and clears the countdown

#### Scenario: Countdown display
- **WHEN** dry mode is active
- **THEN** the page shows the remaining time and updates it as time passes
