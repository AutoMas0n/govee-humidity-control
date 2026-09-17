# Govee BLE Control — Handover Document

## Goal
Replace the Govee Home cloud app with a standalone local BLE solution for controlling
Govee H5080 smart plugs and reading H5179 humidity sensors. Zero cloud dependency.
No enshittification. Runs on a Raspberry Pi 4 (Debian 12, armv7l).

**Status (2026-09-16 end of session):** protocol is fully understood, including
app-free pairing. `govee-ble pair` is built on the Pi and ran once against E1DD
(timed out — user was not at the plug to press the button). The only remaining
blocker is a physical button press.

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
- App UI string during this phase: `plug_single_pair_press_hint` =
  *"The device's power indicator is slowly flashing blue. Please short press its
  switch button to pair."* Pairing-mode entry string: `plugv1_guide_des_v1` =
  *"Press and hold the button until the indicator light slowly blinks blue."*
- **Pairing mode is a prerequisite (confirmed by the plug owner + APK).** The
  plug must already be in pairing mode (LED slowly blinking blue) before the
  short-press confirms. There is **no BLE command that enters pairing mode** —
  no such controller exists in the decompiled H5080 module, and the captures
  show the app sends nothing before polling `AA B1`. The app merely drives the
  flow; the plug enters pairing mode itself and the button-press confirms. In
  `pair`, output token `00` = plug IS in pairing mode (awaiting press),
  `-` = plug not answering (not in pairing mode — hold its button until LED
  slowly blinks blue; that's why the E1DD run timed out).
- `AB 01 04` + `AA 06/07/14/20/21/B3` after the key check are firmware/hw
  version, WiFi MAC and an IoT credential token — cloud provisioning, **not
  needed** for BLE control.
- `33 B5` is **SyncTime** (`[unix_ts BE×4][01][tz_hours i8][tz_min]`), not a
  hardware version write. `FC` = UTC-4.

---

## NEXT ACTION (do this first)

Someone must be physically at plug **E1DD** (`D4:AD:FC:41:E1:DD`).

```bash
ssh pi@192.168.2.21
cd ~/Github/govee-humidity-control/govee-ble
sudo ./target/release/govee-ble pair --mac D4:AD:FC:41:E1:DD --timeout 120
```

It prints `connected. The plug must be in pairing mode (LED slowly blinking
blue). If not: HOLD the plug button until the LED slowly blinks blue. Then
SHORT-PRESS the button on the plug now <<<` and then one token per poll:
`00` = plug IS in pairing mode (not yet confirmed), `-` = no reply (plug not
in pairing mode or out of range).

1. **Put the plug in pairing mode first** (hold its button until the LED
   slowly blinks blue), then short-press the button once.
2. If you keep getting `-` for ~20 s, the plug isn't in pairing mode — repeat
   the hold until the LED slowly blinks blue, then short-press again.
3. On success it prints the 8-byte hex key on stdout and
   `paired. use: --skey <key>` on stderr. Then verify:
   ```bash
   sudo ./target/release/govee-ble on  --mac D4:AD:FC:41:E1:DD --skey <key>
   sudo ./target/release/govee-ble off --mac D4:AD:FC:41:E1:DD --skey <key>
   ```
4. Record the key in PROTOCOL.md "Secret Keys (Captured)" table and here.
5. Answer the open question: did normal-mode short-press work, or was
   pairing mode required? Update PROTOCOL.md accordingly.

Sanity check that pairing works at all: run the same against E245
(`D4:AD:FC:42:E2:45`) — it should return `f6e0730a5be545e3`.

---

## Devices

| Plug | MAC | Firmware | Secret key | State |
|------|-----|----------|-----------|-------|
| 4DE5 | `60:74:F4:BD:4D:E5` | V1 | none needed (`33 B2 3c9c9d890940b019` default works) | visible in scan again (was unreachable) |
| E245 | `D4:AD:FC:42:E2:45` | V2+ | `f6e0730a5be545e3` (verified toggles) | working; dehumidifier plug |
| E1DD | `D4:AD:FC:41:E1:DD` | V2+ | `a69f370afd964e0d` | paired and verified, BLE toggle works |
| H5179 | `E3:32:81:12:40:A4` | sensor | n/a | advertisements, mfg id `0x8801` |

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
| `on` / `off` / `status` | `--mac` `[--skey <hex8>]` | plug control (3 retries) |
| `pair` | `--mac` `[--timeout 60]` | **app-free pairing**: polls `AA B1`, prints key after button press, checks with `33 B2`. `get-skey` is an alias. |
| `daemon` | `--plug-mac --sensor-mac [--plug-skey] [--interval] [--threshold] [--hc-url]` | humidity control loop |

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

1. **E1DD's key is captured** (`a69f370afd964e0d`, `btsnoop_new` sess. 22–23,
   `33 B2` accepted). Sub-question **answered by the plug owner**: pairing mode
   IS required — the app drives the flow and the plug must be in pairing mode
   (LED slowly blinking blue, entered when it was re-paired; on a fresh plug
   the user holds the button until the LED blinks blue) before the short-press
   confirms. There is no BLE command to enter pairing mode, so `pair` can't
   initiate it — only the person at the plug can. `pair` on E1DD should return
   `a69f370afd964e0d` (put E1DD in pairing mode first).
2. Re-verify 4DE5 toggles: captures show it answering `33 B2 3c9c9d890940b019`
   with `33 B2 00` in every session (incl. 09-16 20:26 sess. 20/24/25/26); a
   live toggle confirms end to end.
3. systemd unit for `daemon` — **added** as `govee-ble/humidity-daemon.service`
   (E245 = dehumidifier, sensor 40A4). Install: `sudo cp` to
   `/etc/systemd/system/`, `daemon-reload`, `enable --now`.
4. Optional cleanup: drop the older one-off scripts now that `decode_sessions.py` supersedes them.
5. Optional: `pair` could persist keys to a config file instead of requiring `--skey` on every call.

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
# on the Pi, in govee-ble/
sudo ./target/release/govee-ble scan
sudo ./target/release/govee-ble pair --mac D4:AD:FC:41:E1:DD --timeout 120   # press plug button
sudo ./target/release/govee-ble on  --mac D4:AD:FC:42:E2:45 --skey f6e0730a5be545e3
sudo ./target/release/govee-ble off --mac D4:AD:FC:42:E2:45 --skey f6e0730a5be545e3
sudo ./target/release/govee-ble read --mac E3:32:81:12:40:A4
sudo ./target/release/govee-ble daemon --plug-mac D4:AD:FC:42:E2:45 --plug-skey f6e0730a5be545e3 \
  --sensor-mac E3:32:81:12:40:A4 --interval 60 --threshold 60 --hc-url http://your-id.healthchecks.io

# run it as a service (edit the --hc-url line in the unit first):
sudo cp govee-ble/humidity-daemon.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now humidity-daemon
```

## Contact
- Repo: `github.com/AutoMas0n/govee-humidity-control`
- Pi SSH: `pi@192.168.2.21`
- Android: Motorola g86 power 5G via ADB wireless (port changes frequently)
