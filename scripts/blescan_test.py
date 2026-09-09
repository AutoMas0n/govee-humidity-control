#!/usr/bin/env python3
"""Test BLE status query - if plug responds, BLE command channel works."""
import asyncio
from bleak import BleakClient, BleakScanner

PLUG = "60:74:F4:BD:4D:E5"
WRITE_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b10"

async def test():
    print(f"Connecting to {PLUG}...")
    async with BleakClient(PLUG, timeout=20) as client:
        print("Connected!")
        
        # Subscribe to notify
        notes = []
        def cb(s, d):
            notes.append(d.hex())
            print(f"  NOTIFY ({len(notes)}): {d.hex()}")
        
        await client.start_notify(NOTIFY_CHAR, cb)
        print("Notifications enabled")
        await asyncio.sleep(1)
        
        # Try status query: AA 01 with XOR checksum
        status_q = bytearray(20)
        status_q[0] = 0xaa
        status_q[1] = 0x01
        cs = 0xaa ^ 0x01
        status_q[19] = cs
        
        print(f"Writing status query: {bytes(status_q).hex()}")
        await client.write_gatt_char(WRITE_CHAR, bytes(status_q), response=False)
        await asyncio.sleep(5)
        
        if notes:
            print(f"\n*** Got notification! BLE command channel IS working! ***")
            print(f"Notifications: {notes}")
        else:
            print(f"\n*** No response to status query ***")
        
        # Try OFF command
        off = bytearray(20)
        off[0] = 0x33
        off[1] = 0x01
        off[2] = 0x00
        off[19] = 0x33 ^ 0x01 ^ 0x00
        print(f"\nWriting OFF: {bytes(off).hex()}")
        await client.write_gatt_char(WRITE_CHAR, bytes(off), response=False)
        await asyncio.sleep(3)
        
        # Try ON command
        on = bytearray(20)
        on[0] = 0x33
        on[1] = 0x01
        on[2] = 0x01
        on[19] = 0x33 ^ 0x01 ^ 0x01
        print(f"Writing ON: {bytes(on).hex()}")
        await client.write_gatt_char(WRITE_CHAR, bytes(on), response=False)
        await asyncio.sleep(3)
        
        print(f"\nTotal notifications after all writes: {notes}")
        await client.stop_notify(NOTIFY_CHAR)

asyncio.run(test())