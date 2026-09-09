# Govee Humidity Control — Refactoring Plan

## For: Offline, Reliable, Local-Only Operation

---

## 1. Current State

**Location:** `/home/pi/Github/govee-humidity-control/` on `pi@192.168.2.21`

### What's running

| Component | Technology | Status |
|-----------|-----------|--------|
| `main.py` | Python loop | ✅ Active (PID 572, started ~06:00 daily) |
| Humidity read | Govee Cloud API (POST) | ❌ Fails often — timeouts, DNS failures |
| Plug control | Govee Cloud API (POST) | ❌ Fails often — same issues |
| Rust version | `Cargo.toml` only, no `src/` | ❌ Abandoned, deleted locally |

### Devices

| Device | SKU | MAC | Role |
|--------|-----|-----|------|
| Govee Thermo-Hygrometer | H5179 | `FA:8C:E3:32:81:12:40:A4` | Humidity sensor |
| Govee Smart Plug | H5080 | `B4:8F:D4:AD:FC:41:E1:DC` | Switch (turns on/off) |

### API Key

`d67acb7a-47fa-403a-b0c9-08999d391b29` (stored in `api_key.secret`)

### Logic

```python
while True:
    humidity = check_humidity(api_key)   # via cloud API
    if humidity > 45:
        control_device(api_key, 1)        # turn plug ON
    else:
        control_device(api_key, 0)        # turn plug OFF
    time.sleep(900)                       # every 15 minutes
```

### Known Failures

From `request_logs.log` (latest entry: Aug 5 2026 — entire log is nothing but errors):

