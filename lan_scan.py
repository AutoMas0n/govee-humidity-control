#!/usr/bin/env python3
"""Scan for Govee LAN devices on UDP port 4002-4003."""
import asyncio, socket, struct, time

async def scan_lan():
    print("Scanning for Govee LAN devices (UDP 4002-4003)...")
    
    # Create UDP socket for broadcast
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    sock.settimeout(3)
    
    # Govee LAN discovery message
    # Scan message: 0x68656c6c6f (="hello") followed by 0x00 padding to 20 bytes
    scan_msg = bytes.fromhex("68656c6c6f00000000000000000000000000000000")
    
    responses = []
    
    # Broadcast on port 4002 and 4003
    for port in [4002, 4003, 4001]:
        try:
            sock.sendto(scan_msg, ("255.255.255.255", port))
            sock.sendto(scan_msg, ("192.168.2.255", port))
        except:
            pass
    
    start = time.time()
    while time.time() - start < 4:
        try:
            data, addr = sock.recvfrom(1024)
            responses.append((addr, data.hex()))
            print(f"  Response from {addr}: {data.hex()}")
        except socket.timeout:
            break
    
    sock.close()
    
    if not responses:
        print("  No LAN devices found. Checking all network interfaces...")
        # Try scanning subnet systematically
        for ip in [f"192.168.2.{i}" for i in range(1, 255)]:
            try:
                s2 = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                s2.settimeout(0.5)
                s2.sendto(scan_msg + bytes([0x00] * 20), (ip, 4002))
                try:
                    data, addr = s2.recvfrom(1024)
                    print(f"  Response from {addr}: {data.hex()}")
                except socket.timeout:
                    pass
                s2.close()
            except:
                pass
    
    return responses

async def main():
    r = await scan_lan()
    if r:
        print(f"\nFound {len(r)} LAN devices!")
    else:
        print("\nNo LAN devices found.")

asyncio.run(main())