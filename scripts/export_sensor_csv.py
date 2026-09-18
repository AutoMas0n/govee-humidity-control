#!/usr/bin/env python3
"""Export humidity-daemon sensor history from journald as CSV.

Reads journalctl output on stdin, prints one CSV row per sensor reading:
    ts,temp_c,humidity,battery,action
`action` is ON/OFF when that reading caused the plug to toggle, else empty.
Missed cycles (ERROR "sensor: H5179 not found") show up as time gaps only.

Usage — piped straight from the service's journal:
    # on the Pi:
    journalctl -u humidity-daemon --no-pager | python3 scripts/export_sensor_csv.py > readings.csv

    # from Termux/phone (same thing over ssh):
    ssh pi@192.168.2.21 'journalctl -u humidity-daemon --no-pager' | python3 scripts/export_sensor_csv.py > readings.csv

    # last 30 days only:
    ssh pi@192.168.2.21 'journalctl -u humidity-daemon -S -30d --no-pager' | python3 scripts/export_sensor_csv.py

    # append to a running file (safe to re-run — prints header each time, so
    # drop the first line if you're appending):
    ssh pi@192.168.2.21 'journalctl -u humidity-daemon --no-pager' | \
      python3 scripts/export_sensor_csv.py | tail -n +2 >> readings.csv

Journal retention: systemd keeps /var/log/journal persistently across
reboots (Storage=auto + /var/log/journal exists on this Pi). At ~96 lines
per day this is negligible — decades of history fit in the journal.
"""
import re
import sys

SENSOR_RE = re.compile(
    r"\[([^\]]+Z) INFO\s+govee_ble\] sensor: (-?[\d.]+)C ([\d]+)% batt=([\d]+)%"
)
NEED_RE = re.compile(r"\[([^\]]+Z) INFO\s+govee_ble\] need (ON|OFF)")


def main():
    print("ts,temp_c,humidity,battery,action")
    pending = None  # (ts, temp, hum, batt) awaiting its need-ON/OFF line
    for line in sys.stdin:
        m = SENSOR_RE.search(line)
        if m:
            if pending:
                # previous reading had no toggle (or its need line was missed)
                print(f"{pending[0]},{pending[1]},{pending[2]},{pending[3]},,")
            pending = (m.group(1), m.group(2), m.group(3), m.group(4))
            continue
        m = NEED_RE.search(line)
        if m and pending:
            # sensor reading immediately followed by the toggle it caused
            print(f"{pending[0]},{pending[1]},{pending[2]},{pending[3]},{m.group(2)}")
            pending = None
    if pending:
        print(f"{pending[0]},{pending[1]},{pending[2]},{pending[3]},,")


if __name__ == "__main__":
    main()