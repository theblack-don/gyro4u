use std::collections::HashSet;

use anyhow::Result;

use crate::keycodes;
use crate::platform::key_transitions;
use crate::uinput::{EV_KEY, UinputDevice, UinputSpec};

const NAME: &str = "Gyro4U Virtual Keyboard";
const VENDOR: u16 = 0x1209;
const PRODUCT: u16 = 0x0001;
const VERSION: u16 = 0x8111;

pub struct VirtualKeyboard {
    dev: UinputDevice,
    prev: HashSet<u16>,
}

impl VirtualKeyboard {
    pub fn new() -> Result<Self> {
        let dev = UinputDevice::create(UinputSpec {
            name: NAME,
            vendor: VENDOR,
            product: PRODUCT,
            version: VERSION,
            keys: keycodes::all_codes(),
            axes: &[],
            rels: &[],
            props: &[],
        })?;
        Ok(Self {
            dev,
            prev: HashSet::new(),
        })
    }

    /// Emits key transitions only; holding a key does not synthesize repeats.
    pub fn apply(&mut self, desired: &HashSet<u16>) -> Result<()> {
        let (press, release) = key_transitions(&self.prev, desired);

        let mut changed = false;
        for code in release {
            changed |= self.dev.emit(EV_KEY, code, 0);
        }
        for code in press {
            changed |= self.dev.emit(EV_KEY, code, 1);
        }
        if changed {
            self.dev.sync()?;
        }
        self.prev = desired.clone();
        Ok(())
    }

    pub fn release_all(&mut self) -> Result<()> {
        if self.prev.is_empty() {
            return Ok(());
        }

        let empty = HashSet::new();
        let (_, release) = key_transitions(&self.prev, &empty);
        let mut changed = false;
        for code in release {
            changed |= self.dev.emit(EV_KEY, code, 0);
        }
        if changed {
            self.dev.sync()?;
        }
        self.prev.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_press_first() {
        let prev = HashSet::new();
        let desired = HashSet::from([45, 30, 56]);
        let (press, release) = key_transitions(&prev, &desired);
        assert_eq!(press, vec![56, 30, 45]);
        assert!(release.is_empty());
    }

    #[test]
    fn modifiers_release_last() {
        let prev = HashSet::from([42, 45]);
        let desired = HashSet::new();
        let (press, release) = key_transitions(&prev, &desired);
        assert!(press.is_empty());
        assert_eq!(release, vec![45, 42]);
    }

    #[test]
    fn modifier_family_order() {
        let desired = HashSet::from([45, 126, 56, 42, 29]);
        let (press, _) = key_transitions(&HashSet::new(), &desired);
        assert_eq!(press, vec![29, 42, 56, 126, 45]);
    }

    #[test]
    fn unchanged_state_has_no_transitions() {
        let prev = HashSet::from([30, 45]);
        let (press, release) = key_transitions(&prev, &prev);
        assert!(press.is_empty());
        assert!(release.is_empty());
    }
}
