#!/usr/bin/env python3
"""Decode every H5080 BLE session in a btsnoop log: writes AND notifications, decrypted.

Usage: decode_sessions.py <btsnoop_hci.log> [--filter AAB1] [--all]
"""
import struct, sys
try:
    from Crypto.Cipher import AES
except ImportError:
    from Cryptodome.Cipher import AES

KEY_COMM = b"MakingLifeSmarte"

def rc4(d, k):
    S = list(range(256)); j = 0
    for i in range(256):
        j = (j + S[i] + k[i % len(k)]) & 255; S[i], S[j] = S[j], S[i]
    i = j = 0; o = bytearray(len(d))
    for n in range(len(d)):
        i = (i + 1) & 255; j = (j + S[i]) & 255; S[i], S[j] = S[j], S[i]
        o[n] = d[n] ^ S[(S[i] + S[j]) & 255]
    return bytes(o)

def dec(p, k):
    return AES.new(k, AES.MODE_ECB).decrypt(p[:16]) + rc4(p[16:20], k)

def vfy(f):
    c = 0
    for b in f[:19]: c ^= b
    return c == f[19]

def parse(path):
    data = open(path, "rb").read()
    pos = 16; out = []
    while pos + 24 <= len(data):
        ol, il, fl, dr, tshi, tslo = struct.unpack(">IIIIII", data[pos:pos+24])
        ts = (tshi << 32) | tslo; pos += 24
        if pos + il > len(data): break
        pkt = data[pos:pos+il]; pos += il
        # LE Connection Complete (0x01) / Enhanced (0x0A): map conn handle -> peer MAC
        if len(pkt) >= 15 and pkt[0] == 0x04 and pkt[1] == 0x3E and pkt[3] in (0x01, 0x0A) and pkt[4] == 0:
            ch = struct.unpack("<H", pkt[5:7])[0] & 0x0fff
            mac = ":".join(f"{b:02X}" for b in reversed(pkt[9:15]))
            out.append((ts, ch, "C", 0, mac))
            continue
        if len(pkt) < 10 or pkt[0] != 0x02: continue
        hci_h = struct.unpack("<H", pkt[1:3])[0] & 0x0fff
        cid = struct.unpack("<H", pkt[7:9])[0]
        if cid != 4: continue
        att = pkt[9:]
        op = att[0]
        if op in (0x52, 0x12) and len(att) >= 3:        # write cmd / write req
            out.append((ts, hci_h, "W", struct.unpack("<H", att[1:3])[0], att[3:]))
        elif op == 0x1B and len(att) >= 3:               # notification
            out.append((ts, hci_h, "N", struct.unpack("<H", att[1:3])[0], att[3:]))
    return out

def main():
    path = sys.argv[1]
    filt = None
    if "--filter" in sys.argv:
        filt = bytes.fromhex(sys.argv[sys.argv.index("--filter") + 1])
    show_all = "--all" in sys.argv
    pkts = parse(path)
    sk = None; sess = 0; t0 = None; macs = {}
    for ts, h, d, ah, pl in pkts:
        if d == "C":
            macs[h] = pl; continue
        if len(pl) != 20: continue
        f = dec(pl, KEY_COMM)
        if f[0] == 0xE7 and vfy(f):
            if f[1] == 0x01 and d == "N":
                sk = bytes(f[2:18]); sess += 1; t0 = ts
                print(f"\n=== Session {sess} (conn 0x{h:03x} {macs.get(h,'?')}) SK={sk.hex()}")
            continue
        if sk is None: continue
        f = dec(pl, sk)
        ok = vfy(f)
        if filt and f[:len(filt)] != filt: 
            if not show_all: continue
        arrow = "->" if d == "W" else "<-"
        dt = (ts - t0) / 1e6 if t0 else 0
        print(f"  {dt:8.3f}s {arrow} h{ah:04x} {f[:2].hex()} {f[2:19].hex()} {'ok' if ok else 'BADCS'}")

if __name__ == "__main__":
    main()
