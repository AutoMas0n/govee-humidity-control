## Context

Two previous attempts to infer the H5080 BLE protocol failed: one tried standard Govee XOR-checksum frames (`h5080-ble-alternative`, archived), the other attempted a V2 AES-128-GCM encrypted protocol based on homebridge-govee source from other device families (`h5080-ble-protocol`, archived). Neither toggled the relay.

The Govee app works with WiFi off, confirming BLE control is possible. The only way to determine the real protocol is to **capture and analyze what the app actually sends**.

See `proposal.md — Why` for motivation. See `specs/ble/protocol-reverse-engineering/spec.md` for requirements.

## Goals / Non-Goals

**Goals:**
- Capture complete BLE HCI traffic between Pi and H5080 plug during Govee app control
- Extract the exact power ON/OFF command bytes from the capture
- Replay captured commands to confirm relay toggles
- Document the real protocol for implementation

**Non-Goals:**
- This does NOT implement the production `main.py` — that's downstream
- Not testing speculative protocol variants — only capturing real traffic

## Decisions

### Decision 1: `btmon` over `hcidump` for capture

`btmon` writes structured binary (`.hci` files) that can be replayed and parsed. It captures at the HCI level, showing all events (connection, pairing, GATT). `hcidump` is used for the human-readable hex output. The Pi has BlueZ 5.66 which ships both.

### Decision 2: Capture on Pi (not phone side)

The Pi is a fixed device with `btmon` already installed and confirmed working (we captured H5179 traffic earlier). The phone running the Govee app talks to the plug directly over BLE; we can't sniff phone-to-plug traffic directly. Instead, the Pi connects as a **passive BLE sniffer** by running `btmon` while the phone and plug communicate.

**Alternative**: Use an Android BLE sniffer app. Rejected because the Pi is already set up and the user wants a development workflow, not mobile debugging.

### Decision 3: Govee app user toggles while Pi captures

The user runs the Govee app on their phone, brings it near the plug, and toggles ON then OFF several times. The Pi, running `btmon` in monitor mode, captures all HCI packets visible to the controller.

## Risks / Trade-offs

- **[Risk] Phone-to-plug BLE may not be visible to Pi** → BLE is designed for privacy; a connection between two devices may not be visible to a third-party scanner. However, `btmon` with LE Advertising Report events and a passive watcher on the Pi may still capture the write commands if the Pi's controller is in test mode. **Mitigation**: Try `btmon` with the Pi in sniffer mode (`hcitool cmd 08 0001` to enter LE test mode).
- **[Risk] Govee app may not connect when the Govee cloud is unreachable** → The user confirmed it works with WiFi off, but the app may still try cloud first. **Mitigation**: Put phone in airplane mode, enable only Bluetooth.
- **[Risk] Captures may be encrypted** → If the app uses encrypted GATT (signing/encryption at ATT layer), the payload bytes will be opaque. **Mitigation**: Check for LL_ENCRYPTION_REQ events. If encrypted, sniffing may not reveal plaintext commands.

## Open Questions

- Can the Pi's Bluetooth controller operate as a BLE sniffer (test/direct mode) to capture phone-to-plug traffic, or only as a participant? This determines whether we can sniff passively or need a man-in-the-middle approach.

## Migration Plan

1. Run `btmon -w /tmp/sniff.hci &` in background on Pi
2. User opens Govee app near plug and toggles ON + OFF (3 cycles)
3. Stop capture, convert to readable hex via `btmon -r /tmp/sniff.hci`
4. Extract write characteristics and payload bytes
5. Replay bytes via `gatttool --char-write` to confirm relay toggles
6. If successful, document the protocol for the implementation change