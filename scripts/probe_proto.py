#!/usr/bin/env python3
"""Analyze H5080 BLE protocol - notifications suggest V2 encryption."""
import asyncio
from bleak import BleakClient

PLUG = "60:74:F4:BD:4D:E5"
WRITE_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b10"

async def main():
    print("=== H5080 BLE Protocol Probe ===")
    notes = []
    def cb(s, d):
        notes.append(d)
        print(f"  NOTIFY ({len(notes)}): len={len(d)} hex={d.hex()}")
    
    async with BleakClient(PLUG, timeout=20) as client:
        print("Connected! Subscribing...")
        await client.start_notify(NOTIFY_CHAR, cb)
        await asyncio.sleep(1)
        
        # 1. Try handshake (e7 11 01)
        print("\n1. Sending V2 handshake (e7 11 01)...")
        hs = bytearray(36)
        hs[0] = 0xe7; hs[1] = 0x11; hs[2] = 0x01
        import os; iv = os.urandom(12)
        hs[3:15] = iv; hs[15] = 12
        await client.write_gatt_char(WRITE_CHAR, bytes(hs), response=False)
        await asyncio.sleep(5)
        print(f"   Notifications now: {len(notes)}")
        
        notes.clear()
        
        # 2. Try writing to the notify char directly (write-alt)
        print("\n2. Try writing OFF to notify char instead...")
        off = bytes.fromhex("3301000000000000000000000000000000000032")
        try:
            await client.write_gatt_char(NOTIFY_CHAR, off, response=False)
            print("   Written to NOTIFY char")
        except Exception as e:
            print(f"   Error: {str(e)[:60]}")
        await asyncio.sleep(3)
        print(f"   Notifications: {len(notes)}")
        
        notes.clear()
        
        # 3. Standard OFF to write char
        print("\n3. OFF to write char...")
        await client.write_gatt_char(WRITE_CHAR, off, response=False)
        await asyncio.sleep(3)
        
        # 4. Standard ON to write char
        on = bytes.fromhex("3301010000000000000000000000000000000033")
        print("4. ON to write char...")
        await client.write_gatt_char(WRITE_CHAR, on, response=False)
        await asyncio.sleep(3)
        print(f"   Notifications: {len(notes)}")
        for i, n in enumerate(notes):
            print(f"     [{i}] {n.hex()} ({len(n)} bytes)")
        
        await client.stop_notify(NOTIFY_CHAR)

asyncio.run(main())