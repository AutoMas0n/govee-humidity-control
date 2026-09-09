#!/usr/bin/env python3
import asyncio, os
from bleak import BleakClient

PLUG = "60:74:F4:BD:4D:E5"
WRITE_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b10"

async def main():
    print(f"Connecting to {PLUG}...")
    async with BleakClient(PLUG, timeout=20) as client:
        print("Connected. Subscribing to notifications...")
        notifications = []
        def ncb(s, d):
            notifications.append(d.hex())
            print(f"  NOTIFY: {d.hex()}")
        
        await client.start_notify(NOTIFY_CHAR, ncb)
        await asyncio.sleep(1)
        
        # Try various handshake-like frames
        tests = [
            ("e7 11 01 + 33 zeros", bytes([0xe7, 0x11, 0x01] + [0]*33)),
            ("e7 11 00 + 33 zeros", bytes([0xe7, 0x11, 0x00] + [0]*33)),
            ("aa 01 query", bytes.fromhex("aa010000000000000000000000000000000000ab")),
            ("33 01 00 OFF", bytes.fromhex("3301000000000000000000000000000000000032")),
            ("33 30 00 zone OFF", bytes.fromhex("3330000000000000000000000000000000000003")),
            ("33 30 01 zone ON", bytes.fromhex("3330010000000000000000000000000000000002")),
        ]
        
        for name, data in tests:
            print(f"\n--- {name} ---")
            print(f"  Writing: {data.hex()}")
            try:
                await client.write_gatt_char(WRITE_CHAR, data, response=False)
                print(f"  Written OK")
            except Exception as e:
                print(f"  Write error: {str(e)[:60]}")
            await asyncio.sleep(3)
            print(f"  Notifications: {notifications}")
        
        print(f"\nAll notifications: {notifications}")
        await client.stop_notify(NOTIFY_CHAR)

asyncio.run(main())