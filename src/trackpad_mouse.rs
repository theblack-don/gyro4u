use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::inputs::SteamPadState;
use crate::steam_controller::SteamPadHaptic;

fn default_sensitivity() -> f32 {
    1400.0
}

fn default_scroll_sensitivity() -> f32 {
    100.0
}

fn default_scroll_natural() -> bool {
    true
}

const SCROLL_NOTCHES_PER_COUNT_AT_100: f32 = 0.02 / 120.0;
const TICK_GATE: Duration = Duration::from_millis(40);

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum SteamPadMouseMode {
    #[default]
    Disabled,
    Mouse,
    Scroll,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum SteamPadHapticStrength {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl SteamPadHapticStrength {
    pub fn gain_db(self) -> Option<i8> {
        match self {
            Self::Off => None,
            Self::Low => Some(-18),
            Self::Medium => Some(-9),
            Self::High => Some(0),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct SteamPadMouseConfig {
    pub mode: SteamPadMouseMode,
    pub sensitivity: f32,
    pub invert_x: bool,
    pub invert_y: bool,
    pub scroll_sensitivity: f32,
    pub scroll_natural: bool,
    pub haptic_strength: SteamPadHapticStrength,
}

impl Default for SteamPadMouseConfig {
    fn default() -> Self {
        Self {
            mode: SteamPadMouseMode::Disabled,
            sensitivity: default_sensitivity(),
            invert_x: false,
            invert_y: false,
            scroll_sensitivity: default_scroll_sensitivity(),
            scroll_natural: default_scroll_natural(),
            haptic_strength: SteamPadHapticStrength::Off,
        }
    }
}

#[derive(Default, Clone)]
struct ClickDebounce {
    initialized: bool,
    latched: bool,
    press_streak: u8,
    release_streak: u8,
}

impl ClickDebounce {
    fn update(&mut self, down: bool) -> bool {
        if !self.initialized {
            self.initialized = true;
            // Starting from an already-held pad must not synthesize a click.
            self.latched = down;
            self.press_streak = u8::from(down);
            return false;
        }

        if down {
            self.release_streak = 0;
            if !self.latched {
                self.press_streak = self.press_streak.saturating_add(1);
                if self.press_streak >= 2 {
                    self.press_streak = 0;
                    self.latched = true;
                    return true;
                }
            }
        } else {
            self.press_streak = 0;
            if self.latched {
                self.release_streak = self.release_streak.saturating_add(1);
                if self.release_streak >= 4 {
                    self.release_streak = 0;
                    self.latched = false;
                }
            }
        }
        false
    }
}

#[derive(Default, Clone)]
struct PadRuntime {
    mode: Option<SteamPadMouseMode>,
    previous: Option<(i16, i16)>,
    accum: [f32; 2],
    scroll_accum: f32,
    click: ClickDebounce,
    last_haptic: Option<Instant>,
}

#[derive(Default)]
struct PadOutput {
    rel: (i32, i32),
    notches: i32,
    haptic: Option<SteamPadHaptic>,
}

impl PadRuntime {
    fn reset_motion(&mut self) {
        self.previous = None;
        self.accum = [0.0; 2];
        self.scroll_accum = 0.0;
    }

    fn process(
        &mut self,
        pad: &SteamPadState,
        cfg: &SteamPadMouseConfig,
        now: Instant,
    ) -> PadOutput {
        let click_fired = self.click.update(pad.click);

        if self.mode != Some(cfg.mode) {
            self.mode = Some(cfg.mode);
            self.reset_motion();
        }

        let mut out = PadOutput::default();
        if pad.touch {
            match cfg.mode {
                SteamPadMouseMode::Disabled => self.reset_motion(),
                SteamPadMouseMode::Mouse => {
                    let Some(previous) = self.previous.replace((pad.x, pad.y)) else {
                        return self.finish(out, click_fired, now, cfg);
                    };

                    let scale = cfg.sensitivity.clamp(0.0, 20_000.0) / 65536.0;
                    let dx_raw = pad.x as i32 - previous.0 as i32;
                    let dy_raw = pad.y as i32 - previous.1 as i32;
                    let mut dx = dx_raw as f32 * scale;
                    // Native pad +Y points up; REL_Y is positive downward.
                    let mut dy = -(dy_raw as f32) * scale;
                    if cfg.invert_x {
                        dx = -dx;
                    }
                    if cfg.invert_y {
                        dy = -dy;
                    }

                    self.accum[0] += dx;
                    self.accum[1] += dy;
                    let out_x = self.accum[0].trunc() as i32;
                    let out_y = self.accum[1].trunc() as i32;
                    self.accum[0] -= out_x as f32;
                    self.accum[1] -= out_y as f32;
                    out.rel = (out_x, out_y);
                }
                SteamPadMouseMode::Scroll => {
                    let Some(previous) = self.previous.replace((pad.x, pad.y)) else {
                        return self.finish(out, click_fired, now, cfg);
                    };

                    let dy_raw = pad.y as i32 - previous.1 as i32;
                    let sensitivity = cfg.scroll_sensitivity.clamp(10.0, 400.0) / 100.0;
                    // Natural: content follows the finger, so pad-up scrolls down.
                    let direction = if cfg.scroll_natural { -1.0 } else { 1.0 };
                    self.scroll_accum +=
                        dy_raw as f32 * SCROLL_NOTCHES_PER_COUNT_AT_100 * sensitivity * direction;
                    let notches = self.scroll_accum.trunc() as i32;
                    self.scroll_accum -= notches as f32;
                    out.notches = notches;
                }
            }
        } else {
            self.reset_motion();
        }

        self.finish(out, click_fired, now, cfg)
    }

    fn finish(
        &mut self,
        mut out: PadOutput,
        click_fired: bool,
        now: Instant,
        cfg: &SteamPadMouseConfig,
    ) -> PadOutput {
        let haptic = if click_fired {
            // A click also starts the tick gate; click always wins the tick.
            self.last_haptic = Some(now);
            Some(SteamPadHaptic::Click)
        } else if out.notches != 0
            && self
                .last_haptic
                .is_none_or(|last| now.saturating_duration_since(last) >= TICK_GATE)
        {
            self.last_haptic = Some(now);
            Some(SteamPadHaptic::Tick)
        } else {
            None
        };
        if cfg.haptic_strength != SteamPadHapticStrength::Off {
            out.haptic = haptic;
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SteamTrackpadOutput {
    pub rel: (i32, i32),
    /// `(REL_HWHEEL, REL_WHEEL)`; horizontal scroll is not implemented.
    pub wheel: (i32, i32),
    /// Haptic events indexed by pad: `[left, right]`.
    pub haptics: [Option<SteamPadHaptic>; 2],
}

#[derive(Default)]
pub struct SteamTrackpadMouseRuntime {
    left: PadRuntime,
    right: PadRuntime,
}

impl SteamTrackpadMouseRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn process(
        &mut self,
        left: &SteamPadState,
        left_cfg: &SteamPadMouseConfig,
        right: &SteamPadState,
        right_cfg: &SteamPadMouseConfig,
        now: Instant,
    ) -> SteamTrackpadOutput {
        let left = self.left.process(left, left_cfg, now);
        let right = self.right.process(right, right_cfg, now);
        SteamTrackpadOutput {
            rel: (left.rel.0 + right.rel.0, left.rel.1 + right.rel.1),
            wheel: (0, left.notches + right.notches),
            haptics: [left.haptic, right.haptic],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(x: i16, y: i16) -> SteamPadState {
        SteamPadState {
            touch: true,
            x,
            y,
            ..Default::default()
        }
    }

    fn cfg() -> SteamPadMouseConfig {
        SteamPadMouseConfig {
            mode: SteamPadMouseMode::Mouse,
            sensitivity: 100.0,
            ..Default::default()
        }
    }

    fn scroll_cfg() -> SteamPadMouseConfig {
        SteamPadMouseConfig {
            mode: SteamPadMouseMode::Scroll,
            haptic_strength: SteamPadHapticStrength::High,
            ..Default::default()
        }
    }

    fn process(
        runtime: &mut SteamTrackpadMouseRuntime,
        left: &SteamPadState,
        left_cfg: &SteamPadMouseConfig,
        right: &SteamPadState,
        right_cfg: &SteamPadMouseConfig,
    ) -> SteamTrackpadOutput {
        runtime.process(left, left_cfg, right, right_cfg, Instant::now())
    }

    #[test]
    fn disabled_and_untouched_pads_emit_nothing() {
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = cfg();
        let out = process(&mut runtime, &pad(0, 0), &config, &pad(0, 0), &config);
        assert_eq!(out.rel, (0, 0));
        assert_eq!(out.wheel, (0, 0));
        config.mode = SteamPadMouseMode::Disabled;
        let mut touched = pad(0, 0);
        let out = process(&mut runtime, &touched, &config, &touched, &config);
        assert_eq!(out.rel, (0, 0));
        assert_eq!(out.wheel, (0, 0));
        touched.touch = false;
        config.mode = SteamPadMouseMode::Mouse;
        let out = process(&mut runtime, &touched, &config, &touched, &config);
        assert_eq!(out.rel, (0, 0));
        assert_eq!(out.wheel, (0, 0));
    }

    #[test]
    fn movement_is_relative_and_scaled() {
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let config = cfg();
        let untouched = SteamPadState::default();
        let out = process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        assert_eq!(out.rel, (0, 0));
        // 100 counts per full 65536-unit pad span.
        let out = process(
            &mut runtime,
            &pad(6554, -6554),
            &config,
            &untouched,
            &config,
        );
        assert_eq!(out.rel, (10, 10));
    }

    #[test]
    fn invert_flips_each_axis() {
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = cfg();
        config.invert_x = true;
        config.invert_y = true;
        let untouched = SteamPadState::default();
        process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        let out = process(
            &mut runtime,
            &pad(6554, -6554),
            &config,
            &untouched,
            &config,
        );
        assert_eq!(out.rel, (-10, -10));
    }

    #[test]
    fn pads_are_independent_and_release_resets() {
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let config = cfg();
        let untouched = SteamPadState::default();
        process(&mut runtime, &pad(0, 0), &config, &pad(0, 0), &config);
        let out = process(&mut runtime, &untouched, &config, &pad(6554, 0), &config);
        assert_eq!(out.rel, (10, 0));
        // Right pad released: the next touch starts a fresh gesture.
        let out = process(&mut runtime, &untouched, &config, &untouched, &config);
        assert_eq!(out.rel, (0, 0));
        let out = process(&mut runtime, &untouched, &config, &pad(6554, 0), &config);
        assert_eq!(out.rel, (0, 0));
    }

    #[test]
    fn extreme_coordinate_jumps_do_not_overflow() {
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let config = cfg();
        let untouched = SteamPadState::default();
        process(
            &mut runtime,
            &pad(i16::MIN, i16::MIN),
            &config,
            &untouched,
            &config,
        );
        let out = process(
            &mut runtime,
            &pad(i16::MAX, i16::MAX),
            &config,
            &untouched,
            &config,
        );
        assert!(out.rel.0 > 0);
        assert!(out.rel.1 < 0);
    }

    #[test]
    fn serde_defaults_old_profiles_to_disabled_and_off() {
        let parsed: SteamPadMouseConfig =
            serde_json::from_str("{}").expect("parse empty pad config");
        assert_eq!(parsed, SteamPadMouseConfig::default());
        assert_eq!(parsed.mode, SteamPadMouseMode::Disabled);
        assert_eq!(parsed.scroll_sensitivity, 100.0);
        assert!(parsed.scroll_natural);
        assert_eq!(parsed.haptic_strength, SteamPadHapticStrength::Off);
    }

    #[test]
    fn haptic_strength_gain_mapping_matches_spec() {
        assert_eq!(SteamPadHapticStrength::Off.gain_db(), None);
        assert_eq!(SteamPadHapticStrength::Low.gain_db(), Some(-18));
        assert_eq!(SteamPadHapticStrength::Medium.gain_db(), Some(-9));
        assert_eq!(SteamPadHapticStrength::High.gain_db(), Some(0));
    }

    #[test]
    fn scroll_is_vertical_and_direction_is_configurable() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let config = scroll_cfg();
        process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        // 3000 counts is exactly 0.5 notch at 100%.
        let out = process(&mut runtime, &pad(0, 3000), &config, &untouched, &config);
        assert_eq!(out.wheel, (0, 0));
        let out = process(&mut runtime, &pad(0, 6000), &config, &untouched, &config);
        assert_eq!(out.wheel, (0, -1));
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Tick));

        let mut natural = SteamTrackpadMouseRuntime::new();
        process(
            &mut natural,
            &pad(0, 0),
            &scroll_cfg(),
            &untouched,
            &scroll_cfg(),
        );
        let out = process(
            &mut natural,
            &pad(0, 6000),
            &scroll_cfg(),
            &untouched,
            &scroll_cfg(),
        );
        assert_eq!(out.wheel, (0, -1));
        assert_eq!(out.wheel.0, 0);

        let mut muted = scroll_cfg();
        muted.scroll_natural = false;
        let mut inverted = SteamTrackpadMouseRuntime::new();
        process(&mut inverted, &pad(0, 0), &muted, &untouched, &muted);
        let out = process(&mut inverted, &pad(0, 6000), &muted, &untouched, &muted);
        assert_eq!(out.wheel, (0, 1));
    }

    #[test]
    fn scroll_sensitivity_scales_notches() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = scroll_cfg();
        config.scroll_sensitivity = 400.0;
        process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        let out = process(&mut runtime, &pad(0, 6000), &config, &untouched, &config);
        assert_eq!(out.wheel, (0, -4));
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Tick));
    }

    #[test]
    fn scroll_fraction_resets_on_release_and_mode_change() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let config = scroll_cfg();
        process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        let out = process(&mut runtime, &pad(0, 3000), &config, &untouched, &config);
        assert_eq!(out.wheel, (0, 0));

        process(&mut runtime, &untouched, &config, &untouched, &config);
        process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        let out = process(&mut runtime, &pad(0, 3000), &config, &untouched, &config);
        assert_eq!(out.wheel, (0, 0), "release must clear the partial notch");

        // A mode change also clears a fresh 0.5-notch remainder.
        let mut changed = SteamTrackpadMouseRuntime::new();
        process(&mut changed, &pad(0, 0), &config, &untouched, &config);
        process(&mut changed, &pad(0, 3000), &config, &untouched, &config);
        let mouse = cfg();
        process(&mut changed, &pad(0, 3000), &mouse, &untouched, &mouse);
        process(&mut changed, &pad(0, 3000), &config, &untouched, &config);
        let out = process(&mut changed, &pad(0, 6000), &config, &untouched, &config);
        assert_eq!(
            out.wheel,
            (0, 0),
            "mode change must clear the partial notch"
        );
    }

    #[test]
    fn scroll_ticks_are_gated_and_not_queued() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = scroll_cfg();
        config.scroll_sensitivity = 400.0;
        let start = Instant::now();
        runtime.process(&pad(0, 0), &config, &untouched, &config, start);
        let out = runtime.process(&pad(0, 6000), &config, &untouched, &config, start);
        assert_eq!(out.wheel, (0, -4));
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Tick));

        // More notches inside the 40 ms window: wheel still moves, tick drops.
        let out = runtime.process(
            &pad(0, 12000),
            &config,
            &untouched,
            &config,
            start + Duration::from_millis(10),
        );
        assert_eq!(out.wheel, (0, -4));
        assert_eq!(out.haptics[0], None);

        let out = runtime.process(
            &pad(0, 18000),
            &config,
            &untouched,
            &config,
            start + Duration::from_millis(40),
        );
        assert_eq!(out.wheel, (0, -4));
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Tick));
    }

    #[test]
    fn sustained_scroll_emits_at_most_one_tick_per_40ms_without_backlog() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = scroll_cfg();
        config.scroll_sensitivity = 400.0;
        let start = Instant::now();
        runtime.process(&pad(0, 0), &config, &untouched, &config, start);

        let mut ticks = 0;
        for step in 1..=10 {
            let y = (step * 600) as i16;
            let out = runtime.process(
                &pad(0, y),
                &config,
                &untouched,
                &config,
                start + Duration::from_millis(step * 10),
            );
            if out.haptics[0] == Some(SteamPadHaptic::Tick) {
                ticks += 1;
            }
        }
        assert_eq!(ticks, 2);

        // No movement means no queued tick.
        let out = runtime.process(
            &pad(0, 6000),
            &config,
            &untouched,
            &config,
            start + Duration::from_millis(300),
        );
        assert_eq!(out.wheel, (0, 0));
        assert_eq!(out.haptics[0], None);
    }

    #[test]
    fn clicks_work_in_raw_mouse_and_scroll_modes() {
        for mode in [
            SteamPadMouseMode::Disabled,
            SteamPadMouseMode::Mouse,
            SteamPadMouseMode::Scroll,
        ] {
            let mut runtime = SteamTrackpadMouseRuntime::new();
            let config = SteamPadMouseConfig {
                mode,
                haptic_strength: SteamPadHapticStrength::High,
                ..Default::default()
            };
            let idle = SteamPadState::default();
            process(&mut runtime, &pad(0, 0), &config, &idle, &config);
            let mut down = pad(0, 0);
            down.click = true;
            let out = process(&mut runtime, &down, &config, &idle, &config);
            assert_eq!(out.haptics[0], None, "first debounce report: {mode:?}");
            let out = process(&mut runtime, &down, &config, &idle, &config);
            assert_eq!(
                out.haptics[0],
                Some(SteamPadHaptic::Click),
                "second report: {mode:?}"
            );
        }
    }

    #[test]
    fn click_fires_once_while_held_and_rearms_after_four_releases() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let config = SteamPadMouseConfig {
            haptic_strength: SteamPadHapticStrength::High,
            ..Default::default()
        };
        process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        let mut down = pad(0, 0);
        down.click = true;
        process(&mut runtime, &down, &config, &untouched, &config);
        let out = process(&mut runtime, &down, &config, &untouched, &config);
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Click));
        for _ in 0..20 {
            let out = process(&mut runtime, &down, &config, &untouched, &config);
            assert_eq!(out.haptics[0], None);
        }

        // Chatter shorter than four reports does not re-arm.
        let up = pad(0, 0);
        for _ in 0..3 {
            process(&mut runtime, &up, &config, &untouched, &config);
        }
        process(&mut runtime, &down, &config, &untouched, &config);
        let out = process(&mut runtime, &down, &config, &untouched, &config);
        assert_eq!(out.haptics[0], None);

        // Four consecutive releases re-arm; the next two presses click.
        for _ in 0..4 {
            process(&mut runtime, &up, &config, &untouched, &config);
        }
        process(&mut runtime, &down, &config, &untouched, &config);
        let out = process(&mut runtime, &down, &config, &untouched, &config);
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Click));
    }

    #[test]
    fn starting_from_held_state_is_disarmed() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let config = SteamPadMouseConfig {
            haptic_strength: SteamPadHapticStrength::High,
            ..Default::default()
        };
        let mut held = pad(0, 0);
        held.click = true;
        for _ in 0..10 {
            let out = process(&mut runtime, &held, &config, &untouched, &config);
            assert_eq!(out.haptics[0], None);
        }
        process(&mut runtime, &untouched, &config, &untouched, &config);
        process(&mut runtime, &held, &config, &untouched, &config);
        let out = process(&mut runtime, &held, &config, &untouched, &config);
        assert_eq!(out.haptics[0], None, "still latched until four releases");
    }

    #[test]
    fn off_strength_suppresses_haptics_not_output() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = scroll_cfg();
        config.haptic_strength = SteamPadHapticStrength::Off;
        config.scroll_sensitivity = 400.0;
        process(&mut runtime, &pad(0, 0), &config, &untouched, &config);
        let out = process(&mut runtime, &pad(0, 6000), &config, &untouched, &config);
        assert_eq!(out.wheel, (0, -4));
        assert_eq!(out.haptics[0], None);

        let mut click_config = SteamPadMouseConfig {
            mode: SteamPadMouseMode::Mouse,
            ..Default::default()
        };
        click_config.haptic_strength = SteamPadHapticStrength::Off;
        let mut clicks = SteamTrackpadMouseRuntime::new();
        process(
            &mut clicks,
            &pad(0, 0),
            &click_config,
            &untouched,
            &click_config,
        );
        let mut down = pad(0, 0);
        down.click = true;
        process(&mut clicks, &down, &click_config, &untouched, &click_config);
        let out = process(&mut clicks, &down, &click_config, &untouched, &click_config);
        assert_eq!(out.haptics[0], None);
    }

    #[test]
    fn click_takes_priority_over_tick_and_stamps_the_gate() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = scroll_cfg();
        config.scroll_sensitivity = 400.0;
        let start = Instant::now();
        // First idle report arms the click debounce.
        runtime.process(&pad(0, 0), &config, &untouched, &config, start);
        let mut down = pad(0, 6000);
        down.click = true;
        let out = runtime.process(&down, &config, &untouched, &config, start);
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Tick));

        let mut down = pad(0, 12000);
        down.click = true;
        let out = runtime.process(
            &down,
            &config,
            &untouched,
            &config,
            start + Duration::from_millis(10),
        );
        assert_eq!(
            out.haptics[0],
            Some(SteamPadHaptic::Click),
            "click wins over the scroll notch emitted in the same report"
        );

        let out = runtime.process(
            &pad(0, 18000),
            &config,
            &untouched,
            &config,
            start + Duration::from_millis(30),
        );
        assert_eq!(out.haptics[0], None, "click stamped the 40 ms gate");

        let out = runtime.process(
            &pad(0, 24000),
            &config,
            &untouched,
            &config,
            start + Duration::from_millis(50),
        );
        assert_eq!(out.haptics[0], Some(SteamPadHaptic::Tick));
    }

    #[test]
    fn pads_have_independent_haptic_gates() {
        let untouched = SteamPadState::default();
        let mut runtime = SteamTrackpadMouseRuntime::new();
        let mut config = scroll_cfg();
        config.scroll_sensitivity = 400.0;
        let start = Instant::now();
        runtime.process(&pad(0, 0), &config, &pad(0, 0), &config, start);
        let out = runtime.process(&pad(0, 6000), &config, &pad(0, 6000), &config, start);
        assert_eq!(
            out.haptics,
            [Some(SteamPadHaptic::Tick), Some(SteamPadHaptic::Tick)]
        );
        let out = runtime.process(
            &pad(0, 12000),
            &config,
            &untouched,
            &config,
            start + Duration::from_millis(10),
        );
        assert_eq!(out.haptics[0], None, "left gate is independent");
    }
}
