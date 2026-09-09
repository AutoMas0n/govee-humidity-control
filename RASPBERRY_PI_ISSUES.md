# Raspberry Pi 4 — Issues & Diagnostics

## Device

- **Model:** Raspberry Pi 4 (BCM2711, armv7l)
- **Kernel:** `5.10.103-v7l+ #1529 SMP Tue Mar 8 12:24:00 GMT 2022`
- **Firmware:** May 9 2023 (hash `30aa0d70`)
- **OS:** Debian Buster (armhf)
- **Hostname:** `raspberrypi`
- **IP:** `192.168.2.21` (static via DHCP)
- **MAC:** `DC:A6:32:02:A0:22`
- **Wi-Fi/BT chip:** CYW43455 (combo SDIO chip)

---

## Issue 1: BLE (Bluetooth Low Energy) Broken

### Symptom

```
$ sudo hcitool lescan
Set scan parameters failed: Input/output error
```

### Diagnosis

| Check | Result | Meaning |
|-------|--------|---------|
| `hciconfig hci0 features` | Shows `<LE support>` | Chip *claims* LE support |
| `hcitool lescan` | `I/O error` | LE controller commands fail at transport level |
| `hcitool scan` (classic) | Empty (no devices found) | Classic BT scan **also returns nothing** — but it doesn't error |
| `hciconfig hci0` | `BD Address: AA:AA:AA:AA:AA:AA` | **Fake address** — chip's OTP-burned unique address not being read |
| `btmgmt info` | Empty output | Management interface has no info |
| `hciuart.service` | `Device setup complete` | UART firmware loads, but chip doesn't fully initialize |
| `dmesg` | `sending frame failed (-49)` | HCI command frames fail on the UART link |
| Classic BT scan earlier | Found `Fosi Audio BT20A` and `GVAUDIO` | Classic BT **did work** at least once — so the chip is alive |

### Root Cause

There are **three likely contributing factors**, listed in order of probability:

### 1. No Wi-Fi Country Set (most likely)

The CYW43455 is a **combo chip** — Wi-Fi and Bluetooth share the same radio. The Wi-Fi interface is `Soft blocked: yes` because no country has been configured:

```
$ sudo rfkill list
0: phy0: Wireless LAN
    Soft blocked: yes
```

When the Wi-Fi side is blocked without a country code, the **radio calibration and coexistence** logic may not initialize properly, and the BLE radio front-end won't enable. The classic BT might limp along (it's more tolerant) but LE scanning (which requires precise radio timing) fails.

**Fix:**
```bash
sudo raspi-config
# → Localisation Options → Wi-Fi Country → GB (or appropriate)
# OR non-interactive:
sudo raspi-config nonint do_wifi_country GB
sudo rfkill unblock all
sudo reboot
```

### 2. Fake BD Address (`AA:AA:AA:AA:AA:AA`)

The `AA:AA:AA:AA:AA:AA` address means the BCM chip's OTP (one-time programmable) memory — which holds the factory-burned unique MAC — isn't being read properly. This is a known firmware issue on older Pi 4 firmware/kernel combos.

The `btuart` script logs:
```
Cannot open directory '/etc/firmware': No such file or directory
Patch not found, continue anyway
```

This is **normal** on recent firmware (the patch directory moved), but it means the firmware didn't load a BD address override. The chip should use its OTP address, but doesn't.

**Fix:** Upgrade to the latest firmware and kernel via `rpi-eeprom-update` and `apt-get dist-upgrade` (see Issue 2 below).

### 3. Old Kernel (5.10.103)

The kernel is **5.10.103 from March 2022** — over 4 years old. The BCM4345C0 BLE driver (`hci_uart` / `btbcm`) has had numerous fixes in later kernels for UART timing, LE scanning, and firmware loading. Upgrading to kernel 6.x would likely fix this.

### 4. UART Clock Drift (less likely)

The Pi 4 uses UART for Bluetooth HCI. If the VPU core clock frequency changes (due to power saving or thermal throttling), the UART baud rate drifts and the BCM chip can't maintain the link. This causes `command tx timeout` errors.

