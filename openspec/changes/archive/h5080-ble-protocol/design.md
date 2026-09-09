## Context

The ROADMAP's Phase 1 goal is fully-local BLE control. The H5179 sensor works over BLE advertisements without pairing. The H5080 plug connects over BLE, accepts writes (LED confirms), but the relay never toggles. Investigation of the homebridge-govee plugin's `ble-crypto.js` revealed that newer Govee devices require a **V2 encrypted protocol**: an AES-128-GCM handshake to establish a per-session key, followed by wrapped 20-byte command frames. Without the wrapper, the plug silently ignores plain commands.

Additionally, BlueZ 5.66 on the Pi has a broken interactive agent registration (`Failed to register agent object`), which may prevent proper pairing with the plug — a prerequisite for encrypted communication on some models.

See `proposal.md — Why` for motivation. See `specs/ble/h5080-power-control/spec.md` for requirements.

## Goals / Non-Goals

**Goals:**
- Implement the V2 encrypted BLE protocol (handshake + wrapped frames) for the H5080
- Fix BlueZ pairing on the Pi (D-Bus agent, not interactive bluetoothctl)
- Read H5179 humidity via BLE advertisements
- Replace `main.py` with fully-local hybrid (BLE read + BLE control)
- Fallback to LAN UDP protocol if BLE encrypted path fails

**Non-Goals:**
- Cloud API calls of any kind
- Hardware modifications or plug replacement
- Supporting non-ihoment/Govee BLE switches

## Decisions

### Decision 1: Use `openssl` CLI for AES-128-GCM (not Python crypto library)

The `cryptography` and `pycryptodome` packages fail to build on the Pi's armv7l architecture (`cffi` compilation errors). OpenSSL 3.0.20 is available natively on the Pi. The GCM operations needed (handshake encrypt/decrypt, frame encrypt) are simple: one shot per connection, then one per command. Calling `openssl` subprocess for each is fast enough (< 100ms).

**Alternatives considered:**
- **Python `cryptography`**: Fails to compile on armv7l
- **Pure Python AES-GCM**: Possible but error-prone and slower
- **PyCryptodome**: Same compilation issue as cryptography
- **Node.js**: Would need to install Node on the Pi just for crypto

### Decision 2: Fix BlueZ pairing via D-Bus directly

`bluetoothctl` interactive mode has a race condition on BlueZ 5.66 where `agent NoInputNoOutput` and `default-agent` fail when called in quick succession. The solution is to register the agent via D-Bus calls directly, using `dbus-python` (already installed). This also gives fine-grained control over the pairing flow.

**Alternatives considered:**
- **bleak's `pair()` method**: Fails with `AuthenticationFailed` — bleak doesn't register a proper agent
- **Shell script with sleeps**: Tested — the agent registration timing is unreliable even with delays

### Decision 3: Three-phase attack for plug control

Phase 1: Try V2 encrypted protocol with handshake. If plug responds to `0xe7` handshake, establish session key and send AES-128-GCM wrapped power frames.

Phase 2: If no handshake response, attempt pairing first (register D-Bus agent, pair with `NoInputNoOutput` capability), then try plain commands again.

Phase 3: If both BLE paths fail, switch to LAN UDP broadcast on ports 4001-4003 with Govee hello/scan protocol.

**Rationale**: The homebridge code shows H3001 and other models silently ignore plain commands when they require encryption. The plug LED flash might be the HCI layer acknowledging the write, while the application firmware discards it. The V2 handshake is the key differentiator.

### Decision 4: Single-file script structure with clear separation

Keep the implementation as `main.py` (main loop) with internal functions grouped by concern:
- `ble_reader` functions: H5179 scan + parse
- `v2_crypto` functions: handshake, AES-GCM, key derivation
- `h5080_controller` functions: connect, handshake, encrypt, write, disconnect
- `lan_fallback` functions: UDP discovery + control

This avoids module imports while keeping code organized for the future Rust rewrite.

## Risks / Trade-offs

- **[Risk] Plug doesn't support V2 encryption after all** → The handshake test (write `0xe7` frame, check for `0xe7` response via notifications) will confirm or disprove this immediately. Mitigation: Land in garbage — BLE handshake got no response in our early test. Move to Phase 2 (pairing) or Phase 3 (LAN).
- **[Risk] Pairing still fails with proper D-Bus agent** → Some plugs require user confirmation on the physical button. Mitigation: Sequence with `RequestConfirmation` auto-accept in the agent.
- **[Risk] LAN protocol not supported by this model** → Tested once with no response. May need longer scan or correct discovery message format. Mitigation: Accept and log failure; system stays in manual control.
- **[Risk] OpenSSL subprocess overhead** → Each AES-GCM operation forks a process. Mitigation: Only ~2 operations per 15 minutes — negligible.

## Open Questions

- Does the plug need to be paired (bonded) via the Govee app before accepting encrypted commands, or can the Pi establish a fresh session key on each connection independently?
- The homebridge `ble-crypto.js` shows four magic key constants (`KEY_HANDSHAKE`, `KEY_DEVICE`) — are these universal to all V2 Govee devices or specific to certain models?
- Does the `init` Govee app pairing encode additional state into the plug that's needed for the V2 handshake to succeed?

These affect Phase 1 vs Phase 2 priority but don't change the overall approach.

## Migration Plan

1. Test V2 handshake: write `0xe7` frame to plug, monitor notify char for response
2. If handshake succeeds → implement full V2 protocol → replace `main.py`
3. If handshake fails → implement D-Bus pairing → retry V2
4. If pairing+V2 still fails → implement LAN fallback
5. Test full cycle: H5179 scan → humidity parse → H5080 BLE toggle → verify physically
6. Deploy: stop service, copy new files, restart service