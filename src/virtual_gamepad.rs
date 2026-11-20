#![allow(dead_code)]

use anyhow::Result;

use crate::common::DSE_PID;
use crate::inputs::{
    BTN_CIRCLE, BTN_CREATE, BTN_CROSS, BTN_EDGE_FN_LEFT, BTN_EDGE_FN_RIGHT, BTN_EDGE_PADDLE_LEFT,
    BTN_EDGE_PADDLE_RIGHT, BTN_L1, BTN_L2, BTN_L3, BTN_OPTIONS, BTN_PS, BTN_R1, BTN_R2, BTN_R3,
    BTN_SQUARE, BTN_TRIANGLE, ControllerState, DPAD_E, DPAD_N, DPAD_NE, DPAD_NEUTRAL, DPAD_NW,
    DPAD_S, DPAD_SE, DPAD_SW, DPAD_W, TouchPoint,
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
    has_edge: bool,
}

impl VirtualGamepad {
    pub fn new(vendor: u16, product: u16) -> Result<Self> {
        let has_edge = product == DSE_PID;
        let mut keys = KEYS.to_vec();
        if has_edge {
            keys.extend_from_slice(&EDGE_KEYS);
        }
        let dev = UinputDevice::create(UinputSpec {
            name: "Gyro4U DualSense Wireless Controller",
            vendor,
            product,
            version: DS_EVDEV_VERSION,
            keys: &keys,
            axes: &AXES,
            rels: &[],
            props: &[],
        })?;
        Ok(Self { dev, has_edge })
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
        changed |= dev.emit(EV_KEY, BTN_MODE, (b & BTN_PS != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_THUMBL, (b & BTN_L3 != 0) as i32);
        changed |= dev.emit(EV_KEY, BTN_THUMBR, (b & BTN_R3 != 0) as i32);

        if self.has_edge {
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
        }

        if !changed {
            return Ok(());
        }
        dev.sync()?;
        Ok(())
    }

    pub fn reset(&mut self) {
        let neutral = ControllerState {
            left_x: 128,
            left_y: 128,
            right_x: 128,
            right_y: 128,
            l2: 0,
            r2: 0,
            buttons: 0,
            dpad: DPAD_NEUTRAL,
            gyro: [0; 3],
            accel: [0; 3],
            sensor_timestamp: 0,
            touch_count: 0,
            touch_points: [TouchPoint::default(); 2],
        };
        let _ = self.update(&neutral);
    }
}
