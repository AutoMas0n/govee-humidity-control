#!/usr/bin/env python3
"""Write power command to plug via D-Bus."""
import dbus, time, binascii, struct

PLUG = "60:74:F4:BD:4D:E5"
bus = dbus.SystemBus()

# Find the device path
manager = dbus.Interface(bus.get_object("org.bluez", "/"), "org.freedesktop.DBus.ObjectManager")

dev_path = None
write_char_path = None
notify_char_path = None

for path, ifaces in manager.GetManagedObjects().items():
    if "org.bluez.GattCharacteristic1" in ifaces:
        props = ifaces["org.bluez.GattCharacteristic1"]
        uuid = str(props.get("UUID", ""))
        if uuid == "00010203-0405-0607-0809-0a0b0c0d2b11":
            write_char_path = path
            print("Write char:", path)
        elif uuid == "00010203-0405-0607-0809-0a0b0c0d2b10":
            notify_char_path = path
            print("Notify char:", path)

print("Connecting...")
for path, ifaces in manager.GetManagedObjects().items():
    if "org.bluez.Device1" in ifaces:
        addr = str(ifaces["org.bluez.Device1"].get("Address", ""))
        if addr == PLUG:
            dev_path = path
            break

if dev_path:
    device = dbus.Interface(
        bus.get_object("org.bluez", dev_path), "org.bluez.Device1"
    )
    device.Connect()
    time.sleep(2)
    
    # Try reading the write char first
    if write_char_path:
        char = dbus.Interface(
            bus.get_object("org.bluez", write_char_path),
            "org.bluez.GattCharacteristic1"
        )
        try:
            val = char.ReadValue({})
            print("Read value:", binascii.hexlify(bytes(val)).decode())
        except Exception as e:
            print("Read error:", e)
        
        # Write OFF command
        off = bytes.fromhex("3301000000000000000000000000000000000032")
        print("Writing OFF:", off.hex())
        try:
            char.WriteValue(list(off), {})
            print("OFF written!")
        except Exception as e:
            print("Write error:", e)
        
        time.sleep(3)
        
        # Write ON
        on = bytes.fromhex("3301010000000000000000000000000000000033")
        print("Writing ON:", on.hex())
        try:
            char.WriteValue(list(on), {})
            print("ON written!")
        except Exception as e:
            print("Write error:", e)
        
        time.sleep(3)
        
        # Write OFF again
        print("Writing OFF:", off.hex())
        try:
            char.WriteValue(list(off), {})
            print("OFF written!")
        except Exception as e:
            print("Write error:", e)
    
    print("Checking MFR state...")
    time.sleep(2)
    device.Disconnect()
else:
    print("Device not found")