## 1. H5179 BLE Reader

- [ ] 1.1 Create `ble_reader.py` with a function that uses `BleakScanner` to find the H5179 by MAC (`E3:32:81:12:40:A4`) and capture manufacturer advertisement data within a 15-second timeout, returning the raw data or None
- [ ] 1.2 Implement H5179 data parser: extract temperature from bytes 4-5 (`(data[4] - 100) + data[5] / 10`) and humidity from byte 6 (integer 0-100). Log warnings for malformed data (fewer than 7 bytes).
- [ ] 1.3 Verify locally: run BLE scan on the Pi against the real H5179 sensor, confirm temperature and humidity parse correctly, and match physical readings

## 2. H5080 Cloud Controller

- [ ] 2.1 Create `cloud_controller.py` with a function that sends power ON/OFF to the Govee OpenAPI endpoint (`https://openapi.api.govee.com/router/api/v1/device/control`) using the API key from `api_key.secret`, with SKU `H5080` and device ID `B4:8F:D4:AD:FC:41:E1:DC`
- [ ] 2.2 Track plug state in memory (last-known on/off) and only send API requests when the desired state differs from the last known state — never re-send the same command
- [ ] 2.3 Verify: run a manual toggle (ON then OFF) against the real plug, confirm relay clicks via HTTP API logs

## 3. Main Control Loop

- [ ] 3.1 Write `main.py` that integrates both components: loop every 15 minutes → scan H5179 → parse humidity → threshold check → toggle H5080 if state changed. Include graceful signal handling (SIGINT/SIGTERM).
- [ ] 3.2 Add error handling: BLE scan failures log a warning and retry next cycle (no crash). API failures log the status code and continue. Use exponential backoff if consecutive failures occur (5 min → 15 min → 60 min max).
- [ ] 3.3 Update `requirements.txt` to list `bleak` (remove `requests`)
- [ ] 3.4 Verify: run `python3 main.py` on the Pi for a complete cycle (scan + humidity parse + plug toggle), confirm logging output is correct

## 4. Clean Up Repo

- [ ] 4.1 Delete obsolete files: `Cargo.toml`, `Cargo.lock`, `config.toml`, `setup.sh`, `require.py`, `nohup.out`, `openssl-1.1.1.tar.gz`
- [ ] 4.2 Update `README.md` to document the hybrid architecture (BLE + cloud), installation instructions (pip install bleak), and device MAC references
- [ ] 4.3 Verify: `ls` shows only `main.py`, `ble_reader.py`, `cloud_controller.py`, `api_key.secret`, `README.md`, `ROADMAP.md`, `requirements.txt`, `.gitignore`, `openspec/`

## 5. Deploy to Pi

- [ ] 5.1 SSH into Pi, stop service (`sudo systemctl stop myscript.service`), copy new files, test manually
- [ ] 5.2 Restart systemd service (`sudo systemctl start myscript.service`), verify it stays running (`systemctl status myscript.service`)
- [ ] 5.3 Verify full cycle via journal: `journalctl -u myscript.service -n 20 --no-pager` shows BLE scan + API call