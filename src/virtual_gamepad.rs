#![allow(dead_code)]

use anyhow::Result;

use crate::inputs::{
    APEX_LM, APEX_M1, APEX_M2, APEX_M3, APEX_M4, APEX_RM, BTN_CIRCLE, BTN_CREATE, BTN_CROSS,
    BTN_EDGE_FN_LEFT, BTN_EDGE_FN_RIGHT, BTN_EDGE_PADDLE_LEFT, BTN_EDGE_PADDLE_RIGHT,
    BTN_GAMESIR_C1, BTN_GAMESIR_C2, BTN_GAMESIR_C3, BTN_GAMESIR_C4, BTN_GAMESIR_L4, BTN_GAMESIR_R4,
    BTN_GAMESIR_SHARE, BTN_GAMESIR_T1, BTN_GAMESIR_T2, BTN_GAMESIR_T3, BTN_L1, BTN_L2, BTN_L3,
    BTN_OPTIONS, BTN_PS, BTN_R1, BTN_R2, BTN_R3, BTN_SQUARE, BTN_TRIANGLE, ControllerKind,
    ControllerState, DPAD_E, DPAD_N, DPAD_NE, DPAD_NW, DPAD_S, DPAD_SE, DPAD_SW, DPAD_W,
    EXTRA_BTN_EIGHTBITDO_L4, EXTRA_BTN_EIGHTBITDO_PL, EXTRA_BTN_EIGHTBITDO_PR,
    EXTRA_BTN_EIGHTBITDO_R4, EXTRA_BTN_L4, EXTRA_BTN_L5, EXTRA_BTN_LEFT_GRIP, EXTRA_BTN_LEFT_PAD,
    EXTRA_BTN_LEFT_PAD_TOUCH, EXTRA_BTN_LEFT_STICK_TOUCH, EXTRA_BTN_LEFT_TRIGGER_CLICK,
    EXTRA_BTN_QAM, EXTRA_BTN_R4, EXTRA_BTN_R5, EXTRA_BTN_RIGHT_GRIP, EXTRA_BTN_RIGHT_PAD,
    EXTRA_BTN_RIGHT_PAD_TOUCH, EXTRA_BTN_RIGHT_STICK_TOUCH, EXTRA_BTN_RIGHT_TRIGGER_CLICK,
    EXTRA_BTN_STEAM, VADER_C, VADER_CIRCLE, VADER_Z,
};
use crate::uinput::{EV_ABS, EV_KEY, UinputDevice, UinputSpec};

const BTN_SOUTH: u16 = 0x130;
const BTN_EAST: u16 = 0x131;
const BTN_NORTH: u16 = 0x133;
const BTN_WEST: u16 = 0x134;
const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
const BTN_TL2: u16 = 0x138;
const BTN_TR2: u16 = 0x139;
const BTN_SELECT: u16 = 0x13a;
const BTN_START: u16 = 0x13b;
const BTN_MODE: u16 = 0x13c;
const BTN_THUMBL: u16 = 0x13d;
const BTN_THUMBR: u16 = 0x13e;

const BTN_TRIGGER_HAPPY1: u16 = 0x2c0;
const BTN_TRIGGER_HAPPY2: u16 = 0x2c1;
const BTN_TRIGGER_HAPPY3: u16 = 0x2c2;
const BTN_TRIGGER_HAPPY4: u16 = 0x2c3;
const BTN_TRIGGER_HAPPY5: u16 = 0x2c4;
const BTN_TRIGGER_HAPPY6: u16 = 0x2c5;
const BTN_TRIGGER_HAPPY7: u16 = 0x2c6;
const BTN_TRIGGER_HAPPY8: u16 = 0x2c7;
const BTN_TRIGGER_HAPPY9: u16 = 0x2c8;
const BTN_TRIGGER_HAPPY10: u16 = 0x2c9;
const BTN_TRIGGER_HAPPY11: u16 = 0x2ca;
const BTN_TRIGGER_HAPPY12: u16 = 0x2cb;
const BTN_TRIGGER_HAPPY13: u16 = 0x2cc;
const BTN_TRIGGER_HAPPY14: u16 = 0x2cd;
const BTN_TRIGGER_HAPPY15: u16 = 0x2ce;
const BTN_TRIGGER_HAPPY16: u16 = 0x2cf;

