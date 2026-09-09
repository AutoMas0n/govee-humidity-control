#!/usr/bin/env python3
"""Connect and write power command - with notifications to detect response."""
import asyncio
from bleak import BleakClient, BleakScanner

PLUG = "60:74:F4:BD:4D:E5"
WRITE = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY = "00010203-0405-0607-0809-0a0b0c0d2b10"

ON = bytes.fromhex("3301010000000000000000000000000000000033")
OFF = bytes.fromhex("3301000000000000000000000000000000000032")

async def main():
    notes = []
    def cb(s, d):
        notes.append(d)
        print(f"  NOTIFY ({len(notes)}): {d.hex()}")
    
    # First scan it
    print("Scanning for plug...")
    dev = await BleakScanner.find_device_by_address(PLUG, timeout=10)
    if not dev:
        print("Not found")
        return
    print(f"Found: {dev.name}")
    
    async with BleakClient(PLUG, timeout=20) as client:
        print("Connected!")
        await client.start_notify(NOTIFY, cb)
        print("Subscribed to notifications")
        await asyncio.sleep(1)
        
        # Send OFF
        print(f"\nSending OFF: {OFF.hex()}")
        await client.write_gatt_char(WRITE, OFF, response=False)
        print("Done")
        await asyncio.sleep(4)
        
        # Send ON
        print(f"Sending ON:  {ON.hex()}")
        await client.write_gatt_char(WRITE, ON, response=False)
        print("Done")
        await asyncio.sleep(4)
        
        # Send OFF again
        print(f"Sending OFF: {OFF.hex()}")
        await client.write_gatt_char(WRITE, OFF, response=False)
        print("Done")
        await asyncio.sleep(4)
        
        print(f"\nNotifications: {len(notes)}")
        for n in notes:
            print(f"  {n.hex()} ({len(n)} bytes)")
        
        await client.stop_notify(NOTIFY)

asyncio.run(main())