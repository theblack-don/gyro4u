use std::collections::HashMap;
use std::time::Instant;

use crate::common::{HapticPattern, MicLedState, SpeakerMode, TouchpadMode};
use crate::gyro::GyroConfig;
use crate::inputs::Button;
use crate::profiles::TriggerConfig;
use crate::remap::{BindingSlot, Layer, PressTiming};
use crate::stick_mouse::StickMouseConfig;

#[derive(Debug, Clone)]
pub(crate) enum ProgressUpdate {
    Progress(u32),
    Status(String),
    Complete,
    Error(String),
    LatestVersion(String),
}

#[derive(PartialEq, Copy, Clone)]
pub(crate) enum Section {
    Lightbar,
    Triggers,
    Sticks,
    Haptics,
    Audio,
    Advanced,
    Inputs,
    Mapping,
    Settings,
    Gyroscope,
    Touchpad,
    Profiles,
}

pub(crate) struct LightbarState {
    pub(crate) r: f32,
    pub(crate) g: f32,
    pub(crate) b: f32,
    pub(crate) brightness: f32,
    pub(crate) enabled: bool,
}

pub(crate) struct MicrophoneState {
    pub(crate) enabled: bool,
    pub(crate) led_state: MicLedState,
}

pub(crate) struct StickSettings {
    pub(crate) left_curve: crate::common::SensitivityCurve,
    pub(crate) right_curve: crate::common::SensitivityCurve,
    pub(crate) left_deadzone: f32,
    pub(crate) right_deadzone: f32,
    pub(crate) left_outer_deadzone: f32,
    pub(crate) right_outer_deadzone: f32,
    pub(crate) left_invert_x: bool,
    pub(crate) left_invert_y: bool,
    pub(crate) right_invert_x: bool,
    pub(crate) right_invert_y: bool,
    pub(crate) swap: bool,
    pub(crate) digital_left_deadzone: f32,
    pub(crate) digital_right_deadzone: f32,
    pub(crate) digital_left_analog: bool,
    pub(crate) digital_right_analog: bool,
    pub(crate) stick_mouse: StickMouseConfig,
}

pub(crate) struct AudioSettings {
    pub(crate) volume: u8,
    pub(crate) speaker_mode: SpeakerMode,
}

pub(crate) struct VibrationSettings {
    pub(crate) rumble: u8,
    pub(crate) trigger: u8,
}
pub(crate) struct GyroState {
    pub(crate) mouse: GyroConfig,
    pub(crate) calibration: Option<GyroCalibration>,
}

pub(crate) struct GyroCalibration {
    pub(crate) started: Instant,
    pub(crate) samples: Vec<[i16; 3]>,
}

pub(crate) struct TouchpadState {
    pub(crate) show_overlay: bool,
    pub(crate) mode: TouchpadMode,
    pub(crate) tap_to_click: bool,
    pub(crate) natural_scrolling: bool,
    pub(crate) sensitivity: f32,
}

pub(crate) struct HapticState {
    pub(crate) pattern: HapticPattern,
    pub(crate) strength: u8,
    pub(crate) speed: f32,
}

pub(crate) struct TriggersState {
    pub(crate) left: TriggerConfig,
    pub(crate) right: TriggerConfig,
}

#[derive(Clone, Copy)]
pub(crate) enum CaptureKind {
    Key,
}

pub(crate) struct CaptureRequest {
    pub(crate) control: Button,
    pub(crate) kind: CaptureKind,
    pub(crate) layer: Option<usize>,
}

#[derive(Default)]
pub(crate) struct RemapState {
    pub(crate) bindings: HashMap<Button, Vec<BindingSlot>>,
    pub(crate) layers: Vec<Layer>,
    pub(crate) press_timing: PressTiming,
    pub(crate) selected: Option<Button>,
    pub(crate) selected_layer: Option<usize>,
    pub(crate) capture: Option<CaptureRequest>,
}
