# Windows

Gyro4U builds and runs on Windows 10/11 x86_64 as a single, driver-free
executable. This is an early preview: configuration, live input, HID settings,
and keyboard/mouse remapping are supported; virtual gamepad output and the
hiding features needed for a fully remapped controller are not implemented yet.

## Current status

Works on Windows:

- Controller enumeration and the **Inputs** pages for DualSense, Steam
  Controller, 8BitDo Pro 3, Flydigi Apex 5 / Vader 5 Pro, and GameSir
  Tarantula Pro.
- Lightbar, player LEDs, microphone, trigger, rumble, and other HID settings.
- Gyro, right-stick, and Steam trackpad mouse movement.
- Button, D-pad, and stick bindings to keyboard keys, mouse buttons, and the
  scroll wheel, including layers and press timing.
- The daemon, profiles, hotplug (1 s polling), and the line-delimited JSON IPC
  protocol over a local named pipe.

Not available on Windows yet:

- **Virtual gamepad output.** A binding whose target is a gamepad button
  produces no output. The daemon logs that virtual gamepad output is
  unsupported and keeps running.
- **Physical-device hiding.** There is no HidHide integration, so the
  controller still sends its native input to Windows while remapped
  keyboard/mouse output is also sent. Expect double input for buttons that are
  remapped. Close Steam Input before connecting.
- **Touchpad gesture filter.** The Linux touchpad grab/filter is a no-op on
  Windows. Touchpad state is still visible in the GUI, but `GesturesOnly`
  touchpad mode does not emit filtered gestures.
- **USB raw haptics.** `usb_haptics` uses ALSA on Linux and has no Windows
  backend yet.
- **Noctalia theme.** The Noctalia/GTK/KDE color import is Linux-only; the
  built-in Default theme is used instead.
- **Systemd/service integration.** Start the daemon from the GUI or manually.

No drivers, ViGEmBus, HidHide, or udev rules are required for the features above.

## Option A: download the CI artifact

1. Push a branch, or use an existing branch/PR, and open **Actions → Build** on
   GitHub.
2. Open the `windows-latest` job and download the
   `gyro4u-windows-x86_64` artifact.
3. Extract the downloaded zip (`gyro4u.exe` plus `README-WINDOWS.md`) and run
   `gyro4u.exe` from the extracted folder.

The executable is unsigned. If Windows SmartScreen shows a warning, choose
**More info → Run anyway**. The CI artifact uses the static C runtime, so it
does not need the Visual C++ Redistributable.

This preview still uses the console subsystem, so a console window appears
alongside the GUI. That is expected; it shows startup/daemon messages. Closing
the console stops the application.

## Option B: build from source

Requirements:

- Windows 10/11 x86_64.
- Rust stable with the MSVC toolchain
  (`rustup default stable-x86_64-pc-windows-msvc`).
- Visual Studio 2022 Build Tools with the **Desktop development with C++**
  workload and a Windows SDK.

Build and test from a Developer PowerShell or a normal terminal:

```powershell
cargo build --release
cargo test --release
```

Run the GUI:

```powershell
.\target\release\gyro4u.exe
```

List detected controllers without starting the GUI or daemon:

```powershell
.\target\release\gyro4u.exe --list-devices
```

The `--list-devices` output includes the transport, interface number, bus type,
usage page/usage, and HID path for each controller, which is useful when
reporting a detection problem.

## First run and configuration

- Settings and profiles live in `%APPDATA%\gyro4u`.
- A default profile is created for the connected controller family on first
  run.
- The daemon starts automatically by default. The GUI starts it with
  `CREATE_NO_WINDOW`, so its stdout/stderr are discarded. The GUI is a console
  executable for this preview, so launching it from Explorer opens a console
  window with its messages.
- The daemon is single-instance. It locks `%APPDATA%\gyro4u\gyro4u.lock` and
  listens on the named pipe `\\.\pipe\gyro4u-<username>`.
- Close Steam Input (or disable Steam's controller support for the device)
  before connecting. Steam and Gyro4U cannot both own the controller cleanly.

## Command-line usage

```powershell
gyro4u.exe                 # GUI
gyro4u.exe --daemon        # background daemon in the current console
gyro4u.exe --list-devices  # enumerate HID devices
gyro4u.exe --status        # controller + battery, needs a running daemon
gyro4u.exe --list-profiles
gyro4u.exe --switch-profile Default
gyro4u.exe --reload-profile
```

The profile commands and `--status` require a compatible daemon. Start the
daemon from the GUI or run `gyro4u.exe --daemon` in a terminal. To see daemon
logs, run it manually in a terminal; the GUI-spawned daemon intentionally has
no console.

There is no Windows service wrapper yet. Stop the daemon from
**Settings → Daemon** in the GUI, or end the `gyro4u.exe` process from Task
Manager.

## Uninstall

1. Stop the daemon and close the GUI.
2. Delete `gyro4u.exe` (or the extracted folder).
3. Delete `%APPDATA%\gyro4u` to remove settings, profiles, and themes.

The named pipe disappears when the daemon exits; the lock is released at the
same time. The lock file itself is removed with the config directory.

## Troubleshooting

**The controller is not listed**

- Run `gyro4u.exe --list-devices` and check the `path`, `bus`, `interface`, and
  `usage` fields.
- Close Steam Input and any vendor utility that may have opened the device.
- For DualSense, try USB first. For 8BitDo, use USB-C in D mode. Flydigi devices
  need wired PC mode; the GameSir Tarantula Pro needs its 2.4 GHz dongle in
  XInput/PC mode.
- A device may be present but skipped because its interface is not a supported
  controller interface; the skipped section of `--list-devices` shows why.

**Bluetooth vs USB**

- `bus: Bluetooth` or a `Bluetooth` transport means the controller is connected
  over Bluetooth. Windows can report interface number `0` for Bluetooth HID
  devices, so transport detection uses the HID bus type first.
- DualSense and Steam Controller work over USB and Bluetooth. 8BitDo Bluetooth
  DInput remains experimental. Flydigi and GameSir currently require their
  wired/dongle transports.

**Permissions**

- HID access normally works as a standard user; no driver installation or
  special security policy is needed.
- Windows blocks synthetic input from a lower-integrity process to an elevated
  window. If a game runs as administrator, run Gyro4U at the same elevation.
- If the controller is open by another application, close that application and
  reconnect it.

**Daemon logs**

- The GUI-spawned daemon has its output discarded. Run
  `gyro4u.exe --daemon` from a terminal to see connection, hotplug, profile, and
  remap messages.
- `gyro4u.exe --status` reports whether the daemon is reachable and what
  controller it sees.

**Double input or stuck keys**

- Gamepad output is not implemented on Windows and physical-device hiding is
  not available, so the controller's native input still reaches games.
- Gyro4U releases all synthesized keyboard keys and mouse buttons when the
  controller disconnects, the output is reset, or the active profile changes.
  A crash can still leave a key held; release it manually or restart the
  application that received it.
