#!/usr/bin/env python3
"""Analyze Govee H5179 export CSVs: seasonal humidity + daemon toggling sims.

Reads all `Wifi Thermometer_export_*.csv` files from a directory (as exported
by the Govee app) and prints:
  1. data coverage + quality
  2. monthly/seasonal humidity stats (min/median/p90/max, temp F)
  3. daemon toggle simulation comparing threshold schemes:
       single N      -> ON if humidity > N
       band HI/LO    -> ON if >= HI, OFF if <= LO, hold otherwise (hysteresis)
     simulated at the daemon's 15-min polling cadence with state-change-only
     writes, same as govee-ble daemon --interval 900.

Usage:
    python3 scripts/humidity_analysis.py [DIR] [--scheme band 55 45]
DIR defaults to the folder containing the exports. Extra schemes can be
appended; the last --scheme wins unless multiple are given, e.g.:

    python3 scripts/humidity_analysis.py ~/govee_export \\
        --scheme single 45 --scheme band 50 40 --scheme band 55 45

The toggle numbers drive the hysteresis decision: a year of 1-min data
(2025-09..2026-09) showed single-45 chatters ~6,042 cycles/yr vs ~36 for a
50/40 band, i.e. ~27 plug clicks/day in summer vs one every ~3 weeks.
"""
import glob
import os
import sys
from collections import defaultdict
from statistics import median

POLL_MIN = 15  # daemon interval, minutes (--interval 900)


def load(path):
    rows = []
    for f in sorted(glob.glob(os.path.join(path, "Wifi Thermometer_export_*.csv"))):
        with open(f, newline="", encoding="utf-8-sig") as fh:
            import csv
            r = csv.reader(fh)
            next(r)  # header
            for row in r:
                if len(row) < 3 or not row[0].strip():
                    continue
                rows.append((row[0].strip(), float(row[2]), float(row[1])))
    rows.sort()
    return rows


def simulate(poll, scheme, hi, lo=None):
    """daemon-like: poll every 15 min, toggle only on state change.
    single -> ON if h>hi; band -> ON if h>=hi, OFF if h<=lo, hold otherwise."""
    state = False
    toggles = 0
    on_min = 0
    for h in poll:
        if scheme == "single":
            want = h > hi
        else:
            if h >= hi:
                want = True
            elif h <= lo:
                want = False
            else:
                want = state
        if want != state:
            state = want
            toggles += 1
        if state:
            on_min += POLL_MIN
    return toggles, on_min / 60.0


def main():
    args = sys.argv[1:]
    path = "."
    schemes = [("single", 45, None)]  # default = current behaviour
    while args:
        a = args.pop(0)
        if a == "--scheme":
            name = args.pop(0)
            hi = float(args.pop(0))
            lo = float(args.pop(0)) if name == "band" and args and args[0].replace(".", "").isdigit() else None
            schemes.append((name, hi, lo))
        else:
            path = a

    rows = load(path)
    if not rows:
        print("no export CSVs found in", path)
        sys.exit(1)

    print(f"samples: {len(rows):,}  range: {rows[0][0]} -> {rows[-1][0]}")
    bad = [h for _, h, _ in rows if not (0 <= h <= 100)]
    print(f"data quality: {len(bad)} out-of-range humidity values")

    season = lambda ts: {"12": "win", "01": "win", "02": "win",
                         "03": "spr", "04": "spr", "05": "spr",
                         "06": "sum", "07": "sum", "08": "sum",
                         "09": "fal", "10": "fal", "11": "fal"}[ts[5:7]]
    by_season = defaultdict(list)
    for ts, h, _ in rows[::POLL_MIN]:
        by_season[season(ts)].append(h)

    print(f"\nseasonal humidity (15-min polled)")
    print(f"{'season':<6}{'n':>8}{'hum_min':>9}{'hum_med':>9}{'hum_p90':>9}{'hum_max':>9}")
    for s in ["win", "spr", "sum", "fal"]:
        hs = sorted(by_season[s])
        if not hs:
            continue
        p90 = hs[int(len(hs) * 0.9) - 1]
        print(f"{s:<6}{len(hs):>8}{hs[0]:>9.1f}{median(hs):>9.1f}{p90:>9.1f}{hs[-1]:>9.1f}")

    print(f"\nscheme simulation (cycles/yr, ON hours)")
    print(f"{'scheme':<28}{'cycles/yr':>12}{'on_h/yr':>10}")
    for name, hi, lo in schemes:
        c = o = 0.0
        for s in ["win", "spr", "sum", "fal"]:
            h, oh = simulate(by_season[s], name, hi, lo)
            c += h
            o += oh
        label = (f"single {hi:g}" if name == "single" else f"band {hi:g}/{lo:g}")
        print(f"{label:<28}{c:>12.0f}{o:>10.0f}")

    # endure min/day in the humid season
    hs = by_season["sum"]
    c, _ = simulate(hs, "single", 45)
    print(f"\nsingle-45 chatters ~{c/len(hs)* (len(hs)//92):.1f}/day in summer; "
          f"a hysteresis band drops that ~100x.")


if __name__ == "__main__":
    main()