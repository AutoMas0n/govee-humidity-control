## Context

The original `main.py` uses Govee's cloud HTTP API for both reading humidity (H5179) and controlling the plug (H5080). The ROADMAP's Phase 1 goal was to replace this with a fully local BLE solution. Extensive testing proved:

- **H5179**: BLE advertisement scanning works reliably. Temperature and humidity are readable from manufacturer data without pairing (CompanyID `0x8801` / `0xEC88`).
- **H5080**: BLE connects and writes succeed (LED flashes on command), but the relay **never toggles**. The homebridge-govee plugin source (which supports H5080) confirms the plug only accepts power commands over cloud (AWS MQTT / OpenAPI HTTP), not BLE. Pairing with the plug also fails (`AuthenticationFailed`).

See `proposal.md — Why` for motivation.

## Goals / Non-Goals

**Goals:**
- Replace `main.py` with a hybrid: read H5179 locally via BLE (bleak), control H5080 via Govee OpenAPI HTTP
- Preserve the same threshold logic (humidity > 45% → plug ON)
- Keep the systemd service working (`myscript.service`)
- Remove dead code/files from the old cloud-only approach
- Keep the project ready for a future rust rewrite (Phase 3)

**Non-Goals:**
- Full BLE-only control of the H5080 (proved infeasible for this firmware)
- Replacing the plug with a different model
- Hardware modifications (GPIO relay, etc.)
- encrypting or wrapping BLE commands (V2 protocol handshake not supported by plug firmware)

## Decisions

### Decision 1: Keep cloud API for plug control

The H5080 relay cannot be toggled over BLE. The original Govee OpenAPI HTTP endpoint already works (confirmed working in the current `main.py`). Keeping this endpoint means one HTTP call every 15 minutes — minimal internet dependency.

**Alternatives considered:**
- **GPIO relay**: Fully local but requires wiring and a relay module. Adds hardware dependency and is less flexible.
- **LAN/UDP protocol**: Tested — no response from plug on ports 4001-4004. Not supported by this model.
- **Replace plug**: Would work but costs money and the user already has this plug set up.

### Decision 2: Use bleak for H5179 BLE scanning

The H5179 broadcasts unencrypted advertisement data every 2-3 seconds. Using `BleakScanner` with a detection callback captures the manufacturer data without needing any GATT connection or pairing. bleak is the de-facto Python BLE library (3.0.2 installed on the Pi).

**Alternatives considered:**
- **gatttool / bluetoothctl CLI**: Works but is fragile and slower. Python-native is better for a long-running service.
- **govee-ble / pygovee packages**: Either has incompatible dependencies (cryptography build fails on armv7l), or is cloud-only.

### Decision 3: Single-file Python script

Keep the new `main.py` as a single file, matching the original structure. This keeps the systemd service config unchanged and is simpler to deploy/test than splitting into modules.

### Decision 4: Asyncio event loop

Use Python asyncio for the main loop. bleak is async-native, and mixing synchronous (requests) HTTP calls in async is fine via `asyncio.to_thread()` or the requests library's synchronous calls within a non-blocking loop via `asyncio.get_event_loop().run_in_executor()`.

## Risks / Trade-offs

- **[Internet dependency]** The system needs internet for plug control (~2 HTTP calls per 15 min). If internet goes down, the plug stays in its last state. → **Mitigation**: Log connectivity failures; plug continues operating manually via its physical button.
- **[BLE scan flakiness]** BLE on the Pi's CYW43455 UART chip can be unreliable (kernel bugs, interference). → **Mitigation**: Exponential backoff on failures (5 min → 60 min max), retry on next interval.
- **[API rate limits]** Govee OpenAPI may rate-limit requests. → **Mitigation**: Only 1 request per 15 min per direction — extremely low volume, well within limits.
- **[API key exposure]** `api_key.secret` is stored on-disk. → **Mitigation**: Only the Pi user can read it (file permissions), same as the original setup.
- **[Future Rust rewrite]** The hybrid approach adds complexity to later unwinding the cloud dependency. → **Mitigation**: The BLE reading and cloud control functions are clearly separated functions, making the Rust refactor straightforward.

## Migration Plan

1. Stop the current service: `sudo systemctl stop myscript.service`
2. Replace `main.py` with the new hybrid version
3. Test manually: run `python3 main.py` for one cycle, verify BLE scan + API call
4. Restart the service: `sudo systemctl start myscript.service`
5. Remove obsolete files: `Cargo.toml`, `config.toml`, `setup.sh`, `require.py`

**Rollback**: Keep the old `main.py` as `main.py.old`. Switch back by swapping files and restarting the service.