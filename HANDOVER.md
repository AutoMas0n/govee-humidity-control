# Govee BLE Control — Handover Document

## Goal
Replace the Govee Home cloud app with a standalone local BLE solution for controlling
Govee H5080 smart plugs and reading H5179 humidity sensors. Zero cloud dependency.
No enshittification. Runs on a Raspberry Pi 4 (Debian 12, armv7l).

## What Works (Proven)

### H5179 Humidity Sensor
- BLE advertisements broadcast temperature/humidity every ~10 seconds
- Works via any BLE scanner — no pairing, no connection needed
- Data in manufacturer-specific advertisement fields
- Python reader in `h5080_controller.py`, Rust reader in `govee-ble/src/main.rs`

### H5080 Smart Plug (V1 Firmware)
- **MAC**: `60:74:F4:BD:4D:E5` (also known as 4DE5)
- **BLE toggle works without secret key**
- Uses the default version data `3c9c9d890940b019` for the `33 B2` command
- 7 init commands all respond (AA EF, 33 B2, 33 B5, AA B0×2, AA 12, AA 13)
- Soft version = `00` (field in AA response)
- Currently unreachable — may need power cycle

### H5080 Smart Plug (V2+ Firmware, E245)
- **MAC**: `D4:AD:FC:42:E2:45`
- **BLE toggle works WITH secret key**: `f6e0730a5be545e3`
- Extracted from btsnoop capture of Govee app traffic
- Secret key was written by the Govee app during initial pairing
- Verified: OFF → ON cycle works from Raspberry Pi
- Dehumidifier plug (user-labelled)

### H5080 Smart Plug (V2+ Firmware, E1DD)
- **MAC**: `D4:AD:FC:41:E1:DD`
- **BLE status query works, toggle does NOT** — secret key unknown
- Same key as E245? `f6e0730a5be545e3` — tested, does NOT toggle (each plug has its own)
- **No capture of E1DD exists**: the 09-16 "E1DD re-pairing" bugreport is actually 14× E245 + 2× 4DE5
- **Next step**: `sudo govee-ble pair --mac D4:AD:FC:41:E1:DD`, short-press the plug button when prompted

## Protocol

Full protocol documentation: see **PROTOCOL.md**

### Summary
- **GATT**: Service `00010203-0405-0607-0809-0a0b0c0d1910`
  - Write char: `...2b11` (handle 0x0011)
  - Notify char: `...2b10` (handle 0x000E or 0x000D depending on device)
- **Crypto**: AES-128-ECB (first 16 bytes) + RC4 (last 4 bytes)
- **Static key**: `b"MakingLifeSmarte"` (16 bytes)
- **Frame format**: 20 bytes: `[cmd sub data(0-16) 0x00-padding XOR-checksum]`
- **Handshake**: E7 01 (request) → E7 01 (response with session key) → E7 02 (confirm)
- **Session key**: 16 bytes from handshake, used for all subsequent frames

### Commands (All 20 bytes, encrypted with session key)
| Command | Hex | Description |
|---------|-----|-------------|
| Status query | `AA 01` | Returns state (0=OFF, 1=ON) |
| Toggle ON | `33 01 11` | Turn plug ON |
| Toggle OFF | `33 01 10` | Turn plug OFF |
| Secret key check | `33 B2 <8B key>` | Check key → `33 B2 00` = ok |
| Secret key read | `AA B1` | → `AA B1 <flag> <8B>`; flag 01 only after button press |
| Sync time | `33 B5 <ts BE×4> 01 <tz_h> <tz_m>` | SyncTime (unix ts + UTC offset) |
| Init handshake | `AA EF` | Device init |
| Plug config | `AA B0` | Read plug config |
| Plug config | `AA B0 00 01` | Plug config variant |
| Timer count | `AA 12` | Number of timers |
| Timer data | `AA 13` | Timer data |
| MFR info | `AA 06` | Manufacturer info |
| MFR info | `AA 07 <type>` | Manufacturer info query |
| Unknown | `AA 21` | Seen in pairing |
| Unknown | `AA 20` | Seen in pairing |
| Unknown | `AA 14` | Seen in pairing |
| Unknown | `AA B3` | Seen in pairing |
| Pair confirm | `AB 01 04` | Pairing confirmation command |

### Init Sequences

**V1 firmware (4DE5)** — no secret key needed:
```
AA EF → 33 B2(3c9c9d890940b019) → 33 B5(6aa1bba701fc) → AA B0 → AA B0 00 01 → AA 12 → AA 13
```

