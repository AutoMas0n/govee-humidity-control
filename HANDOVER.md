# Govee BLE Control — Handover Document

## Goal
Replace the Govee Home cloud app with a standalone local BLE solution for controlling
Govee H5080 smart plugs and reading H5179 humidity sensors. Zero cloud dependency.
No enshittification. Runs on a Raspberry Pi 4 (Debian 12, armv7l).

**Status (2026-09-17, end of session):** protocol fully understood and
**verified against live hardware**. All keys captured and toggle-tested.
Dehumidifier plug **identified by the owner at the socket** (off/on click
confirmations). Tool now has named plugs (`--name` / `names`). Repo is synced
(local, Pi, and GitHub all at `88e49a7`). **Remaining task: take it live on
the Pi as the dehumidifier controller** — see NEXT ACTION.

---

## TL;DR — The Secret Key Mystery Is Solved

Newer H5080 firmware (E245, E1DD) requires an 8-byte per-plug "secret key"
before `33 01` toggles work. Previous sessions assumed the Govee app generated
and wrote this key, and hunted for its algorithm / phone storage. **Wrong.**

**The plug owns the key. The app only reads it, and only after a physical
button press.** Decompiled from `SecretKeyController` (`base/classes10.dex`)
and verified byte-for-byte against the 2026-09-16 btsnoop:

```
AA B1            -> AA B1 00 <8 random bytes>    "not confirmed" (app polls every ~240 ms)
  ...user SHORT-PRESSES the plug's button...
AA B1            -> AA B1 01 <8-byte KEY>        real key, persistent per plug
33 B2 <KEY>      -> 33 B2 00                     per-session check (never a SET)
33 01 11 / 10    -> 33 01 00                     toggle now works
```

- Session 13 of the 09-16 capture (E245): 46× `AA B1 00 …`, then
  `AA B1 01 f6e0730a5be545e3` at 12.25 s — the same key E245 had before, so
  the key survives re-pairing. No generation, no reset, no cloud.
- The old `get-skey` reported the random bytes from `AA B1 00` because it
  ignored the flag byte. That's the whole "dynamic challenge" red herring.
- App UI string during the confirm phase: `plug_single_pair_press_hint` =
  *"The device's power indicator is slowly flashing blue. Please short press its
  switch button to pair."* (`plugv1_guide_des_v1` — *"Press and hold the
  button until the indicator light slowly blinks blue"* — is the app's
generic guide; for a *bound* plug the actual re-entry trigger is the cloud
unbind below, not the button hold.)
- **Pairing mode is a prerequisite (plug-owner report + APK + captures).** The
  plug must already be in pairing mode (LED slowly blinking blue) before the
  short-press confirms `AA B1 01`.
- **How pairing mode actually starts (corrected, two cases):**
  - *Fresh plug (never paired):* no WiFi configured yet, starts in pairing
    mode on its own (flashing blue), BLE-only. WiFi creds are written to it
    over BLE AFTER key exchange (WifiChooseAc / handle h0025) — WiFi is never
    used to pair.
  - *Bound plug:* already provisioned (WiFi MAC `AA 14`, IoT token `AB 01 04`)
    and holds a cloud IoT session over WiFi. The Govee app's "forget device"
    is a **cloud-account unbind** (`deleteDevice` →
    `netService4Base.deleteDevice(Request4DeleteDevice)`, response
    `UnBindDeviceFeastInfo`); the plug learns over its *existing* WiFi IoT
    link that it is unbound and drops back into pairing mode on its own.
    That's why "forget → add new → plug flashes": the flash is the plug
    reacting to the cloud revocation, not a BLE pairing request. No BLE
    command is involved — pairing itself is always BLE-only.
