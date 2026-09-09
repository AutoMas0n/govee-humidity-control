## Purpose

Control the H5080 smart plug (ihoment_H5080) power state via Govee OpenAPI HTTP, using the existing API key to toggle the relay on or off based on humidity readings.

## ADDED Requirements

### Requirement: Toggle plug on

The system SHALL send a power ON command to the H5080 plug via the Govee OpenAPI HTTP endpoint.

#### Scenario: Power on succeeds

- **WHEN** the system sends a power ON request to `https://openapi.api.govee.com/router/api/v1/device/control` with SKU `H5080`, device ID `B4:8F:D4:AD:FC:41:E1:DC`, and capability `powerSwitch` value `1`
- **THEN** the system SHALL receive HTTP 200 and log the success

#### Scenario: Power on fails

- **WHEN** the API returns a non-200 HTTP status or a network error occurs
- **THEN** the system SHALL log the error and continue the loop (no crash)

### Requirement: Toggle plug off

The system SHALL send a power OFF command to the H5080 plug via the same endpoint.

#### Scenario: Power off succeeds

- **WHEN** the system sends a power OFF request with capability `powerSwitch` value `0`
- **THEN** the system SHALL receive HTTP 200 and log the success

### Requirement: Only toggle on state change

The system SHALL only send a power command when the desired state differs from the last known state.

#### Scenario: No toggle on same state

- **WHEN** humidity reading is > 45% and the plug was already ON in the previous cycle
- **THEN** the system SHALL NOT send any API request

#### Scenario: Toggle only on transition

- **WHEN** humidity reading was > 45% (ON) and now is <= 45% (OFF)
- **THEN** the system SHALL send exactly one power OFF request

### Requirement: Threshold-based decision

The system SHALL decide the desired plug state based on the current humidity reading and a fixed threshold.

#### Scenario: High humidity turns plug on

- **WHEN** humidity reading is greater than 45%
- **THEN** the desired plug state SHALL be ON

#### Scenario: Low humidity turns plug off

- **WHEN** humidity reading is less than or equal to 45%
- **THEN** the desired plug state SHALL be OFF

### Requirement: Use existing API key

The system SHALL authenticate using the API key stored in `api_key.secret`.

#### Scenario: API key loaded from file

- **WHEN** the system starts
- **THEN** it SHALL read the API key from `api_key.secret` in the project root directory

### Requirement: Handle API errors gracefully

The system SHALL handle HTTP errors and network issues without crashing.

#### Scenario: Timeout on API call

- **WHEN** the Govee API does not respond within 10 seconds
- **THEN** the system SHALL log a timeout warning and proceed to the next cycle

#### Scenario: Non-200 status code

- **WHEN** the API returns status code 429 or 5xx
- **THEN** the system SHALL log the error include the status code and continue