use serde::{Deserialize, Serialize};

use crate::settings::ControllerPreference;

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
// GameSir Tarantula Pro extras sit above the Sony range; bits 29-31 stay reserved.
pub const BTN_GAMESIR_L4: u32 = 1 << 19;
pub const BTN_GAMESIR_R4: u32 = 1 << 20;
pub const BTN_GAMESIR_T1: u32 = 1 << 21;
pub const BTN_GAMESIR_T2: u32 = 1 << 22;
pub const BTN_GAMESIR_T3: u32 = 1 << 23;
pub const BTN_GAMESIR_C1: u32 = 1 << 24;
pub const BTN_GAMESIR_C2: u32 = 1 << 25;
pub const BTN_GAMESIR_C3: u32 = 1 << 26;
pub const BTN_GAMESIR_C4: u32 = 1 << 27;
pub const BTN_GAMESIR_SHARE: u32 = 1 << 28;

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
pub const EXTRA_BTN_EIGHTBITDO_L4: u32 = 1 << 16;
pub const EXTRA_BTN_EIGHTBITDO_R4: u32 = 1 << 17;
pub const EXTRA_BTN_EIGHTBITDO_PL: u32 = 1 << 18;
pub const EXTRA_BTN_EIGHTBITDO_PR: u32 = 1 << 19;
pub const APEX_LM: u32 = 1 << 20;
pub const APEX_RM: u32 = 1 << 21;
pub const APEX_M1: u32 = 1 << 22;
pub const APEX_M2: u32 = 1 << 23;
pub const APEX_M3: u32 = 1 << 24;
pub const APEX_M4: u32 = 1 << 25;
pub const VADER_C: u32 = 1 << 26;
pub const VADER_Z: u32 = 1 << 27;
pub const VADER_CIRCLE: u32 = 1 << 28;

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
pub enum AbxyLayout {
    #[default]
    Xbox,
    Switch,
}

impl AbxyLayout {
    pub fn label(self) -> &'static str {
        match self {
            Self::Xbox => "Xbox",
            Self::Switch => "Switch",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Debug, Default)]
pub enum ControllerFamily {
    #[default]
    DualSense,
    Steam,
    EightBitDo,
    Flydigi,
    GameSir,
}

impl ControllerFamily {
    pub fn is_steam(self) -> bool {
        matches!(self, Self::Steam)
    }

    pub fn is_eightbitdo(self) -> bool {
        matches!(self, Self::EightBitDo)
    }

    pub fn is_flydigi(self) -> bool {
        matches!(self, Self::Flydigi)
    }