- **There is no BLE "enter pairing mode" command.** The H5080 controller set
  (Switch/Timer/SyncTime/Version/Spec/Heart + `SecretKeyController(V1)`
  read/check) has none, and no capture shows any frame before the `AA B1`
  polls — only the handshake + `AA 01`. So btsnoop (Bluetooth-only) can't see
  the trigger because the trigger is the cloud unbind over WiFi.
- **Consequence for `govee-ble pair`:** it cannot make a bound plug pairable —
  that action is cloud-side. It can only detect the state: token `00` = plug
  IS in pairing mode (awaiting press), `-` = not in pairing mode (or out of
  range). Re-pairing a bound plug locally therefore isn't possible; you can
  either read the key during the app's initial pairing (as our captures did)
  or use a fresh/unbound plug.
- `AB 01 04` + `AA 06/07/14/20/21/B3` after the key check are firmware/hw
  version, WiFi MAC and an IoT credential token — cloud provisioning, **not
  needed** for BLE control.
- `33 B5` is **SyncTime** (`[unix_ts BE×4][01][tz_hours i8][tz_min]`), not a
  hardware version write. `FC` = UTC-4.

---

## NEXT ACTION — ✅ DONE: GO LIVE ON THE PI (dehumidifier control)

**Completed 2026-09-18:** Pi service cut over from the cloud `main.py` (now
stopped/disabled as `myscript.service`) to the local Rust BLE daemon
(`humidity-daemon.service`). Threshold **45%**, interval **900 s** (matching the
old cloud script's behaviour), status served on a local port (`--status-port
8843`) instead of an external healthcheck — zero internet. Cloud Python files
(`main.py`, `setup.sh`, `require.py`, `requirements.txt`) stay in the repo as
git-history reference; the production path is `govee-ble` only.

Status page: `curl http://192.168.2.21:8843/` →
`temp=…C / humidity=…% / battery=…% / plug=ON|OFF / threshold / interval / ts`.
Port **8843** = the H5080 manufacturer ID (mfg id `0x8843`); chosen to avoid
the common 8080 web port.

### What was done

1. `govee-ble/src/main.rs`: replaced the healthchecks.io `ping_hc` + `--hc-url`
   with a tiny local HTTP status server (`tokio::net::TcpListener`, status
   shared via `tokio::sync::watch::channel`). Daemon now takes
   `--status-port N` (0 = off).
2. `govee-ble/src/main.rs` — **fixed H5179 read (was never live-verified).**
   Lookup key is `0x8801` (not `0xEC88` — that's the GATT service UUID, a
   red herring). Payload `ec 00 01 01 <temp i16 LE /100> <hum u16 LE /100>
   <batt>`; verified on the Pi: `23.4C 51% 86%`. Cross-checked against
   `sensor.goveetemp_bt_hci`'s H5179 decoder.
3. `govee-ble/humidity-daemon.service`: targets dehumidifier E1DD
   (`D4:AD:FC:41:E1:DD` / `a69f370afd964e0d`, sensor `E3:32:81:12:40:A4`),
   `--interval 900`, **hysteresis band `--hi 55 --lo 45`** (see
   `openspec/changes/humidity-hysteresis-band/` — data-backed: single-45
   chattered 6,042 cycles/yr, band 55/45 runs 318/yr at 1/10 the runtime),
   `--status-port 8843`, plus `Environment=RUST_LOG=info` (env_logger
   silences everything below `error` by default — without it journald shows
   nothing).
4. **BLE session reuse (2026-09-20)** — the daemon leaked one BlueZ
   D-Bus socket per poll cycle (`adapter()` → `BluetoothSession::new()` per
   call; bluez-async/dbus-tokio never close the connection): 12 fds at start
   → 46 after 8.7 h, then reads failed progressively (`H5179 not found`,
   status page showed `error=…`). Fixed by threading one adapter through the
   whole daemon lifetime (`fix-daemon-ble-session-leak`): `daemon_loop`
   creates one session, reuses it for sensor reads and plug toggles; CLI
   one-shots keep their own (process exit closes them).
5. Pi: rebuilt release binary, `systemctl stop/disable myscript.service`
   (the old cloud `main.py`), `cp` unit to `/etc/systemd/system/`,
   `systemctl enable --now humidity-daemon`. Verified:
   `journalctl -u humidity-daemon` shows sensor→need ON/OFF cycles, status
   page live at `http://192.168.2.21:8843/`, plug confirmed ON at the socket
   (`status --name dehumidifier` → ON).

---

## (Reference) Earlier NEXT ACTION — pairing E1DD

Retained for completeness; **already completed** (key `a69f370afd964e0d` was
captured & toggle-verified). `govee-ble pair` on a plug requires the plug to
be in pairing mode first.

---

## Devices

| Plug | MAC | Firmware | Secret key | State |
|------|-----|----------|-----------|-------|
| 4DE5 | `60:74:F4:BD:4D:E5` | V1 | none needed (`33 B2 3c9c9d890940b019` default works) | **human-verified toggle 2026-09-17** — clicked ON on command; next to Pi |
| E245 | `D4:AD:FC:42:E2:45` | V2+ | `f6e0730a5be545e3` | unbound 09-17; **unplugged/out of range as of 09-17 test** |
| E1DD | `D4:AD:FC:41:E1:DD` | V2+ | `a69f370afd964e0d` | **==================== DEHUMIDIFIER PLUG ====================** — confirmed by owner at the plug 09-17 (off/on click); toggle-verified |
| H5179 | `E3:32:81:12:40:A4` | sensor | n/a | advertisements, mfg id `0x8801` |

**Naming (confirmed 2026-09-17 by click-test at each plug):**
- `dehumidifier` = `D4:AD:FC:41:E1:DD` (advertises `ihoment_H5080_E1DD`)
- the `ihoment_H5080_4DE5` plug (`60:74:F4:BD:4D:E5`) sits next to the Pi
- the third (`ihoment_H5080_E245`, `D4:AD:FC:42:E2:45`) is currently unplugged
The BLE advertisement suffix is the authoritative identity. The daemon targets
`dehumidifier` = `D4:AD:FC:41:E1:DD`.

All plugs advertise as `ihoment_H5080_XXXX`, manufacturer id `0x8843`.

**Important (capture chronology):** there are **two** 09-16 bugreports. The
older one (`btsnoop_0916`, 10:46, 16 sessions) contains **zero E1DD sessions**
(only E245 + 4DE5) — it predates the E1DD re-pair. The newer one
(`btsnoop_new`, 20:26, **36 sessions**) contains E1DD: sessions 22–23 connect
to `D4:AD:FC:41:E1:DD` and send `33 B2 a69f370afd964e0d` (accepted → `33 B2 00`).
That is where the E1DD key in the table above came from.

---

## Protocol (summary — full detail in PROTOCOL.md)

- **GATT**: service `00010203-0405-0607-0809-0a0b0c0d1910`, write `…2b11`
  (handle 0x0011), notify `…2b10` (handle 0x000E / 0x000D)
- **Frame**: 20 bytes `[cmd sub data… 0x00-pad XOR-checksum]`
- **Crypto**: AES-128-ECB on bytes 0..16 + RC4 on bytes 16..20
- **Static key**: `b"MakingLifeSmarte"` (from APK resource strings)
- **Handshake** (static key): `E7 01 <16 rand>` → notify `E7 01 <16B session key>` → `E7 02 <16 rand>`
- Everything after the handshake is encrypted with the **session key**

| Cmd | Response | Meaning |
|-----|----------|---------|
| `AA 01` | `AA 01 <0/1>` | status query |
| `33 01 11` / `33 01 10` | `33 01 00` | ON / OFF |
| `AA B1` | `AA B1 <flag> <8B>` | read secret key; flag 01 only after button press |
| `33 B2 <8B>` | `33 B2 00` | check secret key (per session, before toggle) |
| `33 B5 <ts×4> 01 <tz_h> <tz_m>` | `33 B5 00` | SyncTime |
| `AA EF` | | init (V1 only responds meaningfully) |
| `AA B0` / `AA B0 00 01` | echo | plug config |
| `AA 12` / `AA 13` | | timer count / data |
| `AA 06` / `AA 21` | ASCII `"1.00.28"` | firmware version |
| `AA 07 03` / `AA 20` | ASCII `"1.02.00"` | hardware version |
| `AA 14` | 6 bytes | WiFi MAC (= BLE MAC − 1) |
| `AA 07 02` | reversed BLE MAC + 2B | ? |
| `AB 01 04` | multi-frame `AB 00..05` ASCII token | IoT credential — cloud only |

**Init used by `govee-ble`** (works on V1 and V2+):
`33 B2 <key or default>` → `AA EF` → `33 B5 <now>` → `AA B0` → `AA B0 00 01` → `AA 12` → `AA 13` → then toggle.

---

## Code

### Rust binary `govee-ble/` (single file `src/main.rs`, ~470 lines)

| Subcommand | Args | Description |
|------------|------|-------------|
| `scan` | | list BLE devices, 10 s |
| `read` | `--mac` | H5179 temp/humidity/battery |
| `on` / `off` / `status` | `--name <name>` **or** `--mac <addr> [--skey <hex8>]` | plug control (3 retries; `--name` looks up MAC+key in `PLUG_NAMES`) |
| `names` | | print the name → MAC → key table |
| `pair` | `--name` / `--mac` `[--timeout 60]` | app-free pairing: polls `AA B1`, prints key after button press, checks with `33 B2`. `get-skey` is an alias. |
| `daemon` | `--plug-mac --sensor-mac [--plug-skey] [--interval] [--hi] [--lo] [--status-port]` | humidity control loop with **hysteresis band** (ON ≥ hi, OFF ≤ lo, hold in between; `--threshold N` still works as band N/N); `--status-port N` serves an HTML dashboard on `http://<pi>:N/` with `/state.json` + `/poll`, `/dry`, `/dry-off` endpoints (0 = off; defaults interval 900 s, hi 55, lo 45) |

Named plugs live in `PLUG_NAMES` at the top of `main.rs` (no config files):
`dehumidifier` = `D4:AD:FC:41:E1:DD` (`a69f370afd964e0d`),
`pi-side` = `60:74:F4:BD:4D:E5` (V1, no key),
`e245` = `D4:AD:FC:42:E2:45` (`f6e0730a5be545e3`).

Deps: `btleplug`, `tokio`, `aes`, `futures`, `hex`, `log`, `env_logger`.
No clap/reqwest/anyhow. ~1.5 MB release binary. `TZ_HOURS` const = -4.

**Build only on the Pi.** btleplug pulls in `jni` on Android targets, so
`cargo check` fails on Termux. Non-interactive SSH lacks `~/.cargo/bin` in PATH:

```bash
ssh pi@192.168.2.21 'export PATH=$HOME/.cargo/bin:$PATH; cd ~/Github/govee-humidity-control && git pull && cd govee-ble && cargo build --release'
```
(`/usr/bin/cargo` is 1.65 and can't read the v4 lockfile; rustup's is 1.98.)

### Scripts (Python, Termux side — analysis only)

| File | Purpose |
|------|---------|
| `scripts/decode_sessions.py <btsnoop.log> [--filter aab1]` | **The useful one.** Decrypts every write AND notification, grouped per E7 session, with peer MAC. Needs `pycryptodome`. |
| `scripts/govee_ble_protocol.py` | crypto helpers |
| `scripts/parse_btsnoop.py`, `analyze_btsnoop.py`, `extract_skey.py`, `decrypt_e245.py` | older one-off analyses (writes only) |
| `h5080_controller.py` | Python reference controller (bleak) |

### Reverse-engineering artefacts on the phone (Termux `~`)

| Path | What |
|------|------|
| `~/govee_apk/base.apk`, `split_pact_h5080.apk` | pulled APKs |
| `~/govee_apk/h5080/classes/sources/com/govee/h5080/` | jadx-decompiled H5080 module (`add/AbsPairAc4SecretV1.java` = pairing state machine, `ble/controller/SyncTimeController.java`, …) |
| `~/govee_apk/skc/` | `SecretKeyController.java`, `EventSecretKey.java`, `AbsSingleController.java` etc. — extracted with `jadx --single-class com.govee.base2light.ble.controller.SecretKeyController base/classes10.dex` |
| `~/govee_apk/base_full/resources/res/values/strings.xml` | UI strings (`plug_*_press_hint`) |
| `~/btsnoop_0916/btsnoop_hci.log` | 09-16 10:46 capture — 16 sessions, **no E1DD** (E245 + 4DE5 only); contains the `AA B1 01 f6e0730a5be545e3` proof (sess. 13) |
| `~/btsnoop_new/btsnoop_hci.log` | 09-16 20:26 capture — **36 sessions, incl. E1DD 22–23** (`33 B2 a69f370afd964e0d` accepted); also `AA B1 01` reveals for 4DE5 (`3c9c9d890940b019`) |
| `~/btsnoop_e1dd/btsnoop_hci.log` | 09-15 capture (E245 toggles despite the dir name) |
| `~/bugreport*.zip`, `/storage/emulated/0/Download/bugreport-*.zip` | raw bugreports; btsnoop at `FS/data/misc/bluetooth/logs/btsnoop_hci.log` |

---

## Infrastructure

**Raspberry Pi** — `pi@192.168.2.21`, Debian 12 armv7l, kernel 6.1, CYW43455 BLE.
Repo `~/Github/govee-humidity-control/`, binary `govee-ble/target/release/govee-ble`
(needs `sudo`). Rust via rustup (`~/.cargo/bin`). systemd unit:
`govee-ble/humidity-daemon.service` (copy to `/etc/systemd/system/` + `enable --now`).

**Android** — Motorola g86 power 5G, Termux. ADB wireless (port rotates).
btsnoop: `adb shell settings put global bluetooth_hci_snoop_log 1`, then
Developer Options → Bug report → Interactive. jadx 1.5.5 installed in Termux.

---

## Open Items

0. **Cloud-free transition — PROVEN on E245 (2026-09-17).** Unbound E245
   (unbound via Govee app → pairing mode), no WiFi provisioned, tested with
   existing key over pure BLE: `status→ON`, `off→OFF` (confirmed), `on→ON`.
   `33 B2` accepted, `33 01` toggles execute. No cloud/account/WiFi needed
   for control.
   Plan per plug: (a) unbind in Govee app (= cloud-account unbind; the plug
   re-enters pairing mode), (b) `govee-ble pair` to (re)read the key, (c)
   **never provision WiFi** — plug then has no cloud link to phone home to;
   control via BLE `--skey` forever. Verified end-to-end on E245.
   Remaining minor questions (not blocking): does unbinding clear stored WiFi
   creds (no BLE "forget WiFi/reset net" command in the H5080 set)? If creds
   persist on power-up it could still reach Govee infra over WiFi; strictly
   cloud-free would then need a network block (firewall/VLAN) or a reset
   procedure (untested). LED behavior while unbound-and-idle (stays flashing
   vs. settles) also unobserved — cosmetic only.
1. ~~E1DD's key~~ — captured (`a69f370afd964e0d`) and **toggle-verified**.
   Mechanism resolved: pairing mode required; bound plug re-enters it via the
   app's cloud unbind (no BLE command exists).
2. ~~Re-verify 4DE5 toggles~~ — **done**: owner clicked it ON (next to Pi).
3. **Deploy daemon live on the Pi** (systemd) — **done 2026-09-18** (see NEXT
   ACTION): `humidity-daemon.service` installed and enabled, targets the
   dehumidifier plug (`D4:AD:FC:41:E1:DD`), **hysteresis band hi 55 / lo 45**
   (2026-09-19, change `humidity-hysteresis-band`), interval 900 s, status on
   `--status-port 8843` (no external healthcheck).
4. Optional cleanup: drop the older one-off scripts now that `decode_sessions.py` supersedes them.
5. Optional: `pair` could persist keys to a config file instead of requiring `--skey` on every call.
6. ~~Observe cycling + tune threshold~~ — **done 2026-09-19**: a year of
   1-min H5179 exports (2025-09→2026-09) showed single-45 chattered ~27
   times/day in summer; the 55/45 hysteresis band (now live) should cut that
   to a few cycles/yr. Re-export and run
   `scripts/humidity_analysis.py ~/govee_export --scheme band 55 45` after a
   season to re-tune.

## Resolved (don't re-investigate)

- ~~How does the app generate the key?~~ It doesn't; the plug does. Read via `AA B1` after button press.
- ~~Can `33 B2` set a key / is there a factory reset?~~ `33 B2` is a check. No set exists, none needed.
- ~~Extract keys from phone storage / `adb backup`?~~ Unnecessary.
- ~~Does `AB 01 04` commit the key?~~ No, it fetches an IoT token.
- ~~Why doesn't E1DD toggle?~~ Never had its key. No E1DD capture exists.
- ~~What is `33 B5`?~~ SyncTime.

---

## Git History (branch `main`)

- `f097dd1` clean repo, BLE controller works, Rust rewrite
- `70c4ee8` scan · `6f26268` --skey · `cf0120f` get-skey (broken, superseded)
- `8cfa645` PROTOCOL.md firmware variants · `d809d74` HANDOVER.md
- `b0f2d44` **`pair` subcommand; secret key is plug-owned; decode_sessions.py; docs corrected**
- `+1` Cargo.lock revert · `+1` pair: per-poll flag output
- Pushed to origin and pulled/built on the Pi.

## Quick Start

```bash
# on the Pi, in govee-ble/ (rebuild after a pull)
sudo ./target/release/govee-ble names
sudo ./target/release/govee-ble status --name dehumidifier        # D4:AD:FC:41:E1:DD
sudo ./target/release/govee-ble on  --name dehumidifier          # or just --mac + --skey
sudo ./target/release/govee-ble off --name dehumidifier
sudo ./target/release/govee-ble read --mac E3:32:81:12:40:A4
# status page (daemon --status-port 8843) — mobile dashboard, lila.lan style:
curl http://192.168.2.21:8843/                      # HTML dashboard (auto-refresh 30s)
curl http://192.168.2.21:8843/state.json            # JSON state (machine-readable)
curl -X POST http://192.168.2.21:8843/poll          # manual live sensor read (1/30s)
curl -X POST 'http://192.168.2.21:8843/dry?mins=60' # dry mode: force dehumidifier ON 60 min
curl -X POST http://192.168.2.21:8843/dry-off       # cancel dry mode
#   Dry mode = plug held ON until the timer expires (survives the daily 06:00
#   reboot via /var/lib/humidity/force_until), then the band resumes as a fresh
#   first read. Missed polls keep the last good reading on the page with a
#   "missed" banner (no more blank error page). RSSI meter: green ≥-70, amber
#   -70..-85, red <-85 dBm.

# run it as a service (unit targets dehumidifier E1DD, 900 s / 45% / port 8843):
sudo cp govee-ble/humidity-daemon.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now humidity-daemon
```

## Contact
- Repo: `github.com/AutoMas0n/govee-humidity-control`
- Pi SSH: `pi@192.168.2.21`
- Android: Motorola g86 power 5G via ADB wireless (port changes frequently)
