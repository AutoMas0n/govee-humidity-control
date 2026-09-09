## Purpose

Read H5179 thermo-hygrometer temperature and humidity data from unencrypted BLE advertisements, providing local humidity readings without cloud dependency or device pairing.

## ADDED Requirements

### Requirement: Scan for H5179 device

The system SHALL scan for the H5179 device by its BLE MAC address and capture its advertisement data.

#### Scenario: Device found on BLE scan

- **WHEN** the system starts a BLE scan with a 15-second timeout targeting MAC `E3:32:81:12:40:A4`
- **THEN** the system SHALL either return the device's manufacturer data or report the device as not found

#### Scenario: Device not in range

- **WHEN** the H5179 device is not in BLE range after the scan timeout
- **THEN** the system SHALL log a warning and skip the reading cycle (no crash, no retry storm)

### Requirement: Parse temperature from manufacturer data

The system SHALL parse temperature in degrees Celsius from the H5179 manufacturer advertisement data.

#### Scenario: Parse temperature from valid data

- **WHEN** manufacturer data contains a valid temperature reading in the Govee H5179 format (little-endian signed short at byte offset 4 with tenths at byte offset 5; formula: `(data[4] - 100) + data[5] / 10`)
- **THEN** the system SHALL return a temperature as a float in degrees Celsius

### Requirement: Parse humidity from manufacturer data

The system SHALL parse relative humidity percentage from the H5179 manufacturer advertisement data.

#### Scenario: Parse humidity from valid data

- **WHEN** manufacturer data contains a valid humidity reading (byte 6, integer 0-100)
- **THEN** the system SHALL return humidity as an integer percentage (0-100)

### Requirement: Handle parse errors gracefully

The system SHALL handle malformed advertisement data without crashing.

#### Scenario: Missing or truncated manufacturer data

- **WHEN** manufacturer data is shorter than expected (fewer than 7 bytes)
- **THEN** the system SHALL log a warning and return None for both temperature and humidity

## ADDED Requirements

### Requirement: Read on a fixed interval

The system SHALL read humidity on a configurable interval (default: every 15 minutes).

#### Scenario: Cycle executes on schedule

- **WHEN** the configured interval elapses
- **THEN** the system SHALL perform one BLE scan + parse cycle for the H5179

### Requirement: No pairing required

The system SHALL read H5179 data from unencrypted BLE advertisements without pairing or connecting to the device.

#### Scenario: Read without connection

- **WHEN** scanning for the H5179
- **THEN** the system SHALL use passive BLE advertisement scanning only (no GATT connection)

### Requirement: State tracking for plug control decision

The system SHALL expose the humidity reading for downstream use by the plug controller capability.

#### Scenario: State accessible to plug controller

- **WHEN** the plug controller needs the current humidity
- **THEN** the system SHALL provide the latest parsed humidity value