- **Read timeouts** — `HTTPSConnectionPool: Read timed out. (read timeout=10)`
- **DNS failures** — `Temporary failure in name resolution` (Pi's internet drops)
- **Result:** The script has been running but **nothing has worked** since May 2026.

---

## 2. The Problem

The current architecture has **two cloud dependencies** — both single points of failure:

```
┌─────────┐  HTTPS  ┌──────────────┐  HTTPS  ┌─────────┐
│ H5179   │ ──────▶ │  Govee Cloud │ ──────▶ │ main.py │
│ (sensor)│         │   API        │         │         │
└─────────┘         └──────────────┘         │         │
                                              │         │
┌─────────┐  HTTPS  ┌──────────────┐         │         │
│ H5080   │ ◀────── │  Govee Cloud │ ◀────── │         │
│ (plug)  │         │   API        │         └─────────┘
└─────────┘         └──────────────┘
```

When the Pi loses internet (which is frequent), **nothing works**.

---

## 3. The Fix: BLE-Only Local Control

Both devices support **Bluetooth Low Energy (BLE)** — no cloud, no internet, no API key required.

```
┌─────────┐  BLE adv  ┌──────────────┐  BLE GATT  ┌─────────┐
│ H5179   │ ────────▶ │  Pi 4 (BLE)  │ ────────▶  │ H5080   │
│ (sensor)│  (read)   │              │  (control)  │ (plug)  │
└─────────┘           └──────────────┘             └─────────┘
                      │ main.py
                      │ (local, no internet)
                      └──────────────┘
```

### H5179 — Read via BLE Advertisements

The H5179 broadcasts temperature and humidity in **unencrypted BLE advertisement packets**. No connection needed — just scan and decode the manufacturer data.

- **Manufacturer ID:** `0xEC88`
- **Data format (bytes 0-4 of manufacturer data):**
  - Byte 0: Reserved
  - Byte 1: Temperature integer (signed, °C × 10 + 100 offset)
  - Byte 2: Temperature decimal (0-9, tenths)
  - Byte 3: Humidity (%)
  - Byte 4: Battery (%)
- **Full temp:** `(byte1 - 100) + (byte2 / 10)` → °C
- **No pairing needed** — data is in the broadcast, always visible.

### H5080 — Control via BLE GATT

The H5080 is a **dual WiFi + BLE** device. The Govee Home app uses BLE for local control. The protocol is documented by the open-source community (e.g., `govee-ble` projects, Homebridge Govee plugin).

- **GATT Service UUID:** `0000ec88-0000-1000-8000-00805f9b34fb`
- **Write characteristic UUID:** `00010203-0405-0607-0809-0a0b0c0d1912`
- **Power ON command (hex):** `33 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 35`
- **Power OFF command (hex):** `33 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 34`
- **Connection required** — unlike the H5179, you need to connect to the device and write to the characteristic.

### Dependencies

```bash
pip install bleak
```

`bleak` is a cross-platform BLE library that works on Linux (Pi 4) with the built-in BlueZ stack.

---

## 4. Pi 4 BLE Situation

### Investigation Results

The Pi 4's on-board BLE chip is **not working properly**:

- `hci0` shows `AA:AA:AA:AA:AA:AA` as the BD address (fake/default — firmware not loaded)
- `hcitool lescan` fails with `Set scan parameters failed: Input/output error`
- `bluetoothctl` segfaults
- `dmesg` shows **command tx timeouts** on the UART HCI interface
- Classic Bluetooth (BR/EDR) scan works (found `Fosi Audio BT20A` and `GVAUDIO`)
- BLE (Bluetooth Low Energy) is broken

### Likely Causes (in order of probability)

1. **Wi-Fi rfkill blocking BLE** — The Pi's Wi-Fi is soft-blocked (`rfkill` shows `Soft blocked: yes` for phy0). On the CYW43455 combo chip, Wi-Fi and Bluetooth share the same antenna. If Wi-Fi is blocked, Bluetooth may also be affected. **Fix:** `sudo rfkill unblock wifi` and configure Wi-Fi country via `raspi-config`.

2. **Missing or corrupted BLE firmware** — The Pi 4 needs `BCM4345C0.hcd` loaded by `hciattach`. The file exists (`/lib/firmware/brcm/BCM4345C0.hcd`) but the `btattach` fails with `Failed to set flags: Device or resource busy`. This could be because the firmware load is racing with `systemd`'s bluetooth service.

3. **UART clock issues** — The Pi 4 uses UART for the Bluetooth HCI. If the core frequency changes (e.g., due to power saving), the UART baud rate drifts and the BCM chip drops commands.

4. **Hardware fault** — The Pi 4's BCM4345C0 could be damaged. If software fixes fail, a **$5 USB BLE dongle** (e.g., CSR 4.0, ASUS BT-400) is the simplest workaround.

### Proposed Fixes (in order)

1. **Run `sudo raspi-config`** → go to **Localisation Options** → **Wi-Fi Country** → set to `GB` (or appropriate country). This unblocks the antenna and may fix both Wi-Fi and BLE.

2. **If that doesn't work:** `sudo rfkill unblock all` and `sudo systemctl restart bluetooth`.

3. **If still broken:** `sudo apt-get install --reinstall pi-bluetooth firmware-brcm80211`.

4. **If still broken:** Buy a USB BLE dongle (~$5). Plug it in, `hciconfig` will show `hci1` with a real MAC address, and BLE will just work.

---

## 5. Refactored Architecture

### Target: `main.py` rewritten to use BLE

```python
import asyncio
from bleak import BleakScanner, BleakClient

H5179_MAC = "FA:8C:E3:32:81:12:40:A4"
H5080_MAC = "B4:8F:D4:AD:FC:41:E1:DC"
HUMIDITY_THRESHOLD = 45
CHECK_INTERVAL = 900  # 15 minutes

GOVEE_SERVICE_UUID = "0000ec88-0000-1000-8000-00805f9b34fb"
CONTROL_CHAR_UUID  = "00010203-0405-0607-0809-0a0b0c0d1912"
POWER_ON  = bytes.fromhex("3301010000000000000000000000000000000035")
POWER_OFF = bytes.fromhex("3301000000000000000000000000000000000034")

async def read_humidity():
    """Read H5179 via BLE advertisement (no connection needed)."""
    def callback(device, adv_data):
        if device.address == H5179_MAC and adv_data.manufacturer_data:
            data = adv_data.manufacturer_data.get(0xEC88)
            if data and len(data) >= 4:
                temp = (data[1] - 100) + (data[2] / 10)
                humidity = data[3]
                return humidity  # via closure

    scanner = BleakScanner()
    device = await scanner.get_device_by_address(H5179_MAC, timeout=5)
    if device:
        _, adv = await scanner.get_device_and_advertisement_data(device)
        data = adv.manufacturer_data.get(0xEC88)
        if data and len(data) >= 4:
            return data[3]  # humidity
    return None

async def control_plug(power_on: bool):
    """Turn H5080 on/off via BLE GATT (requires connection)."""
    async with BleakClient(H5080_MAC) as client:
        cmd = POWER_ON if power_on else POWER_OFF
        await client.write_gatt_char(CONTROL_CHAR_UUID, cmd)

async def main():
    while True:
        humidity = await read_humidity()
        if humidity is not None:
            await control_plug(humidity > HUMIDITY_THRESHOLD)
        await asyncio.sleep(CHECK_INTERVAL)

asyncio.run(main())
```

### Safety Improvements

| Issue | Current | Fixed |
|-------|---------|-------|
| Internet dependency | Required for both read and control | Zero — fully local BLE |
| DNS failure kills everything | Yes | N/A — no DNS needed |
| No reconnection handling | Dies on first error | BLE stack auto-reconnects |
| 15-min polling even after failures | Yes — spams error logs | Add exponential backoff |
| No state tracking | Calls API every cycle regardless | Track current plug state; only write when changed |
| Single point of failure | Cloud API | BLE — both devices on same room, Pi is always in range |

### Run Strategy

Instead of a bare `while True` loop, use:

1. **systemd service** with `Restart=always` (already planned in README)
2. **Or cron job** every 15 minutes (simpler, no daemon management)
3. **Or crontab for pi user** — unlike the current setup (no cron, no systemd, just a bare process that may or may not survive reboot)

---

## 6. Rollout Plan

### Phase 1: Fix Pi BLE
- [ ] `sudo raspi-config` → set Wi-Fi country
- [ ] `sudo rfkill unblock all`
- [ ] Verify: `sudo hcitool lescan` shows devices
- [ ] Fallback: buy USB BLE dongle

### Phase 2: Test BLE Read Only
- [ ] Write BLE scanner script for H5179
- [ ] Verify humidity readings match Govee app
- [ ] Run alongside existing cloud-based script for comparison

### Phase 3: Test BLE Write
- [ ] Write BLE control script for H5080
- [ ] Test toggle on/off manually
- [ ] Verify plug state in Govee app matches

### Phase 4: Replace Main Loop
- [ ] Deploy `main.py` rewrite using BLE
- [ ] Add state tracking (only write when state changes)
- [ ] Add exponential backoff on failures
- [ ] Add health check / watchdog

### Phase 5: Setup Resilient Launch
- [ ] Install systemd service OR cron job
- [ ] Remove old cloud-based process
- [ ] Test reboot survival
- [ ] Remove `api_key.secret` (no longer needed)
- [ ] Remove `requests` dependency (no longer needed)

---

## 7. BLE Protocol Reference

### H5179 (Thermo-Hygrometer) — Advertisement Data

| Offset | Length | Description |
|--------|--------|-------------|
| 0 | 1 | Reserved (packet type) |
| 1 | 1 | Temperature integer (signed, °C × 10, offset +100) |
| 2 | 1 | Temperature decimal (0-9, tenths of °C) |
| 3 | 1 | Humidity (%) |
| 4 | 1 | Battery (%) |

**Formula:** `temp_c = (data[1] - 100) + data[2] / 10`
**Example:** `data[1]=218, data[2]=5` → `(218-100) + 0.5 = 18.5°C`

### H5080 (Smart Plug) — GATT Control

| Item | Value |
|------|-------|
| Service UUID | `0000ec88-0000-1000-8000-00805f9b34fb` |
| Write char UUID | `00010203-0405-0607-0809-0a0b0c0d1912` |
| Power ON (hex) | `33 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 35` |
| Power OFF (hex) | `33 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 34` |
| Write type | Write with response (or without, depending on device fw) |

**Note:** The command bytes are documented in the open-source community (Homebridge Govee plugin, `govee-ble`, OpenHAB bindings). Verify with a BLE sniffer if the exact commands differ.

---

## 8. Files to Delete After Migration

Once BLE is working and the cloud API is no longer needed:

- `api_key.secret` — API key is irrelevant for local-only
- `request_logs.log` — old log full of errors
- `require.py` — no longer needed
- `requirements.txt` — change from `requests` to `bleak`
- `nohup.out` — old process artifacts

---

*Documented: 2026-09-05*
*Based on investigation of pi@192.168.2.21 and local BLE protocol research*