#!/usr/bin/env python3
"""Govee H5080 BLE Protocol - Full Implementation"""
import asyncio, os, struct
from Crypto.Cipher import AES
from bleak import BleakClient

# ============== DERIVED KEYS ==============
KEY_COMM = b"MakingLifeSmarte"  # 16 bytes AES-128
KEY_X    = bytes.fromhex("FC03783C7C42CB83E202A1643648AFF6")
KEY_Y    = bytes.fromhex("AE028B630BAE6ECC4BFF1B249E22F955")

# ============== UUIDs ==============
SERVICE_UUID = "00010203-0405-0607-0809-0a0b0c0d1910"
WRITE_CHAR_UUID = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY_CHAR_UUID = "00010203-0405-0607-0809-0a0b0c0d2b10"

# ============== Crypto Helpers ==============
def rc4_encrypt(data, key):
    S = list(range(256))
    j = 0
    for i in range(256):
        j = (j + S[i] + key[i % len(key)]) & 0xFF
        S[i], S[j] = S[j], S[i]
    i = j = 0
    out = bytearray(len(data))
    for n in range(len(data)):
        i = (i + 1) & 0xFF
        j = (j + S[i]) & 0xFF
        S[i], S[j] = S[j], S[i]
        out[n] = data[n] ^ S[(S[i] + S[j]) & 0xFF]
    return bytes(out)

def xor_checksum(data):
    cs = 0
    for b in data[:19]:
        cs ^= b
    return cs

def build_frame(cmd, sub, data=b''):
    frame = bytearray(20)
    frame[0] = cmd
    frame[1] = sub
    if data:
        frame[2:2+len(data)] = data
    for i in range(len(data) + 2, 19):
        frame[i] = os.urandom(1)[0]
    frame[19] = xor_checksum(frame)
    return bytes(frame)

def v1_encrypt(frame, key):
    assert len(frame) == 20
    cipher = AES.new(key, AES.MODE_ECB)
    enc = cipher.encrypt(frame[:16])
    rc4_out = rc4_encrypt(frame[16:20], key)
    return enc + rc4_out

def v1_decrypt(payload, key):
    assert len(payload) == 20
    cipher = AES.new(key, AES.MODE_ECB)
    dec = cipher.decrypt(payload[:16])
    rc4_out = rc4_encrypt(payload[16:20], key)
    return dec + rc4_out

def verify_frame(frame):
    return xor_checksum(frame) == frame[19]

