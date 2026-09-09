#!/usr/bin/env python3
"""Parse btsnoop_hci.log for BLE ATT Write commands to H5080."""
import struct, sys

infile = "/data/data/com.termux/files/usr/tmp/btsnoop/btsnoop_hci.log"

# H5080 MAC (from our investigation)
TARGET_MACS = ["60:74:f4:bd:4d:e5", "d4:ad:fc:41:e1:dd", "d4:ad:fc:42:e2:45"]

with open(infile, "rb") as f:
    data = f.read()

# BTSnoop header: "btsnoop" + version fields
# Packet record: original_len(4) included_len(4) flags(4) drops(4) timestamp(8) then data

pos = 16  # Skip header
packet_num = 0
gatt_writes = []

while pos < len(data):
    if pos + 24 > len(data):
        break
    orig_len, incl_len, flags, drops, ts_hi, ts_lo = struct.unpack(">IIIIII", data[pos:pos+24])
    ts = (ts_hi << 32) | ts_lo
    pos += 24
    
    if pos + incl_len > len(data):
        break
    
    pkt = data[pos:pos+incl_len]
    pos += incl_len
    packet_num += 1
    
    # HCI ACL data packets contain L2CAP + ATT
    # Flags: 0x02 = HCI Event, 0x03 = HCI ACL Data
    pkt_type = flags & 0x03
    
    if pkt_type == 0x02:  # HCI Event
        if len(pkt) >= 7:
            event_code = pkt[0]
            # LE Meta Event (0x3e) can have advertising reports
            if event_code == 0x3e and len(pkt) >= 5:
                subevent = pkt[3]
                # Check for LE Advertising Report (0x02)
                if subevent == 0x02 and len(pkt) >= 7:
                    addr_bytes = pkt[5:11] if len(pkt) >= 11 else b""
                    if len(addr_bytes) == 6:
                        addr = ":".join(f"{b:02x}" for b in reversed(addr_bytes))
                        # Only show our target plugs
                        if addr.lower() in TARGET_MACS:
                            print(f"ADV [{addr}] pkt#{packet_num}")
                            
    elif pkt_type == 0x03:  # HCI ACL Data
        if len(pkt) >= 8:
            # ACL handle + PB flags at bytes 0-1
            # L2CAP header at bytes 4-7: len(2) cid(2)
            l2cap_len = struct.unpack("<H", pkt[4:6])[0]
            cid = struct.unpack("<H", pkt[6:8])[0]
            
            if cid == 0x0004:  # ATT protocol channel
                att_data = pkt[8:]
                if len(att_data) >= 1:
                    att_opcode = att_data[0]
                    # ATT Write Command (0x52) or ATT Write Request (0x12)
                    if att_opcode in (0x52, 0x12):
                        handle = struct.unpack("<H", att_data[1:3])[0]
                        payload = att_data[3:]
                        gatt_writes.append((packet_num, att_opcode, handle, payload.hex(), ts))
                        print(f"  WRITE pkt#{packet_num} opcode=0x{att_opcode:02x} handle=0x{handle:04x} payload={payload.hex()}")

print(f"\n=== Summary ===")
print(f"Total packets: {packet_num}")
print(f"GATT writes found: {len(gatt_writes)}")
for pn, op, h, pl, ts in gatt_writes:
    desc = "CMD" if op == 0x52 else "REQ"
    print(f"  pkt#{pn} [{desc}] handle=0x{h:04x} payload={pl}")