**V2+ firmware (E245, E1DD)** — secret key required:
```
33 B2(<8B secret key>) → 33 B5(6aa4a43a01fc) → AA EF → AA B0 → AA B0 00 01 → AA 12
```

### Full Pairing Sequence (from btsnoop capture)
Used when app pairs with a new (or forgotten) device:
```
1. E7 01/02 handshake
2. AA B1 polled every ~240ms → `00 <random>` until user SHORT-PRESSES plug button → `01 <key>`
3. 33 B2(<8B key>) → 33 B2 00 (check key)
4. AA 06
5. AA 07(03)
6. AA 21
7. AA 20
8. AA 14
9. AA B3
10. AA 07(02)
11. AB 01(04) (pair confirm)
12. Handle 0x0025: WiFi provisioning data (not needed for BLE-only)
```

## Secret Key

### What We Know
- **Key for E245**: `f6e0730a5be545e3` (8 bytes, verified working)
- **Key for 4DE5**: None needed (V1 firmware)
- **Key for E1DD**: Unknown — need to extract
- **The plug owns the key.** The app never generates it. `SecretKeyController`
  (decompiled from `base/classes10.dex` → `~/govee_apk/skc/`) only READS it:
  - `AA B1` response: `AA B1 <flag> <8 bytes>`. `parseValidBytes` requires `flag == 0x01`;
    otherwise it fails and `AbsPairAc4SecretV1` retries after 200 ms.
  - `flag == 0x00` responses carry **random bytes** — this is what the old
    `get-skey` printed and why it looked like a "dynamic challenge".
  - The plug flips to `flag == 0x01` when the user **short-presses its button**
    (app string `plug_single_pair_press_hint`).
  - Key read back on re-pair == key from first pair → persistent per plug.
- `33 B2 <key>` → `33 B2 00` is a CHECK done once per session; a wrong key is silently ignored.
- The app stores keys in `SecretKeyConfig` (HashMap<String, String> keyed by BLE MAC) — irrelevant for us now.
- Verified against 2026-09-16 capture (Session 13, E245): 46× `AA B1 00 …`, then `AA B1 01 f6e0730a5be545e3`.

