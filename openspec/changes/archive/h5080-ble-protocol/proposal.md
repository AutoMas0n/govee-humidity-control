> **STATUS: DEPRECATED — This approach was a speculative guess at the BLE protocol and did not work.
> See `h5080-ble-packet-sniff` for the correct approach: capture real Govee app traffic instead.
> Archived as `openspec/changes/archive/h5080-ble-protocol/`**

## Why

The H5080 smart plug (ihoment_H5080) is the only component blocking the ROADMAP's fully-local BLE goal. The H5179 humidity sensor works perfectly over BLE advertisements, and the Govee app proves BLE control of the plug is possible (user confirmed toggling over BLE with WiFi off). However, direct BLE power commands have no effect — the relay never toggles despite successful writes (LED flashes). This change resolves that gap.

## What Changes

- **New**: Python BLE V2 encrypted protocol implementation for H5080 (AES-128-GCM handshake + wrapped commands), following the protocol documented in the homebridge-govee `ble-crypto.js` source
- **New**: Proper BlueZ pairing flow — register a D-Bus agent, pair with the plug, then issue encrypted power commands
- **New**: BLE notify characteristic monitoring to capture plug status responses
- **Modified**: `main.py` replaced with hybrid BLE-local version: read H5179 via BLE, control H5080 via BLE (encrypted protocol)
- **Fallback**: LAN UDP protocol control (ports 4002/4003) if BLE path proves infeasible

## Capabilities

### New Capabilities
- `ble/h5080-power-control`: Control H5080 smart plug power state over BLE using the V2 encrypted protocol (AES-128-GCM handshake + wrapped 20-byte frames)
- `ble/h5179-reader`: Read H5179 temperature and humidity from unencrypted BLE advertisements

### Modified Capabilities
- *(none — no existing specs to modify)*

## Impact

- **Code**: `main.py` completely rewritten. New `ble/` module for H5179 reader + H5080 controller
- **Dependencies**: `bleak` (already installed), `openssl` CLI (available on Pi) for AES-GCM, `dbus-python` for pairing
- **Infrastructure**: Systemd service continues. `api_key.secret` removed — no cloud dependency at all
- **Devices**: Both H5179 and H5080 controlled locally via BLE. Zero internet required
- **Network**: All operations local. Plug discovery and control via BLE only