// Stable Steam Controller extra-button ordering. The values are remappable
// gamepad outputs, not raw pad coordinates.
const STEAM_EXTRA_KEYS: [(u32, u16); 16] = [
    (EXTRA_BTN_L4, BTN_TRIGGER_HAPPY1),
    (EXTRA_BTN_L5, BTN_TRIGGER_HAPPY2),
    (EXTRA_BTN_R4, BTN_TRIGGER_HAPPY3),
    (EXTRA_BTN_R5, BTN_TRIGGER_HAPPY4),
    (EXTRA_BTN_LEFT_GRIP, BTN_TRIGGER_HAPPY5),
    (EXTRA_BTN_RIGHT_GRIP, BTN_TRIGGER_HAPPY6),
    (EXTRA_BTN_LEFT_PAD, BTN_TRIGGER_HAPPY7),
    (EXTRA_BTN_RIGHT_PAD, BTN_TRIGGER_HAPPY8),
    (EXTRA_BTN_QAM, BTN_TRIGGER_HAPPY9),
    (EXTRA_BTN_STEAM, BTN_TRIGGER_HAPPY10),
    (EXTRA_BTN_LEFT_STICK_TOUCH, BTN_TRIGGER_HAPPY11),
    (EXTRA_BTN_RIGHT_STICK_TOUCH, BTN_TRIGGER_HAPPY12),
    (EXTRA_BTN_LEFT_PAD_TOUCH, BTN_TRIGGER_HAPPY13),
    (EXTRA_BTN_RIGHT_PAD_TOUCH, BTN_TRIGGER_HAPPY14),
    (EXTRA_BTN_LEFT_TRIGGER_CLICK, BTN_TRIGGER_HAPPY15),
    (EXTRA_BTN_RIGHT_TRIGGER_CLICK, BTN_TRIGGER_HAPPY16),
];

const EIGHTBITDO_EXTRA_KEYS: [(u32, u16); 4] = [
    (EXTRA_BTN_EIGHTBITDO_L4, BTN_TRIGGER_HAPPY1),
    (EXTRA_BTN_EIGHTBITDO_R4, BTN_TRIGGER_HAPPY2),
    (EXTRA_BTN_EIGHTBITDO_PL, BTN_TRIGGER_HAPPY3),
    (EXTRA_BTN_EIGHTBITDO_PR, BTN_TRIGGER_HAPPY4),
];

const APEX5_EXTRA_KEYS: [(u32, u16); 6] = [
    (APEX_LM, BTN_TRIGGER_HAPPY1),
    (APEX_RM, BTN_TRIGGER_HAPPY2),
    (APEX_M1, BTN_TRIGGER_HAPPY3),
    (APEX_M2, BTN_TRIGGER_HAPPY4),
    (APEX_M3, BTN_TRIGGER_HAPPY5),
    (APEX_M4, BTN_TRIGGER_HAPPY6),
];

const VADER5_EXTRA_KEYS: [(u32, u16); 9] = [
    (APEX_LM, BTN_TRIGGER_HAPPY1),
    (APEX_RM, BTN_TRIGGER_HAPPY2),
    (APEX_M1, BTN_TRIGGER_HAPPY3),
    (APEX_M2, BTN_TRIGGER_HAPPY4),
    (APEX_M3, BTN_TRIGGER_HAPPY5),
    (APEX_M4, BTN_TRIGGER_HAPPY6),
    (VADER_C, BTN_TRIGGER_HAPPY7),
    (VADER_Z, BTN_TRIGGER_HAPPY8),
    (VADER_CIRCLE, BTN_TRIGGER_HAPPY9),
];

// GameSir extras live in ControllerState.buttons, above the Sony mask.
const GAMESIR_EXTRA_KEYS: [(u32, u16); 10] = [
    (BTN_GAMESIR_L4, BTN_TRIGGER_HAPPY1),
    (BTN_GAMESIR_R4, BTN_TRIGGER_HAPPY2),
    (BTN_GAMESIR_T1, BTN_TRIGGER_HAPPY3),
    (BTN_GAMESIR_T2, BTN_TRIGGER_HAPPY4),
    (BTN_GAMESIR_T3, BTN_TRIGGER_HAPPY5),
    (BTN_GAMESIR_C1, BTN_TRIGGER_HAPPY6),
    (BTN_GAMESIR_C2, BTN_TRIGGER_HAPPY7),
    (BTN_GAMESIR_C3, BTN_TRIGGER_HAPPY8),
    (BTN_GAMESIR_C4, BTN_TRIGGER_HAPPY9),
    (BTN_GAMESIR_SHARE, BTN_TRIGGER_HAPPY10),
];

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_Z: u16 = 0x02;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
const ABS_RZ: u16 = 0x05;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

const DS_EVDEV_VERSION: u16 = 0x8111;

