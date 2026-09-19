# Design: Humidity Hysteresis Band

## Context

The daemon loop in `govee-ble/src/main.rs` currently decides the plug state
with a single threshold: `let need_on = h > threshold;`. A year of 1-minute
sensor data (see proposal "Why") shows this chatters ~6,042 cycles/yr because
basement humidity hovers around the setpoint. The fix is a two-setpoint
hysteresis band: ON at ≥ hi, OFF at ≤ lo, hold otherwise. The loop already
tracks `last_on` and only writes on state change — the change is confined to
how `need_on` is computed and what the CLI/status page expose.

## Goals / Non-Goals

**Goals:**
- Replace the single-threshold `need_on` decision with a hysteresis band
- Keep `--threshold N` working (band N/N) so the existing CLI contract and
  systemd unit stay valid
- Surface the active setpoints on the status page
- Pick data-backed defaults (hi 55 / lo 45) from the measured year

**Non-Goals:**
- No changes to BLE protocol, handshake, plug control, or other subcommands
- No persistence of state across daemon restarts (still stateless — on
  restart it reads once and acts)
- No changes to the analysis scripts' behaviour (they already support
  arbitrary band simulation)

## Decisions

### Decision: `--hi` / `--lo` flags, `--threshold N` kept as alias for band N/N

Two explicit flags are the clearest expression of the band. `--threshold`
remains for compatibility: if `--hi`/`--lo` are absent but `--threshold N`
is present, both setpoints become N (identical to today's behaviour). If
neither is given, defaults hi 55 / lo 45 apply.

**Alternatives considered:**
- `--threshold N --hysteresis W` (N±W/2): fewer flags but obscures the
  actual setpoints and is harder to read on the status page. Two explicit
  setpoints are self-documenting.
- Replacing `--threshold` entirely (breaking): unnecessary — the band N/N
  mapping is a zero-cost compatibility shim.

### Decision: Defaults hi 55 / lo 45 (data-backed)

From `scripts/humidity_analysis.py` on the 2025-09→2026-09 export: band
55/45 yields 318 cycles/yr and 324 ON-hours vs single-45's 6,042 cycles and
3,134 ON-hours, while keeping the basement under mold risk (only 80 h/yr
above 55%, 10 h/yr above 60%). The dehumidifier runs 1/10 as hard with the
same dryness outcome. Rationale recorded in the proposal; the analysis is
reproducible via the script.

**Alternatives considered:**
- 50/40 (36 cycles/yr, 3,489 h): over-dries relative to the measured need —
  55/45 was chosen as "run only on genuinely humid spells".

### Decision: Band semantics ON ≥ hi, OFF ≤ lo, hold strictly between

Boundary inclusivity matters at integer humidity: `h >= hi` turns ON at
exactly hi; `h <= lo` turns OFF at exactly lo; `lo < h < hi` holds. With the
single-threshold alias (hi == lo) this reduces to ON > N / OFF ≤ N — exactly
the current behaviour, preserving compatibility.

### Decision: Status page gains `hi=` / `lo=` lines

The existing text/plain status snapshot already prints `threshold` and
`interval`; adding the two active setpoints keeps the page a complete
"what is the controller doing" view with zero new machinery (same
`watch::channel<String>` snapshot).

## Risks / Trade-offs

- **[Risk] Hysteresis changes when the dehumidifier actually starts/stops**
  → The band is a threshold decision, not a room model: a dehumidifier that
  pulls humidity down quickly will still cycle, just with a ~10-point dead
  band instead of chattering at one point. The 1-min simulation already
  accounts for real humidity dynamics from the sensor.
- **[Risk] Defaults chosen from one basement's data may not fit another**
  → Both setpoints remain CLI-configurable; the systemd unit can be tuned
  per-site. The analysis script makes re-evaluation a one-liner.
- **[Trade-off] `--threshold` becomes an alias rather than the primary knob**
  → Kept deliberately for compatibility; documented in the CLI usage line.

## Migration Plan

1. Implement `need_on` band logic + `--hi`/`--lo` args in `govee-ble/src/main.rs`
2. Update usage text and status-page snapshot (add hi/lo)
3. Update `humidity-daemon.service` to `--hi 55 --lo 45` (drop `--threshold`)
4. Build on the Pi, sanity-check `read`, `status`, and one daemon cycle
5. Restart the service; verify journald shows the band and status page shows
   `hi=55` / `lo=45`

Rollback: revert the service unit to `--threshold 45` (still supported) and
restart — no binary downgrade needed for a unit-level revert.

## Open Questions

None — setpoints are configurable and defaults are data-backed; all remaining
tuning (e.g. seasonal setpoints) is a future change, not a spec decision.