---

## Issue 2: OS is Severely Out of Date

### Symptom

```
$ sudo apt-get update
Err:6 http://raspbian.raspberrypi.org/raspbian buster Release
  404  Not Found
```

### Diagnosis

| Check | Result |
|-------|--------|
| OS version | Debian Buster (released 2019, EOL August 2022) |
| Kernel | 5.10.103 (March 2022) |
| Package repos | Most are 404 — the original Debian Buster repos have been archived |
| Security updates | None since 2022 |

### Impact

- **Cannot install new packages** (nmap, arp-scan, etc. all fail with 404)
- **No security patches** for 4+ years
- **No kernel updates** — the BLE fix likely requires a newer kernel
- **No Bluetooth firmware updates**

### Fix

Upgrade to **Debian Bookworm** (current) or at least **Bullseye** (oldstable):

```bash
# First, update the repos to point to archive.debian.org for Buster
sudo sed -i 's/raspbian.raspberrypi.org/archive.raspbian.org/g' /etc/apt/sources.list
sudo sed -i 's/archive.raspbian.org/archive.raspbian.org\/raspbian/g' /etc/apt/sources.list
sudo apt-get update
sudo apt-get upgrade -y

# Then dist-upgrade to Bullseye, then Bookworm:
# This is a major operation — see https://raspberrypi.com/documentation/computers/os.html
```

**⚠️ This is a risky operation** on a machine with no backup. The safest path is to image the SD card before attempting.

---

## Issue 3: Wi-Fi Soft-Blocked at Boot

### Symptom

Every login shows:
```
Wi-Fi is currently blocked by rfkill.
Use raspi-config to set the country before use.
```

### Diagnosis

The rfkill saved state persists the soft block across reboots:
```
$ cat /var/lib/systemd/rfkill/platform-fe300000.mmcnr:wlan
1   # ← soft blocked
```

The saved state for Bluetooth is `0` (not blocked), but the Wi-Fi block may still affect the combo chip's radio.

### Fix

Set the Wi-Fi country (same as Issue 1, Fix 1). This clears the soft block permanently.

---

## Issue 4: The Govee Script Uses Cloud API (Not This Issue, But Related)

Documented in `REFACTOR_PLAN.md`. The govee script (`myscript.service`) uses the Govee Cloud API to read humidity and control the plug. Both fail when the Pi has no internet (which is frequent). The fix is to switch to BLE local control — but that requires BLE to work (Issue 1).

---

## Fix Priority

| Priority | Fix | Effort | Risk | Impact |
|----------|-----|--------|------|--------|
| **P0** | `sudo raspi-config` → set Wi-Fi country → `rfkill unblock all` → reboot | 2 min | None | May fix BLE entirely |
| **P1** | `rpi-eeprom-update` to latest firmware | 5 min | Low | May fix BLE/BD address |
| **P2** | Kernel upgrade (dist-upgrade to Bullseye+ or Bookworm) | 1-2 hours | **High** without backup | Will likely fix BLE, but risky |
| **P3** | Replace BLE with USB dongle (~$5) | 5 min + $5 | None | Guaranteed fix for BLE, bypasses all chip issues |
| **P4** | SD card backup before any of the above | 30 min | None | Enables safe upgrades |

---

## Quick Test Checklist (after any fix)

```bash
# 1. Check BD address is real
sudo hciconfig hci0 | grep "BD Address"
# Expected: DC:A6:32:XX:XX:XX (not AA:AA:AA:AA:AA:AA)

# 2. BLE scan
sudo timeout 10 hcitool lescan --passive
# Expected: list of BLE devices in range

# 3. Classic BT scan
sudo timeout 6 hcitool scan
# Expected: list of classic BT devices in range

# 4. Check rfkill
rfkill list
# Expected: no "Soft blocked: yes" for wifi
```

---

*Documented: 2026-09-05*
*After investigation of fresh boot (kernel 5.10.103-v7l+, firmware May 2023)*