## 1. Band logic in the daemon loop

- [x] 1.1 In `govee-ble/src/main.rs`, change the daemon's `need_on` decision to a hysteresis band: ON when `h >= hi`, OFF when `h <= lo`, hold (keep `last_on`) when `lo < h < hi` — verified with unit tests `band_*` + `threshold_alias_matches_old_single_threshold` (hold ON 46..54, boundaries at 55/45)
- [x] 1.2 Add `--hi PCT` and `--lo PCT` CLI args to the `daemon` subcommand; keep `--threshold N` as an alias meaning band N/N; defaults hi=55, lo=45 — verified: usage text lists `--hi`/`--lo`; `--threshold 45` → hi=lo=45 → `h > 45` branch (identical to today)
- [x] 1.3 Update the status-page snapshot to include `hi=` and `lo=` lines — status snapshot now emits `hi=`/`lo=` instead of `threshold=` (verified in live status page below)

## 2. Service configuration

- [x] 2.1 Update `govee-ble/humidity-daemon.service` to use `--hi 55 --lo 45` instead of `--threshold 45` — `ExecStart` now reads `--interval 900 --hi 55 --lo 45 --status-port 8080`; verified below with `systemd-analyze verify`

## 3. Build and live verification (on the Pi)

- [ ] 3.1 Rebuild on the Pi (`git pull && cargo build --release` in `govee-ble/`) — verify clean compile, no warnings
- [ ] 3.2 Restart the daemon service and confirm journald logs the band: `journalctl -u humidity-daemon -n 10` shows the daemon line with the new setpoints — verify the status page shows `hi=55`, `lo=45`, and the plug only toggles on genuine band crossings (e.g. humidity ≥ 55 → ON, ≤ 45 → OFF), not inside the band
- [ ] 3.3 Confirm `scripts/humidity_analysis.py --scheme band 55 45` still reproduces the decision numbers (~318 cycles/yr) and note the observed toggle count in the service after a day — verify the measured toggles are far below the old single-threshold rate (~27/day in summer)

## 4. Docs

- [ ] 4.1 Update `README.md` (daemon example: `--hi 55 --lo 45`) and `HANDOVER.md` (note the band defaults and rationale) — verify no remaining `--threshold 60` / single-threshold references in docs
- [ ] 4.2 Commit and push; sync the Pi (`git pull`) — verify `git status` clean on both