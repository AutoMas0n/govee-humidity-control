#!/usr/bin/env python3
"""
Extract the per-device secret key from a Govee H5080 smart plug.
Required for newer firmware variants that gate BLE toggle behind a secret key.

Usage:
  python3 get_skey.py <MAC> [--timeout <secs>]

Example:
  python3 get_skey.py D4:AD:FC:42:E2:45
"""
import asyncio, sys
from bleak import BleakClient
from Cryptodome.Cipher import AES

KEY_COMM = b"MakingLifeSmarte"
WRITE = "00010203-0405-0607-0809-0a0b0c0d2b11"
NOTIFY = "00010203-0405-0607-0809-0a0b0c0d2b10"

def rc4(d, k):
    S = list(range(256)); j = 0
    for i in range(256): j = (j + S[i] + k[i % len(k)]) & 255; S[i], S[j] = S[j], S[i]
    i = j = 0; o = bytearray(len(d))
    for n in range(len(d)):
        i = (i + 1) & 255; j = (j + S[i]) & 255
        S[i], S[j] = S[j], S[i]
        o[n] = d[n] ^ S[(S[i] + S[j]) & 255]
    return bytes(o)

def enc(f, k):
    return AES.new(k, AES.MODE_ECB).encrypt(f[:16]) + rc4(f[16:20], k)

def dec(p, k):
    return AES.new(k, AES.MODE_ECB).decrypt(p[:16]) + rc4(p[16:20], k)

def frm(c, s, d=()):
    f = bytearray(20); f[0] = c; f[1] = s
    if d: f[2:2+len(d)] = d
    cs = 0
    for b in f[:19]: cs ^= b
    f[19] = cs
    return bytes(f)

def vfy(f):
    cs = 0
    for b in f[:19]: cs ^= b
    return cs == f[19]

ns = []
def cb(sc, d):
    ns.append(bytes(d))

async def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <MAC>")
        sys.exit(1)
    mac = sys.argv[1]
    
    c = BleakClient(mac)
    await c.connect()
    print(f"[+] Connected to {mac}")
    await c.start_notify(NOTIFY, cb)
    
    # Handshake
    ns.clear()
    await c.write_gatt_char(WRITE, enc(frm(0xE7, 0x01, bytes(16)), KEY_COMM), False)
    await asyncio.sleep(2)
    sk = None
    for n in ns:
        d = dec(n, KEY_COMM)
        if d[0] == 0xE7 and d[1] == 0x01 and vfy(d):
            sk = bytes(d[2:18])
    if not sk:
        print("[-] Handshake failed")
        await c.disconnect()
        sys.exit(1)
    ns.clear()
    await c.write_gatt_char(WRITE, enc(frm(0xE7, 0x02, bytes(16)), KEY_COMM), False)
    await asyncio.sleep(0.5)
    print(f"[+] Session key: {sk.hex()}")
    
    # Read secret key with AA B1
    ns.clear()
    await c.write_gatt_char(WRITE, enc(frm(0xAA, 0xB1, ()), sk), False)
    await asyncio.sleep(1)
    print(f"[+] AA B1 response: {len(ns)} notifications")
    
    found = False
    for n in ns:
        d = dec(n, sk)
        print(f"    {d.hex()} v={vfy(d)}")
        if d[0] == 0xAA and d[1] == 0xB1 and vfy(d):
            key_data = bytes(d[2:19])  # up to 17 bytes
            nz = [b for b in key_data if b != 0]
            if nz:
                hex_key = bytes(nz).hex()
                print(f"\n[+] SECRET KEY: {hex_key} ({len(nz)} bytes)")
                print(f"\nCopy this to clipboard:")
                print(f"  --skey {hex_key}")
                found = True
    
    if not found:
        print("[-] Could not read secret key. Plug may use V1 firmware.")
        print("    Try toggling without --skey (might work on older firmware).")
    
    await c.disconnect()

asyncio.run(main())