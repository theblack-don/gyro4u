use serde::{Deserialize, Serialize};

const TIMESTAMP_UNITS_PER_SECOND: f32 = 3_000_000.0;
const DT_MIN: f32 = 0.0005;
const DT_MAX: f32 = 0.02;
const MAX_COUNTS_PER_TICK: f32 = 4096.0;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct StickMouseConfig {
    pub enabled: bool,
    pub speed: f32,
    pub horizontal_scale: f32,
    pub vertical_scale: f32,
    pub deadzone: f32,
    pub curve: StickMouseCurve,
    pub smoothing: f32,
    pub invert_x: bool,
    pub invert_y: bool,
}

impl Default for StickMouseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            speed: 1200.0,
            horizontal_scale: 1.0,
            vertical_scale: 1.0,
            deadzone: 0.15,
            curve: StickMouseCurve::Linear,
            smoothing: 0.0,
            invert_x: false,
            invert_y: false,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum StickMouseCurve {
    #[default]
    Linear,
    Relaxed,
    Aggressive,
    Exponential,
}

impl StickMouseCurve {
    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Relaxed => "Relaxed",
            Self::Aggressive => "Aggressive",
            Self::Exponential => "Exponential",
        }
    }

    fn apply(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::Relaxed => t * t,
            Self::Aggressive => t.sqrt(),
            Self::Exponential => t * t * t,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StickMouseRuntime {
    prev_ts: u32,
    primed: bool,
    smoothing_primed: bool,
    smoothed: [f32; 2],
    accum: [f32; 2],
}

impl Default for StickMouseRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl StickMouseRuntime {
    pub fn new() -> Self {
        Self {
            prev_ts: 0,
            primed: false,
            smoothing_primed: false,
            smoothed: [0.0; 2],
            accum: [0.0; 2],
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn process(&mut self, raw: (u8, u8), cfg: &StickMouseConfig, timestamp: u32) -> (i32, i32) {
        if !cfg.enabled {
            self.accum = [0.0; 2];
            return (0, 0);
        }

        let nx = (raw.0 as f32 - 128.0) / 127.0;
        let ny = (raw.1 as f32 - 128.0) / 127.0;
        let magnitude = (nx * nx + ny * ny).sqrt();
        let deadzone = cfg.deadzone.clamp(0.0, 1.0);
        if magnitude <= deadzone {
            self.accum = [0.0; 2];
            self.prev_ts = timestamp;
            return (0, 0);
        }

        if !self.primed {
            self.prev_ts = timestamp;
            self.primed = true;
            return (0, 0);
        }

        let delta = timestamp.wrapping_sub(self.prev_ts);
        self.prev_ts = timestamp;
        if delta == 0 {
            return (0, 0);
        }
        let dt = (delta as f32 / TIMESTAMP_UNITS_PER_SECOND).clamp(DT_MIN, DT_MAX);

        let t = ((magnitude - deadzone) / (1.0 - deadzone).max(f32::EPSILON)).clamp(0.0, 1.0);
        let curve = cfg.curve.apply(t);
        let dir_x = nx / magnitude;
        let dir_y = ny / magnitude;
        let invert_x = if cfg.invert_x { -1.0 } else { 1.0 };
        let invert_y = if cfg.invert_y { -1.0 } else { 1.0 };
        let vx = dir_x * cfg.speed * curve * cfg.horizontal_scale * invert_x * dt;
        let vy = dir_y * cfg.speed * curve * cfg.vertical_scale * invert_y * dt;

        let smoothing = cfg.smoothing.clamp(0.0, 0.95);
        if self.smoothing_primed {
            self.smoothed[0] = self.smoothed[0] * smoothing + vx * (1.0 - smoothing);
            self.smoothed[1] = self.smoothed[1] * smoothing + vy * (1.0 - smoothing);
        } else {
            self.smoothed = [vx, vy];
            self.smoothing_primed = true;
        }

        self.accum[0] += self.smoothed[0];
        self.accum[1] += self.smoothed[1];
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

    fn cfg() -> StickMouseConfig {
        StickMouseConfig {
            enabled: true,
            ..Default::default()
        }
    }

    fn run(cfg: &StickMouseConfig, raw: (u8, u8), dt_raw: u32, ticks: u32) -> (i32, i32) {
        let mut runtime = StickMouseRuntime::new();
        let mut ts = 0u32;
        let mut out = (0, 0);
        for _ in 0..=ticks {
            let (x, y) = runtime.process(raw, cfg, ts);
            out.0 += x;
            out.1 += y;
            ts = ts.wrapping_add(dt_raw);
        }
        out
    }

    #[test]
    fn defaults_match_spec() {
        let cfg = StickMouseConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.speed, 1200.0);
        assert_eq!(cfg.horizontal_scale, 1.0);
        assert_eq!(cfg.vertical_scale, 1.0);
        assert_eq!(cfg.deadzone, 0.15);
        assert_eq!(cfg.curve, StickMouseCurve::Linear);
        assert_eq!(cfg.smoothing, 0.0);
        assert!(!cfg.invert_x);
        assert!(!cfg.invert_y);
    }

    #[test]
    fn serde_uses_defaults_for_missing_fields() {
        let parsed: StickMouseConfig = serde_json::from_str("{}").expect("parse empty config");
        assert_eq!(parsed, StickMouseConfig::default());
    }

    #[test]
    fn deadzone_produces_no_output() {
        let cfg = cfg();
        let mut runtime = StickMouseRuntime::new();

        assert_eq!(runtime.process((128, 128), &cfg, 0), (0, 0));
        assert_eq!(runtime.process((145, 128), &cfg, 30_000), (0, 0));
        assert_eq!(runtime.process((128, 128), &cfg, 60_000), (0, 0));

        let mut disabled = cfg.clone();
        disabled.enabled = false;
        assert_eq!(run(&disabled, (255, 128), 30_000, 5), (0, 0));
    }

    #[test]
    fn full_right_matches_speed() {
        let out = run(&cfg(), (255, 128), 30_000, 100);
        assert_eq!(out, (1200, 0));
    }

    #[test]
    fn scales_apply_per_axis() {
        let mut horizontal = cfg();
        horizontal.horizontal_scale = 0.5;
        assert_eq!(run(&horizontal, (255, 128), 30_000, 100), (600, 0));

        let mut vertical = cfg();
        vertical.vertical_scale = 0.25;
        assert_eq!(run(&vertical, (128, 255), 30_000, 100), (0, 300));
    }

    #[test]
    fn curves_match_at_half_tilt() {
        assert_eq!(StickMouseCurve::Linear.apply(0.5), 0.5);
        assert_eq!(StickMouseCurve::Relaxed.apply(0.5), 0.25);
        assert_eq!(StickMouseCurve::Aggressive.apply(0.5), 0.5f32.sqrt());
        assert_eq!(StickMouseCurve::Exponential.apply(0.5), 0.125);
    }

    #[test]
    fn invert_flips_direction() {
        let mut invert_x = cfg();
        invert_x.invert_x = true;
        let out = run(&invert_x, (255, 128), 30_000, 100);
        assert_eq!(out, (-1200, 0));

        let mut invert_y = cfg();
        invert_y.invert_y = true;
        let out = run(&invert_y, (128, 255), 30_000, 100);
        assert_eq!(out, (0, -1200));
    }

    #[test]
    fn smoothing_uses_previous_sample() {
        let mut cfg = cfg();
        cfg.smoothing = 0.5;
        let mut runtime = StickMouseRuntime::new();

        assert_eq!(runtime.process((255, 128), &cfg, 0), (0, 0));
        assert_eq!(runtime.process((255, 128), &cfg, 30_000), (12, 0));
        assert_eq!(runtime.process((191, 128), &cfg, 60_000), (8, 0));
        assert_eq!(runtime.process((255, 128), &cfg, 90_000), (10, 0));
    }

    #[test]
    fn subpixel_remainders_accumulate() {
        let out = run(&cfg(), (160, 128), 30_000, 10);
        assert_eq!(out, (14, 0));
    }

    #[test]
    fn first_sample_and_zero_delta_produce_no_output() {
        let cfg = cfg();
        let mut runtime = StickMouseRuntime::new();

        assert_eq!(runtime.process((255, 128), &cfg, 1_000), (0, 0));
        assert_eq!(runtime.process((255, 128), &cfg, 1_000), (0, 0));
        assert_ne!(runtime.process((255, 128), &cfg, 31_000), (0, 0));
    }

    #[test]
    fn timestamp_wrap_produces_dt() {
        let cfg = cfg();
        let mut runtime = StickMouseRuntime::new();
        let start = u32::MAX - 14_999;

        assert_eq!(runtime.process((255, 128), &cfg, start), (0, 0));
        let (x, _) = runtime.process((255, 128), &cfg, start.wrapping_add(30_000));
        assert_ne!(x, 0, "wrapped timestamp should still produce motion");
    }
}
