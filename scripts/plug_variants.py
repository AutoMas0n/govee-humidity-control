#!/usr/bin/env python3
"""Try H5080-specific command variants."""
import asyncio
from bleak import BleakClient, BleakScanner

PLUG = "60:74:F4:BD:4D:E5"
WRITE_CHAR = "00010203-0405-0607-0809-0a0b0c0d2b11"

def make_frame(cmd, sub):
    f = bytearray(20)
    f[0] = 0x33
    f[1] = cmd
    f[2] = sub
    cs = 0x33 ^ cmd ^ sub
    f[19] = cs
    return bytes(f)

async def main():
    print(f"Finding {PLUG}...")
    dev = await BleakScanner.find_device_by_address(PLUG, timeout=10)
    print(f"Found: {dev.name}") if dev else print("Not found")
    
    async with BleakClient(PLUG, timeout=20) as client:
        print("Connected!")
        
        # Try cmd=0x11 (AWS power value 17) with sub=0x01/0x00
        cmds = [
            ("AWS cmd=0x11 ON", make_frame(0x11, 0x01)),
            ("AWS cmd=0x11 OFF", make_frame(0x11, 0x00)),
            ("Standard ON", make_frame(0x01, 0x01)),
            ("Standard OFF", make_frame(0x01, 0x00)),
            # Try cmd=0x01 with sub=0x11/0x10 (AWS values)
            ("Cmd=0x01 sub=0x11", make_frame(0x01, 0x11)),
            ("Cmd=0x01 sub=0x10", make_frame(0x01, 0x10)),
            # Try cmd=0x05 color mode with RGB
            ("Cmd=0x05 RGB BLACK", bytes.fromhex("3305150200000000000000000000000000000031")),
            ("Cmd=0x05 RGB WHITE", bytes.fromhex("33051502ffffffffffffffffffffffffff00000063")),
        ]
        
        for name, data in cmds:
            print(f"\n--- {name} ---")
            print(f"  Frame: {data.hex()}")
            await client.write_gatt_char(WRITE_CHAR, data, response=False)
            print(f"  Written")
            await asyncio.sleep(2)
        
        # Check mfr data after all writes
        print("\nScanning for state change...")
        state = {}
        def cb(d, a):
            if d.address.upper() == PLUG:
                for k, v in a.manufacturer_data.items():
                    state["mfr"] = v.hex()
        sc = BleakScanner(detection_callback=cb, scanning_mode="active")
        await sc.start()
        await asyncio.sleep(6)
        await sc.stop()
        m = state.get("mfr", "none")
        s = "ON" if m.endswith("01") else "OFF" if m.endswith("00") else m
        print(f"  Final state: {m} => {s}")

asyncio.run(main())