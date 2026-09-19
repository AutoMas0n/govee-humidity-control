## Purpose

Controls Govee H5080 smart plugs via BLE GATT with AES-128-ECB+RC4 encrypted 20-byte frames. Provides ON/OFF toggle, status query, and session handshake — fully local, no cloud dependency.

## ADDED Requirements

### Requirement: Connect and handshake
The SHALL establish a BLE GATT connection to an H5080 plug at a given MAC address and perform the E7 01/02 session handshake before any device commands.

#### Scenario: Successful handshake
- **WHEN** the controller connects to an H5080 plug and sends an E7 01 frame encrypted with KEY_COMM
- **THEN** the plug responds with an E7 01 notification containing a 16-byte session key, and the controller sends E7 02 to confirm

#### Scenario: Handshake timeout
- **WHEN** the plug does not respond to E7 01 within 5 seconds
- **THEN** the controller returns a timeout error and disconnects

### Requirement: Toggle plug state
The SHALL send encrypted ON/OFF commands to the plug after a successful handshake and device initialization sequence.

#### Scenario: Turn plug ON
- **WHEN** the controller sends `33 01 11` (encrypted with session key, zero-padded to 20 bytes with XOR checksum)
- **THEN** the plug toggles its relay ON and responds with a `33 01 00` notification

#### Scenario: Turn plug OFF
- **WHEN** the controller sends `33 01 10` (encrypted with session key, zero-padded to 20 bytes with XOR checksum)
- **THEN** the plug toggles its relay OFF and responds with a `33 01 00` notification

### Requirement: Query plug state
The SHALL query the plug's current ON/OFF state without toggling it.

#### Scenario: Query returns ON
- **WHEN** the controller sends `AA 01` and receives a notification with byte[2] = 0x01
- **THEN** the controller reports state ON

#### Scenario: Query returns OFF
- **WHEN** the controller sends `AA 01` and receives a notification with byte[2] = 0x00
- **THEN** the controller reports state OFF

### Requirement: Device initialization
The SHALL send the full init sequence (AA EF, 33 B2, 33 B5, AA 01, AA B0 ×2, AA 12, AA 13) before the plug accepts toggle commands.

#### Scenario: Init sequence
- **WHEN** the controller completes a handshake
- **THEN** the controller sends all 8 init frames in order with appropriate inter-frame delays

### Requirement: Crypto correctness
The SHALL produce and verify AES-128-ECB + RC4 encrypted 20-byte frames, with bytes 3..18 zeroed and byte 19 = XOR checksum of bytes 0..18.

#### Scenario: Encrypt round-trip
- **WHEN** any plaintext frame is encrypted with a known key and then decrypted with the same key
- **THEN** the decrypted output matches the original plaintext

#### Scenario: Plug rejects non-zero padding
- **WHEN** a frame has non-zero data in bytes 3..18
- **THEN** the plug silently ignores it (no notification, no state change)

### Requirement: Multiple MAC support
The SHALL be configurable to target any H5080 plug MAC address (not hardcoded).

#### Scenario: Different MAC
- **WHEN** the controller is initialized with MAC `D4:AD:FC:41:E1:DD` instead of the default `60:74:F4:BD:4D:E5`
- **THEN** it connects to and controls that plug instead