# Govee Humidity Control

Local, cloud-free control of Govee smart-home devices over Bluetooth LE.

- **H5080 smart plug** (`ihoment_H5080`) — ON/OFF switching
- **H5179 humidity sensor** — temperature/humidity/battery

Zero cloud dependency. Runs on a Raspberry Pi 4 (Debian 12, armv7l), or any
BLE-capable Linux. No Govee account, no API key, no internet required.

Full protocol details: [`PROTOCOL.md`](PROTOCOL.md), operations doc:
[`HANDOVER.md`](HANDOVER.md).

## Usage

The Rust binary lives in `govee-ble/` (single-file `src/main.rs`, build on the
Pi — btleplug can't cross-compile from Termux/Android).

```bash
# on the Pi, in govee-ble/
sudo ./target/release/govee-ble scan                     # discover devices
sudo ./target/release/govee-ble status --mac ... --skey <hex8>
sudo ./target/release/govee-ble on      --mac ... --skey <hex8>
sudo ./target/release/govee-ble off     --mac ... --skey <hex8>
sudo ./target/release/govee-ble pair    --mac ... --timeout 120   # app-free pairing
sudo ./target/release/govee-ble read    --mac <sensor-mac>        # H5179
```

### Known devices

| Device | MAC | Secret key |
|--------|-----|------------|
| H5080 plug E1DD (**dehumidifier**) | `D4:AD:FC:41:E1:DD` | `a69f370afd964e0d` |
| H5080 plug E245 (spare/unplugged) | `D4:AD:FC:42:E2:45` | `f6e0730a5be545e3` |
| H5080 plug 4DE5 (V1 firmware, by the Pi) | `60:74:F4:BD:4D:E5` | none needed |
| H5179 sensor | `E3:32:81:12:40:A4` | n/a |

Newer H5080 firmware requires an 8-byte per-plug secret key (the plug owns
it and reveals it via `AA B1` only after a physical button press) — get it
with `govee-ble pair` or decode it from a btsnoop capture.

## Humidity daemon

```bash
sudo ./target/release/govee-ble daemon \
  --plug-mac D4:AD:FC:41:E1:DD --plug-skey a69f370afd964e0d \
  --sensor-mac E3:32:81:12:40:A4 \
  --interval 900 --threshold 45 --status-port 8080
```

Reads the sensor every `--interval` seconds and switches the plug on when
humidity exceeds `--threshold` % (off otherwise). With `--status-port N` it
also serves a plain-text status page on the LAN — e.g. `curl http://<pi-ip>:8080/`
shows `temp`, `humidity`, `battery`, `plug` and a Unix `ts`. No cloud,
no external healthcheck: everything stays on the Pi.

### systemd (auto-start on boot)

```bash
# check govee-ble/humidity-daemon.service (it targets dehumidifier E1DD,
# interval 900 s, threshold 45%, status port 8080) then install it:
sudo cp govee-ble/humidity-daemon.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now humidity-daemon
```

## Building on the Pi

```bash
ssh pi@192.168.2.21 'export PATH=$HOME/.cargo/bin:$PATH; \
  cd ~/Github/govee-humidity-control && git pull && cd govee-ble && \
  cargo build --release'
```

(`/usr/bin/cargo` is too old for the lockfile; use rustup's).

## Analysis scripts (Termux, optional)

`scripts/decode_sessions.py <btsnoop_hci.log>` decrypts every write and
notification in an Android BLE capture, grouped per E7 session, and validates
each frame's XOR checksum — the ground truth the protocol docs are based on.

## Repository history

- Python cloud proof-of-concept (kept for reference, **not used**): `main.py`,
  `setup.sh`, `require.py`, `requirements.txt` — calls the Govee OpenAPI over
  HTTPS. Retired in favour of the local BLE daemon; cloud files remain in git
  history.
- Rust binary — the production implementation: `govee-ble/` (single-file
  `src/main.rs`) with `scan`/`read`/`on`/`off`/`status`/`names`/`pair`/`daemon`
  subcommands, systemd unit `govee-ble/humidity-daemon.service`.