const KEYS: [u16; 13] = [
    BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST, BTN_TL, BTN_TR, BTN_TL2, BTN_TR2, BTN_SELECT,
    BTN_START, BTN_MODE, BTN_THUMBL, BTN_THUMBR,
];

const EDGE_KEYS: [u16; 4] = [
    BTN_TRIGGER_HAPPY1,
    BTN_TRIGGER_HAPPY2,
    BTN_TRIGGER_HAPPY3,
    BTN_TRIGGER_HAPPY4,
];

const AXES: [(u16, i32, i32, i32); 8] = [
    (ABS_X, -32768, 32767, 1000),
    (ABS_Y, -32768, 32767, 1000),
    (ABS_RX, -32768, 32767, 1000),
    (ABS_RY, -32768, 32767, 1000),
    (ABS_Z, 0, 255, 0),
    (ABS_RZ, 0, 255, 0),
    (ABS_HAT0X, -1, 1, 0),
    (ABS_HAT0Y, -1, 1, 0),
];

pub struct VirtualGamepad {
    dev: UinputDevice,
    kind: ControllerKind,
}

impl VirtualGamepad {
    pub fn new(kind: ControllerKind, vendor: u16, product: u16) -> Result<Self> {
        let mut keys = KEYS.to_vec();
        if kind == ControllerKind::DualSenseEdge {
            keys.extend_from_slice(&EDGE_KEYS);
        } else if kind.is_steam() {
            keys.extend(STEAM_EXTRA_KEYS.iter().map(|(_, key)| *key));
        } else if kind.is_eightbitdo() {
            keys.extend(EIGHTBITDO_EXTRA_KEYS.iter().map(|(_, key)| *key));
        } else if kind.is_flydigi() {
            if kind == ControllerKind::FlydigiVader5Pro {
                keys.extend(VADER5_EXTRA_KEYS.iter().map(|(_, key)| *key));
            } else {
                keys.extend(APEX5_EXTRA_KEYS.iter().map(|(_, key)| *key));
            }
        } else if kind.is_gamesir() {
            keys.extend(GAMESIR_EXTRA_KEYS.iter().map(|(_, key)| *key));
        }
        let name = match kind.family() {
            crate::inputs::ControllerFamily::Steam => "Gyro4U Steam Controller",
            crate::inputs::ControllerFamily::EightBitDo => "Gyro4U 8BitDo Pro 3",
            crate::inputs::ControllerFamily::Flydigi => match kind {
                ControllerKind::FlydigiVader5Pro => "Gyro4U Flydigi Vader 5 Pro",
                _ => "Gyro4U Flydigi Apex 5",
            },
            crate::inputs::ControllerFamily::GameSir => "Gyro4U GameSir Tarantula Pro",
            crate::inputs::ControllerFamily::DualSense => "Gyro4U DualSense Wireless Controller",
        };
        let dev = UinputDevice::create(UinputSpec {
            name,
            vendor,
            product,
            version: DS_EVDEV_VERSION,
            keys: &keys,
            axes: &AXES,
            rels: &[],
            props: &[],
        })?;
        Ok(Self { dev, kind })
    }

    fn axis(raw: u8) -> i32 {
        (((raw as i32) - 128) * 256).clamp(-32768, 32767)
    }

