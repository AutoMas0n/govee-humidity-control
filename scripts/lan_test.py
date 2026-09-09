#!/usr/bin/env python3
"""Test Govee LAN protocol for local UDP control."""
import socket, time, struct, random

# Govee LAN protocol messages
# Device discovery: broadcast "hello" on port 4002/4003
# Device responds with SKU, MAC, IP

def send_lan_command(ip, port=4002, data=None):
    """Send a raw UDP packet to a Govee device."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.settimeout(3)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    
    if data:
        s.sendto(data, (ip, port))
    
    try:
        resp, addr = s.recvfrom(1024)
        return resp, addr
    except socket.timeout:
        return None, None
    finally:
        s.close()

def scan_lan():
    """Broadcast scan for Govee devices."""
    print("Scanning for Govee devices on LAN...")
    
    # Scan message
    scan_msg = bytes.fromhex("68656c6c6f")  # "hello"
    
    # Ports Govee uses
    ports = [4001, 4002, 4003, 4004]
    
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(2)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    
    devices = []
    
    for port in ports:
        try:
            sock.sendto(scan_msg, ("255.255.255.255", port))
            sock.sendto(scan_msg, ("192.168.2.255", port))
        except:
            pass
    
    start = time.time()
    while time.time() - start < 5:
        try:
            data, addr = sock.recvfrom(1024)
            print(f"  Response from {addr[0]}:{addr[1]}: {data.hex()}")
            devices.append((addr, data))
        except socket.timeout:
            if time.time() - start > 5:
                break
    
    sock.close()
    return devices

def try_govee_cmds(ip):
    """Try various Govee LAN commands."""
    print(f"\nTrying LAN commands on {ip}...")
    
    # Common Govee LAN command formats
    cmds = [
        # Govee LAN commands: 33 01 00/01 for power
        ("Power OFF", bytes.fromhex("3301000000000000000000000000000000000032"), 4002),
        ("Power ON",  bytes.fromhex("3301010000000000000000000000000000000033"), 4002),
        # With LAN header
        ("LAN Power OFF", bytes.fromhex("68656c6c6f") + bytes.fromhex("3301000000000000000000000000000000000032"), 4002),
        ("LAN Power ON",  bytes.fromhex("68656c6c6f") + bytes.fromhex("3301010000000000000000000000000000000033"), 4002),
    ]
    
    for name, data, port in cmds:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.settimeout(2)
        try:
            s.sendto(data, (ip, port))
            try:
                resp, addr = s.recvfrom(1024)
                print(f"  [{name}] Response: {resp.hex()} from {addr}")
            except socket.timeout:
                print(f"  [{name}] No response")
        except Exception as e:
            print(f"  [{name}] Error: {e}")
        s.close()
        time.sleep(0.5)

devices = scan_lan()

if devices:
    for (addr, data), resp in zip(devices, devices):
        pass  # Already printed above
    # Try LAN commands on first found device
    ip = devices[0][0][0]
    try_govee_cmds(ip)
else:
    print("\nNo Govee LAN devices found.")
    print("The H5080 plug may not support LAN protocol (it's BLE-only for local).")
    print("Alternative: Use Govee OpenAPI (cloud) or GPIO relay.")