# ============== BLE Controller ==============
class H5080Controller:
    def __init__(self, mac):
        self.mac = mac
        self.client = None
        self.session_key = None
        self.notifications = []
        self.handshake_done = asyncio.Event()
        
    def notify_callback(self, sender, data):
        self.notifications.append(data)
        
    async def connect(self):
        self.client = BleakClient(self.mac)
        await self.client.connect()
        print(f"Connected: {self.client.is_connected}")
        await self.client.start_notify(NOTIFY_CHAR_UUID, self.notify_callback)
        
    async def disconnect(self):
        if self.client:
            await self.client.disconnect()
            
    async def write(self, data, wait_notify=True):
        self.notifications.clear()
        await self.client.write_gatt_char(WRITE_CHAR_UUID, data, response=False)
        if wait_notify:
            await asyncio.sleep(1.5)  # Wait for device response
            
    def get_last_notify_decrypted(self):
        if not self.notifications:
            return None
        for n in self.notifications[-3:]:  # Check last few
            dec = v1_decrypt(bytes(n), KEY_COMM)
            if verify_frame(dec):
                return dec
        return None
        
    async def handshake(self):
        """Perform V1 E7 01 / E7 02 handshake and extract session key."""
        print("Handshaking...")
        
        # Step 1: Send E7 01 (session request)
        req = v1_encrypt(build_frame(0xE7, 0x01, os.urandom(16)), KEY_COMM)
        print(f"  -> E7 01 (encrypted with KEY_COMM)")
        self.notifications.clear()
        await self.client.write_gatt_char(WRITE_CHAR_UUID, req, response=False)
        await asyncio.sleep(2)
        
        # Check notification for device response
        if not self.notifications:
            print("  No notification received after E7 01!")
            return False
        
        for n in self.notifications:
            dec = v1_decrypt(bytes(n), KEY_COMM)
            if verify_frame(dec) and dec[0] == 0xE7 and dec[1] == 0x01:
                self.session_key = bytes(dec[2:18])
                print(f"  <- E7 01 response, session key: {self.session_key.hex()}")
                break
        else:
            print(f"  No E7 01 response found in {len(self.notifications)} notifications")
            for n in self.notifications:
                dec = v1_decrypt(bytes(n), KEY_COMM)
                print(f"  Got notify: {bytes(n).hex()} -> {dec.hex()} (ok={verify_frame(dec)})")
            return False
            
        # Step 2: Send E7 02 (confirm)
        self.notifications.clear()
        confirm = v1_encrypt(build_frame(0xE7, 0x02, os.urandom(16)), KEY_COMM)
        await self.client.write_gatt_char(WRITE_CHAR_UUID, confirm, response=False)
        await asyncio.sleep(1.5)
        
        # Check echo
        for n in self.notifications:
            dec = v1_decrypt(bytes(n), KEY_COMM)
            if verify_frame(dec) and dec[0] == 0xE7 and dec[1] == 0x02:
                print(f"  <- E7 02 echo received")
                break
        else:
            print("  Warning: No E7 02 echo (continuing anyway)")
            
        print(f"Handshake complete! Session key: {self.session_key.hex()}")
        return True
        
    async def send_command(self, cmd, sub, data=b'', wait_notify=True):
        """Send an encrypted command using the session key."""
        if not self.session_key:
            raise RuntimeError("No session key! Run handshake first.")
        frame = build_frame(cmd, sub, data)
        encrypted = v1_encrypt(frame, self.session_key)
        self.notifications.clear()
        await self.client.write_gatt_char(WRITE_CHAR_UUID, encrypted, response=False)
        if wait_notify:
            await asyncio.sleep(1)
        return frame  # return plaintext for reference
        
    async def toggle_plug(self, state):
        """Turn plug ON (True) or OFF (False)."""
        data = bytes([0x11 if state else 0x10]) + b'\x00' * 15
        plain = await self.send_command(0x33, 0x01, data)
        print(f"Sent {'ON' if state else 'OFF'}: plain={plain.hex()}")
        
        # Check the status response
        for n in self.notifications[-3:]:
            dec = v1_decrypt(bytes(n), self.session_key)
            if verify_frame(dec):
                print(f"Response: {dec.hex()} [{dec[0]:02x} {dec[1]:02x}]")
                if dec[0] == 0x33 and dec[1] == 0x01:
                    print(f"  -> Command acknowledged!")
        return plain
        
    async def query_status(self):
        """Query plug status."""
        plain = await self.send_command(0xAA, 0x01)
        # Check notify for response
        for n in self.notifications[-3:]:
            dec = v1_decrypt(bytes(n), self.session_key)
            if verify_frame(dec) and dec[0] == 0xAA and dec[1] == 0x01:
                state = dec[2]
                print(f"Plug state: {'ON' if state else 'OFF'} (byte2={state})")
                return state
        return None

async def main():
    MAC = "60:74:F4:BD:4D:E5"
    plug = H5080Controller(MAC)
    
    try:
        await plug.connect()
        
        # Handshake
        if not await plug.handshake():
            print("Handshake failed!")
            return
            
        # Query initial status
        print("\n=== Querying initial state ===")
        await plug.query_status()
        
        # Turn ON
        print("\n=== Toggle ON ===")
        await plug.toggle_plug(True)
        await asyncio.sleep(2)
        await plug.query_status()
        
        # Turn OFF
        print("\n=== Toggle OFF ===")
        await plug.toggle_plug(False)
        await asyncio.sleep(2)
        await plug.query_status()
        
        print("\nDone!")
    finally:
        await plug.disconnect()

if __name__ == '__main__':
    asyncio.run(main())