[![GitHub](https://img.shields.io/badge/GitHub-Repo-black)](https://github.com/theblack-don/gyro4u)

# Gyro4U

Native Linux GUI tool for configuring DualSense controllers and the 2026 Steam Controller.

> [!note]
> **DualSense Edge** is supported and tested, including the Edge paddles and Fn buttons.
>
> **Steam Controller (2026)** is supported over USB (`28DE:1302`), Bluetooth LE
> (`28DE:1303`), a wireless puck (`28DE:1304`), and the integrated receiver
> (`28DE:1305`). One controller and one puck slot are active at a time.

> [!warning]
> gyro4u talks to the controller directly over HID. Close Steam Input (or disable
> Steam's controller support for this device) before connecting, or both clients
> will fight over lizard mode and input. gyro4u cannot claim exclusive ownership
> while Steam Input is active.

Gyro4U is based on [DS4U](https://git.yokai.digital/deadYokai/ds4u) by deadYokai
([GitHub mirror](https://github.com/deadYokai/ds4u)).

![Gyro4U Steam Controller button mapping editor](preview/app-steam.png)

## Features

### Controller & inputs
- Live input visualization (buttons, D-pad, sticks, triggers, touchpad)
- Battery level and charging status
- USB and Bluetooth connection, with automatic hotplug detection
- Safe read-only Steam receiver probing (daemon) detects a controller waking in
  an idle puck/receiver slot within ~1 s, without changing controller state
- Controller switcher in the top bar (`Automatic`, `DualSense`, or `Steam
  Controller`); `Automatic` keeps the DualSense-first behavior when both are
  present, and family switches retry until the requested controller is available
- DualSense Edge paddles and Fn buttons
- Steam Controller (2026): both trackpads (touch, click, position, pressure),
  capacitive stick touch and grip sense, L4/L5/R4/R5 back buttons, L/R trigger
  clicks, Steam and QAM buttons, battery/charging, and wired/BLE/puck transports
- Steam trackpads can independently act as Raw, Mouse, or vertical Scroll
  surfaces (through the daemon); scroll speed and natural direction are
  configurable per pad
- Per-pad click and scroll-tick haptics with Off/Low/Medium/High intensity
  (software-synthesized one-shots through the daemon); raw
  touch/click/position/pressure state stays visible
- Steam pad clicks/touches and all extra buttons are exposed as extra virtual
  keys; a DualSense-shaped virtual gamepad cannot losslessly represent the two
  raw pressure-sensitive pad surfaces

Steam Controller limits in this release: one active controller and the first
puck/receiver slot with a live report, no LED effects, and no simultaneous
four-controller puck operation. Steam trackpad scrolling is vertical only;
horizontal scrolling and inertia/spin friction are deferred. Pad haptics are
one-shot clicks and scroll ticks rather than arbitrary waveforms.
Microsoft/Sony-only pages (adaptive triggers, lightbar, audio, firmware) are
hidden for Steam. The stacked-rectangle button is View, not Capture.

### Remapping
- Remap buttons to gamepad buttons, keyboard keys, mouse buttons, or scroll wheel directions
- Layers: hold an activator button to swap in a second set of bindings
- Press types per binding: `Regular`, `Long Press`, `Double Tap`
- Profile targets: bind a button to cycle or switch profiles
- Digital stick directions (including WASD-style key output)
- Right-stick mouse

#### Press timing and slot order

`Regular` is a held binding: it is active while the button is down. `Long Press`
activates after the configured Long Press threshold, and `Double Tap` activates
on a second press inside the configured Double Tap window.

By default each button uses **Combine — all active slots**, which is additive:
every slot that is active at that moment fires. For example, `Regular + Long
Press` fires Regular immediately and adds Long Press once the threshold is
reached.

Each button can instead use **First match — slot order wins** in the mapping
editor. Slot order is priority, grouped by press type: all slots of one press
type form one group, and a group's position is the position of its first slot.
All slots in the winning group fire together; lower-priority groups are
suppressed.

| Slot order | Result |
| --- | --- |
| `Regular` first | Fires immediately and wins. Later Long Press / Double Tap groups are unreachable. |
| `Long Press` before `Regular` | Nothing is emitted until the Long Press threshold. Crossing it emits only the Long Press group. A quick release emits the Regular group as a short pulse. |
| `Double Tap` first | A single press or hold can be delayed until the Double Tap window expires. If the double occurs it wins; otherwise the highest-priority group that was eligible during the gesture wins (Long Press if the threshold was reached, otherwise Regular) and is emitted as a short pulse when the button is already released. |
| Repeated press type | All slots of that type fire together, at the group's first position. |
| Mixed order | Lower groups are deferred while a higher group can still activate. Release, the Long Press threshold, or the Double Tap window marks the higher group impossible, then the highest eligible remaining group wins. |

Use the `↑` / `↓` controls on each slot row to change priority; order is saved
with the profile and applies to both base and layer bindings. The editor previews
the grouped priority and warns about unreachable or delayed cases.

First Match latches the resolved bindings and group order when the button is
pressed, so layer or profile changes in the middle of a gesture cannot change its
delayed output. Pending First Match gestures are cleared on profile switch or
reset. Combine remains the default, so existing profiles keep their additive
behavior.

### Analog sticks
- Per-stick sensitivity curves
- Inner and outer deadzones per stick
- Invert X / Y per stick
- Swap left/right sticks
- Digital stick output with configurable deadzones

### Gyroscope
- Gyro-to-mouse output with activation modes (always, hold, toggle, touchpad)
- Dots-per-360 calibration, sensitivity, acceleration, smoothing, and axis inversion

### Adaptive triggers
- Independent left/right configuration
- Effect modes: `Off`, `Feedback`, `Weapon`, `Bow`, `Galloping`, `Vibration`, `Machine`
- Adjustable start/end position, strength and frequency
- Per-trigger deadband (release point and full-stroke point)

### Haptics & vibration
- Haptic patterns: `Constant`, `Pulse`, `Ramp`, `Wave` (with strength and speed)
- Raw PCM haptic streaming to the voice-coil actuators (USB, and Bluetooth via the daemon)

### Lightbar & LEDs
- RGB lightbar color, brightness and on/off
- Animated lightbar effects (via the daemon):
    - **Breath**
    - **Rainbow**
    - **Strobe**
- Player-number indicator LEDs
- Microphone mute LED: `Off`, `On`, `Pulse`

### Audio
- Output routing: internal speaker, headphone, or both
- Microphone on/off toggle

### Profiles
- Save, load, switch and delete profiles (see [Profiles](#profiles))
- A profile captures everything: lightbar/LEDs, sticks, triggers, haptics, gyro,
  touchpad, mic, plus button remaps and disabled buttons
- Switch/list/reload profiles from the [command line](#command-line-usage)

### Firmware
- Read the controller's current firmware version and build info
- Check the latest firmware version
- Download and flash official firmware for DualSense / DualSense Edge

### Themes
- Built-in themes:
    - Default
    - Author theme (my custom theme)
    - Deep Dark
    - Tokyo Night
    - Noctalia (follows the current Noctalia / KineticWE color scheme)
- The Noctalia theme reads the colors Noctalia generates for GTK/KDE and refreshes automatically when the palette changes.
- Custom theme support (see [Custom Themes](#custom-themes))

### Daemon
- Background service that keeps the controller connection alive and runs live effects and input processing (see [Daemon](#daemon))
- Start/stop directly from the GUI, run via the CLI, or install as a systemd user service

## Installation

### Nix (flakes)

From the repository:

```sh
nix profile install .#gyro4u
```

Or directly from GitHub:

```sh
nix profile install github:theblack-don/gyro4u
```

The package installs the `Gyro4U` desktop entry and hicolor icon. See the
[NixOS](#nixos) notes for `uinput` and udev setup.

### Fedora / COPR

A spec file is provided at `packaging/fedora/gyro4u.spec`. Create a COPR project
from the GitHub repository and point it at that spec; once the project has built:

```sh
sudo dnf install gyro4u
```

The source tag `v0.2.0` must exist for the RPM `Source0` URL. COPR builds may
need vendored crates or network access for Cargo; configure the project
accordingly.

## Building

### Requirements
- Rust
- libraries: `libudev`, `alsa-lib`, `openssl`, `libxkbcommon`

### Build
```sh
git clone https://github.com/theblack-don/gyro4u.git
cd gyro4u
cargo build --release
```

### Run
```sh
./target/release/gyro4u
```

### Nix

```sh
nix build .#
./result/bin/gyro4u
```

Or run it directly:

```sh
nix run .#
```

### Device permissions

To talk to the controller without running as root, install a `udev` rule that
grants your user access to the DualSense `hidraw` device and to `uinput`.
The RPM package installs these files for you; when building from source or
using Nix, install them manually (on NixOS use the configuration below).

`/etc/udev/rules.d/70-gyro4u.rules`:

```
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="0ce6", MODE="0664", GROUP="input", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="0df2", MODE="0664", GROUP="input", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1302", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1303", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1304", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1305", TAG+="uaccess"
KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0660", GROUP="input", TAG+="uaccess", OPTIONS+="static_node=uinput"
```

`/etc/modules-load.d/gyro4u.conf`:

```
uinput
```

Then load the module and reload the rules, and reconnect the controller:

```sh
sudo modprobe uinput
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Command-line usage

Running `gyro4u` with no arguments launches the GUI.
`--daemon` runs the background daemon; `--list-devices` only enumerates HID
devices. The status and profile commands talk to a running daemon over its Unix
socket:


| Command | Description |
|---------|-------------|
| `gyro4u` | Launch the GUI |
| `gyro4u --daemon` | Run as the background daemon |
| `gyro4u --list-devices` | List detected DualSense and Steam Controller devices, their transport and path, and candidates that were skipped |
| `gyro4u --status` | Show the connected controller (serial, product id, USB/BT) and battery |
| `gyro4u --list-profiles` | List saved profile names |
| `gyro4u --switch-profile <name>` | Switch the active profile |
| `gyro4u --reload-profile` | Reload the current profile from disk |


The profile commands and `--status` require a compatible daemon to be running;
`--list-devices` does not start or contact the daemon.

## Daemon

The daemon is a background process that owns the controller connection and runs
the *continuous* work the GUI cannot do on its own.  
While it runs it:
- maintains the connection to the controller and watches for hotplug events,
- safely probes idle Steam puck/receiver slots so a controller waking later is
  detected within about a second,
- applies the active profile, and
- runs the live loops for **animated lightbar effects**, **haptic-pattern playback**, **gyro processing**, and **input transforms** (stick curves, deadzones, invert/swap, button remap, touchpad behavior, Steam trackpad mouse/scrolling and pad haptics).

One-shot hardware settings (lightbar color, brightness, player LEDs, mic, mic LED) work whether or not the daemon is running;  
the live/animated features above need it.

### Starting daemon

From the GUI (Settings), via the CLI, or as a service.

```sh
gyro4u --daemon
```

Or run as a systemd user service (`gyro4u-daemon.service`):

```ini
[Unit]
Description=Gyro4U daemon
After=graphical-session.target

[Service]
ExecStart=/usr/bin/gyro4u --daemon
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
```

### Closing or upgrading a running daemon

The GUI talks to the daemon over a versioned IPC protocol. If the running daemon
was started from an older build, the GUI shows **Incompatible** under
Settings → Daemon. Clicking **Replace** (or starting the app with auto-start
enabled) asks that daemon to shut down, waits up to three seconds for the socket
to be released, then starts the daemon from the current binary. If the old daemon
cannot be stopped, the GUI reports the error and does not attach to it; stop it
manually and restart:

```sh
pkill -f "gyro4u --daemon"
# or, when installed as a user service:
systemctl --user restart gyro4u-daemon
```

Use the same flow after upgrading a package or Nix profile while the daemon is
still running. `--status` and the profile CLI commands refuse to talk to a daemon
with an incompatible protocol and print the reason.

### Controller switching and multiple controllers

The controller switcher sits in the top bar next to the connection and battery
indicators. It shows one row per controller family: DualSense and Steam
Controller. All Steam puck/receiver slots collapse into a single Steam row. The
daemon probes idle receiver slots read-only about once per second, so waking a
Steam Controller makes that row live within about a second. A puck with no live
controller is labeled **Steam receiver** instead of looking like a connected
controller. Empty probes, busy handles, and permission errors are treated as
unknown, so they never mark a receiver dead.

- Choosing **Automatic** uses the DualSense when both families are available and
  falls back to the Steam Controller otherwise. It stays DualSense-first: a
  Steam Controller waking while a DualSense is active updates the row but does
  not switch automatically.
- Clicking the Steam row, even while the controller is asleep, selects Steam
  explicitly and retries. While it is off, the switcher shows `Steam Controller
  not detected — turn it on; retrying`, and the current DualSense stays active
  until the Steam Controller actually opens.
- Choosing a family is explicit: if that family is not available, Gyro4U keeps
  the current controller and retries on hotplug, safe liveness probes, and
  connection checks. It never silently substitutes the other family.
- A switch opens the requested controller before releasing the current one. If
  startup fails, the current controller stays active and the failure is shown in
  the switcher.
- The preference is persisted in `settings.json` and shared with the daemon, so
  changing it reconnects through the daemon without restarting it.
- Each controller family remembers its own last active profile across switches;
  switching back restores that family's profile.

Idle receiver liveness needs the daemon. In direct mode (app detached from the
daemon) a live direct Steam Controller is still detected and selected correctly,
but a receiver with no active controller is not probed until you click its row
and Gyro4U retries the connection.

Only one controller is active at a time. Selecting a specific controller by
serial number or path is still supported for multiple controllers of the same
family; without a selector the first matching device is used.

### NixOS

Enable `uinput` and add the udev rules:

```nix
hardware.uinput.enable = true;

services.udev.extraRules = ''
  SUBSYSTEM=="hidraw", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="0ce6", MODE="0664", GROUP="input", TAG+="uaccess"
  SUBSYSTEM=="hidraw", ATTRS{idVendor}=="054c", ATTRS{idProduct}=="0df2", MODE="0664", GROUP="input", TAG+="uaccess"
  SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1302", TAG+="uaccess"
  SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1303", TAG+="uaccess"
  SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1304", TAG+="uaccess"
  SUBSYSTEM=="hidraw", ATTRS{idVendor}=="28de", ATTRS{idProduct}=="1305", TAG+="uaccess"
  KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0660", GROUP="input", TAG+="uaccess", OPTIONS+="static_node=uinput"
'';
```

When running the daemon as a systemd user unit from a Nix profile, point it at
the profile binary:

```ini
[Service]
ExecStart=%h/.nix-profile/bin/gyro4u --daemon
```

## Profiles

Profiles are stored as JSON in `~/.config/gyro4u/profiles/<name>.json`, and the
filename is derived from the profile name, so profile names are global across
both controller families.
The active profile for each family is recorded separately in `settings.json`. A
`Default` profile is created automatically for DualSense and a `SteamDefault`
profile for the Steam Controller. Profiles carry their device family, so
Steam-only bindings do not leak into DualSense profiles (and vice versa).

The **Profiles** section shows every profile with a DualSense or Steam family
badge. Profiles for the other family can be converted to the active controller;
conversion preserves every mapping and setting and only changes the profile's
device family. Creating a profile whose name already exists for the other
family reports the collision and offers the same conversion instead of silently
overwriting anything.

Manage profiles from the GUI (the **Profiles** section) or from the [command line](#command-line-usage).

## Custom Themes

Filename must match `id` field inside JSON.

### Theme file format
```json
{
  "id": "tokyo_night",
  "name": "Tokyo Night",
  "dark_mode": true,
  "colors": {
    "window_bg":       [26,  27,  38],
    "panel_bg":        [22,  22,  30],
    "extreme_bg":      [16,  16,  24],
    "accent":          [122, 162, 247],
    "widget_hovered":  [41,  46,  66],
    "widget_inactive": [32,  36,  54],
    "text":            [192, 202, 245],
    "text_dim":        [86,  95,  137],
    "success":         [158, 206, 106],
    "error":           [247, 118, 142],
    "warning":         [224, 175, 104]
  }
}
```

All color values are `[R, G, B]` in the 0–255 range.

### Color roles

| Key              | Description                                      |
|------------------|--------------------------------------------------|
| `window_bg`      | Main application background                      |
| `panel_bg`       | Sidebar and panel backgrounds                    |
| `extreme_bg`     | Deepest background (headers, separators)         |
| `accent`         | Highlights, active indicators, selected items    |
| `widget_hovered` | Widget background on hover                       |
| `widget_inactive`| Widget background at rest                        |
| `text`           | Primary text                                     |
| `text_dim`       | Secondary / hint text                            |
| `success`        | Success indicators (e.g. connected, applied)     |
| `error`          | Error indicators                                 |
| `warning`        | Warning indicators                               |

## Config & Data Paths

| Path | Contents |
|------|----------|
| `~/.config/gyro4u/settings.json` | Active theme ID, per-family active profile names, daemon auto-start, controller preference |
| `~/.config/gyro4u/profiles/` | Saved profiles (`<name>.json`) |
| `~/.config/gyro4u/themes/` | Custom theme files (`<id>.json`) |
| `$XDG_RUNTIME_DIR/gyro4u.socket` | Daemon Unix socket |

On first run, an existing `~/.config/ds4u` directory is copied to
`~/.config/gyro4u`; the old directory is left untouched.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## References:
- https://github.com/nowrep/dualsensectl
- https://github.com/hurryman2212/vds
- Steam Controller illustration adapted from https://github.com/linuxgamerlife/lgl-noctalia-widgets
  (see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md))

## Todo
- [ ] other possible issues
- [x] DualSense Edge support

## Support:
Support the original DS4U author:
[![MySite](https://img.shields.io/badge/yokai.digital-black?style=for-the-badge)](https://yokai.digital)
[![Paypal](https://img.shields.io/badge/Paypal-black?style=for-the-badge&logo=paypal)](https://www.paypal.com/donate/?hosted_button_id=RLGYGXH4LZ8PC)
[![Monobank](https://img.shields.io/badge/Monobank-black?style=for-the-badge)](https://send.monobank.ua/jar/9oVcUiHxPd)
[![Liberapay](https://img.shields.io/liberapay/receives/deadYokai.svg?logo=liberapay&style=for-the-badge&logoColor=white&labelColor=black&color=orange&label=Liberapay)](https://liberapay.com/deadYokai/donate)
