use std::collections::HashSet;

use anyhow::{Result, anyhow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
};

use super::send_input::send;

const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_SIDE: u16 = 0x113;
const BTN_EXTRA: u16 = 0x114;

const XBUTTON1: u32 = 1;
const XBUTTON2: u32 = 2;
const WHEEL_DELTA: i32 = 120;

pub struct VirtualMouse {
    prev: HashSet<u16>,
}

impl VirtualMouse {
    pub fn new() -> Result<Self> {
        Ok(Self {
            prev: HashSet::new(),
        })
    }

    pub fn apply(
        &mut self,
        buttons: &HashSet<u16>,
        rel: (i32, i32),
        wheel: (i32, i32),
    ) -> Result<()> {
        let mut inputs = Vec::new();

        for code in self.prev.difference(buttons) {
            inputs.push(button_input(*code, false)?);
        }
        for code in buttons.difference(&self.prev) {
            inputs.push(button_input(*code, true)?);
        }

        if rel != (0, 0) {
            inputs.push(mouse_input(rel.0, rel.1, 0, MOUSEEVENTF_MOVE));
        }
        if wheel.0 != 0 {
            inputs.push(mouse_input(
                0,
                0,
                (wheel.0 * WHEEL_DELTA) as u32,
                MOUSEEVENTF_HWHEEL,
            ));
        }
        if wheel.1 != 0 {
            inputs.push(mouse_input(
                0,
                0,
                (wheel.1 * WHEEL_DELTA) as u32,
                MOUSEEVENTF_WHEEL,
            ));
        }

        send(&inputs)?;
        self.prev = buttons.clone();
        Ok(())
    }

    pub fn release_all(&mut self) -> Result<()> {
        if self.prev.is_empty() {
            return Ok(());
        }

        let inputs: Vec<INPUT> = self
            .prev
            .iter()
            .map(|code| button_input(*code, false))
            .collect::<Result<_>>()?;
        send(&inputs)?;
        self.prev.clear();
        Ok(())
    }
}

fn button_input(code: u16, down: bool) -> Result<INPUT> {
    let (down_flag, up_flag, mouse_data) = match code {
        BTN_LEFT => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
        BTN_RIGHT => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
        BTN_MIDDLE => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
        BTN_SIDE => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON1),
        BTN_EXTRA => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON2),
        _ => return Err(anyhow!("no Windows mapping for mouse button {code:#x}")),
    };
    let flags = if down { down_flag } else { up_flag };
    Ok(mouse_input(0, 0, mouse_data, flags))
}

fn mouse_input(dx: i32, dy: i32, mouse_data: u32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
