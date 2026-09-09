# H5080 BLE Protocol

Complete reverse-engineered BLE protocol for the Govee H5080 smart plug
(`ihoment_H5080`). Control from any BLE-capable Linux device — no cloud, no WiFi.

## Device Info

| Property | Value |
|----------|-------|
| Model | H5080 (ihoment_H5080) |
| MACs in range | `60:74:F4:BD:4D:E5`, `D4:AD:FC:41:E1:DD`, `D4:AD:FC:42:E2:45` |
| GATT Service UUID | `00010203-0405-0607-0809-0a0b0c0d1910` |
| Write Characteristic | `00010203-0405-0607-0809-0a0b0c0d2b11` (handle 0x0011) |
| Notify Characteristic | `00010203-0405-0607-0809-0a0b0c0d2b10` (handle 0x000e) |
| MTU | 20-byte payloads (matches BLE minimum) |
| Encryption | AES-128-ECB (first 16B) + RC4 (last 4B) |

## Protocol Overview

Every interaction is **20-byte frames** encrypted with AES-128-ECB + RC4:

```
Plaintext frame: [cmd(1) sub(1) data(0-16) zeros(16-data) xor-checksum(1)]
Encrypted:        AES-128-ECB(frame[0:16]) + RC4(frame[16:20])
```

- Bytes 2..18 after data are **zero-filled** (the plug validates this)
- Byte 19 = XOR of bytes 0..18
- All fields big-endian

## Static Key

Derived from reverse-engineering the Govee Home app APK:

```
KEY_COMM = b"MakingLifeSmarte"  # 16 bytes, AES-128
```

Decryption chain in the APK:
- `app_communication` resource string: `B8D8F6B2C294122FF9...`
- `app_session` resource string: `chiygnveeihhmme_govee_sessioniyz`
- AES-256-ECB decrypt → `4D616B696E674C696665536D61727465`
- Parse hex → `MakingLifeSmarte` (16 bytes)

## Session Handshake (Required)

A session handshake is required before any commands. Each connection gets a
unique 16-byte session key.

### Step 1: E7 01 (Phone → Device)

Encrypted with `KEY_COMM`:

```
Plaintext:  E7 01 [16 random bytes] [padding] [XOR checksum]
```

### Step 2: E7 01 Response (Device → Phone, via NOTIFY)

Decrypt notification with `KEY_COMM`:

```
Plaintext:  E7 01 [16-byte session key] [padding] [XOR checksum]
```

### Step 3: E7 02 (Phone → Device)

Encrypted with `KEY_COMM`. The device echoes this back as acknowledgment.

```
Plaintext:  E7 02 [16 random bytes] [padding] [XOR checksum]
```

All subsequent frames use the **16-byte session key** from Step 2.

## Command Formats

All decrypted with session key. All frames are 20 bytes.

### Switch State Query

| Byte | Value | Description |
|------|-------|-------------|
| 0 | `0xAA` | Command: status |
| 1 | `0x01` | Sub-command: query |
| 2..18 | `0x00` | Padding (zeros) |
| 19 | XOR CS | Checksum |

**Device response** (notification): byte[2] = `0x00` (OFF) or `0x01` (ON).

### Plug Initialization (Required before toggle)

Before toggling, the app sends these init commands (in order):

| Order | Cmd | Sub | Data | Description |
|-------|-----|-----|------|-------------|
| 1 | `AA` | `EF` | none | Initial handshake |
| 2 | `33` | `B2` | `3C 9C 9D 89 09 40 B0 19` + zeros | Soft version write |
| 3 | `33` | `B5` | `6A A1 BB A7 01 FC` + zeros | Hard/wifi version write |
| 4 | `AA` | `01` | none | Status query (response = current state) |
| 5 | `AA` | `B0` | none | Plug state query A |
| 6 | `AA` | `B0` | `00 01` | Plug state query B |
| 7 | `AA` | `12` | none | Timer count query |
| 8 | `AA` | `13` | none | Timer data query |

### Toggle Plug ON

| Byte | Value | Description |
|------|-------|-------------|
| 0 | `0x33` | Command: switch |
| 1 | `0x01` | Sub-command: toggle |
| 2 | `0x11` | ON |
| 3..18 | `0x00` | Padding |
| 19 | `0x23` | Checksum (or calculated) |

Device responds with NOTIFY: `33 01 00 00 ... [checksum]`

### Toggle Plug OFF

| Byte | Value | Description |
|------|-------|-------------|
| 0 | `0x33` | Command: switch |
| 1 | `0x01` | Sub-command: toggle |
| 2 | `0x10` | OFF |
| 3..18 | `0x00` | Padding |
| 19 | `0x22` | Checksum (or calculated) |

Device responds with NOTIFY: `33 01 00 00 ... [checksum]`

## Usage

From the Raspberry Pi (or any Linux with Bluetooth + bleak):

```python
from h5080_controller import H5080Controller
import asyncio

async def main():
    ctrl = H5080Controller()
    await ctrl.connect()
    await ctrl.handshake()
    await ctrl.initialize()
    
    state = await ctrl.get_state()
    print(f"Plug is {'ON' if state else 'OFF'}")
    
    await ctrl.turn_on()
    await asyncio.sleep(2)
    
    await ctrl.turn_off()

asyncio.run(main())
```

Or from CLI:

```bash
# Turn on
python3 h5080_controller.py on

# Turn off
python3 h5080_controller.py off
```

## Discovery Story

1. **BTSnoop capture**: Captured app-to-plug BLE traffic via Android bugreport
   (Settings → Developer Options → Bug report → Interactive report)
2. **APK decompilation**: Pulled `com.govee.home` APK + `split_pact_h5080.apk`
   from phone, decompiled with `jadx`
3. **Key extraction**: Found static AES key in resource strings
   (`KEY_COMM = "MakingLifeSmarte"`)
4. **Protocol analysis**: Multiple captures across reboots revealed session
   handshake and encrypted 20-byte frame structure
5. **Working controller**: Verified ON/OFF toggle from Raspberry Pi via `bleak`

## Tools

| Script | Purpose |
|--------|---------|
| `h5080_controller.py` | Working BLE controller (handshake + init + toggle) |
| `scripts/govee_ble_protocol.py` | Crypto library and key definitions |
| `scripts/govee_controller.py` | Clean OOP implementation |
| `scripts/parse_btsnoop.py` | BTSnoop → ATT write extraction |
| `scripts/analyze_btsnoop.py` | Timing/pattern analysis of captures |
| `scripts/extract_payloads.py` | Save captured payloads to file |

## Archived Approaches

See `openspec/changes/archive/` for failed attempts:
- `h5080-ble-protocol` — Guessed V2 crypto (wrong)
- `h5080-ble-alternative` — Hybrid cloud approach (rejected)