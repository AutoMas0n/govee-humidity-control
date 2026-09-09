#!/usr/bin/env python3
"""Try AWS power values (17/16) as BLE commands."""
import asyncio
from bleak import BleakClient, BleakScanner

PLUG = "60:74:F4:BD:4D:E5"
WRITE = "00010203-0405-0607-0809-0a0b0c0d2b11"

def make_20(cmd, sub):
    """Standard 20-byte Govee frame with XOR checksum."""
    f = bytearray(20)
    f[0] = cmd
    f[1] = sub
    cs = cmd ^ sub
    for i in range(2, 19):
        cs ^= f[i]
    f[19] = cs
    return bytes(f)

async def main():
    print(f"Finding {PLUG}...")
    dev = await BleakScanner.find_device_by_address(PLUG, timeout=10)
    if not dev:
        print("Not found")
        return
    print(f"Found: {dev.name}")
    
    async with BleakClient(PLUG, timeout=20) as client:
        print("Connected!")
        
        # Try AWS values: cmd=0x01, sub=0x11 (17) for ON, sub=0x10 (16) for OFF
        cmds = [
            ("Cmd=0x01 Sub=0x11 ON", make_20(0x01, 0x11)),
            ("Cmd=0x01 Sub=0x10 OFF", make_20(0x01, 0x10)),
            # Try cmd=0x11 with sub=0x01/0x00
            ("Cmd=0x11 Sub=0x01 ON", make_20(0x11, 0x01)),
            ("Cmd=0x11 Sub=0x00 OFF", make_20(0x11, 0x00)),
            # Standard
            ("Cmd=0x01 Sub=0x01 ON std", make_20(0x01, 0x01)),
            ("Cmd=0x01 Sub=0x00 OFF std", make_20(0x01, 0x00)),
        ]
        
        mfr_before = []
        def cb(d, a):
            if d.address.upper() == PLUG:
                for k, v in a.manufacturer_data.items():
                    mfr_before.append(f"0x{k:04x}:{v.hex()}")
        
        sc = BleakScanner(detection_callback=cb, scanning_mode="active")
        sc.start()
        
        # Give time for mfr capture
        await asyncio.sleep(5)
        
        for name, data in cmds:
            print(f"\n  {name}: {data.hex()}")
            await client.write_gatt_char(WRITE, data, response=False)
            await asyncio.sleep(2)
        
        # Final mfr
        await asyncio.sleep(5)
        await sc.stop()
        
        print(f"\nMfr samples: {mfr_before[:3]}")

asyncio.run(main())