    pub fn is_gamesir(self) -> bool {
        matches!(self, Self::GameSir)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DualSense => "DualSense",
            Self::Steam => "Steam Controller",
            Self::EightBitDo => "8BitDo",
            Self::Flydigi => "Flydigi",
            Self::GameSir => "GameSir",
        }
    }

    pub fn preference(self) -> ControllerPreference {
        match self {
            Self::DualSense => ControllerPreference::DualSense,
            Self::Steam => ControllerPreference::Steam,
            Self::EightBitDo => ControllerPreference::EightBitDo,
            Self::Flydigi => ControllerPreference::Flydigi,
            Self::GameSir => ControllerPreference::GameSir,
        }
    }

    pub fn priority(self) -> u8 {
        match self {
            Self::DualSense => 0,
            Self::Steam => 1,
            Self::EightBitDo => 2,
            Self::Flydigi => 3,
            Self::GameSir => 4,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ProfileGroup {
    DualSense,
    Steam,
    EightBitDo,
    FlydigiApex5,
    FlydigiVader5Pro,
    GameSirTarantulaPro,
}

pub const PROFILE_GROUPS: [ProfileGroup; 6] = [
    ProfileGroup::DualSense,
    ProfileGroup::Steam,
    ProfileGroup::EightBitDo,
    ProfileGroup::FlydigiApex5,
    ProfileGroup::FlydigiVader5Pro,
    ProfileGroup::GameSirTarantulaPro,
];

impl ProfileGroup {
    pub fn all() -> [ProfileGroup; 6] {
        PROFILE_GROUPS
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DualSense => "DualSense / Edge",
            Self::Steam => "Steam Controller (2026)",
            Self::EightBitDo => "8BitDo Pro 3",
            Self::FlydigiApex5 => "Flydigi Apex 5",
            Self::FlydigiVader5Pro => "Flydigi Vader 5 Pro",
            Self::GameSirTarantulaPro => "GameSir Tarantula Pro",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Debug, Default)]
pub enum ControllerKind {
    #[default]
    DualSense,
    DualSenseEdge,
    SteamController,
    EightBitDoPro3,
    FlydigiApex5,
    FlydigiVader5Pro,
    GameSirTarantulaPro,
}

impl ControllerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::DualSense => "DualSense",
            Self::DualSenseEdge => "DualSense Edge",
            Self::SteamController => "Steam Controller (2026)",
            Self::EightBitDoPro3 => "8BitDo Pro 3",
            Self::FlydigiApex5 => "Flydigi Apex 5",
            Self::FlydigiVader5Pro => "Flydigi Vader 5 Pro",
            Self::GameSirTarantulaPro => "GameSir Tarantula Pro",
        }
    }

    pub fn family(self) -> ControllerFamily {
        match self {
            Self::DualSense | Self::DualSenseEdge => ControllerFamily::DualSense,
            Self::SteamController => ControllerFamily::Steam,
            Self::EightBitDoPro3 => ControllerFamily::EightBitDo,
            Self::FlydigiApex5 | Self::FlydigiVader5Pro => ControllerFamily::Flydigi,
            Self::GameSirTarantulaPro => ControllerFamily::GameSir,
        }
    }

    pub fn profile_group(self) -> ProfileGroup {
        match self {
            Self::DualSense | Self::DualSenseEdge => ProfileGroup::DualSense,
            Self::SteamController => ProfileGroup::Steam,
            Self::EightBitDoPro3 => ProfileGroup::EightBitDo,
            Self::FlydigiApex5 => ProfileGroup::FlydigiApex5,
            Self::FlydigiVader5Pro => ProfileGroup::FlydigiVader5Pro,
            Self::GameSirTarantulaPro => ProfileGroup::GameSirTarantulaPro,
        }
    }

    pub fn same_profile_group(self, other: ControllerKind) -> bool {
        self.profile_group() == other.profile_group()
    }

    pub fn is_steam(self) -> bool {
        self.family().is_steam()
    }

    pub fn is_eightbitdo(self) -> bool {
        self.family().is_eightbitdo()
    }

    pub fn is_flydigi(self) -> bool {
        self.family().is_flydigi()
    }

    pub fn is_gamesir(self) -> bool {
        self.family().is_gamesir()
    }

    pub fn is_vader5(self) -> bool {
        self == Self::FlydigiVader5Pro
    }

    pub fn target_buttons(self) -> &'static [Button] {
        match self.family() {
            ControllerFamily::Steam => &STEAM_TARGETS,
            ControllerFamily::EightBitDo => &EIGHTBITDO_TARGETS,
            ControllerFamily::Flydigi => match self {
                Self::FlydigiVader5Pro => &VADER5_TARGETS,
                _ => &APEX5_TARGETS,
            },
            ControllerFamily::GameSir => &TARANTULA_TARGETS,
            ControllerFamily::DualSense => &GAMEPAD_TARGETS,
        }
    }

    pub fn capabilities(self) -> ControllerCapabilities {
        match self.family() {
            ControllerFamily::Steam => ControllerCapabilities {
                kind: self,
                lightbar: false,
                player_leds: false,
                microphone: false,
                adaptive_triggers: false,
                haptics: false,
                trackpad_haptics: true,
                rumble: false,
                firmware: false,
                motion: true,
                touchpad: TouchpadKind::DualCircular,
            },
            ControllerFamily::EightBitDo
            | ControllerFamily::Flydigi
            | ControllerFamily::GameSir => ControllerCapabilities {
                kind: self,
                lightbar: false,
                player_leds: false,
                microphone: false,
                adaptive_triggers: false,
                haptics: false,
                trackpad_haptics: false,
                rumble: true,
                firmware: false,
                motion: true,
                touchpad: TouchpadKind::None,
            },
            ControllerFamily::DualSense => ControllerCapabilities {
                kind: self,
                lightbar: true,
                player_leds: true,
                microphone: true,
                adaptive_triggers: true,
                haptics: true,
                trackpad_haptics: false,
                rumble: true,
                firmware: true,
                motion: true,
                touchpad: TouchpadKind::DualSensePanel,
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TouchpadKind {
    DualSensePanel,
    DualCircular,
    None,
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
    pub rumble: bool,
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
    EightBitDoL4,
    EightBitDoR4,
    EightBitDoPL,
    EightBitDoPR,
    ApexLM,
    ApexRM,
    ApexM1,
    ApexM2,
    ApexM3,
    ApexM4,
    VaderC,
    VaderZ,
    VaderCircle,
    GameSirL4,
    GameSirR4,
    GameSirT1,
    GameSirT2,
    GameSirT3,
    GameSirC1,
    GameSirC2,
    GameSirC3,
    GameSirC4,
    GameSirShare,
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

pub const EIGHTBITDO_TARGETS: [Button; 21] = [
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
    Button::EightBitDoL4,
    Button::EightBitDoR4,
    Button::EightBitDoPL,
    Button::EightBitDoPR,
];

pub const APEX5_TARGETS: [Button; 23] = [
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
    Button::ApexLM,
    Button::ApexRM,
    Button::ApexM1,
    Button::ApexM2,
    Button::ApexM3,
    Button::ApexM4,
];

pub const VADER5_TARGETS: [Button; 26] = [
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
    Button::ApexLM,
    Button::ApexRM,
    Button::ApexM1,
    Button::ApexM2,
    Button::ApexM3,
    Button::ApexM4,
    Button::VaderC,
    Button::VaderZ,
    Button::VaderCircle,
];

pub const TARANTULA_TARGETS: [Button; 27] = [
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
    Button::GameSirL4,
    Button::GameSirR4,
    Button::GameSirT1,
    Button::GameSirT2,
    Button::GameSirT3,
    Button::GameSirC1,
    Button::GameSirC2,
    Button::GameSirC3,
    Button::GameSirC4,
    Button::GameSirShare,
];

pub const ALL_BUTTONS: [Button; 70] = [
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
    Button::EightBitDoL4,
    Button::EightBitDoR4,
    Button::EightBitDoPL,
    Button::EightBitDoPR,
    Button::ApexLM,
    Button::ApexRM,
    Button::ApexM1,
    Button::ApexM2,
    Button::ApexM3,
    Button::ApexM4,
    Button::VaderC,
    Button::VaderZ,
    Button::VaderCircle,
    Button::GameSirL4,
    Button::GameSirR4,
    Button::GameSirT1,
    Button::GameSirT2,
    Button::GameSirT3,
    Button::GameSirC1,
    Button::GameSirC2,
    Button::GameSirC3,
    Button::GameSirC4,
    Button::GameSirShare,
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
            Button::GameSirL4 => Some(BTN_GAMESIR_L4),
            Button::GameSirR4 => Some(BTN_GAMESIR_R4),
            Button::GameSirT1 => Some(BTN_GAMESIR_T1),
            Button::GameSirT2 => Some(BTN_GAMESIR_T2),
            Button::GameSirT3 => Some(BTN_GAMESIR_T3),
            Button::GameSirC1 => Some(BTN_GAMESIR_C1),
            Button::GameSirC2 => Some(BTN_GAMESIR_C2),
            Button::GameSirC3 => Some(BTN_GAMESIR_C3),
            Button::GameSirC4 => Some(BTN_GAMESIR_C4),
            Button::GameSirShare => Some(BTN_GAMESIR_SHARE),
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
            | Button::RightTriggerClick
            | Button::EightBitDoL4
            | Button::EightBitDoR4
            | Button::EightBitDoPL
            | Button::EightBitDoPR
            | Button::ApexLM
            | Button::ApexRM
            | Button::ApexM1
            | Button::ApexM2
            | Button::ApexM3
            | Button::ApexM4
            | Button::VaderC
            | Button::VaderZ
            | Button::VaderCircle => None,
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
            Button::EightBitDoL4 => Some(EXTRA_BTN_EIGHTBITDO_L4),
            Button::EightBitDoR4 => Some(EXTRA_BTN_EIGHTBITDO_R4),
            Button::EightBitDoPL => Some(EXTRA_BTN_EIGHTBITDO_PL),
            Button::EightBitDoPR => Some(EXTRA_BTN_EIGHTBITDO_PR),
            Button::ApexLM => Some(APEX_LM),
            Button::ApexRM => Some(APEX_RM),
            Button::ApexM1 => Some(APEX_M1),
            Button::ApexM2 => Some(APEX_M2),
            Button::ApexM3 => Some(APEX_M3),
            Button::ApexM4 => Some(APEX_M4),
            Button::VaderC => Some(VADER_C),
            Button::VaderZ => Some(VADER_Z),
            Button::VaderCircle => Some(VADER_CIRCLE),
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
            Button::EightBitDoL4 => "L4",
            Button::EightBitDoR4 => "R4",
            Button::EightBitDoPL => "PL",
            Button::EightBitDoPR => "PR",
            Button::ApexLM => "LM",
            Button::ApexRM => "RM",
            Button::ApexM1 => "M1",
            Button::ApexM2 => "M2",
            Button::ApexM3 => "M3",
            Button::ApexM4 => "M4",
            Button::VaderC => "C",
            Button::VaderZ => "Z",
            Button::VaderCircle => "Circle",
            Button::GameSirL4 => "L4",
            Button::GameSirR4 => "R4",
            Button::GameSirT1 => "T1",
            Button::GameSirT2 => "T2",
            Button::GameSirT3 => "T3",
            Button::GameSirC1 => "C1",
            Button::GameSirC2 => "C2",
            Button::GameSirC3 => "C3",
            Button::GameSirC4 => "C4",
            Button::GameSirShare => "Share",
        }
    }

    pub fn device_label(self, kind: ControllerKind) -> &'static str {
        self.device_label_with_layout(kind, AbxyLayout::Xbox)
    }

    pub fn device_label_with_layout(
        self,
        kind: ControllerKind,
        layout: AbxyLayout,
    ) -> &'static str {
        match kind.family() {
            ControllerFamily::DualSense => self.label(),
            ControllerFamily::Steam => match self {
                Button::Cross => "A",
                Button::Circle => "B",
                Button::Square => "X",
                Button::Triangle => "Y",
                Button::Create => "View",
                Button::Options => "Menu",
                Button::Touchpad => "Left Pad Click",
                Button::Mute => "Right Pad Click",
                _ => self.label(),
            },
            ControllerFamily::EightBitDo => match self {
                Button::Cross => match layout {
                    AbxyLayout::Xbox => "A",
                    AbxyLayout::Switch => "B",
                },
                Button::Circle => match layout {
                    AbxyLayout::Xbox => "B",
                    AbxyLayout::Switch => "A",
                },
                Button::Square => match layout {
                    AbxyLayout::Xbox => "X",
                    AbxyLayout::Switch => "Y",
                },
                Button::Triangle => match layout {
                    AbxyLayout::Xbox => "Y",
                    AbxyLayout::Switch => "X",
                },
                Button::Create => "Select",
                Button::Options => "Start",
                Button::PS => "Home",
                _ => self.label(),
            },
            ControllerFamily::Flydigi => match self {
                Button::Cross => "A",
                Button::Circle => "B",
                Button::Square => "X",
                Button::Triangle => "Y",
                Button::Create => "View",
                Button::Options => "Menu",
                Button::PS => "Guide",
                Button::L1 => "LB",
                Button::R1 => "RB",
                Button::L2 => "LT",
                Button::R2 => "RT",
                _ => self.label(),
            },
            ControllerFamily::GameSir => match self {
                Button::Cross => "A",
                Button::Circle => "B",
                Button::Square => "X",
                Button::Triangle => "Y",
                Button::Create => "View",
                Button::Options => "Menu",
                Button::PS => "Home",
                Button::L1 => "LB",
                Button::R1 => "RB",
                Button::L2 => "LT",
                Button::R2 => "RT",
                _ => self.label(),
            },
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
        assert!(!steam.rumble);
        assert!(!steam.firmware);
        assert!(steam.motion);
        assert_eq!(steam.touchpad, TouchpadKind::DualCircular);

        for kind in [ControllerKind::DualSense, ControllerKind::DualSenseEdge] {
            let sony = kind.capabilities();
            assert!(sony.lightbar);
            assert!(sony.adaptive_triggers);
            assert!(sony.haptics);
            assert!(!sony.trackpad_haptics);
            assert!(sony.rumble);
            assert!(sony.firmware);
            assert_eq!(sony.touchpad, TouchpadKind::DualSensePanel);
        }
    }

    #[test]
    fn eightbitdo_capabilities_are_rumble_and_motion_only() {
        let pad = ControllerKind::EightBitDoPro3.capabilities();
        assert!(!pad.lightbar);
        assert!(!pad.player_leds);
        assert!(!pad.microphone);
        assert!(!pad.adaptive_triggers);
        assert!(!pad.haptics);
        assert!(!pad.trackpad_haptics);
        assert!(pad.rumble);
        assert!(!pad.firmware);
        assert!(pad.motion);
        assert_eq!(pad.touchpad, TouchpadKind::None);
        assert_eq!(
            ControllerKind::EightBitDoPro3.family(),
            ControllerFamily::EightBitDo
        );
    }

    #[test]
    fn apex5_capabilities_are_rumble_and_motion_only() {
        let pad = ControllerKind::FlydigiApex5.capabilities();
        assert!(!pad.lightbar);
        assert!(!pad.player_leds);
        assert!(!pad.microphone);
        assert!(!pad.adaptive_triggers);
        assert!(!pad.haptics);
        assert!(!pad.trackpad_haptics);
        assert!(pad.rumble);
        assert!(!pad.firmware);
        assert!(pad.motion);
        assert_eq!(pad.touchpad, TouchpadKind::None);
        assert_eq!(
            ControllerKind::FlydigiApex5.family(),
            ControllerFamily::Flydigi
        );
        assert!(ControllerKind::FlydigiApex5.is_flydigi());
        assert_eq!(ControllerKind::FlydigiApex5.label(), "Flydigi Apex 5");
        assert!(!ControllerKind::FlydigiApex5.is_vader5());
    }

    #[test]
    fn vader5_capabilities_match_apex5_and_use_its_family() {
        let pad = ControllerKind::FlydigiVader5Pro.capabilities();
        assert!(!pad.lightbar);
        assert!(!pad.player_leds);
        assert!(!pad.microphone);
        assert!(!pad.adaptive_triggers);
        assert!(!pad.haptics);
        assert!(!pad.trackpad_haptics);
        assert!(pad.rumble);
        assert!(!pad.firmware);
        assert!(pad.motion);
        assert_eq!(pad.touchpad, TouchpadKind::None);
        assert_eq!(
            ControllerKind::FlydigiVader5Pro.family(),
            ControllerFamily::Flydigi
        );
        assert!(ControllerKind::FlydigiVader5Pro.is_flydigi());
        assert_eq!(
            ControllerKind::FlydigiVader5Pro.label(),
            "Flydigi Vader 5 Pro"
        );
        assert!(ControllerKind::FlydigiVader5Pro.is_vader5());
    }

    #[test]
    fn gamesir_capabilities_are_rumble_and_motion_only() {
        let pad = ControllerKind::GameSirTarantulaPro.capabilities();
        assert!(!pad.lightbar);
        assert!(!pad.player_leds);
        assert!(!pad.microphone);
        assert!(!pad.adaptive_triggers);
        assert!(!pad.haptics);
        assert!(!pad.trackpad_haptics);
        assert!(pad.rumble);
        assert!(!pad.firmware);
        assert!(pad.motion);
        assert_eq!(pad.touchpad, TouchpadKind::None);
        assert_eq!(
            ControllerKind::GameSirTarantulaPro.family(),
            ControllerFamily::GameSir
        );
        assert!(ControllerKind::GameSirTarantulaPro.is_gamesir());
        assert_eq!(
            ControllerKind::GameSirTarantulaPro.label(),
            "GameSir Tarantula Pro"
        );
        assert_eq!(ControllerFamily::GameSir.label(), "GameSir");
    }

    #[test]
    fn profile_groups_map_kinds_and_share_dualsense_edge() {
        assert_eq!(
            ControllerKind::DualSense.profile_group(),
            ProfileGroup::DualSense
        );
        assert_eq!(
            ControllerKind::DualSenseEdge.profile_group(),
            ProfileGroup::DualSense
        );
        assert_eq!(
            ControllerKind::SteamController.profile_group(),
            ProfileGroup::Steam
        );
        assert_eq!(
            ControllerKind::EightBitDoPro3.profile_group(),
            ProfileGroup::EightBitDo
        );
        assert_eq!(
            ControllerKind::FlydigiApex5.profile_group(),
            ProfileGroup::FlydigiApex5
        );
        assert_eq!(
            ControllerKind::FlydigiVader5Pro.profile_group(),
            ProfileGroup::FlydigiVader5Pro
        );
        assert_eq!(
            ControllerKind::GameSirTarantulaPro.profile_group(),
            ProfileGroup::GameSirTarantulaPro
        );

        assert!(ControllerKind::DualSense.same_profile_group(ControllerKind::DualSenseEdge));
        assert!(!ControllerKind::FlydigiApex5.same_profile_group(ControllerKind::FlydigiVader5Pro));
        assert!(!ControllerKind::SteamController.same_profile_group(ControllerKind::DualSense));
    }

    #[test]
    fn profile_group_list_is_unique_and_labelled() {
        let groups = ProfileGroup::all();
        assert_eq!(groups.len(), 6);
        let unique: HashSet<_> = groups.into_iter().collect();
        assert_eq!(unique.len(), PROFILE_GROUPS.len());
        for group in groups {
            assert!(!group.label().is_empty());
        }
        assert_eq!(ProfileGroup::DualSense.label(), "DualSense / Edge");
        assert_eq!(
            ProfileGroup::FlydigiVader5Pro.label(),
            "Flydigi Vader 5 Pro"
        );
    }

    #[test]
    fn gamesir_targets_cover_standard_and_ten_extras() {
        let targets = ControllerKind::GameSirTarantulaPro.target_buttons();
        assert_eq!(targets.len(), 27);
        for button in [
            Button::Cross,
            Button::Circle,
            Button::Square,
            Button::Triangle,
            Button::L1,
            Button::R1,
            Button::L2,
            Button::R2,
            Button::L3,
            Button::R3,
            Button::Create,
            Button::Options,
            Button::PS,
            Button::GameSirL4,
            Button::GameSirR4,
            Button::GameSirT1,
            Button::GameSirT2,
            Button::GameSirT3,
            Button::GameSirC1,
            Button::GameSirC2,
            Button::GameSirC3,
            Button::GameSirC4,
            Button::GameSirShare,
        ] {
            assert!(targets.contains(&button), "{button:?}");
        }
        for button in [
            Button::Mute,
            Button::Touchpad,
            Button::EdgeFnLeft,
            Button::Steam,
            Button::LeftPad,
            Button::L4,
            Button::EightBitDoL4,
            Button::ApexM1,
            Button::VaderC,
        ] {
            assert!(!targets.contains(&button));
        }
    }

    #[test]
    fn gamesir_labels_use_xbox_caps_and_keep_extras() {
        let kind = ControllerKind::GameSirTarantulaPro;
        for layout in [AbxyLayout::Xbox, AbxyLayout::Switch] {
            assert_eq!(Button::Cross.device_label_with_layout(kind, layout), "A");
            assert_eq!(Button::Circle.device_label_with_layout(kind, layout), "B");
            assert_eq!(Button::Square.device_label_with_layout(kind, layout), "X");
            assert_eq!(Button::Triangle.device_label_with_layout(kind, layout), "Y");
            assert_eq!(
                Button::Create.device_label_with_layout(kind, layout),
                "View"
            );
            assert_eq!(
                Button::Options.device_label_with_layout(kind, layout),
                "Menu"
            );
            assert_eq!(Button::PS.device_label_with_layout(kind, layout), "Home");
            assert_eq!(Button::L1.device_label_with_layout(kind, layout), "LB");
            assert_eq!(Button::R1.device_label_with_layout(kind, layout), "RB");
            assert_eq!(Button::L2.device_label_with_layout(kind, layout), "LT");
            assert_eq!(Button::R2.device_label_with_layout(kind, layout), "RT");
            assert_eq!(Button::GameSirL4.label(), "L4");
            assert_eq!(Button::GameSirR4.label(), "R4");
            assert_eq!(Button::GameSirT1.label(), "T1");
            assert_eq!(Button::GameSirT3.label(), "T3");
            assert_eq!(Button::GameSirC1.label(), "C1");
            assert_eq!(Button::GameSirC4.label(), "C4");
            assert_eq!(Button::GameSirShare.label(), "Share");
        }
    }

    #[test]
    fn gamesir_extra_bits_round_trip_and_report_presses() {
        for (button, mask) in [
            (Button::GameSirL4, BTN_GAMESIR_L4),
            (Button::GameSirR4, BTN_GAMESIR_R4),
            (Button::GameSirT1, BTN_GAMESIR_T1),
            (Button::GameSirT2, BTN_GAMESIR_T2),
            (Button::GameSirT3, BTN_GAMESIR_T3),
            (Button::GameSirC1, BTN_GAMESIR_C1),
            (Button::GameSirC2, BTN_GAMESIR_C2),
            (Button::GameSirC3, BTN_GAMESIR_C3),
            (Button::GameSirC4, BTN_GAMESIR_C4),
            (Button::GameSirShare, BTN_GAMESIR_SHARE),
        ] {
            assert_eq!(button.to_bitmask(), Some(mask));
            assert_eq!(button.to_extra_mask(), None);
            let mut state = ControllerState::default();
            assert!(!button.is_pressed(&state), "{button:?} unpressed");
            state.buttons = mask;
            assert!(button.is_pressed(&state), "{button:?} pressed");
            assert_eq!(state.extra_buttons, 0);
        }
    }

    #[test]
    fn apex5_targets_cover_standard_and_extra_buttons() {
        let targets = ControllerKind::FlydigiApex5.target_buttons();
        assert_eq!(targets.len(), 23);
        for button in [
            Button::ApexLM,
            Button::ApexRM,
            Button::ApexM1,
            Button::ApexM2,
            Button::ApexM3,
            Button::ApexM4,
            Button::PS,
        ] {
            assert!(targets.contains(&button));
        }
        for button in [
            Button::Mute,
            Button::Touchpad,
            Button::EdgeFnLeft,
            Button::Steam,
            Button::LeftPad,
            Button::L4,
            Button::EightBitDoL4,
            Button::VaderC,
            Button::VaderZ,
            Button::VaderCircle,
        ] {
            assert!(!targets.contains(&button));
        }
    }

    #[test]
    fn vader5_targets_add_c_z_and_circle() {
        let targets = ControllerKind::FlydigiVader5Pro.target_buttons();
        assert_eq!(targets.len(), 26);
        for button in [
            Button::ApexLM,
            Button::ApexRM,
            Button::ApexM1,
            Button::ApexM2,
            Button::ApexM3,
            Button::ApexM4,
            Button::VaderC,
            Button::VaderZ,
            Button::VaderCircle,
            Button::PS,
        ] {
            assert!(targets.contains(&button));
        }
        for button in [
            Button::Mute,
            Button::Touchpad,
            Button::EdgeFnLeft,
            Button::Steam,
            Button::LeftPad,
            Button::L4,
            Button::EightBitDoL4,
        ] {
            assert!(!targets.contains(&button));
        }
    }

    #[test]
    fn apex5_labels_use_xbox_caps_and_keep_extras() {
        let kind = ControllerKind::FlydigiApex5;
        for layout in [AbxyLayout::Xbox, AbxyLayout::Switch] {
            assert_eq!(Button::Cross.device_label_with_layout(kind, layout), "A");
            assert_eq!(Button::Circle.device_label_with_layout(kind, layout), "B");
            assert_eq!(Button::Square.device_label_with_layout(kind, layout), "X");
            assert_eq!(Button::Triangle.device_label_with_layout(kind, layout), "Y");
            assert_eq!(
                Button::Create.device_label_with_layout(kind, layout),
                "View"
            );
            assert_eq!(
                Button::Options.device_label_with_layout(kind, layout),
                "Menu"
            );
            assert_eq!(Button::PS.device_label_with_layout(kind, layout), "Guide");
            assert_eq!(Button::L1.device_label_with_layout(kind, layout), "LB");
            assert_eq!(Button::R1.device_label_with_layout(kind, layout), "RB");
            assert_eq!(Button::L2.device_label_with_layout(kind, layout), "LT");
            assert_eq!(Button::R2.device_label_with_layout(kind, layout), "RT");
            assert_eq!(Button::ApexLM.device_label_with_layout(kind, layout), "LM");
            assert_eq!(Button::ApexM4.device_label_with_layout(kind, layout), "M4");
            assert_eq!(Button::VaderC.device_label_with_layout(kind, layout), "C");
            assert_eq!(Button::VaderZ.device_label_with_layout(kind, layout), "Z");
            assert_eq!(
                Button::VaderCircle.device_label_with_layout(kind, layout),
                "Circle"
            );
        }
    }

    #[test]
    fn eightbitdo_targets_exclude_sony_and_steam_controls() {
        let targets = ControllerKind::EightBitDoPro3.target_buttons();
        for button in [
            Button::EightBitDoL4,
            Button::EightBitDoR4,
            Button::EightBitDoPL,
            Button::EightBitDoPR,
            Button::PS,
        ] {
            assert!(targets.contains(&button));
        }
        for button in [
            Button::Mute,
            Button::Touchpad,
            Button::EdgeFnLeft,
            Button::EdgePaddleLeft,
            Button::Steam,
            Button::LeftPad,
            Button::L4,
        ] {
            assert!(!targets.contains(&button));
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
        assert_eq!(masks.len(), 29);
    }

    #[test]
    fn all_buttons_includes_flydigi_extras() {
        assert_eq!(ALL_BUTTONS.len(), 70);
        for button in [
            Button::ApexLM,
            Button::ApexRM,
            Button::ApexM1,
            Button::ApexM2,
            Button::ApexM3,
            Button::ApexM4,
            Button::VaderC,
            Button::VaderZ,
            Button::VaderCircle,
            Button::GameSirL4,
            Button::GameSirShare,
        ] {
            assert!(ALL_BUTTONS.contains(&button));
        }
    }

    #[test]
    fn flydigi_extra_masks_round_trip_and_report_presses() {
        for (button, mask) in [
            (Button::ApexLM, APEX_LM),
            (Button::ApexRM, APEX_RM),
            (Button::ApexM1, APEX_M1),
            (Button::ApexM2, APEX_M2),
            (Button::ApexM3, APEX_M3),
            (Button::ApexM4, APEX_M4),
            (Button::VaderC, VADER_C),
            (Button::VaderZ, VADER_Z),
            (Button::VaderCircle, VADER_CIRCLE),
        ] {
            assert_eq!(button.to_extra_mask(), Some(mask));
            assert_eq!(button.to_bitmask(), None);
            let mut state = ControllerState::default();
            assert!(!button.is_pressed(&state), "{button:?} unpressed");
            state.extra_buttons = mask;
            assert!(button.is_pressed(&state), "{button:?} pressed");
        }
    }

    #[test]
    fn eightbitdo_face_labels_follow_abxy_layout() {
        let kind = ControllerKind::EightBitDoPro3;
        for (layout, triangle, square, circle, cross) in [
            (AbxyLayout::Xbox, "Y", "X", "B", "A"),
            (AbxyLayout::Switch, "X", "Y", "A", "B"),
        ] {
            assert_eq!(
                Button::Triangle.device_label_with_layout(kind, layout),
                triangle,
                "Triangle in {layout:?}"
            );
            assert_eq!(
                Button::Square.device_label_with_layout(kind, layout),
                square,
                "Square in {layout:?}"
            );
            assert_eq!(
                Button::Circle.device_label_with_layout(kind, layout),
                circle,
                "Circle in {layout:?}"
            );
            assert_eq!(
                Button::Cross.device_label_with_layout(kind, layout),
                cross,
                "Cross in {layout:?}"
            );
        }
    }

    #[test]
    fn eightbitdo_meta_labels_are_select_and_start() {
        let kind = ControllerKind::EightBitDoPro3;
        for layout in [AbxyLayout::Xbox, AbxyLayout::Switch] {
            assert_eq!(
                Button::Create.device_label_with_layout(kind, layout),
                "Select"
            );
            assert_eq!(
                Button::Options.device_label_with_layout(kind, layout),
                "Start"
            );
            assert_eq!(Button::PS.device_label_with_layout(kind, layout), "Home");
        }
        assert_eq!(Button::Create.device_label(kind), "Select");
        assert_eq!(Button::Options.device_label(kind), "Start");
    }

    #[test]
    fn device_label_defaults_to_xbox_layout_for_eightbitdo() {
        assert_eq!(AbxyLayout::default(), AbxyLayout::Xbox);
        let kind = ControllerKind::EightBitDoPro3;
        for button in [
            Button::Cross,
            Button::Circle,
            Button::Square,
            Button::Triangle,
        ] {
            assert_eq!(
                button.device_label(kind),
                button.device_label_with_layout(kind, AbxyLayout::Xbox)
            );
        }
    }

    #[test]
    fn dualsense_and_steam_labels_ignore_abxy_layout() {
        for layout in [AbxyLayout::Xbox, AbxyLayout::Switch] {
            assert_eq!(
                Button::Cross.device_label_with_layout(ControllerKind::DualSense, layout),
                "Cross"
            );
            assert_eq!(
                Button::Create.device_label_with_layout(ControllerKind::DualSense, layout),
                "Create"
            );
            assert_eq!(
                Button::Circle.device_label_with_layout(ControllerKind::SteamController, layout),
                "B"
            );
            assert_eq!(
                Button::Options.device_label_with_layout(ControllerKind::SteamController, layout),
                "Menu"
            );
        }
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
