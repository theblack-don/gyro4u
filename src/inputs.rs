use serde::{Deserialize, Serialize};

pub const TOUCHPAD_MAX_X: u16 = 1920;
pub const TOUCHPAD_MAX_Y: u16 = 1080;
pub const STEAM_PAD_RANGE: f32 = 65536.0;
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

// Steam Controller extras live outside the Sony-shaped u32 mask.
pub const EXTRA_BTN_STEAM: u32 = 1 << 0;
pub const EXTRA_BTN_QAM: u32 = 1 << 1;
pub const EXTRA_BTN_L4: u32 = 1 << 2;
pub const EXTRA_BTN_L5: u32 = 1 << 3;
pub const EXTRA_BTN_R4: u32 = 1 << 4;
pub const EXTRA_BTN_R5: u32 = 1 << 5;
pub const EXTRA_BTN_LEFT_GRIP: u32 = 1 << 6;
pub const EXTRA_BTN_RIGHT_GRIP: u32 = 1 << 7;
pub const EXTRA_BTN_LEFT_STICK_TOUCH: u32 = 1 << 8;
pub const EXTRA_BTN_RIGHT_STICK_TOUCH: u32 = 1 << 9;
pub const EXTRA_BTN_LEFT_PAD_TOUCH: u32 = 1 << 10;
pub const EXTRA_BTN_RIGHT_PAD_TOUCH: u32 = 1 << 11;
pub const EXTRA_BTN_LEFT_PAD: u32 = 1 << 12;
pub const EXTRA_BTN_RIGHT_PAD: u32 = 1 << 13;
pub const EXTRA_BTN_LEFT_TRIGGER_CLICK: u32 = 1 << 14;
pub const EXTRA_BTN_RIGHT_TRIGGER_CLICK: u32 = 1 << 15;

pub const DS_GYRO_COUNTS_PER_DPS: f32 = 16.0;
pub const DS_ACCEL_COUNTS_PER_G: f32 = 8192.0;
pub const DS_TIMESTAMP_UNIT_US: f32 = 1.0 / 3.0;

fn default_gyro_counts_per_dps() -> f32 {
    DS_GYRO_COUNTS_PER_DPS
}

fn default_accel_counts_per_g() -> f32 {
    DS_ACCEL_COUNTS_PER_G
}

fn default_timestamp_unit_us() -> f32 {
    DS_TIMESTAMP_UNIT_US
}

#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Debug)]
pub struct TouchPoint {
    pub active: bool,
    pub id: u8,
    pub x: u16,
    pub y: u16,
}

#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq)]
pub struct SteamPadState {
    pub touch: bool,
    pub click: bool,
    pub x: i16,
    pub y: i16,
    pub pressure: u16,
}

impl SteamPadState {
    pub fn normalized(&self) -> (f32, f32) {
        (
            (self.x as f32 / STEAM_PAD_RANGE + 0.5).clamp(0.0, 1.0),
            (-(self.y as f32) / STEAM_PAD_RANGE + 0.5).clamp(0.0, 1.0),
        )
    }

    pub fn pressure_normalized(&self) -> f32 {
        (self.pressure as f32 / 32768.0).clamp(0.0, 1.0)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
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
    #[serde(default)]
    pub extra_buttons: u32,
    #[serde(default)]
    pub steam_left_pad: SteamPadState,
    #[serde(default)]
    pub steam_right_pad: SteamPadState,
    #[serde(default = "default_gyro_counts_per_dps")]
    pub gyro_counts_per_dps: f32,
    #[serde(default = "default_accel_counts_per_g")]
    pub accel_counts_per_g: f32,
    #[serde(default = "default_timestamp_unit_us")]
    pub timestamp_unit_us: f32,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
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
            extra_buttons: 0,
            steam_left_pad: SteamPadState::default(),
            steam_right_pad: SteamPadState::default(),
            gyro_counts_per_dps: DS_GYRO_COUNTS_PER_DPS,
            accel_counts_per_g: DS_ACCEL_COUNTS_PER_G,
            timestamp_unit_us: DS_TIMESTAMP_UNIT_US,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Debug, Default)]
pub enum ControllerKind {
    #[default]
    DualSense,
    DualSenseEdge,
    SteamController,
}

