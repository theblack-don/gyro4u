use serde::{Deserialize, Serialize};

pub const TOUCHPAD_MAX_X: u16 = 1920;
pub const TOUCHPAD_MAX_Y: u16 = 1080;
pub const DPAD_N: u8 = 0;
pub const DPAD_NE: u8 = 1;
pub const DPAD_E: u8 = 2;
pub const DPAD_SE: u8 = 3;
pub const DPAD_S: u8 = 4;
pub const DPAD_SW: u8 = 5;
pub const DPAD_W: u8 = 6;
pub const DPAD_NW: u8 = 7;
pub const DPAD_NEUTRAL: u8 = 8;
pub const BTN_SQUARE: u32 = 1 << 0;
pub const BTN_CROSS: u32 = 1 << 1;
pub const BTN_CIRCLE: u32 = 1 << 2;
pub const BTN_TRIANGLE: u32 = 1 << 3;
pub const BTN_L1: u32 = 1 << 4;
pub const BTN_R1: u32 = 1 << 5;
pub const BTN_L2: u32 = 1 << 6;
pub const BTN_R2: u32 = 1 << 7;
pub const BTN_CREATE: u32 = 1 << 8;
pub const BTN_OPTIONS: u32 = 1 << 9;
pub const BTN_L3: u32 = 1 << 10;
pub const BTN_R3: u32 = 1 << 11;
pub const BTN_PS: u32 = 1 << 12;
pub const BTN_TOUCHPAD: u32 = 1 << 13;
pub const BTN_MUTE: u32 = 1 << 14;
pub const BTN_EDGE_FN_LEFT: u32 = 1 << 15;
pub const BTN_EDGE_FN_RIGHT: u32 = 1 << 16;
pub const BTN_EDGE_PADDLE_LEFT: u32 = 1 << 17;
pub const BTN_EDGE_PADDLE_RIGHT: u32 = 1 << 18;

#[derive(Serialize, Deserialize, Default, Clone, Copy)]
pub struct TouchPoint {
    pub active: bool,
    pub id: u8,
    pub x: u16,
    pub y: u16,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct ControllerState {
    pub left_x: u8,
    pub left_y: u8,
    pub right_x: u8,
    pub right_y: u8,
    pub l2: u8,
    pub r2: u8,
    pub buttons: u32,
    pub dpad: u8,
    pub gyro: [i16; 3],
    pub accel: [i16; 3],
    pub sensor_timestamp: u32,
    pub touch_count: u8,
    pub touch_points: [TouchPoint; 2],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Debug, Default)]
pub enum Button {
    #[default]
    Create,
    L3,
    R3,
    Options,
    DPadUp,
    DPadRight,
    DPadDown,
    DPadLeft,
    L2,
    R2,
    L1,
    R1,
    Triangle,
    Circle,
    Cross,
    Square,
    PS,
    Touchpad,
    Mute,
    EdgeFnLeft,
    EdgeFnRight,
    EdgePaddleLeft,
    EdgePaddleRight,
    LeftStickUp,
    LeftStickDown,
    LeftStickLeft,
    LeftStickRight,
    RightStickUp,
    RightStickDown,
    RightStickLeft,
    RightStickRight,
}

pub const GAMEPAD_TARGETS: [Button; 23] = [
    Button::Cross,
    Button::Circle,
    Button::Square,
    Button::Triangle,
    Button::DPadUp,
    Button::DPadRight,
    Button::DPadDown,
    Button::DPadLeft,
    Button::L1,
    Button::R1,
    Button::L2,
    Button::R2,
    Button::L3,
    Button::R3,
    Button::Create,
    Button::Options,
    Button::PS,
    Button::Mute,
    Button::Touchpad,
    Button::EdgeFnLeft,
    Button::EdgeFnRight,
    Button::EdgePaddleLeft,
    Button::EdgePaddleRight,
];

pub const ALL_BUTTONS: [Button; 31] = [
    Button::Cross,
    Button::Circle,
    Button::Square,
    Button::Triangle,
    Button::DPadUp,
    Button::DPadRight,
    Button::DPadDown,
    Button::DPadLeft,
    Button::L1,
    Button::R1,
    Button::L2,
    Button::R2,
    Button::L3,
    Button::R3,
    Button::Create,
    Button::Options,
    Button::PS,
    Button::Mute,
    Button::Touchpad,
    Button::EdgeFnLeft,
    Button::EdgeFnRight,
    Button::EdgePaddleLeft,
    Button::EdgePaddleRight,
    Button::LeftStickUp,
    Button::LeftStickDown,
    Button::LeftStickLeft,
    Button::LeftStickRight,
    Button::RightStickUp,
    Button::RightStickDown,
    Button::RightStickLeft,
    Button::RightStickRight,
];

