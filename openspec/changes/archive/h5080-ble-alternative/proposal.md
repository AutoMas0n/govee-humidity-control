## Why

The H5080 smart plug (ihoment_H5080) does not support BLE power control — extensive testing proved BLE connects and writes succeed, but the relay never toggles. Only the Govee cloud API (HTTP/MQTT) and LAN/UDP protocol can control the plug's relay. This blocks Phase 1 of the ROADMAP's BLE-local approach.

Meanwhile, the H5179 humidity sensor works perfectly over BLE advertisements — no cloud needed for reading temperature and humidity.

We need to replace the current cloud-only `main.py` with a hybrid: read humidity locally via BLE (H5179), but use the Govee cloud HTTP API to toggle the plug (H5080). This gets us 90% to the goal of local-first control, while avoiding hardware changes or plug replacements.

## What Changes

- Replace `main.py` with a new Python script that reads H5179 humidity via BLE (bleak) and controls H5080 via Govee OpenAPI HTTP
- Update `requirements.txt` from `requests` to `bleak` (remove requests, add bleak)
- Update `README.md` to document the hybrid BLE+cloud architecture
- Remove obsolete files: `api_key.secret` is still needed (for cloud API), but `Cargo.toml`, `config.toml`, `setup.sh`, `require.py` are deleted
- Preserve the ROSADMAP's Phases 3 (Rust rewrite) and 4 (deploy) for future

## Capabilities

### New Capabilities
- `ble/h5179-reader`: Read H5179 temperature and humidity from BLE advertisements using bleak
- `cloud/h5080-controller`: Control H5080 smart plug via Govee OpenAPI HTTP

### Modified Capabilities
- *(none — no existing specs to modify)*

## Impact

- **Code**: `main.py` completely rewritten. New file replaces cloud-only with hybrid BLE+cloud
- **Dependencies**: `requests` removed, `bleak` added to requirements
- **Infrastructure**: `api_key.secret` retained (cloud API still needed for plug). Systemd service continues unchanged
- **Devices**: H5179 reads BLE locally; H5080 controlled via cloud API (same endpoint as original)
- **Network**: Internet required only for plug toggle (~2 requests per 15 min), humidity reading is fully local