    pub fn update(&mut self, s: &ControllerState) -> Result<()> {
        let dev = &mut self.dev;
        let mut changed = false;

        changed |= dev.emit(EV_ABS, ABS_X, Self::axis(s.left_x));
        changed |= dev.emit(EV_ABS, ABS_Y, Self::axis(s.left_y));
        changed |= dev.emit(EV_ABS, ABS_RX, Self::axis(s.right_x));
        changed |= dev.emit(EV_ABS, ABS_RY, Self::axis(s.right_y));
        changed |= dev.emit(EV_ABS, ABS_Z, s.l2 as i32);
        changed |= dev.emit(EV_ABS, ABS_RZ, s.r2 as i32);

        let (hx, hy) = match s.dpad {
            DPAD_N => (0, -1),
            DPAD_NE => (1, -1),
            DPAD_E => (1, 0),
            DPAD_SE => (1, 1),
            DPAD_S => (0, 1),
            DPAD_SW => (-1, 1),
            DPAD_W => (-1, 0),
            DPAD_NW => (-1, -1),
            _ => (0, 0),
        };
        changed |= dev.emit(EV_ABS, ABS_HAT0X, hx);
        changed |= dev.emit(EV_ABS, ABS_HAT0Y, hy);

        let b = s.buttons;
        changed |= dev.emit(EV_KEY, BTN_SOUTH, (b & BTN_CROSS != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_EAST, (b & BTN_CIRCLE != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_WEST, (b & BTN_SQUARE != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_NORTH, (b & BTN_TRIANGLE != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_TL, (b & BTN_L1 != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_TR, (b & BTN_R1 != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_TL2, (b & BTN_L2 != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_TR2, (b & BTN_R2 != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_SELECT, (b & BTN_CREATE != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_START, (b & BTN_OPTIONS != 0) as i32);
        let mode = b & BTN_PS != 0 || s.extra_buttons & EXTRA_BTN_STEAM != 0;
        changed |= dev.emit(EV_KEY, BTN_MODE, mode as i32);
        changed |= dev.emit(EV_KEY, BTN_THUMBL, (b & BTN_L3 != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_THUMBR, (b & BTN_R3 != 0) as i32);

        if self.kind == ControllerKind::DualSenseEdge {
            changed |= dev.emit(
                EV_KEY,
                BTN_TRIGGER_HAPPY1,
                (b & BTN_EDGE_FN_LEFT != 0) as i32,
            );
            changed |= dev.emit(
                EV_KEY,
                BTN_TRIGGER_HAPPY2,
                (b & BTN_EDGE_FN_RIGHT != 0) as i32,
            );
            changed |= dev.emit(
                EV_KEY,
                BTN_TRIGGER_HAPPY3,
                (b & BTN_EDGE_PADDLE_LEFT != 0) as i32,
            );
            changed |= dev.emit(
                EV_KEY,
                BTN_TRIGGER_HAPPY4,
                (b & BTN_EDGE_PADDLE_RIGHT != 0) as i32,
            );
        } else if self.kind.is_steam() {
            for (mask, key) in STEAM_EXTRA_KEYS {
                changed |= dev.emit(EV_KEY, key, (s.extra_buttons & mask != 0) as i32);
            }
        } else if self.kind.is_eightbitdo() {
            for (mask, key) in EIGHTBITDO_EXTRA_KEYS {
                changed |= dev.emit(EV_KEY, key, (s.extra_buttons & mask != 0) as i32);
            }
        } else if self.kind.is_flydigi() {
            let keys = if self.kind == ControllerKind::FlydigiVader5Pro {
                &VADER5_EXTRA_KEYS[..]
            } else {
                &APEX5_EXTRA_KEYS[..]
            };
            for &(mask, key) in keys {
                changed |= dev.emit(EV_KEY, key, (s.extra_buttons & mask != 0) as i32);
            }
        } else if self.kind.is_gamesir() {
            for (mask, key) in GAMESIR_EXTRA_KEYS {
                changed |= dev.emit(EV_KEY, key, (s.buttons & mask != 0) as i32);
            }
        }

        if !changed {
            return Ok(());
        }
        dev.sync()?;
        Ok(())
    }

    pub fn reset(&mut self) {
        let _ = self.update(&ControllerState::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn steam_extra_keys_cover_every_extra_mask_once() {
        let masks: HashSet<u32> = STEAM_EXTRA_KEYS.iter().map(|(mask, _)| *mask).collect();
        let keys: HashSet<u16> = STEAM_EXTRA_KEYS.iter().map(|(_, key)| *key).collect();
        assert_eq!(masks.len(), STEAM_EXTRA_KEYS.len());
        assert_eq!(keys.len(), STEAM_EXTRA_KEYS.len());
        for (mask, _) in STEAM_EXTRA_KEYS {
            assert_eq!(mask.count_ones(), 1);
            assert!(mask <= crate::inputs::EXTRA_BTN_RIGHT_TRIGGER_CLICK);
        }
    }

    #[test]
    fn eightbitdo_extra_keys_use_happy_buttons_and_disjoint_masks() {
        let masks: HashSet<u32> = EIGHTBITDO_EXTRA_KEYS
            .iter()
            .map(|(mask, _)| *mask)
            .collect();
        let keys: HashSet<u16> = EIGHTBITDO_EXTRA_KEYS.iter().map(|(_, key)| *key).collect();
        assert_eq!(masks.len(), 4);
        assert_eq!(keys.len(), 4);
        assert_eq!(
            keys,
            HashSet::from([
                BTN_TRIGGER_HAPPY1,
                BTN_TRIGGER_HAPPY2,
                BTN_TRIGGER_HAPPY3,
                BTN_TRIGGER_HAPPY4,
            ])
        );
        for (mask, _) in EIGHTBITDO_EXTRA_KEYS {
            assert_eq!(mask.count_ones(), 1);
            assert!(mask >= crate::inputs::EXTRA_BTN_EIGHTBITDO_L4);
            assert!(mask <= crate::inputs::EXTRA_BTN_EIGHTBITDO_PR);
        }
        for (_, key) in EIGHTBITDO_EXTRA_KEYS {
            assert!(!KEYS.contains(&key), "{key:#x} collides with a base key");
        }
    }

    #[test]
    fn apex5_extra_keys_cover_happy1_through_6() {
        let masks: HashSet<u32> = APEX5_EXTRA_KEYS.iter().map(|(mask, _)| *mask).collect();
        let keys: HashSet<u16> = APEX5_EXTRA_KEYS.iter().map(|(_, key)| *key).collect();
        assert_eq!(masks.len(), 6);
        assert_eq!(keys.len(), 6);
        assert_eq!(
            keys,
            HashSet::from([
                BTN_TRIGGER_HAPPY1,
                BTN_TRIGGER_HAPPY2,
                BTN_TRIGGER_HAPPY3,
                BTN_TRIGGER_HAPPY4,
                BTN_TRIGGER_HAPPY5,
                BTN_TRIGGER_HAPPY6,
            ])
        );
        for (mask, _) in APEX5_EXTRA_KEYS {
            assert_eq!(mask.count_ones(), 1);
            assert!(mask >= crate::inputs::APEX_LM);
            assert!(mask <= crate::inputs::APEX_M4);
        }
        for (_, key) in APEX5_EXTRA_KEYS {
            assert!(!KEYS.contains(&key), "{key:#x} collides with a base key");
        }
    }

    #[test]
    fn vader5_extra_keys_cover_happy1_through_9() {
        let masks: HashSet<u32> = VADER5_EXTRA_KEYS.iter().map(|(mask, _)| *mask).collect();
        let keys: HashSet<u16> = VADER5_EXTRA_KEYS.iter().map(|(_, key)| *key).collect();
        assert_eq!(masks.len(), 9);
        assert_eq!(keys.len(), 9);
        assert_eq!(
            keys,
            HashSet::from([
                BTN_TRIGGER_HAPPY1,
                BTN_TRIGGER_HAPPY2,
                BTN_TRIGGER_HAPPY3,
                BTN_TRIGGER_HAPPY4,
                BTN_TRIGGER_HAPPY5,
                BTN_TRIGGER_HAPPY6,
                BTN_TRIGGER_HAPPY7,
                BTN_TRIGGER_HAPPY8,
                BTN_TRIGGER_HAPPY9,
            ])
        );
        for (mask, _) in VADER5_EXTRA_KEYS {
            assert_eq!(mask.count_ones(), 1);
            assert!(mask >= crate::inputs::APEX_LM);
            assert!(mask <= crate::inputs::VADER_CIRCLE);
        }
        for (_, key) in VADER5_EXTRA_KEYS {
            assert!(!KEYS.contains(&key), "{key:#x} collides with a base key");
        }
    }

    #[test]
    fn gamesir_extra_keys_cover_happy1_through_10_in_buttons() {
        let masks: HashSet<u32> = GAMESIR_EXTRA_KEYS.iter().map(|(mask, _)| *mask).collect();
        let keys: HashSet<u16> = GAMESIR_EXTRA_KEYS.iter().map(|(_, key)| *key).collect();
        assert_eq!(masks.len(), 10);
        assert_eq!(keys.len(), 10);
        assert_eq!(
            keys,
            HashSet::from([
                BTN_TRIGGER_HAPPY1,
                BTN_TRIGGER_HAPPY2,
                BTN_TRIGGER_HAPPY3,
                BTN_TRIGGER_HAPPY4,
                BTN_TRIGGER_HAPPY5,
                BTN_TRIGGER_HAPPY6,
                BTN_TRIGGER_HAPPY7,
                BTN_TRIGGER_HAPPY8,
                BTN_TRIGGER_HAPPY9,
                BTN_TRIGGER_HAPPY10,
            ])
        );
        for (mask, _) in GAMESIR_EXTRA_KEYS {
            assert_eq!(mask.count_ones(), 1);
            assert!(mask >= crate::inputs::BTN_GAMESIR_L4);
            assert!(mask <= crate::inputs::BTN_GAMESIR_SHARE);
            assert_eq!(mask & 0xe000_0000, 0, "bits 29-31 stay reserved");
        }
        for (_, key) in GAMESIR_EXTRA_KEYS {
            assert!(!KEYS.contains(&key), "{key:#x} collides with a base key");
        }
    }
}
