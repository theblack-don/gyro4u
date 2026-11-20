use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::inputs::{Button, ControllerState};

pub const RAW_COUNTS_PER_DPS: f32 = 16.0;
const TIMESTAMP_UNITS_PER_SECOND: f32 = 3_000_000.0;
const DT_MIN: f32 = 0.0005;
const DT_MAX: f32 = 0.02;
const MAX_COUNTS_PER_TICK: f32 = 4096.0;

fn timestamp_delta_seconds(delta: u32) -> f32 {
    delta as f32 / TIMESTAMP_UNITS_PER_SECOND
}

fn clamp_dt(dt: f32) -> f32 {
    dt.clamp(DT_MIN, DT_MAX)
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct GyroConfig {
    pub enabled: bool,
    pub activation: GyroActivation,
    pub dots_per_360: f32,
    pub sensitivity: f32,
    pub output_mixer: f32,
    pub acceleration: GyroAcceleration,
    pub speed_deadzone: f32,
    pub precision_speed: f32,
    pub smoothing: f32,
    pub invert_x: bool,
    pub invert_y: bool,
    pub bias_dps: [f32; 3],
}

impl Default for GyroConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            activation: GyroActivation::Touchpad,
            dots_per_360: 6545.0,
            sensitivity: 2.5,
            output_mixer: 0.0,
            acceleration: GyroAcceleration::Off,
            speed_deadzone: 0.36,
            precision_speed: 0.75,
            smoothing: 0.0,
            invert_x: false,
            invert_y: false,
            bias_dps: [0.0; 3],
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum GyroActivation {
    Always,
    Hold {
        button: Button,
    },
    Toggle {
        button: Button,
    },
    #[default]
    Touchpad,
    Trigger {
        button: Button,
        threshold: u8,
    },
    AnyOf(Vec<GyroActivation>),
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum GyroAcceleration {
    #[default]
    Off,
    Linear,
    Relaxed,
    Aggressive,
}

impl GyroConfig {
    pub fn dps_from_raw(&self, raw: [i16; 3]) -> [f32; 3] {
        std::array::from_fn(|i| raw[i] as f32 / RAW_COUNTS_PER_DPS - self.bias_dps[i])
    }

    pub fn acceleration_multiplier(&self, speed: f32) -> f32 {
        let t = (speed / 360.0).max(0.0);
        let multiplier = match self.acceleration {
            GyroAcceleration::Off => 1.0,
            GyroAcceleration::Linear => 1.0 + 2.0 * t,
            GyroAcceleration::Relaxed => 1.0 + 3.0 * t * t,
            GyroAcceleration::Aggressive => 1.0 + 1.5 * t.sqrt(),
        };
        multiplier.min(8.0)
    }

    pub fn precision_multiplier(&self, speed: f32) -> f32 {
        if self.precision_speed > self.speed_deadzone && speed < self.precision_speed {
            (speed / self.precision_speed).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    pub fn mixer_factors(&self) -> (f32, f32) {
        let mixer = self.output_mixer.clamp(-1.0, 1.0);
        (1.0 - mixer.max(0.0), 1.0 - (-mixer).max(0.0))
    }
}

#[derive(Clone, Debug)]
pub struct GyroRuntime {
    prev_ts: u32,
    primed: bool,
    filter_primed: bool,
    smoothed: [f32; 3],
    accum: [f32; 2],
    deadzone_active: bool,
    toggle_states: HashMap<Button, bool>,
    prev_toggle_buttons: HashSet<Button>,
}

impl Default for GyroRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl GyroRuntime {
    pub fn new() -> Self {
        Self {
            prev_ts: 0,
            primed: false,
            filter_primed: false,
            smoothed: [0.0; 3],
            accum: [0.0; 2],
            deadzone_active: false,
            toggle_states: HashMap::new(),
            prev_toggle_buttons: HashSet::new(),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn is_activation_active(
        &mut self,
        s: &ControllerState,
        activation: &GyroActivation,
        touchpad_owned: bool,
    ) -> bool {
        match activation {
            GyroActivation::Always => true,
            GyroActivation::Hold { button } => button.is_pressed(s),
            GyroActivation::Toggle { button } => {
                let pressed = button.is_pressed(s);
                if pressed && !self.prev_toggle_buttons.contains(button) {
                    let state = self.toggle_states.entry(*button).or_insert(false);
                    *state = !*state;
                }
                if pressed {
                    self.prev_toggle_buttons.insert(*button);
                } else {
                    self.prev_toggle_buttons.remove(button);
                }
                self.toggle_states.get(button).copied().unwrap_or(false)
            }
            GyroActivation::Touchpad => touchpad_owned && s.touch_count == 1,
            GyroActivation::Trigger { button, threshold } => match button {
                Button::L2 => s.l2 >= *threshold,
                Button::R2 => s.r2 >= *threshold,
                _ => false,
            },
            GyroActivation::AnyOf(list) => {
                let mut active = false;
                for source in list {
                    // Evaluate every source so toggle edges update while another is active.
                    active |= self.is_activation_active(s, source, touchpad_owned);
                }
                active
            }
        }
    }

    fn reset_filter(&mut self) {
        self.prev_ts = 0;
        self.primed = false;
        self.filter_primed = false;
        self.smoothed = [0.0; 3];
        self.accum = [0.0; 2];
        self.deadzone_active = false;
    }

    pub fn process(
        &mut self,
        s: &ControllerState,
        cfg: &GyroConfig,
        touchpad_owned: bool,
    ) -> (i32, i32) {
        if !cfg.enabled {
            self.reset();
            return (0, 0);
        }

        let active = self.is_activation_active(s, &cfg.activation, touchpad_owned);

        if !active {
            self.reset_filter();
            return (0, 0);
        }

        let rate = cfg.dps_from_raw(s.gyro);

        if !self.primed {
            self.prev_ts = s.sensor_timestamp;
            self.primed = true;
            self.smoothed = rate;
            self.filter_primed = true;
            return (0, 0);
        }

        let delta = s.sensor_timestamp.wrapping_sub(self.prev_ts);
        self.prev_ts = s.sensor_timestamp;
        if delta == 0 {
            return (0, 0);
        }
        let dt = clamp_dt(timestamp_delta_seconds(delta));

        if self.filter_primed {
            let smoothing = cfg.smoothing.clamp(0.0, 0.95);
            for (smoothed, current) in self.smoothed.iter_mut().zip(rate) {
                *smoothed = *smoothed * smoothing + current * (1.0 - smoothing);
            }
        } else {
            self.smoothed = rate;
            self.filter_primed = true;
        }

        // Raw +RY = yaw left, raw +RX = pitch up on DualSense; flip so default right/up are positive/negative correctly.
        let yaw = -self.smoothed[1];
        let pitch = -self.smoothed[0];
        let speed = (yaw * yaw + pitch * pitch).sqrt();

        let deadzone = cfg.speed_deadzone.max(0.0);
        if self.deadzone_active {
            if speed < deadzone * 0.8 {
                self.deadzone_active = false;
                self.accum = [0.0; 2];
                return (0, 0);
            }
        } else if speed < deadzone {
            self.accum = [0.0; 2];
            return (0, 0);
        } else {
            self.deadzone_active = true;
        }

        let precision = cfg.precision_multiplier(speed);
        let accel = cfg.acceleration_multiplier(speed);
        let (h, v) = cfg.mixer_factors();
        let scale = dt * (cfg.dots_per_360 / 360.0) * cfg.sensitivity.max(0.0) * accel * precision;

        let x = if cfg.invert_x { -yaw } else { yaw } * scale * h;
        let y = if cfg.invert_y { -pitch } else { pitch } * scale * v;

        self.accum[0] += x;
        self.accum[1] += y;
        let out_x = self.accum[0]
            .trunc()
            .clamp(-MAX_COUNTS_PER_TICK, MAX_COUNTS_PER_TICK) as i32;
        let out_y = self.accum[1]
            .trunc()
            .clamp(-MAX_COUNTS_PER_TICK, MAX_COUNTS_PER_TICK) as i32;
        self.accum[0] -= out_x as f32;
        self.accum[1] -= out_y as f32;
        (out_x, out_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::{BTN_CROSS, BTN_L3, BTN_R3, BTN_TRIANGLE, DPAD_NEUTRAL, TouchPoint};

    fn cfg() -> GyroConfig {
        GyroConfig {
            enabled: true,
            activation: GyroActivation::Always,
            sensitivity: 1.0,
            speed_deadzone: 0.0,
            precision_speed: 0.0,
            ..Default::default()
        }
    }

    fn sample(ts: u32, gyro: [i16; 3]) -> ControllerState {
        ControllerState {
            left_x: 128,
            left_y: 128,
            right_x: 128,
            right_y: 128,
            l2: 0,
            r2: 0,
            buttons: 0,
            dpad: DPAD_NEUTRAL,
            gyro,
            accel: [0; 3],
            sensor_timestamp: ts,
            touch_count: 0,
            touch_points: [TouchPoint::default(); 2],
        }
    }

    fn run_ticks(cfg: &GyroConfig, gyro: [i16; 3], dt_raw: u32, ticks: u32) -> (i32, i32) {
        let mut runtime = GyroRuntime::new();
        let mut ts = 0u32;
        let mut out = (0, 0);
        for _ in 0..=ticks {
            let (x, y) = runtime.process(&sample(ts, gyro), cfg, true);
            out.0 += x;
            out.1 += y;
            ts = ts.wrapping_add(dt_raw);
        }
        out
    }

    #[test]
    fn defaults_match_spec() {
        let cfg = GyroConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.activation, GyroActivation::Touchpad);
        assert_eq!(cfg.dots_per_360, 6545.0);
        assert_eq!(cfg.sensitivity, 2.5);
        assert_eq!(cfg.output_mixer, 0.0);
        assert_eq!(cfg.acceleration, GyroAcceleration::Off);
        assert_eq!(cfg.speed_deadzone, 0.36);
        assert_eq!(cfg.precision_speed, 0.75);
        assert_eq!(cfg.smoothing, 0.0);
        assert!(!cfg.invert_x);
        assert!(!cfg.invert_y);
        assert_eq!(cfg.bias_dps, [0.0; 3]);
    }

    #[test]
    fn serde_uses_defaults_for_missing_fields() {
        let parsed: GyroConfig = serde_json::from_str("{}").expect("parse empty config");
        assert_eq!(parsed, GyroConfig::default());
    }

    #[test]
    fn serde_round_trips_activation() {
        let cfg = GyroConfig {
            enabled: true,
            activation: GyroActivation::Trigger {
                button: Button::R2,
                threshold: 42,
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: GyroConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, cfg);
    }

    #[test]
    fn raw_counts_convert_to_dps() {
        let cfg = GyroConfig::default();
        assert_eq!(cfg.dps_from_raw([1440, 0, 0])[0], 90.0);
        assert_eq!(cfg.dps_from_raw([-1440, 0, 0])[0], -90.0);
    }

    #[test]
    fn bias_is_subtracted_from_raw_rates() {
        let cfg = GyroConfig {
            bias_dps: [1.5, -0.7, -0.5],
            ..Default::default()
        };
        let dps = cfg.dps_from_raw([1464, -11, -8]);
        assert!((dps[0] - 90.0).abs() < 1e-4);
        assert!((dps[1] - 0.0).abs() < 0.1);
        assert!((dps[2] - 0.0).abs() < 0.1);
    }

    #[test]
    fn timestamp_seconds_and_clamp() {
        assert_eq!(timestamp_delta_seconds(3_000_000), 1.0);
        assert_eq!(timestamp_delta_seconds(1_500), 0.0005);
        assert_eq!(clamp_dt(1.0), 0.02);
        assert_eq!(clamp_dt(0.0), 0.0005);
    }

    #[test]
    fn timestamp_wrap_delta_is_positive() {
        let prev = u32::MAX - 999;
        assert_eq!(1000u32.wrapping_sub(prev), 2000);
    }

    #[test]
    fn wrapped_timestamp_produces_dt() {
        let cfg = cfg();
        let mut runtime = GyroRuntime::new();
        let start = u32::MAX - 14_999;
        runtime.process(&sample(start, [0, 1440, 0]), &cfg, true);
        let (x, _) = runtime.process(
            &sample(start.wrapping_add(30_000), [0, 1440, 0]),
            &cfg,
            true,
        );
        assert_ne!(x, 0, "wrapped timestamp should still produce motion");
    }

    #[test]
    fn ninety_dps_for_one_second() {
        let out = run_ticks(&cfg(), [0, 1440, 0], 60_000, 50);
        assert_eq!(out.1, 0);
        assert!(
            (out.0 as f32 + 1636.25).abs() <= 1.0,
            "expected ~1636 counts, got {}",
            out.0
        );
    }

    #[test]
    fn deadzone_uses_release_hysteresis() {
        let mut cfg = cfg();
        cfg.speed_deadzone = 10.0;
        let mut runtime = GyroRuntime::new();
        let mut ts = 0u32;
        let mut tick = |raw_yaw: i16| {
            let out = runtime.process(&sample(ts, [0, raw_yaw, 0]), &cfg, true);
            ts = ts.wrapping_add(30_000);
            out
        };

        assert_eq!(tick(80), (0, 0));
        assert_eq!(tick(80), (0, 0));
        assert_ne!(tick(192), (0, 0));
        assert_ne!(tick(144), (0, 0));
        assert_eq!(tick(112), (0, 0));
        assert_eq!(tick(144), (0, 0));
        assert_ne!(tick(192), (0, 0));
    }

    #[test]
    fn precision_multiplier_scales_low_speed() {
        let mut cfg = cfg();
        cfg.precision_speed = 100.0;
        assert_eq!(cfg.precision_multiplier(50.0), 0.5);
        assert_eq!(cfg.precision_multiplier(100.0), 1.0);
        assert_eq!(cfg.precision_multiplier(200.0), 1.0);

        let out = run_ticks(&cfg, [0, 800, 0], 30_000, 1);
        assert_eq!(out, (-4, 0));
    }

    #[test]
    fn acceleration_curves_and_cap() {
        let mut cfg = cfg();
        assert_eq!(cfg.acceleration_multiplier(360.0), 1.0);

        cfg.acceleration = GyroAcceleration::Linear;
        assert_eq!(cfg.acceleration_multiplier(360.0), 3.0);

        cfg.acceleration = GyroAcceleration::Relaxed;
        assert_eq!(cfg.acceleration_multiplier(360.0), 4.0);

        cfg.acceleration = GyroAcceleration::Aggressive;
        assert_eq!(cfg.acceleration_multiplier(360.0), 2.5);

        cfg.acceleration = GyroAcceleration::Linear;
        assert_eq!(cfg.acceleration_multiplier(360_000.0), 8.0);
        cfg.acceleration = GyroAcceleration::Aggressive;
        assert_eq!(cfg.acceleration_multiplier(360_000.0), 8.0);
    }

    #[test]
    fn mixer_factors_match_mapping() {
        let mut cfg = cfg();
        assert_eq!(cfg.mixer_factors(), (1.0, 1.0));
        cfg.output_mixer = 1.0;
        assert_eq!(cfg.mixer_factors(), (0.0, 1.0));
        cfg.output_mixer = -1.0;
        assert_eq!(cfg.mixer_factors(), (1.0, 0.0));
        cfg.output_mixer = 0.5;
        assert_eq!(cfg.mixer_factors(), (0.5, 1.0));
        cfg.output_mixer = -0.5;
        assert_eq!(cfg.mixer_factors(), (1.0, 0.5));
    }

    #[test]
    fn smoothing_uses_previous_sample() {
        let mut cfg = cfg();
        cfg.smoothing = 0.5;
        let mut runtime = GyroRuntime::new();

        runtime.process(&sample(0, [0, 1600, 0]), &cfg, true);
        let (x, _) = runtime.process(&sample(30_000, [0, 0, 0]), &cfg, true);

        assert_eq!(x, -9);
    }

    #[test]
    fn invert_axes_flip_output() {
        let mut cfg = cfg();
        cfg.invert_x = true;
        cfg.invert_y = true;
        let out = run_ticks(&cfg, [1440, 1440, 0], 30_000, 1);
        assert_eq!(out, (16, 16));
    }

    #[test]
    fn default_axis_signs() {
        let out = run_ticks(&cfg(), [0, 1440, 0], 30_000, 1);
        assert!(out.0 < 0, "raw +RY (yaw left) should move REL_X negative");
        assert_eq!(out.1, 0);

        let out = run_ticks(&cfg(), [1440, 0, 0], 30_000, 1);
        assert!(out.1 < 0, "raw +RX (pitch up) should move REL_Y negative");
        assert_eq!(out.0, 0);

        let mut inverted = cfg();
        inverted.invert_x = true;
        inverted.invert_y = true;
        let out = run_ticks(&inverted, [1440, 1440, 0], 30_000, 1);
        assert!(out.0 > 0, "Invert X flips yaw");
        assert!(out.1 > 0, "Invert Y flips pitch");
    }

    #[test]
    fn subpixel_remainders_accumulate() {
        let mut cfg = cfg();
        cfg.dots_per_360 = 360.0;
        let out = run_ticks(&cfg, [0, 800, 0], 30_000, 10);
        assert!(
            (4..=5).contains(&out.0.abs()),
            "expected carry to emit ~5 counts, got {}",
            out.0
        );
    }

    #[test]
    fn per_tick_output_is_clamped() {
        let mut cfg = cfg();
        cfg.dots_per_360 = 20_000.0;
        cfg.sensitivity = 10.0;
        let out = run_ticks(&cfg, [0, 20_000, 0], 60_000, 1);
        assert_eq!(out.0, -4096);
    }

    #[test]
    fn hold_activation_requires_button() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::Hold { button: Button::R3 };
        let mut runtime = GyroRuntime::new();
        let mut s = sample(0, [0, 1440, 0]);

        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 30_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));

        s.buttons = BTN_R3;
        s.sensor_timestamp = 60_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 90_000;
        assert_ne!(runtime.process(&s, &cfg, true), (0, 0));
    }

    #[test]
    fn toggle_activation_uses_rising_edge_and_resets() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::Toggle { button: Button::R3 };
        let mut runtime = GyroRuntime::new();
        let mut s = sample(0, [0, 1440, 0]);
        s.buttons = BTN_R3;

        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.buttons = 0;
        s.sensor_timestamp = 30_000;
        assert!(runtime.process(&s, &cfg, true).0 < 0);

        s.buttons = BTN_R3;
        s.sensor_timestamp = 60_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.buttons = 0;
        s.sensor_timestamp = 90_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));

        runtime.process(&s, &cfg, true);
        s.buttons = BTN_R3;
        s.sensor_timestamp = 120_000;
        runtime.process(&s, &cfg, true);
        runtime.reset();
        s.buttons = 0;
        s.sensor_timestamp = 150_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 180_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
    }

    #[test]
    fn touchpad_activation_needs_ownership_and_one_finger() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::Touchpad;
        let mut runtime = GyroRuntime::new();
        let mut s = sample(0, [0, 1440, 0]);
        s.touch_count = 1;

        assert_eq!(runtime.process(&s, &cfg, false), (0, 0));
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 30_000;
        assert!(runtime.process(&s, &cfg, true).0 < 0);

        s.touch_count = 2;
        s.sensor_timestamp = 60_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
    }

    #[test]
    fn trigger_activation_uses_raw_threshold() {
        for button in [Button::L2, Button::R2] {
            let mut cfg = cfg();
            cfg.activation = GyroActivation::Trigger {
                button,
                threshold: 100,
            };
            let set = |s: &mut ControllerState, value: u8| match button {
                Button::L2 => s.l2 = value,
                _ => s.r2 = value,
            };
            let mut runtime = GyroRuntime::new();
            let mut s = sample(0, [0, 1440, 0]);

            set(&mut s, 99);
            assert_eq!(runtime.process(&s, &cfg, true), (0, 0));

            set(&mut s, 100);
            s.sensor_timestamp = 30_000;
            assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
            s.sensor_timestamp = 60_000;
            assert_ne!(runtime.process(&s, &cfg, true), (0, 0));
        }
    }

    #[test]
    fn disabled_config_returns_no_output() {
        let mut cfg = cfg();
        cfg.enabled = false;
        assert_eq!(run_ticks(&cfg, [0, 1440, 0], 30_000, 5), (0, 0));
    }

    #[test]
    fn any_of_activates_when_one_source_is_active() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::AnyOf(vec![
            GyroActivation::Hold {
                button: Button::Triangle,
            },
            GyroActivation::Hold {
                button: Button::Cross,
            },
        ]);
        let mut runtime = GyroRuntime::new();
        let mut s = sample(0, [0, 1440, 0]);

        s.buttons = BTN_TRIANGLE;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 30_000;
        assert!(runtime.process(&s, &cfg, true).0 < 0);

        s.buttons = BTN_CROSS;
        s.sensor_timestamp = 60_000;
        assert!(runtime.process(&s, &cfg, true).0 < 0);

        s.buttons = BTN_TRIANGLE | BTN_CROSS;
        s.sensor_timestamp = 90_000;
        assert!(runtime.process(&s, &cfg, true).0 < 0);
    }

    #[test]
    fn any_of_touchpad_and_two_holds() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::AnyOf(vec![
            GyroActivation::Touchpad,
            GyroActivation::Hold {
                button: Button::Triangle,
            },
            GyroActivation::Hold {
                button: Button::Cross,
            },
        ]);
        let mut runtime = GyroRuntime::new();
        let mut s = sample(0, [0, 1440, 0]);
        s.touch_count = 1;

        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 30_000;
        assert!(runtime.process(&s, &cfg, true).0 < 0);

        s.touch_count = 0;
        s.sensor_timestamp = 60_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));

        s.buttons = BTN_TRIANGLE;
        s.sensor_timestamp = 90_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 120_000;
        assert_ne!(runtime.process(&s, &cfg, true).0, 0);

        s.buttons = BTN_CROSS;
        s.sensor_timestamp = 150_000;
        assert_ne!(runtime.process(&s, &cfg, true).0, 0);

        s.buttons = 0;
        s.sensor_timestamp = 180_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
    }

    #[test]
    fn any_of_with_all_sources_inactive() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::AnyOf(vec![
            GyroActivation::Hold {
                button: Button::Triangle,
            },
            GyroActivation::Toggle { button: Button::R3 },
            GyroActivation::Touchpad,
            GyroActivation::Trigger {
                button: Button::R2,
                threshold: 100,
            },
        ]);

        assert_eq!(run_ticks(&cfg, [0, 1440, 0], 30_000, 5), (0, 0));
    }

    #[test]
    fn any_of_updates_toggle_edges_while_another_source_is_active() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::AnyOf(vec![
            GyroActivation::Hold {
                button: Button::Triangle,
            },
            GyroActivation::Toggle { button: Button::R3 },
        ]);
        let mut runtime = GyroRuntime::new();
        let mut s = sample(0, [0, 1440, 0]);

        s.buttons = BTN_TRIANGLE | BTN_R3;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));

        s.buttons = 0;
        s.sensor_timestamp = 30_000;
        assert!(runtime.process(&s, &cfg, true).0 < 0);

        s.buttons = BTN_R3;
        s.sensor_timestamp = 60_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));

        s.buttons = 0;
        s.sensor_timestamp = 90_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
        s.sensor_timestamp = 120_000;
        assert_eq!(runtime.process(&s, &cfg, true), (0, 0));
    }

    #[test]
    fn multiple_toggles_are_independent() {
        let mut cfg = cfg();
        cfg.activation = GyroActivation::AnyOf(vec![
            GyroActivation::Toggle { button: Button::R3 },
            GyroActivation::Toggle { button: Button::L3 },
        ]);
        let mut runtime = GyroRuntime::new();
        let mut s = sample(0, [0, 1440, 0]);
        let mut tick = |s: &mut ControllerState, buttons: u32| {
            s.buttons = buttons;
            let out = runtime.process(s, &cfg, true);
            s.sensor_timestamp = s.sensor_timestamp.wrapping_add(30_000);
            out
        };

        assert_eq!(tick(&mut s, 0), (0, 0));
        assert_eq!(tick(&mut s, BTN_R3), (0, 0));
        assert_ne!(tick(&mut s, 0).0, 0);
        assert_ne!(tick(&mut s, BTN_L3).0, 0);
        assert_ne!(tick(&mut s, 0).0, 0);
        assert_ne!(tick(&mut s, BTN_R3).0, 0);
        assert_ne!(tick(&mut s, 0).0, 0);
        assert_eq!(tick(&mut s, BTN_L3), (0, 0));
        assert_eq!(tick(&mut s, 0), (0, 0));
    }

    #[test]
    fn legacy_activation_variants_parse_and_round_trip() {
        let cases = [
            ("\"Always\"", GyroActivation::Always),
            ("\"Touchpad\"", GyroActivation::Touchpad),
            (
                "{\"Hold\":{\"button\":\"R3\"}}",
                GyroActivation::Hold { button: Button::R3 },
            ),
            (
                "{\"Toggle\":{\"button\":\"L3\"}}",
                GyroActivation::Toggle { button: Button::L3 },
            ),
            (
                "{\"Trigger\":{\"button\":\"R2\",\"threshold\":42}}",
                GyroActivation::Trigger {
                    button: Button::R2,
                    threshold: 42,
                },
            ),
        ];

        for (json, expected) in cases {
            let parsed: GyroActivation = serde_json::from_str(json).expect("parse legacy variant");
            assert_eq!(parsed, expected);
            let round: GyroActivation =
                serde_json::from_str(&serde_json::to_string(&parsed).expect("serialize"))
                    .expect("round trip");
            assert_eq!(round, expected);
        }
    }

    #[test]
    fn any_of_round_trips() {
        let activation = GyroActivation::AnyOf(vec![
            GyroActivation::Touchpad,
            GyroActivation::Hold {
                button: Button::Triangle,
            },
            GyroActivation::Hold {
                button: Button::Cross,
            },
        ]);
        let json = serde_json::to_string(&activation).expect("serialize");
        assert!(json.contains("AnyOf"));
        let back: GyroActivation = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, activation);
    }
}
