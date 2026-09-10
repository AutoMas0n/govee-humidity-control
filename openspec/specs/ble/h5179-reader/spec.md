# H5179 Reader Specification

## Purpose

Reads temperature, humidity, and battery level from Govee H5179 thermo-hygrometers by scanning BLE advertisements (no connection/pairing needed). One-shot read for use in a polling daemon.

## Requirements

### Requirement: Scan for H5179 advertisements
The SHALL scan BLE advertisements and filter for Govee H5179 devices by MAC address or manufacturer data.

#### Scenario: Find H5179 by MAC
- **WHEN** the scanner runs with target MAC `E3:32:81:12:40:A4` and the device is in range broadcasting
- **THEN** the scanner returns the device advertisement data within 10 seconds

#### Scenario: H5179 not in range
- **WHEN** the scanner runs and the target H5179 is out of range or powered off
- **THEN** the scanner returns a "not found" error within 15 seconds

### Requirement: Parse manufacturer data
The SHALL extract temperature, humidity, and battery from the H5179 manufacturer-specific data field (manufacturer ID 0xEC88).

#### Scenario: Parse valid advertisement
- **WHEN** the scanner receives manufacturer data with ID 0xEC88
- **THEN** it decodes: temperature = `(data[1] - 100) + data[2] / 10`, humidity = `data[3]`, battery = `data[4]`

#### Scenario: Unknown manufacturer data
- **WHEN** the advertisement has a manufacturer ID other than 0xEC88
- **THEN** the scanner skips that advertisement without error

### Requirement: Return structured reading
The SHALL return a structured result containing temperature (float °C), humidity (integer 0-100%), and battery percentage.

#### Scenario: Valid reading
- **WHEN** a valid H5179 advertisement is parsed
- **THEN** the result contains temperature ∈ [-20, 60], humidity ∈ [0, 100], battery ∈ [0, 100]

### Requirement: Configurable MAC
The SHALL accept the H5179 MAC address as a parameter (not hardcoded).

#### Scenario: Different MAC
- **WHEN** the reader is configured with a different H5179 MAC
- **THEN** it reads advertisements for that device instead of the default