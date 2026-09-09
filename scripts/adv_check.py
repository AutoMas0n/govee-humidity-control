#!/usr/bin/env python3
"""Send OFF, then read plug state from advertisement."""
import asyncio
from bleak import BleakClient, BleakScanner

PLUG = "60:74:F4:BD:4D:E5"
WRITE = "00010203-0405-0607-0809-0a0b0c0d2b11"

def frame(on):
    f = bytearray(20)
    f[0] = 0x33; f[1] = 0x01; f[2] = 0x01 if on else 0x00
    f[19] = 0x33 ^ f[1] ^ f[2]
    return bytes(f)

samples = []

async def scan_plug(desc):
    samples.clear()
    def cb(d, a):
        if d.address.upper() == PLUG:
            mfr = a.manufacturer_data
            if mfr:
                for k, v in mfr.items():
                    samples.append(f"0x{k:04x}:{v.hex()}")
    sc = BleakScanner(detection_callback=cb, scanning_mode="active")
    await sc.start()
    await asyncio.sleep(6)
    await sc.stop()
    print(f"  {desc}: {samples}")

async def main():
    # Check baseline state
    await scan_plug("BEFORE")
    
    print("Writing OFF...")
    try:
        async with BleakClient(PLUG, timeout=15) as client:
            await client.write_gatt_char(WRITE, frame(True), response=False)
        print("  ON written (to wake)")
        await asyncio.sleep(3)
        async with BleakClient(PLUG, timeout=15) as client:
            await client.write_gatt_char(WRITE, frame(False), response=False)
        print("  OFF written")
    except Exception as e:
        print(f"  Write error: {str(e)[:80]}")
    
    await asyncio.sleep(3)
    await scan_plug("AFTER WRITE")
    
    # Write ON
    print("Writing ON...")
    async with BleakClient(PLUG, timeout=15) as client:
        await client.write_gatt_char(WRITE, frame(True), response=False)
    print("  ON written")
    await asyncio.sleep(3)
    await scan_plug("AFTER ON")

asyncio.run(main())