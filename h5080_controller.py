#!/usr/bin/env python3
"""
Govee H5080 Smart Plug BLE Controller
Fully local BLE control - no cloud, no WiFi required.

Protocol: AES-128-ECB + RC4 encrypted 20-byte frames over BLE GATT.
Session handshake: E7 01 (request) -> E7 01 (device response w/ session key) -> E7 02 (confirm)
Commands are encrypted with session key:
  - Turn ON:  33 01 11 00...
  - Turn OFF: 33 01 10 00...
  - Status:   AA 01 (response byte[2]: 0=OFF, 1=ON)
"""
import asyncio, os

# Import AES (works with either pycryptodome or pycryptodomex)
try:
    from Cryptodome.Cipher import AES  # Debian/Ubuntu
except ImportError:
    from Crypto.Cipher import AES

from bleak import BleakClient

# ============== CONSTANTS ==============
KEY_COMM = b"MakingLifeSmarte"  # Static AES-128 key (reverse-engineered from Govee Home APK)
SERVICE_UUID = "00010203-0405-0607-0809-0a0b0c0d1910"
WRITE_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b10"
PLUG_MAC = "60:74:F4:BD:4D:E5"

# ============== CRYPTO ==============
def rc4(data, key):
    """RC4 PRNG (symmetric)."""
    S = list(range(256))
    j = 0
    for i in range(256):
        j = (j + S[i] + key[i % len(key)]) & 255
        S[i], S[j] = S[j], S[i]
    i = j = 0
    out = bytearray(len(data))
    for n in range(len(data)):
        i = (i + 1) & 255
        j = (j + S[i]) & 255
        S[i], S[j] = S[j], S[i]
        out[n] = data[n] ^ S[(S[i] + S[j]) & 255]
    return bytes(out)


def frame_from(cmd, sub, data=()):
    """Build a 20-byte frame: [cmd, sub, data..., 0x00 padding, XOR checksum].

    NOTE: bytes 3..18 must be 0x00 (the plug validates them).
    """
    f = bytearray(20)
    f[0] = cmd
    f[1] = sub
    if data:
        f[2:2 + len(data)] = data
    cs = 0
    for b in f[:19]:
        cs ^= b
    f[19] = cs
    return bytes(f)


def encrypt(frame, key):
    """AES-128-ECB on first 16 bytes, RC4 on last 4 bytes."""
    c = AES.new(key, AES.MODE_ECB)
    return c.encrypt(frame[:16]) + rc4(frame[16:20], key)


def decrypt(payload, key):
    assert len(payload) == 20
    c = AES.new(key, AES.MODE_ECB)
    return c.decrypt(payload[:16]) + rc4(payload[16:20], key)


def verify(frame):
    cs = 0
    for b in frame[:19]:
        cs ^= b
    return cs == frame[19]


# ============== CONTROLLER ==============
class H5080Controller:
    def __init__(self, mac=PLUG_MAC):
        self.mac = mac
        self.client = None
        self.sk = None
        self._notifs = []

    def _cb(self, s, d):
        self._notifs.append(bytes(d))

    async def connect(self):
        self.client = BleakClient(self.mac)
        await self.client.connect()
        await self.client.start_notify(NOTIFY_CHAR, self._cb)

    async def disconnect(self):
        if self.client and self.client.is_connected:
            await self.client.disconnect()

    async def _write_plain(self, frame_bytes, key):
        self._notifs.clear()
        await self.client.write_gatt_char(WRITE_CHAR, encrypt(frame_bytes, key), response=False)
        await asyncio.sleep(0.5)

    async def handshake(self):
        """E7 01 request -> device sends session key -> E7 02 confirm."""
        f1 = frame_from(0xE7, 0x01, os.urandom(16))
        await self._write_plain(f1, KEY_COMM)
        await asyncio.sleep(1.5)  # wait for device response

        for n in self._notifs:
            d = decrypt(n, KEY_COMM)
            if d[0] == 0xE7 and d[1] == 0x01 and verify(d):
                self.sk = bytes(d[2:18])
                break
        if not self.sk:
            raise RuntimeError("Handshake failed: no E7 01 response from device")

        f2 = frame_from(0xE7, 0x02, os.urandom(16))
        await self._write_plain(f2, KEY_COMM)
        await asyncio.sleep(0.5)

    async def initialize(self):
        """Run the device initialization sequence (required before commands work)."""
        # aa ef - initial device handshake
        await self._cmd(0xAA, 0xEF)
        # 33 b2 - soft version write
        await self._cmd(0x33, 0xB2, (0x3C, 0x9C, 0x9D, 0x89, 0x09, 0x40, 0xB0, 0x19))
        # 33 b5 - hard/wifi version write
        await self._cmd(0x33, 0xB5, (0x6A, 0xA1, 0xBB, 0xA7, 0x01, 0xFC))
        await asyncio.sleep(0.5)
        # aa b0 - plug state query pair
        await self._cmd(0xAA, 0xB0)
        await self._cmd(0xAA, 0xB0, (0x00, 0x01))
        # aa 12 / aa 13 - timer count / timer data
        await self._cmd(0xAA, 0x12)
        await self._cmd(0xAA, 0x13)
        await asyncio.sleep(0.5)

    async def _cmd(self, cmd, sub, data=()):
        f = frame_from(cmd, sub, data)
        await self._write_plain(f, self.sk)

    async def turn_on(self):
        await self._cmd(0x33, 0x01, (0x11,))

    async def turn_off(self):
        await self._cmd(0x33, 0x01, (0x10,))

    async def get_state(self):
        """Return 0 (OFF), 1 (ON), or None."""
        await self._cmd(0xAA, 0x01)
        await asyncio.sleep(0.5)
        for n in self._notifs:
            d = decrypt(n, self.sk)
            if d[0] == 0xAA and d[1] == 0x01 and verify(d):
                return d[2]
        return None


# ============== CLI ==============
async def toggle(mac=PLUG_MAC, force=None):
    ctrl = H5080Controller(mac)
    try:
        await ctrl.connect()
        await ctrl.handshake()
        await ctrl.initialize()
        state = await ctrl.get_state()
        print(f"Plug state: {'ON' if state else 'OFF'}")

        if force is None:
            force = not state
        if force:
            print("Turning ON...")
            await ctrl.turn_on()
        else:
            print("Turning OFF...")
            await ctrl.turn_off()
        await asyncio.sleep(2)

        new_state = await ctrl.get_state()
        print(f"New state: {'ON' if new_state else 'OFF'}")
        return new_state
    finally:
        await ctrl.disconnect()


if __name__ == "__main__":
    import sys
    force = None
    if len(sys.argv) > 1:
        arg = sys.argv[1].lower()
        if arg in ("on", "1", "true"):
            force = True
        elif arg in ("off", "0", "false"):
            force = False
    asyncio.run(toggle(force=force))