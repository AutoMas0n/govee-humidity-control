## Purpose

Control the H5080 smart plug (ihoment_H5080) power state over BLE using the V2 encrypted protocol that newer Govee devices require — AES-128-GCM handshake followed by wrapped 20-byte command frames.

## ADDED Requirements

### Requirement: BLE V2 handshake on each connection

The system SHALL perform a V2 encrypted handshake when connecting to the H5080 plug, before sending any command frames.

#### Scenario: Handshake succeeds

- **WHEN** the system connects to the plug and sends a handshake frame (`0xe7 0x11 0x01` + random IV + GCM-encrypted txIvKey)
- **THEN** the system SHALL receive a handshake response (`0xe7 0x11 0x00` + IV + GCM-encrypted rxIvKey + device info) within 3 seconds

#### Scenario: Handshake fails

- **WHEN** the plug does not respond to the handshake or responds with a non-zero status byte
- **THEN** the system SHALL log the error, disconnect, and retry on the next cycle

### Requirement: Derive per-connection device key

The system SHALL derive the per-connection AES device key from the handshake response to encrypt command frames.

#### Scenario: Key derivation from handshake response

- **WHEN** a successful handshake response is received
- **THEN** the system SHALL derive the device key using AES-128-ECB(KEY_DEVICE, padded devInfo) and store txIvKey + rxIvKey for this session

### Requirement: Send wrapped power ON command

The system SHALL send a power ON command wrapped in the V2 encrypted frame format.

#### Scenario: Power ON succeeds

- **WHEN** the system sends a V2-wrapped frame containing `0x33 0x01 0x01` with XOR checksum, using the per-session txIvKey and counter
- **THEN** the plug relay SHALL close (power ON) and the system SHALL log success

#### Scenario: Power ON fails

- **WHEN** the write fails or no notification response is received
- **THEN** the system SHALL log the error and retry on the next cycle

### Requirement: Send wrapped power OFF command

The system SHALL send a power OFF command wrapped in the V2 encrypted frame format.

#### Scenario: Power OFF succeeds

- **WHEN** the system sends a V2-wrapped frame containing `0x33 0x01 0x00` with XOR checksum
- **THEN** the plug relay SHALL open (power OFF) and the system SHALL log success

### Requirement: Only toggle on state change

The system SHALL only send a power command when the desired state differs from the last known state.

#### Scenario: No toggle on same state

- **WHEN** humidity reading is > 45% and the plug was already ON in the previous cycle
- **THEN** the system SHALL NOT send any BLE power command

#### Scenario: Toggle on transition

- **WHEN** humidity reading crosses the 45% threshold
- **THEN** the system SHALL send exactly one power command (ON or OFF)

### Requirement: Threshold-based decision

The system SHALL decide plug state based on the current humidity reading.

#### Scenario: High humidity turns plug on

- **WHEN** humidity reading is greater than 45%
- **THEN** the desired plug state SHALL be ON

#### Scenario: Low humidity turns plug off

- **WHEN** humidity reading is less than or equal to 45%
- **THEN** the desired plug state SHALL be OFF

### Requirement: Handle BLE connection errors gracefully

The system SHALL handle BLE errors without crashing.

#### Scenario: Connection timeout

- **WHEN** the plug does not respond to connection within 20 seconds
- **THEN** the system SHALL log a timeout warning and proceed to the next cycle

#### Scenario: Intermittent connection failures

- **WHEN** consecutive BLE failures occur
- **THEN** the system SHALL use exponential backoff (5 min → 15 min → 60 min maximum)

### Requirement: Fallback to LAN protocol

If BLE power control fails after multiple retries, the system SHALL attempt to discover and control the plug via Govee LAN UDP protocol.

#### Scenario: LAN discovery

- **WHEN** BLE fails to toggle the plug after 3 consecutive attempts
- **THEN** the system SHALL broadcast a scan for Govee LAN devices on ports 4001-4003

#### Scenario: LAN power toggle

- **WHEN** a Govee device is discovered via LAN with matching MAC/identifier
- **THEN** the system SHALL attempt to send power commands over UDP using the Govee LAN protocol