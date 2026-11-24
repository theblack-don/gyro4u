# gyro4u — Session Context (2026-11-23)

## Objective

- Fix C-button mapping leakage (C1–C4 must honor gyro4u mappings).
- Fix gyro-to-mouse drift.

## Decisions

- Moved the Rust hotspot constants in `src/ui/tarantula_layout.rs` together with the SVG to the reference layout. (Approved.)
- Extended `InputGrab` to grab **all** GameSir dongle event nodes (joystick, keyboard, mouse, aux), not just the joystick. (Approved.)
- Treated the remaining in-game C2 middle mouse and gyro drift as a **Steam Input conflict**, not a gyro4u code bug. User confirmed disabling Steam Input resolved both.

## Work completed

- `assets/tarantula_pro.svg`: removed to avoid confusion; the app uses
  `assets/GAMESIR_Controller_White_Line_Art.svg` for the live view and gallery.
- `src/ui/tarantula_layout.rs`: hotspot constants updated to the new spread-out geometry.
  - `nix develop -c cargo test tarantula` passes 10/10.
- `src/input_grab.rs`: grabs all GameSir `3537:103e` event nodes.
  - Daemon had `event26`, `event30`, `event31`, `event261`, `event263` open after the fix.
- User verified C1/C3/C4 work; disabling Steam Input fixed C2 and gyro drift.
- `nix develop -c cargo check` passed; `cargo fmt --check` clean.

## Current layout coordinates

- D-pad (35,37)
- Sticks (55,54) / (105,54)
- Face Y/X/B/A (123,26) / (112,37) / (134,37) / (123,48)
- Home (80,49)
- T row `y=34.5`
- C1–C4 on upper slopes
- View/Menu top corners
- Share right edge
- L4/R4 bottom center

## Uncommitted state

Modified (staged):

- `src/input_grab.rs`
- `src/ui/tarantula_layout.rs`

Untracked:

- `assets/GAMESIR_Controller_White_Line_Art.svg`

Reference/scratch files:

- `Screenshot_20261123_105550.png` (gitignored reference image)
- `/tmp/opencode/tarantula_preview_final.png`
- `/tmp/opencode/tarantula_transparent.png`
- `/tmp/opencode/layoutfront-1.png` (official front-view layout used for geometry)
- `/tmp/opencode/gamesir_probe.py` (vendor-state probe for C/T buttons and IMU)

## Config notes

- `/home/don/.config/gyro4u/settings.json`: active profile `gamesir_profile: TarantulaFinals`.
- `/home/don/.config/gyro4u/profiles/TarantulaFinals.json`: C2 → `Key: M`; gyro bias zero, speed deadzone `0.0`.
- Steam Input was enabled for The Finals (app `2073850`): `UseSteamControllerConfig=2` plus custom gyro layouts under
  `/home/don/.local/share/Steam/userdata/155476655/config/localconfig.vdf` and
  `/home/don/.local/share/Steam/steamapps/common/Steam Controller Configs/155476655/config/2073850/`.
- Steam Input fights gyro4u; the README warns about this conflict.

## Next steps

1. If requested, review and commit the three staged modified files.
2. If gyro drift returns with Steam Input off: run Gyro → Center bias calibration for 1.5 s, and set speed deadzone to ~0.5–1.0 °/s in `TarantulaFinals`.
