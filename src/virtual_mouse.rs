use std::collections::HashSet;

use anyhow::Result;

use crate::uinput::{EV_KEY, EV_REL, INPUT_PROP_POINTER, UinputDevice, UinputSpec};

const NAME: &str = "Gyro4U Virtual Mouse";
const VENDOR: u16 = 0x1209;
const PRODUCT: u16 = 0x0002;
const VERSION: u16 = 0x8111;

const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_SIDE: u16 = 0x113;
const BTN_EXTRA: u16 = 0x114;

const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_HWHEEL: u16 = 0x06;
const REL_WHEEL: u16 = 0x08;

const KEYS: [u16; 5] = [BTN_LEFT, BTN_RIGHT, BTN_MIDDLE, BTN_SIDE, BTN_EXTRA];
const RELS: [u16; 4] = [REL_X, REL_Y, REL_WHEEL, REL_HWHEEL];

pub struct VirtualMouse {
    dev: UinputDevice,
    prev: HashSet<u16>,
}

impl VirtualMouse {
    pub fn new() -> Result<Self> {
        let dev = UinputDevice::create(UinputSpec {
            name: NAME,
            vendor: VENDOR,
            product: PRODUCT,
            version: VERSION,
            keys: &KEYS,
            axes: &[],
            rels: &RELS,
            props: &[INPUT_PROP_POINTER],
        })?;
        Ok(Self {
            dev,
            prev: HashSet::new(),
        })
    }

    pub fn apply(
        &mut self,
        buttons: &HashSet<u16>,
        rel: (i32, i32),
        wheel: (i32, i32),
    ) -> Result<()> {
        let mut changed = false;

        for code in self.prev.difference(buttons) {
            changed |= self.dev.emit(EV_KEY, *code, 0);
        }
        for code in buttons.difference(&self.prev) {
            changed |= self.dev.emit(EV_KEY, *code, 1);
        }

        // Relative axes are never deduplicated: every notch must be reported.
        for (axis, value) in [
            (REL_X, rel.0),
            (REL_Y, rel.1),
            (REL_HWHEEL, wheel.0),
            (REL_WHEEL, wheel.1),
        ] {
            if value != 0 {
                self.dev.push(EV_REL, axis, value);
                changed = true;
            }
        }

        if changed {
            self.dev.sync()?;
        }
        self.prev = buttons.clone();
        Ok(())
    }

    pub fn release_all(&mut self) -> Result<()> {
        if self.prev.is_empty() {
            return Ok(());
        }

        let mut changed = false;
        for code in &self.prev {
            changed |= self.dev.emit(EV_KEY, *code, 0);
        }
        if changed {
            self.dev.sync()?;
        }
        self.prev.clear();
        Ok(())
    }
}
