use std::collections::HashSet;

use anyhow::{Result, anyhow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE,
};

use crate::platform::key_transitions;
use crate::platform::keymap::key_stroke;

use super::send_input::send;

pub struct VirtualKeyboard {
    prev: HashSet<u16>,
}

impl VirtualKeyboard {
    pub fn new() -> Result<Self> {
        Ok(Self {
            prev: HashSet::new(),
        })
    }

    /// Emits key transitions only; holding a key does not synthesize repeats.
    pub fn apply(&mut self, desired: &HashSet<u16>) -> Result<()> {
        let (press, release) = key_transitions(&self.prev, desired);
        let mut inputs = Vec::with_capacity(press.len() + release.len());
        for code in release {
            inputs.push(key_input(code, false)?);
        }
        for code in press {
            inputs.push(key_input(code, true)?);
        }
        send(&inputs)?;
        self.prev = desired.clone();
        Ok(())
    }

    pub fn release_all(&mut self) -> Result<()> {
        if self.prev.is_empty() {
            return Ok(());
        }

        let (_, release) = key_transitions(&self.prev, &HashSet::new());
        let inputs: Vec<INPUT> = release
            .into_iter()
            .map(|code| key_input(code, false))
            .collect::<Result<_>>()?;
        send(&inputs)?;
        self.prev.clear();
        Ok(())
    }
}

fn key_input(code: u16, down: bool) -> Result<INPUT> {
    let stroke =
        key_stroke(code).ok_or_else(|| anyhow!("no Windows mapping for key code {code}"))?;
    let mut flags = 0;
    if stroke.scancode {
        flags |= KEYEVENTF_SCANCODE;
    }
    if stroke.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }

    Ok(INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: stroke.vk,
                wScan: stroke.scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    })
}