impl ControllerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::DualSense => "DualSense",
            Self::DualSenseEdge => "DualSense Edge",
            Self::SteamController => "Steam Controller (2026)",
        }
    }

    pub fn is_steam(self) -> bool {
        self == Self::SteamController
    }

    pub fn target_buttons(self) -> &'static [Button] {
        if self.is_steam() {
            &STEAM_TARGETS
        } else {
            &GAMEPAD_TARGETS
        }
    }

    pub fn capabilities(self) -> ControllerCapabilities {
        if self.is_steam() {
            ControllerCapabilities {
                kind: self,
                lightbar: false,
                player_leds: false,
                microphone: false,
                adaptive_triggers: false,
                haptics: false,
                trackpad_haptics: true,
                firmware: false,
                motion: true,
                touchpad: TouchpadKind::DualCircular,
            }
        } else {
            ControllerCapabilities {
                kind: self,
                lightbar: true,
                player_leds: true,
                microphone: true,
                adaptive_triggers: true,
                haptics: true,
                trackpad_haptics: false,
                firmware: true,
                motion: true,
                touchpad: TouchpadKind::DualSensePanel,
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TouchpadKind {
    DualSensePanel,
    DualCircular,
}

#[derive(Clone, Copy, Debug)]
pub struct ControllerCapabilities {
    pub kind: ControllerKind,
    pub lightbar: bool,
    pub player_leds: bool,
    pub microphone: bool,
    pub adaptive_triggers: bool,
    pub haptics: bool,
    pub trackpad_haptics: bool,
    pub firmware: bool,
    pub motion: bool,
    pub touchpad: TouchpadKind,
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
    Steam,
    Qam,
    L4,
    L5,
    R4,
    R5,
    LeftGrip,
    RightGrip,
    LeftStickTouch,
    RightStickTouch,
    LeftPadTouch,
    RightPadTouch,
    LeftPad,
    RightPad,
    LeftTriggerClick,
    RightTriggerClick,
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

pub const STEAM_TARGETS: [Button; 30] = [
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
    Button::Steam,
    Button::Qam,
    Button::L4,
    Button::L5,
    Button::R4,
    Button::R5,
    Button::LeftGrip,
    Button::RightGrip,
    Button::LeftPad,
    Button::RightPad,
    Button::LeftStickTouch,
    Button::RightStickTouch,
    Button::LeftPadTouch,
    Button::RightPadTouch,
];

pub const ALL_BUTTONS: [Button; 47] = [
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
    Button::Steam,
    Button::Qam,
    Button::L4,
    Button::L5,
    Button::R4,
    Button::R5,
    Button::LeftGrip,
    Button::RightGrip,
    Button::LeftStickTouch,
    Button::RightStickTouch,
    Button::LeftPadTouch,
    Button::RightPadTouch,
    Button::LeftPad,
    Button::RightPad,
    Button::LeftTriggerClick,
    Button::RightTriggerClick,
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
            Button::DPadUp
            | Button::DPadDown
            | Button::DPadLeft
            | Button::DPadRight
            | Button::LeftStickUp
            | Button::LeftStickDown
            | Button::LeftStickLeft
            | Button::LeftStickRight
            | Button::RightStickUp
            | Button::RightStickDown
            | Button::RightStickLeft
            | Button::RightStickRight
            | Button::Steam
            | Button::Qam
            | Button::L4
            | Button::L5
            | Button::R4
            | Button::R5
            | Button::LeftGrip
            | Button::RightGrip
            | Button::LeftStickTouch
            | Button::RightStickTouch
            | Button::LeftPadTouch
            | Button::RightPadTouch
            | Button::LeftPad
            | Button::RightPad
            | Button::LeftTriggerClick
            | Button::RightTriggerClick => None,
        }
    }

    pub fn to_extra_mask(self) -> Option<u32> {
        match self {
            Button::Steam => Some(EXTRA_BTN_STEAM),
            Button::Qam => Some(EXTRA_BTN_QAM),
            Button::L4 => Some(EXTRA_BTN_L4),
            Button::L5 => Some(EXTRA_BTN_L5),
            Button::R4 => Some(EXTRA_BTN_R4),
            Button::R5 => Some(EXTRA_BTN_R5),
            Button::LeftGrip => Some(EXTRA_BTN_LEFT_GRIP),
            Button::RightGrip => Some(EXTRA_BTN_RIGHT_GRIP),
            Button::LeftStickTouch => Some(EXTRA_BTN_LEFT_STICK_TOUCH),
            Button::RightStickTouch => Some(EXTRA_BTN_RIGHT_STICK_TOUCH),
            Button::LeftPadTouch => Some(EXTRA_BTN_LEFT_PAD_TOUCH),
            Button::RightPadTouch => Some(EXTRA_BTN_RIGHT_PAD_TOUCH),
            Button::LeftPad => Some(EXTRA_BTN_LEFT_PAD),
            Button::RightPad => Some(EXTRA_BTN_RIGHT_PAD),
            Button::LeftTriggerClick => Some(EXTRA_BTN_LEFT_TRIGGER_CLICK),
            Button::RightTriggerClick => Some(EXTRA_BTN_RIGHT_TRIGGER_CLICK),
            _ => None,
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
            _ => {
                if let Some(mask) = self.to_bitmask() {
                    s.buttons & mask != 0
                } else if let Some(mask) = self.to_extra_mask() {
                    s.extra_buttons & mask != 0
                } else {
                    false
                }
            }
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
            Button::Steam => "Steam",
            Button::Qam => "QAM",
            Button::L4 => "L4",
            Button::L5 => "L5",
            Button::R4 => "R4",
            Button::R5 => "R5",
            Button::LeftGrip => "Left Grip",
            Button::RightGrip => "Right Grip",
            Button::LeftStickTouch => "Left Stick Touch",
            Button::RightStickTouch => "Right Stick Touch",
            Button::LeftPadTouch => "Left Pad Touch",
            Button::RightPadTouch => "Right Pad Touch",
            Button::LeftPad => "Left Pad Click",
            Button::RightPad => "Right Pad Click",
            Button::LeftTriggerClick => "Left Trigger Click",
            Button::RightTriggerClick => "Right Trigger Click",
        }
    }

    pub fn device_label(self, kind: ControllerKind) -> &'static str {
        if !kind.is_steam() {
            return self.label();
        }
        match self {
            Button::Cross => "A",
            Button::Circle => "B",
            Button::Square => "X",
            Button::Triangle => "Y",
            Button::Create => "View",
            Button::Options => "Menu",
            Button::Touchpad => "Left Pad Click",
            Button::Mute => "Right Pad Click",
            _ => self.label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn capabilities_gate_sony_only_features_for_steam() {
        let steam = ControllerKind::SteamController.capabilities();
        assert!(!steam.lightbar);
        assert!(!steam.player_leds);
        assert!(!steam.microphone);
        assert!(!steam.adaptive_triggers);
        assert!(!steam.haptics);
        assert!(steam.trackpad_haptics);
        assert!(!steam.firmware);
        assert!(steam.motion);
        assert_eq!(steam.touchpad, TouchpadKind::DualCircular);

        for kind in [ControllerKind::DualSense, ControllerKind::DualSenseEdge] {
            let sony = kind.capabilities();
            assert!(sony.lightbar);
            assert!(sony.adaptive_triggers);
            assert!(sony.haptics);
            assert!(!sony.trackpad_haptics);
            assert!(sony.firmware);
            assert_eq!(sony.touchpad, TouchpadKind::DualSensePanel);
        }
    }

    #[test]
    fn steam_targets_replace_sony_only_outputs() {
        let steam = ControllerKind::SteamController.target_buttons();
        assert!(steam.contains(&Button::Steam));
        assert!(steam.contains(&Button::Qam));
        assert!(steam.contains(&Button::L4));
        assert!(steam.contains(&Button::RightPad));
        assert!(!steam.contains(&Button::EdgeFnLeft));
        assert!(!steam.contains(&Button::Mute));

        assert!(
            ControllerKind::DualSense
                .target_buttons()
                .contains(&Button::EdgePaddleLeft)
        );
    }

    #[test]
    fn extra_button_masks_are_unique_and_stay_out_of_the_sony_mask() {
        let mut masks = HashSet::new();
        for button in ALL_BUTTONS {
            if let Some(mask) = button.to_extra_mask() {
                assert!(masks.insert(mask), "duplicate extra mask for {button:?}");
                assert!(button.to_bitmask().is_none());
            }
        }
        assert_eq!(masks.len(), 16);
    }

    #[test]
    fn controller_state_serde_defaults_new_fields() {
        let json = r#"{
            "left_x":128,"left_y":128,"right_x":128,"right_y":128,
            "l2":0,"r2":0,"buttons":0,"dpad":8,"gyro":[0,0,0],
            "accel":[0,0,0],"sensor_timestamp":0,"touch_count":0,
            "touch_points":[{"active":false,"id":0,"x":0,"y":0},
                            {"active":false,"id":0,"x":0,"y":0}]
        }"#;
        let state: ControllerState = serde_json::from_str(json).expect("legacy state parses");
        assert_eq!(state.extra_buttons, 0);
        assert!(!state.steam_left_pad.touch);
        assert_eq!(state.gyro_counts_per_dps, DS_GYRO_COUNTS_PER_DPS);
        assert_eq!(state.timestamp_unit_us, DS_TIMESTAMP_UNIT_US);
    }
}
