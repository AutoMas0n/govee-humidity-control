## 1. Setup Capture — Android BTSnoop via Bugreport (phone-side, preferred)

> Pi-side btmon was the original plan, but Android btsnoop via bugreport is far superior for this use case because it captures the app's BLE writes directly rather than sniffing over-the-air traffic.

- [x] 1.1 Enabled Bluetooth HCI snoop log on phone:
  ```bash
  adb shell settings put global bluetooth_hci_snoop_log 1
  adb shell dumpsys bluetooth_manager | grep Snoop  # → FULL
  ```
- [x] 1.2 Reset Bluetooth for fresh capture:
  ```bash
  adb shell "svc bluetooth disable; sleep 3; svc bluetooth enable"
  ```
- [x] 1.3 Created `scripts/` directory with analysis tools:
  - `scripts/parse_btsnoop.py` — basic ATT write extraction
  - `scripts/analyze_btsnoop.py` — timing/pattern analysis
  - `scripts/extract_payloads.py` — save all captured payloads

## 2. First Capture Session

- [x] 2.1 User generated bugreport from phone: **Settings → Developer Options → Bug report → Interactive report → Save to Downloads**
- [x] 2.2 Pulled bugreport zip and extracted btsnoop:
  ```bash
  adb pull /storage/emulated/0/Download/bugreport-XXXXX.zip ~/bugreport2.zip
  unzip -o ~/bugreport2.zip "FS/data/misc/bluetooth/logs/btsnoop_hci.log" -d ~/btsnoop2/
  ```
  File: 418KB, containing 6120 HCI packets including ADV advertisements for both H5080 and H5179
- [x] 2.3 Confirmed 27 ATT Write Commands to handle 0x0011 (write characteristic) with 20-byte AES-GCM encrypted payloads
- [x] 2.4 Verified two distinct phases: initial connection (17 writes at t=0) and user toggle (10 writes at t=744s)

## 3. Analyze Capture

- [x] 3.1 Extracted all 27 ATT Write Commands to handle 0x0011 with timestamps. Payload size: exactly 20 bytes each (AES-GCM block size).
- [x] 3.2 Identified payload patterns:
  - **Phase 1 (initial connection/pairing)**: 10 unique payloads in rapid succession, then 8× repeated keepalive
  - **Phase 2 (user toggle)**: 3 unique payloads, then 7× repeated keepalive
  - Repeated payloads: `bd011c7b3bfaee130ee71a637ad2c5e93f7bc84a` (initial), `1fb61a11321c49040ba920b8e3618c5dd818702d` (toggle)
  - Captured payloads saved to `captured_payloads.txt`
- [x] 3.3 Found **no SMP pairing packets, no LE Start Encryption, no Long Term Key exchange** in the capture — the encryption session may be pre-established from phone OS-level bonding, or the GCM key may be static/derived from device identity.
  - **Note**: Handle 0x0018 (Prepare Write) was used during initial connection with TLV-like structures (tag 0x05, 0x06, 0x07) containing device info strings — likely a bonding exchange at the application level, not BLE encryption level.
- [x] 3.4 Analysis scripts saved to `scripts/`: `parse_btsnoop.py`, `analyze_btsnoop.py`, `extract_payloads.py`
- [x] 3.5 Protocol findings documented in `PROTOCOL.md` at repo root

## 4. Replay Confirmation

- [x] 4.1 **Superseded** — The protocol uses AES-128-ECB+RC4, not AES-GCM. Commands CAN be constructed from scratch once the session key is established via handshake.
- [x] 4.2 **Done** — AES-128 key `MakingLifeSmarte` extracted from APK decompilation (`split_pact_h5080.apk` → `EncryptionManager` + `Controller4Aes` + `LibTools`)
- [x] 4.3 **Done** — Session handshake works: E7 01 (request) → device sends session key → E7 02 (confirm)
- [x] 4.4 **Superseded** — BLE control works perfectly. Cloud API not needed.

## 5. Alternative Approaches (Evaluated)

- [x] 5.1 **Android BTSnoop** ✅ Successfully captured real app-to-plug traffic (method above)
- [x] 5.2 **BTSNOOP_SAVE broadcast** ❌ Fails on Motorola (file stays at 0 bytes or doesn't appear in Downloads)
- [x] 5.3 **Direct file access via ADB** ❌ Permission denied (`/data/misc/bluetooth/logs/`) without root
- [x] 5.4 **Bugreport via Developer Options** ✅ The ONLY reliable method on this device (and likely similar non-rooted Androids)
- [x] 5.5 **Done** — Full APK reverse engineering completed. Keys extracted from `com.govee.encryp` package: `KEY_COMM=MakingLifeSmarte`, `KEY_X`, `KEY_Y`.
- [ ] 5.6 GoveeHome WiFi API — not implemented (BLE works locally without cloud)

## 6. Package and Hand Off

- [x] 6.1 Capture data and analysis scripts saved to repo:
  - `PROTOCOL.md` — full protocol findings document
  - `captured_payloads.txt` — all 27 captured write payloads with timestamps
  - `scripts/parse_btsnoop.py` — btsnoop → ATT write parser
  - `scripts/analyze_btsnoop.py` — timing/pattern analysis
  - `scripts/extract_payloads.py` — payload extraction
  - `scripts/capture.sh` — Pi-side btmon helper (for future use)
- [x] 6.2 Verified archive contains `h5080-ble-protocol` and `h5080-ble-alternative`
- [x] 6.3 All 4 artifacts present for `h5080-ble-packet-sniff` change: proposal.md, design.md, spec, tasks.md (updated)
- [x] 6.4 Zip repo delivered to `/storage/emulated/0/Download/govee-repo.zip`
- [x] 6.5 Created `btsnoop-capture` skill at `.pi/agent/skills/btsnoop-capture/` for future use