### Implemented
`govee-ble pair --mac <addr> [--timeout 60]` does exactly the app's flow, prints the key.
Not yet run against real hardware — needs the Pi (btleplug won't build on Termux/Android).

## Architecture

### Raspberry Pi (Target)
- **Host**: `raspberrypi` (192.168.2.21)
- **OS**: Debian 12 Bookworm, armv7l
- **Python**: 3.11.2 with bleak 3.0.2, pycryptodomex
- **Rust**: 1.75+ (via rustup), btleplug 0.11, tokio, aes crate
- **BLE**: CYW43455 UART (built-in), works with kernel 6.1.21

### Code Assets

| File | Description |
|------|-------------|
| `govee-ble/src/main.rs` | Rust binary — single-file, ~400 lines |
| `govee-ble/Cargo.toml` | Rust dependencies |
| `h5080_controller.py` | Python BLE controller (reference implementation) |
| `scripts/govee_ble_protocol.py` | Crypto library and key definitions |
| `scripts/get_skey.py` | (obsolete — ignores the flag byte) |
| `scripts/decode_sessions.py` | Decrypt all writes+notifies per session from a btsnoop, with peer MAC |
| `scripts/parse_btsnoop.py` | Parse btsnoop log to extract ATT writes |
| `scripts/analyze_btsnoop.py` | Session analysis and command extraction |
| `scripts/decrypt_e245.py` | E245-specific btsnoop decryption |
| `scripts/extract_skey.py` | Extract secret key from btsnoop |
| `PROTOCOL.md` | Full protocol documentation |
| `captured_payloads.txt` | Raw payloads from btsnoop captures |

**BTSnoop captures** (on Android device, bugreport zips):
- `bugreport-*-2026-09-15-*` — E245 toggle (8 sessions, all with `f6e0730a5be545e3`)
- `bugreport-*-2026-09-16-*` — E245 re-pairing (14 sessions E245, 2 sessions 4DE5; **no E1DD**). Extracted to `~/btsnoop_0916/`

### Rust Binary (`govee-ble`)
Commands:
| Subcommand | Args | Description |
|------------|------|-------------|
| `on` | `--mac <addr>` `[--skey <hex8>]` | Turn plug ON |
| `off` | `--mac <addr>` `[--skey <hex8>]` | Turn plug OFF |
| `status` | `--mac <addr>` `[--skey <hex8>]` | Query plug state |
| `read` | `--mac <addr>` | Read H5179 sensor |
| `scan` | (none) | List nearby BLE devices (10s scan) |
| `pair` | `--mac <addr>` `[--timeout <sec>]` | App-free pairing: prints plug's secret key after button press |
| `daemon` | `--plug-mac <addr>` `--sensor-mac <addr>` `[--plug-skey <hex8>]` `[--interval <sec>]` `[--threshold <%>]` `[--hc-url <url>]` | Continuous humidity-based control loop |

`pair` implemented (this session), compiles on the Pi only. Untested on hardware.

### Dependencies (ponytail-minimal)
- **Rust**: `btleplug`, `tokio`, `aes`, `log`, `env_logger`, `hex`
- No clap (manual argv parsing), no reqwest (TCP healthcheck), no thiserror/anyhow
- Single source file, ~1.5MB release binary

### Raspberry Pi Paths
- Repo: `~/Github/govee-humidity-control/`
- Binary: `govee-ble/target/release/govee-ble`
- systemd: Not yet set up (planned)

### Android Tools (Motorola g86 power 5G)
- ADB wireless debugging (ports rotate frequently)
- Bugreport via Developer Options → Interactive report
- BTSnoop enabled: `adb shell settings put global bluetooth_hci_snoop_log 1`
- APK decompiled with jadx 1.5.5

## Resolved Questions (2026-09-16 session)

1. ~~Can `33 B2` SET a key?~~ No. It's a check. The plug owns the key; nothing sets it.
2. ~~How does the app generate keys?~~ It doesn't. `SecretKeyController.parseValidBytes` reads
   `AA B1 01 <key>` from the plug. Decompiled to `~/govee_apk/skc/`.
3. ~~Extract keys from phone?~~ Unnecessary — read from the plug with a button press.
4. ~~Factory reset?~~ Not needed. App guide: hold button until LED slowly blinks blue = pairing mode.
5. ~~Does AB 01 04 commit the key?~~ No. It fetches an IoT/cloud credential token. BLE-only ignores it.
6. ~~Why doesn't E1DD toggle?~~ We never had its key; its "capture" was actually E245.

## Open

1. Does `AA B1` unlock on a short press while the plug is in *normal* mode, or must it be in
   pairing mode (long-press → slow blue blink) first? Try normal mode first.
2. Run `pair` on E1DD from the Pi, then verify `on/off --skey <key>`.
3. 4DE5 is unreachable — power cycle it.
4. systemd unit for `daemon` on the Pi.

## Git History

**Branch**: `main`
- `f097dd1` — Clean repo, BLE controller works, Rust rewrite
- `f52d541` — OpenSpec change created
- `70c4ee8` — scan subcommand
- `6f26268` — --skey flag
- `cf0120f` — get-skey subcommand
- `8cfa645` — PROTOCOL.md update with firmware variants + secret keys
- `d809d74` — HANDOVER.md
- Latest: `pair` subcommand + decode_sessions.py + protocol corrections (secret key is plug-owned, read via AA B1 after button press)

## Quick Start

```bash
# Scan for plugs
sudo govee-ble scan

# Control E245 (key known)
sudo govee-ble on --mac D4:AD:FC:42:E2:45 --skey f6e0730a5be545e3
sudo govee-ble off --mac D4:AD:FC:42:E2:45 --skey f6e0730a5be545e3

# Read H5179 sensor
sudo govee-ble read --sensor-mac E3:32:81:12:40:A4

# Pair E1DD without the Govee app (short-press the plug's button when prompted)
sudo govee-ble pair --mac D4:AD:FC:41:E1:DD
# -> prints 8-byte hex key; then:
sudo govee-ble on --mac D4:AD:FC:41:E1:DD --skey <key>

# Daemon mode (humidity-based control loop)
sudo govee-ble daemon --plug-mac D4:AD:FC:42:E2:45 --plug-skey f6e0730a5be545e3 \
  --sensor-mac E3:32:81:12:40:A4 --interval 60 --threshold 60 --hc-url http://your-id.healthchecks.io
```

## Contact
- Repo: `github.com/AutoMas0n/govee-humidity-control`
- Pi SSH: `pi@192.168.2.21`
- Android: Motorola g86 power 5G via ADB wireless (port changes frequently)