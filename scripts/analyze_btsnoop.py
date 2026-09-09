#!/usr/bin/env python3
"""Full analysis of btsnoop capture focusing on H5080 writes."""
import struct

infile = '/data/data/com.termux/files/usr/tmp/btsnoop/btsnoop_hci2.log'

with open(infile, 'rb') as f:
    data = f.read()

# First, find the LE Connection Complete for the H5080 (MAC 60:74:f4:bd:4d:e5)
# MediaTek uses sub=0x0d for extended advertising, sub=0x01 for connection
# Also look for sub=0x00 (LE Connection Complete in standard format)

pos = 16
connections = {}  # handle -> (mac, role, timestamp)
plug_mac_rev = bytes.fromhex('e54dbdf47460')  # reversed for btsnoop
plug_mac_fwd = bytes.fromhex('6074f4bd4de5')

# Find all connection events
while pos < len(data):
    if pos + 24 > len(data): break
    ol, il, fl, dr, tshi, tslo = struct.unpack('>IIIIII', data[pos:pos+24])
    ts = (tshi << 32) | tslo
    pos += 24
    if pos + il > len(data): break
    pkt = data[pos:pos+il]
    pos += il
    if len(pkt) < 1 or pkt[0] != 0x04 or pkt[1] != 0x3e: continue
    
    sub = pkt[3]
    if sub == 0x01 and len(pkt) >= 14:  # LE Connection Complete
        status = pkt[4]
        conn_h = struct.unpack('<H', pkt[5:7])[0]
        peer = ':'.join(f'{b:02x}' for b in pkt[9:15])
        if plug_mac_fwd.hex() in peer.replace(':', '') or \
           any(m in peer.lower() for m in ['4d:e5', 'e1:dd', 'e2:45']):
            connections[conn_h] = (peer, pkt[8], ts)
            print(f'CONN (sub=01) handle=0x{conn_h:04x} {peer} role={\"central\" if pkt[8]==0 else \"peripheral\"} ts={ts}')

# Now find the encrypted write commands and map to connections
# First find all ATT writes to handle 0x0011 with timestamps
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
        hci_h = struct.unpack('<H', pkt[1:3])[0]
        l2len = struct.unpack('<H', pkt[5:7])[0]
        cid = struct.unpack('<H', pkt[7:9])[0]
        if cid != 4 or len(pkt) < 10: continue
        att = pkt[9:]
        op = att[0]
        if op in (0x52, 0x16):
            ah = struct.unpack('<H', att[1:3])[0]
            pl = att[3:]
            if ah == 0x0011 or ah == 0x0010:
                writes.append((ts, op, ah, pl.hex(), len(pl), hci_h))

# Also find all LE Connection Update events (0x03) to map handle->peer
pos = 16
conn_updates = {}
while pos < len(data):
    if pos + 24 > len(data): break
    ol, il, fl, dr, tshi, tslo = struct.unpack('>IIIIII', data[pos:pos+24])
    pos += 24
    if pos + il > len(data): break
    pkt = data[pos:pos+il]
    pos += il
    if len(pkt) < 1 or pkt[0] != 0x04 or pkt[1] != 0x3e: continue
    if pkt[3] == 0x03 and len(pkt) >= 12:
        handle = struct.unpack('<H', pkt[6:8])[0]
        if handle == 0x0200:
            sup_time = tshi  # unused
            if handle not in conn_updates:
                conn_updates[handle] = pkt.hex()

print(f'\n=== Connection updates for handle 0x0200 ===')
for h, hexdata in conn_updates.items():
    print(f'  {hexdata[:60]}')

# Check if handle 0x0200 maps to a connection by looking for LE Create Connection
pos = 16
print(f'\n=== Looking for HCI LE Create Connection command ===')
while pos < len(data):
    if pos + 24 > len(data): break
    ol, il, fl, dr, tshi, tslo = struct.unpack('>IIIIII', data[pos:pos+24])
    pos += 24
    if pos + il > len(data): break
    pkt = data[pos:pos+il]
    pos += il
    if len(pkt) < 1: continue
    # HCI Command packets: type 0x01
    if pkt[0] == 0x01 and len(pkt) >= 4:
        opcode = struct.unpack('<H', pkt[1:3])[0]
        if opcode == 0x200d:  # LE Create Connection
            peer = ':'.join(f'{b:02x}' for b in reversed(pkt[6:12])) if len(pkt) >= 12 else '?'
            if '4d:e5' in peer or 'e1:dd' in peer or 'e2:45' in peer:
                print(f'  LE Create Connection: peer={peer}')
        elif opcode == 0x200b:  # LE Set Scan Parameters
            pass  # too common
    
    # Also look for the ADV_DIRECT_IND or CONNECT_REQ in ACL (for phone as central)
    if pkt[0] == 0x02 and len(pkt) >= 14:
        # Check for LL_CONNECTION_IND in advertising PDU  
        pass

# Print the writes sorted by timestamp
print(f'\n=== All ATT writes to handle 0x0011 ({len(writes)}) ===')
for ts, op, ah, pl, plen, hci_h in sorted(writes, key=lambda x: x[0]):
    opname = 'CMD' if op == 0x52 else 'REQ'
    # Identify unique payloads
    marker = 'ON' if '3f7bc8' in pl else 'OFF' if '3f7bc' in pl else 'other' if len(pl) > 20 else 'short'
    print(f'  ts={ts} [{opname}] h=0x{ah:04x} ({plen}b) {pl[:60]} [{marker}]')

# Group by unique payload patterns
print(f'\n=== Unique payload patterns ===')
unique = {}
for ts, op, ah, pl, plen, hci_h in sorted(writes, key=lambda x: x[0]):
    if pl not in unique:
        unique[pl] = {'count': 0, 'ts': ts, 'op': op}
    unique[pl]['count'] += 1

for pl, info in sorted(unique.items(), key=lambda x: x[1]['ts']):
    m = 'ON' if '3f7bc8' in pl else 'OFF' if '3f7bc' in pl else 'other' if len(pl) > 10 else 'short'
    print(f'  [{m}] ts={info["ts"]} count={info["count"]} pl={pl[:60]}')