impl Button {
    pub fn is_stick_dir(&self) -> bool {
        matches!(
            self,
            Button::LeftStickUp
                | Button::LeftStickDown
                | Button::LeftStickLeft
                | Button::LeftStickRight
                | Button::RightStickUp
                | Button::RightStickDown
                | Button::RightStickLeft
                | Button::RightStickRight
        )
    }

    pub fn is_left_stick(&self) -> bool {
        matches!(
            self,
            Button::LeftStickUp
                | Button::LeftStickDown
                | Button::LeftStickLeft
                | Button::LeftStickRight
        )
    }

    pub fn is_right_stick(&self) -> bool {
        matches!(
            self,
            Button::RightStickUp
                | Button::RightStickDown
                | Button::RightStickLeft
                | Button::RightStickRight
        )
    }

    pub fn to_bitmask(self) -> Option<u32> {
        match self {
            Button::Square => Some(BTN_SQUARE),
            Button::Cross => Some(BTN_CROSS),
            Button::Circle => Some(BTN_CIRCLE),
            Button::Triangle => Some(BTN_TRIANGLE),
            Button::L1 => Some(BTN_L1),
            Button::R1 => Some(BTN_R1),
            Button::L2 => Some(BTN_L2),
            Button::R2 => Some(BTN_R2),
            Button::L3 => Some(BTN_L3),
            Button::R3 => Some(BTN_R3),
            Button::PS => Some(BTN_PS),
            Button::Create => Some(BTN_CREATE),
            Button::Options => Some(BTN_OPTIONS),
            Button::Touchpad => Some(BTN_TOUCHPAD),
            Button::Mute => Some(BTN_MUTE),
            Button::EdgeFnLeft => Some(BTN_EDGE_FN_LEFT),
            Button::EdgeFnRight => Some(BTN_EDGE_FN_RIGHT),
            Button::EdgePaddleLeft => Some(BTN_EDGE_PADDLE_LEFT),
            Button::EdgePaddleRight => Some(BTN_EDGE_PADDLE_RIGHT),
            Button::DPadUp | Button::DPadDown | Button::DPadLeft | Button::DPadRight => None,
            Button::LeftStickUp
            | Button::LeftStickDown
            | Button::LeftStickLeft
            | Button::LeftStickRight
            | Button::RightStickUp
            | Button::RightStickDown
            | Button::RightStickLeft
            | Button::RightStickRight => None,
        }
    }

    pub fn is_pressed(&self, s: &ControllerState) -> bool {
        let dirs = [
            matches!(s.dpad, DPAD_N | DPAD_NE | DPAD_NW),
            matches!(s.dpad, DPAD_E | DPAD_NE | DPAD_SE),
            matches!(s.dpad, DPAD_S | DPAD_SE | DPAD_SW),
            matches!(s.dpad, DPAD_W | DPAD_NW | DPAD_SW),
        ];
        match self {
            Button::DPadUp => dirs[0],
            Button::DPadRight => dirs[1],
            Button::DPadDown => dirs[2],
            Button::DPadLeft => dirs[3],
            Button::LeftStickUp
            | Button::LeftStickDown
            | Button::LeftStickLeft
            | Button::LeftStickRight
            | Button::RightStickUp
            | Button::RightStickDown
            | Button::RightStickLeft
            | Button::RightStickRight => false,
            _ => self.to_bitmask().is_some_and(|m| s.buttons & m != 0),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Button::Cross => "Cross",
            Button::Circle => "Circle",
            Button::Square => "Square",
            Button::Triangle => "Triangle",
            Button::DPadUp => "D-Pad Up",
            Button::DPadRight => "D-Pad Right",
            Button::DPadDown => "D-Pad Down",
            Button::DPadLeft => "D-Pad Left",
            Button::L1 => "L1",
            Button::R1 => "R1",
            Button::L2 => "L2",
            Button::R2 => "R2",
            Button::L3 => "L3",
            Button::R3 => "R3",
            Button::Create => "Create",
            Button::Options => "Options",
            Button::PS => "PS",
            Button::Mute => "Mute",
            Button::Touchpad => "Touchpad",
            Button::EdgeFnLeft => "Fn Left",
            Button::EdgeFnRight => "Fn Right",
            Button::EdgePaddleLeft => "Paddle Left",
            Button::EdgePaddleRight => "Paddle Right",
            Button::LeftStickUp => "Left Stick Up",
            Button::LeftStickDown => "Left Stick Down",
            Button::LeftStickLeft => "Left Stick Left",
            Button::LeftStickRight => "Left Stick Right",
            Button::RightStickUp => "Right Stick Up",
            Button::RightStickDown => "Right Stick Down",
            Button::RightStickLeft => "Right Stick Left",
            Button::RightStickRight => "Right Stick Right",
        }
    }
}
