use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::common::TouchpadMode;
use crate::gyro::GyroConfig;
use crate::remap::{Binding, BindingSlot, Layer, PressTiming, StickConfig};
use crate::stick_mouse::StickMouseConfig;
use crate::{common::SensitivityCurve, inputs::*};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct TriggerDeadband {
    pub release: u8,
    pub full_stroke: u8,
}

impl Default for TriggerDeadband {
    fn default() -> Self {
        Self {
            release: 0,
            full_stroke: 255,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InputTransform {
    pub left_curve: SensitivityCurve,
    pub right_curve: SensitivityCurve,
    #[serde(default)]
    pub left_deadzone: f32,
    #[serde(default)]
    pub right_deadzone: f32,
    #[serde(default)]
    pub left_outer_deadzone: f32,
    #[serde(default)]
    pub right_outer_deadzone: f32,
    #[serde(default)]
    pub left_invert_x: bool,
    #[serde(default)]
    pub left_invert_y: bool,
    #[serde(default)]
    pub right_invert_x: bool,
    #[serde(default)]
    pub right_invert_y: bool,
    #[serde(default)]
    pub stick_swap: bool,

    pub trigger_left: TriggerDeadband,
    pub trigger_right: TriggerDeadband,

    pub button_remap: HashMap<Button, Button>,
    pub disabled_buttons: HashSet<Button>,
    #[serde(default)]
    pub button_bindings: HashMap<Button, Vec<BindingSlot>>,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub press_timing: PressTiming,

    #[serde(default)]
    pub stick_digital: StickConfig,

    #[serde(default)]
    pub touchpad_enabled: bool,
    #[serde(default)]
    pub touchpad_mode: TouchpadMode,
    #[serde(default)]
    pub gyro_mouse: GyroConfig,
    #[serde(default)]
    pub stick_mouse: StickMouseConfig,
}

impl Default for InputTransform {
    fn default() -> Self {
        Self {
            left_curve: SensitivityCurve::Default,
            right_curve: SensitivityCurve::Default,
            left_deadzone: 0.0,
            right_deadzone: 0.0,
            left_outer_deadzone: 1.0,
            right_outer_deadzone: 1.0,
            left_invert_x: false,
            left_invert_y: false,
            right_invert_x: false,
            right_invert_y: false,
            stick_swap: false,
            trigger_left: TriggerDeadband::default(),
            trigger_right: TriggerDeadband::default(),
            button_remap: HashMap::new(),
            disabled_buttons: HashSet::new(),
            button_bindings: HashMap::new(),
            layers: Vec::new(),
            press_timing: PressTiming::default(),
            stick_digital: StickConfig::default(),
            touchpad_enabled: true,
            touchpad_mode: TouchpadMode::Mouse,
            gyro_mouse: GyroConfig::default(),
            stick_mouse: StickMouseConfig::default(),
        }
    }
}

impl InputTransform {
    pub fn normalize_bindings(&mut self) {
        if !self.button_bindings.is_empty() {
            return;
        }
        for (from, to) in &self.button_remap {
            self.button_bindings
                .insert(*from, vec![BindingSlot::from(Binding::Gamepad(*to))]);
        }
        for btn in &self.disabled_buttons {
            self.button_bindings.insert(*btn, Vec::new());
        }
    }

    pub fn apply_axes(&self, s: &mut ControllerState) {
        if self.stick_swap {
            std::mem::swap(&mut s.left_x, &mut s.right_x);
            std::mem::swap(&mut s.left_y, &mut s.right_y);
        }

        if self.left_invert_x {
            s.left_x = 255u8.saturating_sub(s.left_x);
        }
        if self.left_invert_y {
            s.left_y = 255u8.saturating_sub(s.left_y);
        }
        if self.right_invert_x {
            s.right_x = 255u8.saturating_sub(s.right_x);
        }
        if self.right_invert_y {
            s.right_y = 255u8.saturating_sub(s.right_y);
        }

        apply_stick(
            s.left_x,
            s.left_y,
            self.left_deadzone,
            self.left_outer_deadzone,
            &self.left_curve,
            &mut s.left_x,
            &mut s.left_y,
        );
        apply_stick(
            s.right_x,
            s.right_y,
            self.right_deadzone,
            self.right_outer_deadzone,
            &self.right_curve,
            &mut s.right_x,
            &mut s.right_y,
        );
        s.l2 = apply_trigger(s.l2, &self.trigger_left);
        s.r2 = apply_trigger(s.r2, &self.trigger_right);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GyroProcessor {
    pub enabled: bool,
    pub smoothing: f32,
    pub sensitivity: f32,
    #[serde(skip)]
    pub prev: [f32; 3],
}

impl Default for GyroProcessor {
    fn default() -> Self {
        Self {
            enabled: true,
            smoothing: 0.0,
            sensitivity: 1.0,
            prev: [0.0; 3],
        }
    }
}

impl GyroProcessor {
    pub fn process(&mut self, gyro: &mut [i16; 3]) {
        if !self.enabled {
            *gyro = [0, 0, 0];
            self.prev = [0.0; 3];
            return;
        }
        let s = self.smoothing.clamp(0.0, 0.95);
        for (i, g) in gyro.iter_mut().enumerate().take(3) {
            let cur = *g as f32 * self.sensitivity;
            let smoothed = self.prev[i] * s + cur * (1.0 - s);
            self.prev[i] = smoothed;
            *g = smoothed.clamp(-32768.0, 32767.0) as i16;
        }
    }
}

fn curve_apply(t: f32, curve: &SensitivityCurve) -> f32 {
    match curve {
        SensitivityCurve::Default => t,
        SensitivityCurve::Quick => t.powf(0.5),
        SensitivityCurve::Precise => t.powf(2.2),
        SensitivityCurve::Steady => t.powf(1.6),
        SensitivityCurve::Digital => {
            if t > 0.5 {
                1.0
            } else {
                0.0
            }
        }
        SensitivityCurve::Dynamic => {
            let t2 = t * 2.0;
            if t < 0.5 {
                0.5 * t2 * t2
            } else {
                1.0 - 0.5 * (2.0 - t2) * (2.0 - t2)
            }
        }
    }
}

fn apply_stick(
    raw_x: u8,
    raw_y: u8,
    deadzone: f32,
    outer_deadzone: f32,
    curve: &SensitivityCurve,
    out_x: &mut u8,
    out_y: &mut u8,
) {
    let nx = (raw_x as f32 - 128.0) / 127.0;
    let ny = (raw_y as f32 - 128.0) / 127.0;
    let magnitude = (nx * nx + ny * ny).sqrt().min(1.0);

    if magnitude <= deadzone {
        *out_x = 128;
        *out_y = 128;
        return;
    }

    let outer = outer_deadzone.clamp(deadzone + f32::EPSILON, 1.0);

    if magnitude >= outer {
        let factor = 1.0 / magnitude;
        *out_x = (nx * factor * 127.0 + 128.0).round().clamp(0.0, 255.0) as u8;
        *out_y = (ny * factor * 127.0 + 128.0).round().clamp(0.0, 255.0) as u8;
        return;
    }

    let scaled = (magnitude - deadzone) / (outer - deadzone);
    let curved = curve_apply(scaled, curve);
    let factor = curved / magnitude;

    *out_x = (nx * factor * 127.0 + 128.0).round().clamp(0.0, 255.0) as u8;
    *out_y = (ny * factor * 127.0 + 128.0).round().clamp(0.0, 255.0) as u8;
}

fn apply_trigger(raw: u8, db: &TriggerDeadband) -> u8 {
    if *db == TriggerDeadband::default() {
        return raw;
    }
    let full = db.full_stroke.max(db.release.saturating_add(1));
    if raw <= db.release {
        return 0;
    }
    if raw >= full {
        return 255;
    }
    ((raw - db.release) as f32 / (full - db.release) as f32 * 255.0).round() as u8
}
