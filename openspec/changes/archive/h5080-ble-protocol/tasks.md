## 1. AES-128-GCM Crypto Implementation (via OpenSSL CLI)

- [ ] 1.1 Implement Python wrapper functions for AES-128-GCM encrypt/decrypt using `openssl enc` subprocess: verify with known test vectors (`openssl enc -aes-128-gcm -K <hex> -iv <hex>` produces correct ciphertext + tag)
- [ ] 1.2 Implement V2 handshake builder: construct `0xe7 0x11 0x01` frame with random IV, encrypt txIvKey with KEY_HANDSHAKE, write to `/tmp/handshake_test.bin` and verify frame structure manually
- [ ] 1.3 Implement handshake response parser: decrypt the plug's `0xe7 0x11 0x00` response, extract rxIvKey and devInfo, derive per-session deviceKey via AES-128-ECB(KEY_DEVICE, padded devInfo)
- [ ] 1.4 Implement frame wrapper: wrap a 20-byte Govee power frame into the V2 encrypted format (counter + GCM{ frame }), with tag appended
- [ ] 1.5 Verify: all crypto primitives work correctly by round-tripping test data through encrypt/decrypt with known keys

## 2. H5080 V2 Encrypted Power Commands

- [ ] 2.1 Connect to the H5080 plug via bleak with notifications enabled; write the V2 handshake frame to WRITE_CHAR (`...2b11`); capture any response on NOTIFY_CHAR (`...2b10`) within 5 seconds
- [ ] 2.2 If handshake response received: parse it, derive device key, wrap an OFF frame (`0x33 0x01 0x00`), write it, and listen for relay click. Repeat with ON (`0x33 0x01 0x01`).
- [ ] 2.3 If no handshake response: implement D-Bus agent registration (Python `dbus`, `NoInputNoOutput` capability), pair with the plug, disconnect, then retry V2 handshake
- [ ] 2.4 Verify: confirm relay physically toggles when V2 frame is written. Confirm state via advertisement data (mfr last byte `01`=ON, `00`=OFF).

## 3. LAN UDP Fallback

- [ ] 3.1 Implement Govee LAN discovery: broadcast `hello` on ports 4001-4003, capture any response within 5 seconds, extract SKU/IP/MAC from response
- [ ] 3.2 If LAN device found with matching MAC: implement power toggle using Govee LAN protocol (`0x33 0x01 0x00/01` XOR frame over UDP)
- [ ] 3.3 Verify: ping the plug's IP from the Pi, send LAN power ON/OFF, confirm relay toggles

## 4. H5179 BLE Reader

- [ ] 4.1 Implement H5179 scanner: use `BleakScanner.find_device_by_address()` with 15-second timeout targeting MAC `E3:32:81:12:40:A4`
- [ ] 4.2 Implement H5179 parser: extract temperature from bytes 4-5 (`(data[4] - 100) + data[5] / 10`) and humidity from byte 6 (integer)
- [ ] 4.3 Verify: run scanner on the Pi against the real H5179 sensor, confirm parsed values match expected readings

## 5. Main Control Loop

- [ ] 5.1 Write `main.py` integrating all components: asyncio loop every 15 minutes → scan H5179 → parse humidity → threshold check (45%) → send V2 BLE command to H5080 (or LAN fallback) if state changed → sleep
- [ ] 5.2 Add error handling: exponential backoff on consecutive BLE failures (5 min → 15 min → 60 min max), graceful SIGINT/SIGTERM handling, logging with timestamps
- [ ] 5.3 Update `requirements.txt` to list `bleak` (remove `requests`)
- [ ] 5.4 Verify: run `python3 main.py` on the Pi, confirm full cycle: BLE scan → humidity parse → BLE power toggle → log output

## 6. Clean Up Repo and Deploy

- [ ] 6.1 Delete obsolete files: `Cargo.toml`, `Cargo.lock`, `config.toml`, `setup.sh`, `require.py`, `nohup.out`, `openssl-1.1.1.tar.gz`, `api_key.secret`
- [ ] 6.2 Update `README.md` for fully-local BLE architecture: document BLE V2 protocol requirement, device MACs, installation (`pip install bleak`), systemd service
- [ ] 6.3 SSH into Pi, stop service (`sudo systemctl stop myscript.service`), copy new files, start service (`sudo systemctl start myscript.service`)
- [ ] 6.4 Verify systemd service is active and journal shows a complete cycle: `systemctl status myscript.service` and `journalctl -u myscript.service -n 20 --no-pager`