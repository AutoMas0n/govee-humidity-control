#!/usr/bin/env python3
"""Connect to plug and try to write - D-Bus approach."""
import dbus, time, sys

bus = dbus.SystemBus()
PLUG = "60:74:F4:BD:4D:E5"

manager = dbus.Interface(
    bus.get_object("org.bluez", "/"),
    "org.freedesktop.DBus.ObjectManager"
)

dev_path = None
for path, ifaces in manager.GetManagedObjects().items():
    if "org.bluez.Device1" in ifaces:
        addr = str(ifaces["org.bluez.Device1"].get("Address", ""))
        if addr == PLUG:
            dev_path = path
            print("Found device:", path)
            break

if not dev_path:
    print("Starting discovery...")
    adapter = dbus.Interface(
        bus.get_object("org.bluez", "/org/bluez/hci0"),
        "org.bluez.Adapter1"
    )
    adapter.StartDiscovery()
    time.sleep(8)
    adapter.StopDiscovery()
    
    for path, ifaces in manager.GetManagedObjects().items():
        if "org.bluez.Device1" in ifaces:
            addr = str(ifaces["org.bluez.Device1"].get("Address", ""))
            if addr == PLUG:
                dev_path = path
                print("Found after scan:", path)
                break

if dev_path:
    device = dbus.Interface(
        bus.get_object("org.bluez", dev_path),
        "org.bluez.Device1"
    )
    print("Connecting...")
    try:
        device.Connect()
        print("Connected!")
        time.sleep(2)
        props = dbus.Interface(
            bus.get_object("org.bluez", dev_path),
            "org.freedesktop.DBus.Properties"
        )
        for flag in ["Connected", "ServicesResolved", "Paired", "Trusted"]:
            try:
                val = props.Get("org.bluez.Device1", flag)
                print("  %s: %s" % (flag, val))
            except:
                pass
        
        # List services
        try:
            from pprint import pprint
            mgr = dbus.Interface(
                bus.get_object("org.bluez", dev_path),
                "org.freedesktop.DBus.ObjectManager"
            )
            # Actually just check the Device1 properties for GATT
            char_paths = []
            for path2, ifaces2 in manager.GetManagedObjects().items():
                if "org.bluez.GattCharacteristic1" in ifaces2:
                    props2 = ifaces2["org.bluez.GattCharacteristic1"]
                    uuid = str(props2.get("UUID", ""))
                    # Check if this characteristic belongs to our device
                    if dev_path in path2:
                        print("  Char: %s [%s]" % (uuid, path2))
                        char_paths.append((path2, uuid, props2.get("Flags", [])))
            
            for cpath, uuid, flags in char_paths:
                print("    %s flags=%s" % (uuid, list(flags)))
                if b"write" in str(flags).lower().encode() or b"Write" in str(flags).encode():
                    print("    *** WRITABLE ***")
        except Exception as e:
            print("  Service list error:", e)
            
    except Exception as e:
        print("Connect failed:", e)
else:
    print("Device not found")