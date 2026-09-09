#!/usr/bin/env python3
"""Try ON/OFF with notifications. Analyze 20-byte response frames."""
import asyncio
from bleak import BleakClient, BleakScanner

PLUG = "60:74:F4:BD:4D:E5"
WRITE = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY = "00010203-0405-0607-0809-0a0b0c0d2b10"

def frame(on):
    f = bytearray(20)
    f[0] = 0x33; f[1] = 0x01; f[2] = 0x01 if on else 0x00
    f[19] = 0x33 ^ f[1] ^ f[2]
    return bytes(f)

async def main():
    notes = []
    def cb(s, d):
        notes.append(d)
        print(f"  NOTIFY ({len(notes)}): {d.hex()}")
    
    async with BleakClient(PLUG, timeout=20) as client:
        print("Connected! Subscribing...")
        await client.start_notify(NOTIFY, cb)
        await asyncio.sleep(1)
        
        # Write OFF with notification subscription active
        off = frame(False)
        print(f"\nOFF: {off.hex()}")
        await client.write_gatt_char(WRITE, off, response=False)
        await asyncio.sleep(3)
        
        on = frame(True)
        print(f"ON:  {on.hex()}")
        await client.write_gatt_char(WRITE, on, response=False)
        await asyncio.sleep(3)
        
        off2 = frame(False)
        print(f"OFF: {off2.hex()}")
        await client.write_gatt_char(WRITE, off2, response=False)
        await asyncio.sleep(3)
        
        on2 = frame(True)
        print(f"ON:  {on2.hex()}")
        await client.write_gatt_char(WRITE, on2, response=False)
        await asyncio.sleep(3)
        
        print(f"\n=== Total notifications: {len(notes)} ===")
        for i, n in enumerate(notes):
            print(f"  [{i}] {n.hex()}")
        
        await client.stop_notify(NOTIFY)

asyncio.run(main())