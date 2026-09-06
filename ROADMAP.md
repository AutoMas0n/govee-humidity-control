# Govee Humidity Control — Roadmap

**Project:** `/home/pi/Github/govee-humidity-control/` on `pi@192.168.2.21`
**Goal:** Reliable local-only BLE control for H5179 humidity sensor and H5080 smart plug.

---

## Phase 1: Python BLE (tonight)

Replace the cloud API `main.py` with a local BLE version using `bleak`. No internet, no API key, no failure modes. Runs on the existing `myscript.service`.

| Deliverable | Target |
|-------------|--------|
| Write BLE scanner for H5179 | Read humidity from BLE advertisements |
| Write BLE controller for H5080 | Turn plug on/off via GATT |
| Replace `main.py` | Deploy and test |
| Clean up obsolete files | Remove api_key.secret, request_logs.log, Cargo.toml, etc |

---

## Phase 2: Rust Standalone (this weekend)

A proper Rust crate — **`govee-ble`** — matching octx conventions (clap, thiserror, anyhow, LTO, `strip = "symbols"`, no `unwrap()`). Sits alongside the working Python version; initially a CLI tool that mirrors what the Python script does.

| Deliverable | Target |
|-------------|--------|
| `govee-ble` crate with lib.rs + bin | BLE scan + control via btleplug |
| 1:1 CLI for read/write/daemon | `govee-ble read`, `govee-ble on`, `govee-ble off`, `govee-ble daemon` |
| Replace Python + systemd service | Deploy Rust binary, keep same `myscript.service` |

**Note:** Phase 2 is the boilerplate for a future `octx/arms/govee` — the crate structure and device protocol will be the same, just wired into the octx CLI instead.

---

## Devices & BLE Protocol (ref)

### H5179 (Thermo-Hygrometer) — advertisement scan

```
MAC:   E3:32:81:12:40:A4
Mfr:   0xEC88
Data:
  [0]  reserved
  [1]  temp_int = signed, deg C * 10 + offset 100
  [2]  temp_dec = 0-9 (tenths)
  [3]  humidity  = 0-100 (%)
  [4]  battery   = 0-100 (%)
Temp C = (data[1] - 100) + data[2] / 10
```

No connection needed. The H5179 broadcasts every ~2-3 seconds.

### H5080 (Smart Plug) — GATT control

```
MAC:            D4:AD:FC:41:E1:DD
Service UUID:   0000ec88-0000-1000-8000-00805f9b34fb
Write Char:     00010203-0405-0607-0809-0a0b0c0d1912
Power ON  hex:  33 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 35
Power OFF hex:  33 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 34
```

Connection + GATT write required. Takes ~1-2 seconds to connect, write, disconnect.

---

## File Cleanup (after Python deploy)

| File | Why delete |
|------|-----------|
| `api_key.secret` | No cloud API needed |
| `request_logs.log` | 75KB of error history, no longer relevant |
| `Cargo.toml` + `Cargo.lock` | Old abandoned Rust attempt |
| `openssl-1.1.1.tar.gz` | Source tarball sitting in repo (2.6MB) |
| `nohup.out` | Old process artifact |
| `config.toml` | Cloud config no longer used |
| `require.py` | Deprecated |

---

## History

- **2026-09-05:** Pi OS upgraded Buster → Bullseye → Bookworm (kernel 5.10 → 6.1, Python 3.7 → 3.11)
- **2026-09-05:** BLE confirmed working — BD address fixed, `bluetoothctl scan` finds all devices
- **2026-09-05:** SD card backup done
- **2026-09-06:** This roadmap written