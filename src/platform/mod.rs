use std::collections::HashSet;

#[cfg(target_os = "linux")]
pub(crate) use crate::{
    input_grab::InputGrab,
    noctalia,
    touchpad_filter::{TouchpadFilter, TouchpadModeHandle, TouchpadOwnedHandle},
    usb_haptics::{UsbHapticStream, find_dualsense_card},
    virtual_gamepad::VirtualGamepad,
    virtual_keyboard::VirtualKeyboard,
    virtual_mouse::VirtualMouse,
};

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub(crate) use windows::{
    input_grab::InputGrab,
    noctalia,
    touchpad_filter::{TouchpadFilter, TouchpadModeHandle, TouchpadOwnedHandle},
    usb_haptics::{UsbHapticStream, find_dualsense_card},
    virtual_gamepad::VirtualGamepad,
    virtual_keyboard::VirtualKeyboard,
    virtual_mouse::VirtualMouse,
};

#[cfg(any(target_os = "windows", test))]
#[allow(dead_code)]
mod keymap;

const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_LEFTALT: u16 = 56;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_RIGHTALT: u16 = 100;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;

/// Returns `(press, release)` in emission order.
///
/// Modifiers go down before other keys and come up after them so combined
/// bindings (e.g. LeftAlt + X) are never briefly seen as plain keys.
pub(crate) fn key_transitions(prev: &HashSet<u16>, desired: &HashSet<u16>) -> (Vec<u16>, Vec<u16>) {
    let mut press: Vec<u16> = desired.difference(prev).copied().collect();
    press.sort_unstable_by_key(|code| (modifier_rank(*code).unwrap_or(4), *code));

    let mut release: Vec<u16> = prev.difference(desired).copied().collect();
    release.sort_unstable_by_key(|code| (u8::from(modifier_rank(*code).is_some()), *code));

    (press, release)
}

fn modifier_rank(code: u16) -> Option<u8> {
    match code {
        KEY_LEFTCTRL | KEY_RIGHTCTRL => Some(0),
        KEY_LEFTSHIFT | KEY_RIGHTSHIFT => Some(1),
        KEY_LEFTALT | KEY_RIGHTALT => Some(2),
        KEY_LEFTMETA | KEY_RIGHTMETA => Some(3),
        _ => None,
    }
}
