#!/usr/bin/env python3
"""Test: write command to H5080, then scan to see if mfr data changed."""
import asyncio, sys
from bleak import BleakScanner, BleakClient

H5080_MAC = "D4:AD:FC:41:E1:DD"
WRITE_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b11"
POWER_OFF = bytes.fromhex("3301000000000000000000000000000000000034")
POWER_ON  = bytes.fromhex("3301010000000000000000000000000000000035")

sdata = {}

def cb(device, adv):
    if device.address.upper() == H5080_MAC:
        sdata["mfr"] = {k: v.hex() for k, v in adv.manufacturer_data.items()}
        sdata["rssi"] = adv.rssi

async def scan():
    sdata.clear()
    scanner = BleakScanner(detection_callback=cb, scanning_mode="active")
    await scanner.start()
    await asyncio.sleep(8)
    await scanner.stop()
    return dict(sdata)

async def write_state(want_on):
    async with BleakClient(H5080_MAC, timeout=15) as client:
        cmd = POWER_ON if want_on else POWER_OFF
        await client.write_gatt_char(WRITE_CHAR, cmd, response=False)

async def main():
    r = await scan()
    print("BASELINE:", r)
    
    await write_state(False)
    print("Wrote OFF")
    await asyncio.sleep(3)
    
    r = await scan()
    print("AFTER OFF:", r)
    
    await write_state(True)
    print("Wrote ON")
    await asyncio.sleep(3)
    
    r = await scan()
    print("AFTER ON:", r)

asyncio.run(main())