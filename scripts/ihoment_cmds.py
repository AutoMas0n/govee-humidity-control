#!/usr/bin/env python3
"""
Try the ihoment-specific protocol.
The H5080 is branded ihoment/IntelliRocks, not standard Govee.
Maybe the commands are different.
"""
import asyncio
from bleak import BleakClient, BleakScanner

PLUG = "60:74:F4:BD:4D:E5"
WRITE = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY = "00010203-0405-0607-0809-0a0b0c0d2b10"

def make_20(cmd, sub):
    """Standard 20-byte Govee frame with XOR checksum."""
    f = bytearray(20)
    f[0] = cmd
    f[1] = sub
    # For multi-byte payloads
    cs = cmd ^ sub
    for i in range(2, 19):
        cs ^= f[i]
    f[19] = cs
    return bytes(f)

def make_std(on):
    f = bytearray(20)
    f[0] = 0x33; f[1] = 0x01; f[2] = 0x01 if on else 0x00
    f[19] = 0x33 ^ f[1] ^ f[2]
    return bytes(f)

async def main():
    # Connect without subscribing - just blast commands and check state after
    for attempt, (name, data) in enumerate([
        # Standard Govee
        ("33 01 00 OFF", make_std(False)),
        ("33 01 01 ON ", make_std(True)),
        # Try with different header bytes
        ("AA 01 00 OFF", make_20(0xaa, 0x00)),
        ("EE 01 01 ON ", make_20(0xee, 0x01)),
        # Try ihoment commands (manufacturer data style)
        ("EC 00 00 OFF", bytes.fromhex("ec00020000000000000000000000000000000000ec")),
        ("EC 00 01 ON ", bytes.fromhex("ec000201010000000000000000000000000000ec")),
    ]):
        print(f"\n[{attempt}] {name}: {data.hex()}")
        async with BleakClient(PLUG, timeout=15) as client:
            await client.write_gatt_char(WRITE, data, response=False)
            print(f"  Written")
        await asyncio.sleep(3)
    
    # Check state via scan
    print("\n=== Checking final state via scan ===")
    samples = []
    def cb(d, a):
        if d.address.upper() == PLUG:
            for k, v in a.manufacturer_data.items():
                samples.append(f"0x{k:04x}:{v.hex()}")
    sc = BleakScanner(detection_callback=cb, scanning_mode="active")
    await sc.start()
    await asyncio.sleep(8)
    await sc.stop()
    for s in samples:
        print(f"  {s}")

asyncio.run(main())