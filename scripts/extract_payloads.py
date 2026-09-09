#!/usr/bin/env python3
"""Extract and save all captured H5080 BLE traffic for analysis."""
import struct

infile = '/data/data/com.termux/files/usr/tmp/btsnoop/btsnoop_hci2.log'
outfile = '/data/data/com.termux/files/home/github/govee-humidity-control/captured_payloads.txt'

with open(infile, 'rb') as f:
    data = f.read()

pos = 16
writes = []
while pos < len(data):
    if pos + 24 > len(data): break
    ol, il, fl, dr, tshi, tslo = struct.unpack('>IIIIII', data[pos:pos+24])
    ts = (tshi << 32) | tslo
    pos += 24
    if pos + il > len(data): break
    pkt = data[pos:pos+il]
    pos += il
    if len(pkt) < 1 or pkt[0] != 0x02: continue
    if len(pkt) >= 10:
        cid = struct.unpack('<H', pkt[7:9])[0]
        if cid != 4 or len(pkt) < 10: continue
        att = pkt[9:]
        op = att[0]
        if op == 0x52:  # Write Command
            ah = struct.unpack('<H', att[1:3])[0]
            pl = att[3:]
            if ah == 0x0011:
                writes.append((ts, len(pl), pl.hex()))

with open(outfile, 'w') as f:
    f.write("H5080 BLE Captured Payloads (handle 0x0011)\n")
    f.write("=" * 60 + "\n\n")
    
    t0 = writes[0][0]
    # Group by phase
    phase1 = [(ts, plen, pl) for ts, plen, pl in writes if (ts - t0) < 20000000]  # first 20 seconds
    phase2 = [(ts, plen, pl) for ts, plen, pl in writes if (ts - t0) >= 20000000]  # rest
    
    f.write("PHASE 1: Initial connection / pairing sequence\n")
    f.write("-" * 50 + "\n")
    write_num = 0
    for ts, plen, pl in phase1:
        dt = (ts - t0) / 1000000
        write_num += 1
        first12 = pl[:24]
        last8 = pl[-16:]
        f.write(f"  [{write_num:>2}] +{dt:>6.1f}s ({plen}B) {pl}\n")
        f.write(f"        first12: {first12}\n")
        f.write(f"        last8:   {last8}\n")
    
    t2 = phase2[0][0] if phase2 else 0
    f.write(f"\nPHASE 2: User toggle commands (after {(t2-t0)/1000000:.0f}s)\n")
    f.write("-" * 50 + "\n")
    for ts, plen, pl in phase2:
        dt = (ts - t2) / 1000000
        write_num += 1
        first12 = pl[:24]
        last8 = pl[-16:]
        f.write(f"  [{write_num:>2}] +{dt:>6.1f}s ({plen}B) {pl}\n")
        f.write(f"        first12: {first12}\n")
        f.write(f"        last8:   {last8}\n")
    
    # Find repeated patterns
    f.write("\nREPEATED PATTERNS\n")
    from collections import Counter
    all_pls = [pl for _, _, pl in writes]
    counts = Counter(all_pls)
    for pl, count in counts.most_common():
        if count > 1:
            f.write(f"  count={count}: {pl}\n")

print(f"Saved to {outfile}")
print(f"Total writes: {len(writes)}")
print(f"Phase 1: {len(phase1)} writes")
print(f"Phase 2: {len(phase2)} writes")