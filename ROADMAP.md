# Govee Humidity Control — Complete Plan

**Repo:** `github.com/AutoMas0n/govee-humidity-control`
**Target:** `pi@192.168.2.21` (Raspberry Pi 4, Debian 12 Bookworm, kernel 6.1.21)
**Goal:** Reliable local-only BLE control for H5179 humidity sensor and H5080 smart plug. Zero cloud dependency. Python first, then Rust.

---

## Table of Contents

1. [System Context](#1-system-context)
2. [Current State](#2-current-state)
3. [Phase 0: Prerequisites](#3-phase-0-prerequisites)
4. [Phase 1: Python BLE Script](#4-phase-1-python-ble-script)
5. [Phase 2: Clean Up Repo](#5-phase-2-clean-up-repo)
6. [Phase 3: Rust Rewrite](#6-phase-3-rust-rewrite)
7. [Phase 4: Deploy & Cutover](#7-phase-4-deploy--cutover)
8. [Appendix: BLE Protocol Reference](#8-appendix-ble-protocol-reference)

---

## 1. System Context

| Property | Value |
|----------|-------|
| Host | `raspberrypi` |
| IP | `192.168.2.21` |
| OS | Debian 12 (Bookworm) |
| Kernel | `6.1.21-v7l+` |
| Python | 3.11.2 |
| Architecture | armv7l (32-bit) |
| RAM | 2 GB |
| Storage | 30 GB (SD card, 2.6G used) |
| SSH user | `pi` |
| SSH key (optional) | Pi has `~/.ssh/id_ed25519.pub` for GitHub, not yet for SSH auth |
| SSH port | 22 (also 443) |
| WiFi | Disabled (soft-blocked rfkill), wired ethernet only |
| Bluetooth | CYW43455 UART — working with kernel 6.1 |
| systemd service | `myscript.service` runs `/home/pi/Github/govee-humidity-control/main.py` |

### What's Running

```bash
# myscript.service — the govee humidity controller
systemctl status myscript.service

# Current main.py — STILL THE OLD CLOUD API VERSION (needs replacement)
cat /home/pi/Github/govee-humidity-control/main.py

# WireGuard VPN
sudo wg show

# noip2 dynamic DNS
ps aux | grep noip2
```

### Devices in Range (BLE scanned and confirmed)

| Device | MAC | Type | Protocol |
|--------|-----|------|----------|
| Govee H5179 | `E3:32:81:12:40:A4` | Thermo-hygrometer | BLE advertisement (no pairing) |
| Govee H5080 | `D4:AD:FC:41:E1:DD` | Smart plug | BLE GATT (connect + write) |

---

## 2. Current State

### ✅ Already Done

- [x] OS upgraded: Buster → Bullseye → Bookworm
- [x] Kernel upgraded: 5.10.103 → 6.1.21
- [x] Python upgraded: 3.7 → 3.11
- [x] EEPROM updated (bootloader 2026)
- [x] SD card backup (block-level 32GB image, on laptop)
- [x] BLE confirmed working — `bluetoothctl scan` finds H5179, H5080, other devices
- [x] Git remote set to `git@github.com:AutoMas0n/govee-humidity-control.git`
- [x] GitHub deploy key added (on Pi: `~/.ssh/id_ed25519.pub` as GitHub deploy key)
- [x] Git identity configured: `AutoMas0n <automason@users.noreply.github.com>`
- [x] Initial commit pushed preserving original history
- [x] This ROADMAP.md written

### ❌ Still To Do

- [ ] Write and deploy Python BLE script (`main.py` replacement)
- [ ] Install `bleak` Python package
- [ ] Clean up old artifacts (Cargo.toml, config.toml, setup.sh, etc.)
- [ ] Update README.md for BLE-only architecture
- [ ] Write Rust crate (`govee-ble`)
- [ ] Compile & deploy Rust binary
- [ ] Cut over systemd service to Rust binary

---

## 3. Phase 0: Prerequisites

### 3.1 SSH Access

```bash
ssh pi@192.168.2.21
```

If using SSH key (recommended for agent automation), add the agent's public key:
```bash
# On the Pi:
echo '<agent-public-key>' >> ~/.ssh/authorized_keys
```

### 3.2 Remote Git Access

The Pi has a GitHub SSH deploy key already set up. Clone/push works:
```bash
cd /home/pi/Github/govee-humidity-control
git pull origin main
git push origin main
```

### 3.3 Verify System Health

```bash
# Check Bluetooth is working
hciconfig hci0 | grep 'BD Address'
# Expected: DC:A6:32:02:A0:24 (real, not AA:AA:AA:AA:AA:AA)

# BLE scan works
bluetoothctl scan on   # or use hcitool lescan

# Python version
python3 --version      # Expected: 3.11.2
```

### 3.4 Verify BLE Devices Are in Range

```bash
timeout 10 bluetoothctl scan on 2>&1 | grep -E 'H5179|H5080'
# Expected:
#   Device E3:32:81:12:40:A4 Govee_H5179_40A4
#   Device D4:AD:FC:41:E1:DD ihoment_H5080_E1DD
```

---

## 4. Phase 1: Python BLE Script

### 4.1 Install bleak

```bash
pip3 install bleak
```

### 4.2 Write main.py

Replace the existing cloud API `main.py` with a BLE-local version reading H5179 advertisements and controlling H5080 via GATT.

**Architecture:**

```
┌─────────────────────────────────────────┐
│  main.py (Python + bleak)              │
│                                         │
│  every 15 min:                          │
│    1. scan BLE ads for H5179            │
│    2. parse humidity from mfr data      │
│    3. if humidity > 45: connect to      │
│       H5080 → write power ON            │
│    4. if humidity <= 45: connect to     │
│       H5080 → write power OFF           │
│    5. only write if state changed       │
│                                         │
│  error handling:                        │
│    - BLE scan timeout → retry next cycle│
│    - GATT connection failure → log+retry│
│    - exponential backoff 5min→60min     │
└─────────────────────────────────────────┘
```

**Key details:**
- Use `BleakScanner` to capture H5179 advertisement data (manufacturer data `0xEC88`)
- Temperature formula: `(data[1] - 100) + data[2] / 10`
- Humidity: `data[3]` (integer 0-100)
- H5080 connection: `BleakClient(H5080_MAC)`, write to char `00010203-0405-0607-0809-0a0b0c0d1912`
- Power ON hex: `3301010000000000000000000000000000000035`
- Power OFF hex: `3301000000000000000000000000000000000034`
- Track previous on/off state; only write when different

**Test the H5179 scanner first:**
```python
# quick_test.py — one-shot read of H5179
import asyncio
from bleak import BleakScanner

H5179_MAC = "E3:32:81:12:40:A4"

async def main():
    device = await BleakScanner.find_device_by_address(H5179_MAC, timeout=10)
    if device:
        print(f"Found: {device}")
        # Need to catch the advertisement data...

asyncio.run(main())
```

**Test the H5080 controller after scanner works:**
```python
# quick_test_plug.py — one-shot toggle
import asyncio
from bleak import BleakClient

H5080_MAC = "D4:AD:FC:41:E1:DD"
CONTROL_CHAR = "00010203-0405-0607-0809-0a0b0c0d1912"
POWER_ON = bytes.fromhex("3301010000000000000000000000000000000035")

async def main():
    async with BleakClient(H5080_MAC) as client:
        await client.write_gatt_char(CONTROL_CHAR, POWER_ON)
        print("Plug turned ON")

asyncio.run(main())
```

### 4.3 Update requirements.txt

```txt
bleak
```

Remove `requests` — no longer needed.

### 4.4 Test

```bash
# Stop the old cloud service
sudo systemctl stop myscript.service

# Run the new BLE script manually
python3 /home/pi/Github/govee-humidity-control/main.py

# Verify in logs that it reads humidity and controls the plug
# Ctrl+C to stop, then restart the old service if needed:
sudo systemctl start myscript.service
```

---

## 5. Phase 2: Clean Up Repo

After Python BLE script is working, remove obsolete files and update docs.

### 5.1 Files to Delete

```bash
cd /home/pi/Github/govee-humidity-control
git rm api_key.secret      # No cloud API
git rm request_logs.log     # Old error logs
git rm Cargo.toml           # Abandoned Rust attempt (will be replaced in Phase 3)
git rm config.toml          # Old cloud config
git rm setup.sh             # Deprecated
git rm require.py           # Deprecated
git rm nohup.out            # Old process artifact
rm openssl-1.1.1.tar.gz     # Untracked, just delete
```

### 5.2 Update README.md

Replace the cross-compile Rust instructions with:
- Project overview: BLE-local humidity control
- Requirements: Python 3.9+, bleak
- Setup: `pip install bleak`
- Usage: runs as systemd service `myscript.service`
- Device info: MACs, BLE protocol reference
- Link to ROADMAP.md for future Rust phase

### 5.3 Update .gitignore (if needed)

Already handles: `*.secret`, `*.log`, `nohup.out`, `openssl-*.tar.gz`, `Cargo.lock`

### 5.4 Commit & Push

```bash
git add -A
git commit -m 'Clean up obsolete files, update README for BLE-local architecture'
git push origin main
```

---

## 6. Phase 3: Rust Rewrite

### 6.1 Project Structure

```
govee-ble/
├── Cargo.toml
├── src/
│   ├── main.rs          # CLI entrypoint (thin)
│   └── lib.rs           # library: scan, control, protocol parsing
└── tests/
    └── integration.rs
```

**`Cargo.toml`:**

```toml
[package]
name = "govee-ble"
version = "0.1.0"
edition = "2021"

[dependencies]
btleplug = "0.11"          # BLE library (BlueZ backend)
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
thiserror = "2"
anyhow = "1"
hex = "0.4"
chrono = "0.4"
log = "0.4"
env_logger = "0.11"

[profile.release]
opt-level = "z"
lto = true
strip = "symbols"
codegen-units = 1
panic = "abort"
```

**CLI interface (mirrors `octx` conventions):**

```
govee-ble read              # Read H5179 humidity (one-shot)
govee-ble on                # Turn H5080 plug ON
govee-ble off               # Turn H5080 plug OFF
govee-ble status            # Show current plug state
govee-ble daemon            # Run continuous loop (same logic as main.py)
govee-ble --help            # Self-documenting
```

### 6.2 Library (`lib.rs`)

```
pub mod ble;

// H5179 struct — parse advertisement data
pub struct H5179Reading {
    pub temperature_c: f32,
    pub humidity: u8,
    pub battery: u8,
}

impl H5179Reading {
    pub fn from_manufacturer_data(data: &[u8]) -> Option<Self>;
}

// H5080 struct — control the plug
pub struct H5080 {
    pub mac: String,
}

impl H5080 {
    pub async fn power_on(&self) -> Result<()>;
    pub async fn power_off(&self) -> Result<()>;
    pub async fn state(&self) -> Result<OnOff>;
}
```

### 6.3 Key Differences from Python

| Aspect | Python | Rust |
|--------|--------|------|
| BLE lib | `bleak` | `btleplug` |
| Async runtime | asyncio | tokio |
| Error handling | try/except | thiserror + anyhow |
| CLI parsing | argparse | clap (derive) |
| Logging | logging | env_logger |
| State tracking | global var | struct field |
| Binary size | ~40MB (Python + libs) | ~8MB (stripped LTO) |

### 6.4 Building

```bash
# On the Pi (native compile):
cd govee-ble
cargo build --release

# Binary at: target/release/govee-ble
# Strip further (already done by profile):
ls -lh target/release/govee-ble
```

First build will take ~20-30 minutes (compiling btleplug and its BlueZ DBus bindings). Subsequent builds are ~10-20 seconds for code changes.

### 6.5 Testing

```bash
# Quick sanity check
govee-ble read       # should output temp/humidity
govee-ble status     # should show current plug state

# Toggle test (manually verify plug clicks)
govee-ble off
sleep 3
govee-ble on
```

### 6.6 Future: octx Arm

Once `govee-ble` is stable, the crate can be ported to an `octx` arm:

```
octx/arms/govee/     ← just wrap the govee-ble lib functions as octx subcommands
    Cargo.toml       ← depends on govee-ble (or inline the code)
    src/main.rs      ← parse args, call govee-ble::*
```

The protocol parsing and BLE logic in `lib.rs` stays identical; only the CLI scaffolding changes.

---

## 7. Phase 4: Deploy & Cutover

### 7.1 Deploy Rust Binary

```bash
# Copy binary to system location
sudo cp target/release/govee-ble /usr/local/bin/

# Test daemon mode (dry run, won't persist if it crashes)
sudo -u pi /usr/local/bin/govee-ble daemon
```

### 7.2 Update Systemd Service

The existing `myscript.service` runs Python. To cut over to Rust:

```bash
sudo sed -i 's|/usr/bin/python3 /home/pi/Github/govee-humidity-control/main.py|/usr/local/bin/govee-ble daemon|' /etc/systemd/system/myscript.service
sudo systemctl daemon-reload
sudo systemctl restart myscript.service

# Verify
systemctl status myscript.service
journalctl -u myscript.service -n 20 --no-pager
```

### 7.3 Rollback

If Rust has issues, swap back:

```bash
sudo sed -i 's|/usr/local/bin/govee-ble daemon|/usr/bin/python3 /home/pi/Github/govee-humidity-control/main.py|' /etc/systemd/system/myscript.service
sudo systemctl daemon-reload
sudo systemctl restart myscript.service
```

Both Python and Rust versions can coexist; just change the systemd `ExecStart` line.

### 7.4 Optional: Remove Python

Once Rust is stable for a week:
```bash
pip3 uninstall bleak
```

Or keep it — it's 5MB and doesn't hurt anything.

---

## 8. Appendix: BLE Protocol Reference

### 8.1 H5179 (Thermo-Hygrometer) — Advertisement Data

The H5179 broadcasts temperature and humidity in **unencrypted BLE advertisement packets** every ~2-3 seconds. No connection or pairing needed.

| Field | Bytes | Format | Example |
|-------|-------|--------|---------|
| Manufacturer ID | 0-1 | `0xEC88` (little-endian) | |
| Reserved | 2 | unknown | |
| Temp integer | 3 | signed, `°C × 10 + 100` offset | `218` → `11.8°C` |
| Temp decimal | 4 | 0-9 (tenths) | `5` → `0.5°C` |
| Humidity | 5 | 0-100 (%) | `62` → `62%` |
| Battery | 6 | 0-100 (%) | `85` → `85%` |

**Temperature formula:** `°C = (byte[3] - 100) + byte[4] / 10`

**Example:** Bytes `[0x88, 0xEC, 0x00, 0xDA, 0x05, 0x3E, 0x55]`
- Temp: `(218 - 100) + 5/10 = 118.5°C` → note: 218 seems high, verify offset
- Actual formula from community: `°C = (byte[3] - 100) + byte[4] / 10`

### 8.2 H5080 (Smart Plug) — GATT Control

| Property | Value |
|----------|-------|
| Service UUID | `0000ec88-0000-1000-8000-00805f9b34fb` |
| Write char UUID | `00010203-0405-0607-0809-0a0b0c0d1912` |
| Power ON | `33 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 35` |
| Power OFF | `33 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 34` |

**Protocol:** Connect to the device, write the command to the characteristic, disconnect. Takes ~1-2 seconds. Write type is typically "write with response" — confirm the plug acknowledges the command.

Commands documented in open-source community (Homebridge Govee plugin, `govee-ble` Python library, OpenHAB bindings).

### 8.3 BLE on Pi 4 Notes

- BlueZ 5.50 is the Bluetooth stack
- The CYW43455 chip communicates over UART (`/dev/ttyAMA0`)
- BLE was broken on kernel 5.10 (fake BD address, timeouts)
- Upgrading to kernel 6.1.21 fixed it — firmware loads properly, real BD address, working LE
- `hcitool lescan` may still report errors on kernel 6.1 — use `bluetoothctl scan on` instead
- `btleplug` (Rust) and `bleak` (Python) both talk to BlueZ over DBus — same reliability

---

## Device MAC Addresses

```
H5179 (Humidity Sensor):  E3:32:81:12:40:A4
H5080 (Smart Plug #1):    D4:AD:FC:41:E1:DD
```

---

## History

| Date | Event |
|------|-------|
| 2026-09-05 | Pi OS upgraded Buster → Bullseye → Bookworm (kernel 5.10 → 6.1, Python 3.7 → 3.11) |
| 2026-09-05 | BLE confirmed working — real BD address, devices found via bluetoothctl |
| 2026-09-05 | SD card full backup done (32GB block image) |
| 2026-09-05 | Git remote + SSH deploy key set up, initial commit pushed |
| 2026-09-06 | This roadmap written |