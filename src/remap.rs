use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::inputs::*;
use crate::keycodes;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
#[serde(transparent)]
pub struct Key(pub String);

impl Key {
    pub fn new(name: &str) -> Option<Self> {
        keycodes::keycode(name).map(|_| Self(name.to_string()))
    }

    pub fn evdev(&self) -> Option<u16> {
        keycodes::keycode(&self.0)
    }

    pub fn label(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    pub fn evdev(self) -> u16 {
        match self {
            Self::Left => 0x110,
            Self::Right => 0x111,
            Self::Middle => 0x112,
            Self::Back => 0x113,
            Self::Forward => 0x114,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Middle => "Middle",
            Self::Back => "Back",
            Self::Forward => "Forward",
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub enum WheelDir {
    Up,
    Down,
    Left,
    Right,
}

impl WheelDir {
    /// `(REL_HWHEEL, REL_WHEEL)` delta for one notch.
    pub fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, 1),
            Self::Down => (0, -1),
            Self::Right => (1, 0),
            Self::Left => (-1, 0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum ProfileAction {
    Cycle,
    Switch(String),
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum Binding {
    Gamepad(Button),
    Key(Key),
    Mouse(MouseButton),
    Wheel(WheelDir),
    Profile(ProfileAction),
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum PressType {
    #[default]
    Regular,
    LongPress,
    DoubleTap,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum SlotPolicy {
    #[default]
    Combine,
    FirstMatch,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PressTiming {
    pub long_press_ms: u32,
    pub double_tap_ms: u32,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub slot_policy: HashMap<Button, SlotPolicy>,
}

impl Default for PressTiming {
    fn default() -> Self {
        Self {
            long_press_ms: 400,
            double_tap_ms: 300,
            slot_policy: HashMap::new(),
        }
    }
}

impl PressTiming {
    pub fn policy(&self, button: Button) -> SlotPolicy {
        self.slot_policy.get(&button).copied().unwrap_or_default()
    }

    pub fn set_policy(&mut self, button: Button, policy: SlotPolicy) {
        match policy {
            SlotPolicy::Combine => {
                self.slot_policy.remove(&button);
            }
            SlotPolicy::FirstMatch => {
                self.slot_policy.insert(button, policy);
            }
        }
    }
}

/// Press-type groups in first-appearance order; slots of one type always fire together.
pub fn press_group_order(slots: &[BindingSlot]) -> Vec<PressType> {
    let mut groups = Vec::new();
    for slot in slots {
        if !groups.contains(&slot.press) {
            groups.push(slot.press);
        }
    }
    groups
}

#[derive(Clone, Serialize, PartialEq, Eq, Debug)]
pub struct BindingSlot {
    pub binding: Binding,
    pub press: PressType,
}

impl From<Binding> for BindingSlot {
    fn from(binding: Binding) -> Self {
        Self {
            binding,
            press: PressType::Regular,
        }
    }
}

impl<'de> Deserialize<'de> for BindingSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            New {
                binding: Binding,
                #[serde(default)]
                press: PressType,
            },
            Old(Binding),
        }

        Ok(match Repr::deserialize(deserializer)? {
            Repr::New { binding, press } => Self { binding, press },
            Repr::Old(binding) => Self::from(binding),
        })
    }
}

/// Bindings that apply while `activator` is held.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct Layer {
    pub activator: Button,
    pub bindings: HashMap<Button, Vec<BindingSlot>>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct StickConfig {
    pub left_deadzone: f32,
    pub right_deadzone: f32,
    pub left_analog: bool,
    pub right_analog: bool,
}

impl Default for StickConfig {
    fn default() -> Self {
        Self {
            left_deadzone: 0.40,
            right_deadzone: 0.40,
            left_analog: false,
            right_analog: false,
        }
    }
}

pub const STICK_DIRS: [Button; 8] = [
    Button::LeftStickUp,
    Button::LeftStickDown,
    Button::LeftStickLeft,
    Button::LeftStickRight,
    Button::RightStickUp,
    Button::RightStickDown,
    Button::RightStickLeft,
    Button::RightStickRight,
];

fn dir_values(raw_x: u8, raw_y: u8) -> [f32; 4] {
    let nx = (raw_x as f32 - 128.0) / 127.0;
    let ny = (raw_y as f32 - 128.0) / 127.0;
    [-ny, ny, -nx, nx]
}

fn update_stick_dirs(active: &mut [bool], raw: (u8, u8), deadzone: f32) {
    let release = (deadzone * 0.8).max(0.0);
    for (slot, value) in active.iter_mut().zip(dir_values(raw.0, raw.1)) {
        *slot = if *slot {
            value > release
        } else {
            value > deadzone
        };
    }
}

/// Enter-threshold readout used by the UI; no hysteresis state.
pub fn active_stick_dirs(state: &ControllerState, cfg: &StickConfig) -> Vec<Button> {
    let left = dir_values(state.left_x, state.left_y);
    let right = dir_values(state.right_x, state.right_y);
    let mut active = Vec::new();
    for (i, dir) in STICK_DIRS.iter().enumerate() {
        let (values, deadzone) = if i < 4 {
            (left, cfg.left_deadzone)
        } else {
            (right, cfg.right_deadzone)
        };
        if values[i % 4] > deadzone {
            active.push(*dir);
        }
    }
    active
}

#[derive(Default)]
pub struct AuxOutput {
    pub keys: HashSet<u16>,
    pub mouse_buttons: HashSet<u16>,
    pub rel: (i32, i32),
    pub wheel: (i32, i32),
    pub profile: Option<ProfileAction>,
}

impl AuxOutput {
    pub fn clear(&mut self) {
        self.keys.clear();
        self.mouse_buttons.clear();
        self.rel = (0, 0);
        self.wheel = (0, 0);
        self.profile = None;
    }
}

fn activator_active(activator: Button, s: &ControllerState, touchpad_allowed: bool) -> bool {
    !activator.is_stick_dir()
        && (activator != Button::Touchpad || touchpad_allowed)
        && activator.is_pressed(s)
}

/// Highest active layer index, for display purposes.
pub fn top_active_layer(
    layers: &[Layer],
    s: &ControllerState,
    touchpad_allowed: bool,
) -> Option<usize> {
    layers
        .iter()
        .enumerate()
        .rev()
        .find(|(_, layer)| activator_active(layer.activator, s, touchpad_allowed))
        .map(|(index, _)| index)
}

/// Effective bindings for `btn`: highest active layer that defines it, else base.
pub fn resolve_binding<'a>(
    base: &'a HashMap<Button, Vec<BindingSlot>>,
    layers: &'a [Layer],
    s: &ControllerState,
    btn: Button,
    touchpad_allowed: bool,
) -> Option<&'a Vec<BindingSlot>> {
    for layer in layers.iter().rev() {
        if activator_active(layer.activator, s, touchpad_allowed)
            && let Some(list) = layer.bindings.get(&btn)
        {
            return Some(list);
        }
    }
    base.get(&btn)
}

#[derive(Default)]
struct PressState {
    pressed_at: Option<Instant>,
    long_active: bool,
    double_active: bool,
    last_release: Option<Instant>,
    long_fired: bool,
    double_fired: bool,
}

const FIRST_MATCH_PULSE: Duration = Duration::from_millis(12);

/// One physical press under `FirstMatch`, with its bindings latched at press start.
struct FirstMatchGesture {
    button: Button,
    latched: Option<Vec<BindingSlot>>,
    groups: Vec<PressType>,
    pressed_at: Instant,
    double_until: Instant,
    down: bool,
    first_release: Option<Instant>,
    long_reached: bool,
    second_press_at: Option<Instant>,
    winner: Option<PressType>,
    one_shot_fired: bool,
    pulse_until: Option<Instant>,
}

enum FirstMatchPick {
    Winner(PressType),
    Deferred,
    None,
}

impl FirstMatchGesture {
    fn new(
        button: Button,
        latched: Option<Vec<BindingSlot>>,
        pressed_at: Instant,
        timing: &PressTiming,
    ) -> Self {
        let groups = latched
            .as_deref()
            .map(press_group_order)
            .unwrap_or_default();
        let double_window = Duration::from_millis(timing.double_tap_ms as u64);
        let double_until = if groups.contains(&PressType::DoubleTap) {
            pressed_at + double_window
        } else {
            pressed_at
        };
        Self {
            button,
            latched,
            groups,
            pressed_at,
            double_until,
            down: true,
            first_release: None,
            long_reached: false,
            second_press_at: None,
            winner: None,
            one_shot_fired: false,
            pulse_until: None,
        }
    }

    fn can_accept_second_press(&self, now: Instant) -> bool {
        self.winner.is_none()
            && self.groups.contains(&PressType::DoubleTap)
            && self.second_press_at.is_none()
            && now <= self.double_until
    }

    fn analog_identity_allowed(&self, trigger: Button) -> bool {
        match self.latched.as_deref() {
            None => true,
            Some(list) => {
                self.down
                    && self.winner.is_some_and(|winner| {
                        list.iter().any(|slot| {
                            slot.press == winner && slot.binding == Binding::Gamepad(trigger)
                        })
                    })
            }
        }
    }

    fn group_held(&self, group: PressType, down: bool) -> bool {
        match group {
            PressType::Regular => down,
            PressType::LongPress => down && self.long_reached,
            PressType::DoubleTap => down && self.second_press_at.is_some(),
        }
    }

    fn pick_winner(&self, now: Instant) -> FirstMatchPick {
        for group in &self.groups {
            let eligible = match group {
                PressType::Regular => true,
                PressType::LongPress => self.long_reached,
                PressType::DoubleTap => self.second_press_at.is_some(),
            };
            if eligible {
                return FirstMatchPick::Winner(*group);
            }
            let possible = match group {
                PressType::Regular => false,
                PressType::LongPress => {
                    self.down && self.first_release.is_none() && !self.long_reached
                }
                PressType::DoubleTap => self.second_press_at.is_none() && now <= self.double_until,
            };
            if possible {
                return FirstMatchPick::Deferred;
            }
        }
        FirstMatchPick::None
    }

    #[allow(clippy::too_many_arguments)]
    fn drive(
        &mut self,
        down: bool,
        now: Instant,
        timing: &PressTiming,
        out_buttons: &mut u32,
        out_extra: &mut u32,
        dirs: &mut [bool; 4],
        out: &mut AuxOutput,
    ) -> bool {
        if down && !self.down {
            self.down = true;
            if self.can_accept_second_press(now) {
                self.second_press_at = Some(now);
            }
        } else if !down && self.down {
            self.down = false;
            self.first_release.get_or_insert(now);
            if self.groups.contains(&PressType::DoubleTap)
                && self.second_press_at.is_none()
                && now <= self.double_until
            {
                self.double_until = now + Duration::from_millis(timing.double_tap_ms as u64);
            }
        }

        if down && !self.long_reached && self.first_release.is_none() {
            let threshold = Duration::from_millis(timing.long_press_ms as u64);
            if now.saturating_duration_since(self.pressed_at) >= threshold {
                self.long_reached = true;
            }
        }

        match self.latched.as_deref() {
            None => {
                if down {
                    encode_button(&self.button, out_buttons, out_extra, dirs);
                }
                return !down;
            }
            Some([]) => return !down,
            Some(_) => {}
        }

        if self.winner.is_none() {
            match self.pick_winner(now) {
                FirstMatchPick::Winner(group) => {
                    self.winner = Some(group);
                    self.one_shot_fired = false;
                    if !self.group_held(group, down) {
                        self.pulse_until = Some(now + FIRST_MATCH_PULSE);
                    }
                }
                FirstMatchPick::Deferred => return false,
                FirstMatchPick::None => return !down,
            }
        }

        let winner = self.winner.expect("winner selected above");
        let active = match self.pulse_until {
            Some(until) => now < until,
            None => self.group_held(winner, down),
        };
        if active {
            let slots = self.latched.as_deref().unwrap_or(&[]);
            emit_first_match_group(
                slots,
                winner,
                &mut self.one_shot_fired,
                out_buttons,
                out_extra,
                dirs,
                out,
            );
            false
        } else {
            !down
        }
    }
}

fn emit_first_match_group(
    slots: &[BindingSlot],
    press: PressType,
    one_shot_fired: &mut bool,
    out_buttons: &mut u32,
    out_extra: &mut u32,
    dirs: &mut [bool; 4],
    out: &mut AuxOutput,
) {
    for slot in slots.iter().filter(|slot| slot.press == press) {
        match &slot.binding {
            Binding::Gamepad(target) => {
                encode_button(target, out_buttons, out_extra, dirs);
            }
            Binding::Key(key) => {
                if let Some(code) = key.evdev() {
                    out.keys.insert(code);
                }
            }
            Binding::Mouse(button) => {
                out.mouse_buttons.insert(button.evdev());
            }
            Binding::Wheel(dir) => {
                if !*one_shot_fired {
                    let (h, v) = dir.delta();
                    out.wheel.0 += h;
                    out.wheel.1 += v;
                }
            }
            Binding::Profile(action) => {
                if !*one_shot_fired {
                    out.profile = Some(action.clone());
                }
            }
        }
    }
    *one_shot_fired = true;
}

pub struct RemapEngine {
    // Needed for edge-triggered wheel output.
    prev_controls: HashSet<Button>,
    stick_active: [bool; 8],
    press_state: HashMap<Button, PressState>,
    first_match: HashMap<Button, FirstMatchGesture>,
}

impl RemapEngine {
    pub fn new() -> Self {
        Self {
            prev_controls: HashSet::new(),
            stick_active: [false; 8],
            press_state: HashMap::new(),
            first_match: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.prev_controls.clear();
        self.stick_active = [false; 8];
        self.press_state.clear();
        self.first_match.clear();
    }

    /// Drop in-flight First Match gestures without disturbing Combine press state.
    pub fn reset_pending(&mut self) {
        self.first_match.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &mut self,
        s: &mut ControllerState,
        bindings: &HashMap<Button, Vec<BindingSlot>>,
        layers: &[Layer],
        timing: &PressTiming,
        now: Instant,
        touchpad_allowed: bool,
        sticks: &StickConfig,
        stick_mouse_enabled: bool,
        raw_left: (u8, u8),
        raw_right: (u8, u8),
        out: &mut AuxOutput,
    ) {
        out.clear();

        update_stick_dirs(&mut self.stick_active[0..4], raw_left, sticks.left_deadzone);
        update_stick_dirs(
            &mut self.stick_active[4..8],
            raw_right,
            sticks.right_deadzone,
        );

        let mut pressed: Vec<Button> = Vec::new();
        for btn in ALL_BUTTONS {
            let is_down = if btn.is_stick_dir() {
                let index = STICK_DIRS
                    .iter()
                    .position(|dir| *dir == btn)
                    .expect("stick direction in STICK_DIRS");
                self.stick_active[index]
            } else {
                btn.is_pressed(s)
            };

            let prev = self.prev_controls.contains(&btn);
            let st = self.press_state.entry(btn).or_default();
            if is_down && !prev {
                let double = st.last_release.is_some_and(|released| {
                    now.duration_since(released).as_millis() <= timing.double_tap_ms as u128
                });
                st.pressed_at = Some(now);
                st.long_active = false;
                st.double_active = double;
                st.long_fired = false;
                st.double_fired = false;
                if double {
                    st.last_release = None;
                }
            } else if is_down {
                if let Some(pressed_at) = st.pressed_at
                    && now.duration_since(pressed_at).as_millis() >= timing.long_press_ms as u128
                {
                    st.long_active = true;
                }
            } else if prev {
                st.pressed_at = None;
                st.last_release = Some(now);
                st.long_active = false;
                st.double_active = false;
                st.long_fired = false;
                st.double_fired = false;
            }

            if is_down && !(stick_mouse_enabled && btn.is_right_stick()) {
                pressed.push(btn);
            }
        }

        let mut out_buttons: u32 = 0;
        let mut out_extra: u32 = 0;
        let mut dirs = [false; 4];
        let down_set: HashSet<Button> = pressed.iter().copied().collect();

        for btn in ALL_BUTTONS {
            let is_down = down_set.contains(&btn);
            let rising = is_down && !self.prev_controls.contains(&btn);

            if rising
                && let Some(gesture) = self.first_match.get(&btn)
                && !gesture.can_accept_second_press(now)
            {
                self.first_match.remove(&btn);
            }

            if self.first_match.contains_key(&btn) {
                let finished = {
                    let gesture = self
                        .first_match
                        .get_mut(&btn)
                        .expect("First Match gesture present");
                    gesture.drive(
                        is_down,
                        now,
                        timing,
                        &mut out_buttons,
                        &mut out_extra,
                        &mut dirs,
                        out,
                    )
                };
                if finished {
                    self.first_match.remove(&btn);
                }
                continue;
            }

            if !is_down {
                continue;
            }

            let resolved = resolve_binding(bindings, layers, s, btn, touchpad_allowed);
            if btn == Button::Touchpad && !touchpad_allowed && resolved.is_some() {
                continue;
            }

            if timing.policy(btn) == SlotPolicy::FirstMatch {
                let started_at = self
                    .press_state
                    .get(&btn)
                    .and_then(|st| st.pressed_at)
                    .unwrap_or(now);
                let mut gesture =
                    FirstMatchGesture::new(btn, resolved.cloned(), started_at, timing);
                let finished = gesture.drive(
                    true,
                    now,
                    timing,
                    &mut out_buttons,
                    &mut out_extra,
                    &mut dirs,
                    out,
                );
                if !finished {
                    self.first_match.insert(btn, gesture);
                }
                continue;
            }

            match resolved {
                None => encode_button(&btn, &mut out_buttons, &mut out_extra, &mut dirs),
                Some(list) => {
                    let prev = self.prev_controls.contains(&btn);
                    let st = self.press_state.get(&btn);
                    let mut long_fired = false;
                    let mut double_fired = false;
                    for slot in list {
                        let (active, one_shot) = match slot.press {
                            PressType::Regular => (true, !prev),
                            PressType::LongPress => {
                                let active = st.is_some_and(|st| st.long_active);
                                (active, active && !st.is_some_and(|st| st.long_fired))
                            }
                            PressType::DoubleTap => {
                                let active = st.is_some_and(|st| st.double_active);
                                (active, active && !st.is_some_and(|st| st.double_fired))
                            }
                        };
                        match &slot.binding {
                            Binding::Gamepad(target) => {
                                if active {
                                    encode_button(
                                        target,
                                        &mut out_buttons,
                                        &mut out_extra,
                                        &mut dirs,
                                    );
                                }
                            }
                            Binding::Key(key) => {
                                if active && let Some(code) = key.evdev() {
                                    out.keys.insert(code);
                                }
                            }
                            Binding::Mouse(button) => {
                                if active {
                                    out.mouse_buttons.insert(button.evdev());
                                }
                            }
                            Binding::Wheel(dir) => {
                                if one_shot {
                                    let (h, v) = dir.delta();
                                    out.wheel.0 += h;
                                    out.wheel.1 += v;
                                    match slot.press {
                                        PressType::LongPress => long_fired = true,
                                        PressType::DoubleTap => double_fired = true,
                                        PressType::Regular => {}
                                    }
                                }
                            }
                            // Edge-triggered; last profile binding in a tick wins.
                            Binding::Profile(action) => {
                                if one_shot {
                                    out.profile = Some(action.clone());
                                    match slot.press {
                                        PressType::LongPress => long_fired = true,
                                        PressType::DoubleTap => double_fired = true,
                                        PressType::Regular => {}
                                    }
                                }
                            }
                        }
                    }
                    if let Some(st) = self.press_state.get_mut(&btn) {
                        if long_fired {
                            st.long_fired = true;
                        }
                        if double_fired {
                            st.double_fired = true;
                        }
                    }
                }
            }
        }

        let analog_allowed = |button: Button| {
            if let Some(gesture) = self.first_match.get(&button) {
                return gesture.analog_identity_allowed(button);
            }
            !resolve_binding(bindings, layers, s, button, touchpad_allowed).is_some_and(|list| {
                !list
                    .iter()
                    .any(|slot| slot.binding == Binding::Gamepad(button))
            })
        };
        let l2_allowed = analog_allowed(Button::L2);
        let r2_allowed = analog_allowed(Button::R2);
        if !l2_allowed {
            s.l2 = 0;
        }
        if !r2_allowed {
            s.r2 = 0;
        }

        let has_effective_binding = |btn: Button| {
            resolve_binding(bindings, layers, s, btn, touchpad_allowed)
                .is_some_and(|list| !list.is_empty())
        };
        let left_mapped = STICK_DIRS[0..4].iter().any(|b| has_effective_binding(*b));
        let right_mapped = STICK_DIRS[4..8].iter().any(|b| has_effective_binding(*b));

        if !sticks.left_analog && left_mapped {
            s.left_x = 128;
            s.left_y = 128;
        }
        if !sticks.right_analog && (stick_mouse_enabled || right_mapped) {
            s.right_x = 128;
            s.right_y = 128;
        }

        s.buttons = out_buttons;
        s.extra_buttons = out_extra;
        s.dpad = dirs_to_dpad(dirs);

        self.prev_controls.clear();
        self.prev_controls.extend(pressed);
    }
}

impl Default for RemapEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn dirs_to_dpad(d: [bool; 4]) -> u8 {
    match d {
        [true, false, false, false] => DPAD_N,
        [true, true, false, false] => DPAD_NE,
        [false, true, false, false] => DPAD_E,
        [false, true, true, false] => DPAD_SE,
        [false, false, true, false] => DPAD_S,
        [false, false, true, true] => DPAD_SW,
        [false, false, false, true] => DPAD_W,
        [true, false, false, true] => DPAD_NW,
        _ => DPAD_NEUTRAL,
    }
}

fn encode_button(btn: &Button, out: &mut u32, extra: &mut u32, dirs: &mut [bool; 4]) {
    match btn {
        Button::DPadUp => dirs[0] = true,
        Button::DPadRight => dirs[1] = true,
        Button::DPadDown => dirs[2] = true,
        Button::DPadLeft => dirs[3] = true,
        _ => {
            if let Some(mask) = btn.to_bitmask() {
                *out |= mask;
            }
            if let Some(mask) = btn.to_extra_mask() {
                *extra |= mask;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn state(buttons: u32, dpad: u8) -> ControllerState {
        ControllerState {
            buttons,
            dpad,
            ..ControllerState::default()
        }
    }

    fn slot(binding: Binding) -> BindingSlot {
        BindingSlot::from(binding)
    }

    fn slot_press(binding: Binding, press: PressType) -> BindingSlot {
        BindingSlot { binding, press }
    }

    fn apply(
        s: &mut ControllerState,
        bindings: HashMap<Button, Vec<BindingSlot>>,
        touchpad_allowed: bool,
    ) {
        apply_with_layers(s, &bindings, &[], touchpad_allowed);
    }

    fn apply_with_layers(
        s: &mut ControllerState,
        bindings: &HashMap<Button, Vec<BindingSlot>>,
        layers: &[Layer],
        touchpad_allowed: bool,
    ) {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine_layers(&mut engine, s, bindings, layers, touchpad_allowed, &mut aux);
    }

    fn layer(activator: Button, bindings: HashMap<Button, Vec<BindingSlot>>) -> Layer {
        Layer {
            activator,
            bindings,
        }
    }

    fn one(from: Button, to: Button) -> HashMap<Button, Vec<BindingSlot>> {
        HashMap::from([(from, vec![slot(Binding::Gamepad(to))])])
    }

    fn key(name: &str) -> Binding {
        Binding::Key(Key::new(name).unwrap())
    }

    fn apply_engine(
        engine: &mut RemapEngine,
        s: &mut ControllerState,
        bindings: &HashMap<Button, Vec<BindingSlot>>,
        touchpad_allowed: bool,
        out: &mut AuxOutput,
    ) {
        apply_engine_layers(engine, s, bindings, &[], touchpad_allowed, out);
    }

    fn apply_engine_layers(
        engine: &mut RemapEngine,
        s: &mut ControllerState,
        bindings: &HashMap<Button, Vec<BindingSlot>>,
        layers: &[Layer],
        touchpad_allowed: bool,
        out: &mut AuxOutput,
    ) {
        apply_engine_layers_at(
            engine,
            s,
            bindings,
            layers,
            &PressTiming::default(),
            Instant::now(),
            touchpad_allowed,
            out,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_engine_layers_at(
        engine: &mut RemapEngine,
        s: &mut ControllerState,
        bindings: &HashMap<Button, Vec<BindingSlot>>,
        layers: &[Layer],
        timing: &PressTiming,
        now: Instant,
        touchpad_allowed: bool,
        out: &mut AuxOutput,
    ) {
        let raw_left = (s.left_x, s.left_y);
        let raw_right = (s.right_x, s.right_y);
        engine.apply(
            s,
            bindings,
            layers,
            timing,
            now,
            touchpad_allowed,
            &StickConfig::default(),
            false,
            raw_left,
            raw_right,
            out,
        );
    }

    #[test]
    fn identity_when_binding_absent() {
        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply(&mut s, HashMap::new(), true);
        assert_eq!(s.buttons, BTN_CROSS | BTN_L1);
        assert_eq!(s.dpad, DPAD_NEUTRAL);
    }

    #[test]
    fn empty_binding_disables() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply(&mut s, HashMap::from([(Button::Cross, vec![])]), true);
        assert_eq!(s.buttons, 0);
    }

    #[test]
    fn single_target_remap() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply(&mut s, one(Button::Cross, Button::Circle), true);
        assert_eq!(s.buttons, BTN_CIRCLE);
    }

    #[test]
    fn multi_target_remap() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply(
            &mut s,
            HashMap::from([(
                Button::Cross,
                vec![
                    slot(Binding::Gamepad(Button::Circle)),
                    slot(Binding::Gamepad(Button::Triangle)),
                ],
            )]),
            true,
        );
        assert_eq!(s.buttons, BTN_CIRCLE | BTN_TRIANGLE);
    }

    #[test]
    fn dpad_to_button() {
        let mut s = state(0, DPAD_N);
        apply(&mut s, one(Button::DPadUp, Button::Cross), true);
        assert_eq!(s.buttons, BTN_CROSS);
        assert_eq!(s.dpad, DPAD_NEUTRAL);
    }

    #[test]
    fn button_to_dpad() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply(&mut s, one(Button::Cross, Button::DPadUp), true);
        assert_eq!(s.buttons, 0);
        assert_eq!(s.dpad, DPAD_N);
    }

    #[test]
    fn dpad_diagonal_to_dpad_diagonal() {
        let mut s = state(0, DPAD_SE);
        apply(
            &mut s,
            HashMap::from([
                (
                    Button::DPadDown,
                    vec![slot(Binding::Gamepad(Button::DPadLeft))],
                ),
                (
                    Button::DPadRight,
                    vec![slot(Binding::Gamepad(Button::DPadDown))],
                ),
            ]),
            true,
        );
        assert_eq!(s.dpad, DPAD_SW);
    }

    #[test]
    fn edge_paddle_to_cross() {
        let mut s = state(BTN_EDGE_PADDLE_LEFT, DPAD_NEUTRAL);
        apply(&mut s, one(Button::EdgePaddleLeft, Button::Cross), true);
        assert_eq!(s.buttons, BTN_CROSS);
    }

    #[test]
    fn steam_extra_buttons_pass_through_and_remap() {
        let mut s = state(0, DPAD_NEUTRAL);
        s.extra_buttons = crate::inputs::EXTRA_BTN_L4 | crate::inputs::EXTRA_BTN_QAM;
        apply(&mut s, HashMap::new(), true);
        assert_eq!(
            s.extra_buttons,
            crate::inputs::EXTRA_BTN_L4 | crate::inputs::EXTRA_BTN_QAM
        );
        assert_eq!(s.buttons, 0);

        let mut s = state(0, DPAD_NEUTRAL);
        s.extra_buttons = crate::inputs::EXTRA_BTN_L4;
        apply(&mut s, one(Button::L4, Button::R5), true);
        assert_eq!(s.extra_buttons, crate::inputs::EXTRA_BTN_R5);

        let mut s = state(0, DPAD_NEUTRAL);
        s.extra_buttons = crate::inputs::EXTRA_BTN_RIGHT_GRIP;
        apply(&mut s, one(Button::RightGrip, Button::Cross), true);
        assert_eq!(s.buttons, BTN_CROSS);
        assert_eq!(s.extra_buttons, 0);
    }

    #[test]
    fn touchpad_binding_respects_gate() {
        let bindings = one(Button::Touchpad, Button::Cross);

        let mut allowed = state(BTN_TOUCHPAD, DPAD_NEUTRAL);
        apply(&mut allowed, bindings.clone(), true);
        assert_eq!(allowed.buttons, BTN_CROSS);

        let mut blocked = state(BTN_TOUCHPAD, DPAD_NEUTRAL);
        apply(&mut blocked, bindings, false);
        assert_eq!(blocked.buttons, 0);
    }

    #[test]
    fn l2_analog_suppressed_without_identity_target() {
        let mut s = state(BTN_L2, DPAD_NEUTRAL);
        s.l2 = 200;
        apply(&mut s, one(Button::L2, Button::Cross), true);
        assert_eq!(s.l2, 0);
        assert_eq!(s.buttons, BTN_CROSS);
    }

    #[test]
    fn l2_analog_kept_with_identity_target() {
        let mut s = state(BTN_L2, DPAD_NEUTRAL);
        s.l2 = 200;
        apply(
            &mut s,
            HashMap::from([(
                Button::L2,
                vec![
                    slot(Binding::Gamepad(Button::Cross)),
                    slot(Binding::Gamepad(Button::L2)),
                ],
            )]),
            true,
        );
        assert_eq!(s.l2, 200);
        assert_eq!(s.buttons, BTN_CROSS | BTN_L2);
    }

    #[test]
    fn l2_analog_kept_when_binding_absent() {
        let mut s = state(BTN_L2, DPAD_NEUTRAL);
        s.l2 = 200;
        apply(&mut s, HashMap::new(), true);
        assert_eq!(s.l2, 200);
    }

    #[test]
    fn r2_analog_suppressed_without_identity_target() {
        let mut s = state(BTN_R2, DPAD_NEUTRAL);
        s.r2 = 180;
        apply(&mut s, one(Button::R2, Button::Square), true);
        assert_eq!(s.r2, 0);
        assert_eq!(s.buttons, BTN_SQUARE);
    }

    #[test]
    fn reset_clears_previous_controls() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine(&mut engine, &mut s, &HashMap::new(), true, &mut aux);
        assert!(!engine.prev_controls.is_empty());
        engine.reset();
        assert!(engine.prev_controls.is_empty());
    }

    #[test]
    fn key_new_validates_names() {
        assert!(Key::new("X").is_some());
        assert!(Key::new("NotAKey").is_none());
    }

    #[test]
    fn key_binding_emits_evdev_code() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(Button::Cross, vec![slot(key("X"))])]),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
        assert_eq!(aux.keys, HashSet::from([45]));
    }

    #[test]
    fn steam_menu_and_view_bindings_do_not_cross_fire() {
        let bindings = HashMap::from([
            (Button::Options, vec![slot(key("Esc"))]),
            (Button::Create, vec![slot(key("Tab"))]),
        ]);

        let mut menu = state(BTN_OPTIONS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(&mut engine, &mut menu, &bindings, true, &mut aux);
        assert_eq!(aux.keys, HashSet::from([1]), "Options/Menu emits KEY_ESC");

        let mut view = state(BTN_CREATE, DPAD_NEUTRAL);
        engine.reset();
        apply_engine(&mut engine, &mut view, &bindings, true, &mut aux);
        assert_eq!(aux.keys, HashSet::from([15]), "Create/View emits KEY_TAB");
    }

    #[test]
    fn mouse_binding_emits_button_code() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(
                Button::Cross,
                vec![slot(Binding::Mouse(MouseButton::Right))],
            )]),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
        assert_eq!(aux.mouse_buttons, HashSet::from([0x111]));
    }

    #[test]
    fn wheel_binding_triggers_on_rising_edge_only() {
        let bindings = HashMap::from([(Button::DPadUp, vec![slot(Binding::Wheel(WheelDir::Up))])]);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut held = state(0, DPAD_N);
        apply_engine(&mut engine, &mut held, &bindings, true, &mut aux);
        assert_eq!(aux.wheel, (0, 1));

        let mut still_held = state(0, DPAD_N);
        apply_engine(&mut engine, &mut still_held, &bindings, true, &mut aux);
        assert_eq!(aux.wheel, (0, 0));

        let mut released = state(0, DPAD_NEUTRAL);
        apply_engine(&mut engine, &mut released, &bindings, true, &mut aux);

        let mut pressed_again = state(0, DPAD_N);
        apply_engine(&mut engine, &mut pressed_again, &bindings, true, &mut aux);
        assert_eq!(aux.wheel, (0, 1));
    }

    #[test]
    fn wheel_binding_maps_horizontal_direction() {
        let mut s = state(0, DPAD_E);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(
                Button::DPadRight,
                vec![slot(Binding::Wheel(WheelDir::Right))],
            )]),
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (1, 0));
    }

    #[test]
    fn profile_binding_triggers_on_rising_edge_only() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![slot(Binding::Profile(ProfileAction::Cycle))],
        )]);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut held = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine(&mut engine, &mut held, &bindings, true, &mut aux);
        assert_eq!(aux.profile, Some(ProfileAction::Cycle));

        let mut still_held = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine(&mut engine, &mut still_held, &bindings, true, &mut aux);
        assert_eq!(aux.profile, None);

        let mut released = state(0, DPAD_NEUTRAL);
        apply_engine(&mut engine, &mut released, &bindings, true, &mut aux);
        assert_eq!(aux.profile, None);

        let mut pressed_again = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine(&mut engine, &mut pressed_again, &bindings, true, &mut aux);
        assert_eq!(aux.profile, Some(ProfileAction::Cycle));
    }

    #[test]
    fn profile_binding_suppresses_identity_output() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(
                Button::Cross,
                vec![slot(Binding::Profile(ProfileAction::Cycle))],
            )]),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
        assert_eq!(aux.profile, Some(ProfileAction::Cycle));
    }

    #[test]
    fn mixed_profile_and_key_binding() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(
                Button::Cross,
                vec![
                    slot(Binding::Profile(ProfileAction::Switch("Main".into()))),
                    slot(key("X")),
                ],
            )]),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
        assert_eq!(aux.keys, HashSet::from([45]));
        assert_eq!(aux.profile, Some(ProfileAction::Switch("Main".to_string())));
    }

    #[test]
    fn last_profile_binding_wins() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(
                Button::Cross,
                vec![
                    slot(Binding::Profile(ProfileAction::Cycle)),
                    slot(Binding::Profile(ProfileAction::Switch("Main".into()))),
                ],
            )]),
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, Some(ProfileAction::Switch("Main".to_string())));
    }

    #[test]
    fn profile_action_serde_round_trip() {
        for action in [
            ProfileAction::Cycle,
            ProfileAction::Switch("Main".to_string()),
        ] {
            let json = serde_json::to_string(&action).expect("serialize");
            let back: ProfileAction = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, action);
        }

        let binding = Binding::Profile(ProfileAction::Switch("Main".to_string()));
        let json = serde_json::to_string(&binding).expect("serialize");
        let back: Binding = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, binding);
    }

    #[test]
    fn two_controls_can_share_a_key() {
        let mut s = state(BTN_CROSS | BTN_CIRCLE, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([
                (Button::Cross, vec![slot(key("A"))]),
                (Button::Circle, vec![slot(key("A"))]),
            ]),
            true,
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([30]));
    }

    #[test]
    fn mixed_gamepad_and_key_binding() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(
                Button::Cross,
                vec![slot(Binding::Gamepad(Button::Circle)), slot(key("X"))],
            )]),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE);
        assert_eq!(aux.keys, HashSet::from([45]));
    }

    #[test]
    fn touchpad_key_binding_respects_gate() {
        let bindings = HashMap::from([(Button::Touchpad, vec![slot(key("X"))])]);

        let mut allowed = state(BTN_TOUCHPAD, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(&mut engine, &mut allowed, &bindings, true, &mut aux);
        assert_eq!(aux.keys, HashSet::from([45]));

        let mut blocked = state(BTN_TOUCHPAD, DPAD_NEUTRAL);
        engine.reset();
        apply_engine(&mut engine, &mut blocked, &bindings, false, &mut aux);
        assert!(aux.keys.is_empty());
    }

    #[test]
    fn unknown_key_is_skipped() {
        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(
                Button::Cross,
                vec![slot(Binding::Key(Key("NotAKey".to_string())))],
            )]),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());
        assert_eq!(s.buttons, 0);
    }

    #[test]
    fn trigger_suppressed_with_key_binding() {
        let mut s = state(BTN_L2, DPAD_NEUTRAL);
        s.l2 = 200;
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        apply_engine(
            &mut engine,
            &mut s,
            &HashMap::from([(Button::L2, vec![slot(key("X"))])]),
            true,
            &mut aux,
        );
        assert_eq!(s.l2, 0);
        assert_eq!(aux.keys, HashSet::from([45]));
    }

    #[test]
    fn reset_clears_stick_hysteresis() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let mut s = state(0, DPAD_NEUTRAL);
        engine.apply(
            &mut s,
            &HashMap::new(),
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (220, 128),
            &mut aux,
        );
        assert!(engine.stick_active[7]);
        engine.reset();
        assert_eq!(engine.stick_active, [false; 8]);
    }

    #[test]
    fn right_stick_right_emits_bound_key() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::from([(Button::RightStickRight, vec![slot(key("D"))])]);
        let mut s = state(0, DPAD_NEUTRAL);
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (220, 128),
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([32]));
    }

    #[test]
    fn stick_mouse_ignores_right_stick_direction_bindings() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::from([(Button::RightStickRight, vec![slot(key("D"))])]);
        let mut s = state(0, DPAD_NEUTRAL);
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            true,
            (128, 128),
            (220, 128),
            &mut aux,
        );
        assert!(aux.keys.is_empty());
        assert!(engine.stick_active[7], "hysteresis still tracks the stick");
    }

    #[test]
    fn stick_mouse_off_restores_right_stick_direction_bindings() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::from([(Button::RightStickRight, vec![slot(key("D"))])]);
        let mut s = state(0, DPAD_NEUTRAL);
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (220, 128),
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([32]));
    }

    #[test]
    fn left_stick_directions_work_with_stick_mouse_enabled() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::from([
            (Button::LeftStickLeft, vec![slot(key("A"))]),
            (Button::RightStickRight, vec![slot(key("D"))]),
        ]);
        let mut s = state(0, DPAD_NEUTRAL);
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            true,
            (40, 128),
            (220, 128),
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([30]));
        assert!(engine.stick_active[2]);
        assert!(engine.stick_active[7]);
    }

    #[test]
    fn right_stick_diagonal_emits_both_keys() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::from([
            (Button::RightStickUp, vec![slot(key("W"))]),
            (Button::RightStickRight, vec![slot(key("D"))]),
        ]);
        let mut s = state(0, DPAD_NEUTRAL);
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (220, 40),
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([17, 32]));
        assert!(engine.stick_active[4]);
        assert!(engine.stick_active[7]);
    }

    #[test]
    fn stick_hysteresis_enters_and_releases() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::new();
        let mut s = state(0, DPAD_NEUTRAL);

        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (180, 128),
            &mut aux,
        );
        assert!(engine.stick_active[7], "0.41 enters");

        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (173, 128),
            &mut aux,
        );
        assert!(engine.stick_active[7], "0.35 stays while active");

        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (166, 128),
            &mut aux,
        );
        assert!(!engine.stick_active[7], "0.30 exits");

        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (128, 128),
            (173, 128),
            &mut aux,
        );
        assert!(!engine.stick_active[7], "0.35 stays off while inactive");
    }

    #[test]
    fn left_and_right_sticks_are_independent() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::from([
            (Button::LeftStickLeft, vec![slot(key("A"))]),
            (Button::RightStickUp, vec![slot(key("W"))]),
        ]);
        let mut s = state(0, DPAD_NEUTRAL);
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (40, 128),
            (128, 40),
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([30, 17]));
        assert_eq!(
            engine.stick_active,
            [false, false, true, false, true, false, false, false]
        );
    }

    #[test]
    fn stick_analog_suppressed_when_mapped() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let bindings = HashMap::from([(Button::LeftStickLeft, vec![slot(key("A"))])]);
        let mut s = state(0, DPAD_NEUTRAL);
        s.left_x = 40;
        s.left_y = 90;
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (40, 90),
            (128, 128),
            &mut aux,
        );
        assert_eq!((s.left_x, s.left_y), (128, 128));
        assert_eq!(aux.keys, HashSet::from([30]));
    }

    #[test]
    fn stick_analog_kept_when_enabled() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig {
            left_analog: true,
            ..StickConfig::default()
        };
        let bindings = HashMap::from([(Button::LeftStickLeft, vec![slot(key("A"))])]);
        let mut s = state(0, DPAD_NEUTRAL);
        s.left_x = 40;
        s.left_y = 90;
        engine.apply(
            &mut s,
            &bindings,
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (40, 90),
            (128, 128),
            &mut aux,
        );
        assert_eq!((s.left_x, s.left_y), (40, 90));
    }

    #[test]
    fn stick_analog_untouched_when_unmapped() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let mut s = state(0, DPAD_NEUTRAL);
        s.left_x = 40;
        s.left_y = 90;
        engine.apply(
            &mut s,
            &HashMap::new(),
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            false,
            (40, 90),
            (128, 128),
            &mut aux,
        );
        assert_eq!((s.left_x, s.left_y), (40, 90));
    }

    #[test]
    fn stick_mouse_centers_right_stick_without_direction_bindings() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig::default();
        let mut s = state(0, DPAD_NEUTRAL);
        s.left_x = 40;
        s.left_y = 90;
        s.right_x = 220;
        s.right_y = 40;
        engine.apply(
            &mut s,
            &HashMap::new(),
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            true,
            (40, 90),
            (220, 40),
            &mut aux,
        );
        assert_eq!((s.right_x, s.right_y), (128, 128));
        assert_eq!((s.left_x, s.left_y), (40, 90));
    }

    #[test]
    fn stick_mouse_keeps_right_stick_when_analog_enabled() {
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let cfg = StickConfig {
            right_analog: true,
            ..StickConfig::default()
        };
        let mut s = state(0, DPAD_NEUTRAL);
        s.right_x = 220;
        s.right_y = 40;
        engine.apply(
            &mut s,
            &HashMap::new(),
            &[],
            &PressTiming::default(),
            Instant::now(),
            true,
            &cfg,
            true,
            (128, 128),
            (220, 40),
            &mut aux,
        );
        assert_eq!((s.right_x, s.right_y), (220, 40));
    }

    #[test]
    fn active_stick_dirs_uses_enter_threshold() {
        let mut s = state(0, DPAD_NEUTRAL);
        s.right_x = 220;
        s.right_y = 40;
        assert_eq!(
            active_stick_dirs(&s, &StickConfig::default()),
            vec![Button::RightStickUp, Button::RightStickRight]
        );

        s.left_x = 30;
        assert_eq!(
            active_stick_dirs(&s, &StickConfig::default()),
            vec![
                Button::LeftStickLeft,
                Button::RightStickUp,
                Button::RightStickRight,
            ]
        );
    }

    #[test]
    fn active_layer_overrides_base_binding() {
        let base = one(Button::Cross, Button::Circle);
        let layers = [layer(Button::L1, one(Button::Cross, Button::Triangle))];

        let mut with_layer = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_with_layers(&mut with_layer, &base, &layers, true);
        assert_eq!(with_layer.buttons, BTN_TRIANGLE | BTN_L1);

        let mut without_layer = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_with_layers(&mut without_layer, &base, &layers, true);
        assert_eq!(without_layer.buttons, BTN_CIRCLE);
    }

    #[test]
    fn layer_falls_back_to_base_for_undefined_controls() {
        let base = one(Button::Circle, Button::Triangle);
        let layers = [layer(Button::L1, HashMap::new())];

        let mut s = state(BTN_CIRCLE | BTN_L1, DPAD_NEUTRAL);
        apply_with_layers(&mut s, &base, &layers, true);
        assert_eq!(s.buttons, BTN_TRIANGLE | BTN_L1);
    }

    #[test]
    fn empty_layer_entry_disables_without_falling_back() {
        let base = one(Button::Cross, Button::Circle);
        let layers = [layer(
            Button::L1,
            HashMap::from([(Button::Cross, Vec::new())]),
        )];

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_with_layers(&mut s, &base, &layers, true);
        assert_eq!(s.buttons, BTN_L1);
    }

    #[test]
    fn higher_layer_wins_and_lower_fills_gaps() {
        let layers = [
            layer(
                Button::L1,
                HashMap::from([
                    (Button::Cross, vec![slot(Binding::Gamepad(Button::Circle))]),
                    (Button::Circle, vec![slot(Binding::Gamepad(Button::Square))]),
                ]),
            ),
            layer(
                Button::R1,
                HashMap::from([(
                    Button::Cross,
                    vec![slot(Binding::Gamepad(Button::Triangle))],
                )]),
            ),
        ];

        let mut s = state(BTN_CROSS | BTN_CIRCLE | BTN_L1 | BTN_R1, DPAD_NEUTRAL);
        apply_with_layers(&mut s, &HashMap::new(), &layers, true);
        assert_eq!(s.buttons, BTN_TRIANGLE | BTN_SQUARE | BTN_L1 | BTN_R1);
    }

    #[test]
    fn layer_activator_keeps_base_action() {
        let base = one(Button::L1, Button::R1);
        let layers = [layer(Button::L1, one(Button::Cross, Button::Triangle))];

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_with_layers(&mut s, &base, &layers, true);
        assert_eq!(s.buttons, BTN_TRIANGLE | BTN_R1);

        let layers = [layer(Button::L1, one(Button::L1, Button::Circle))];
        let mut s = state(BTN_L1, DPAD_NEUTRAL);
        apply_with_layers(&mut s, &base, &layers, true);
        assert_eq!(s.buttons, BTN_CIRCLE);
    }

    #[test]
    fn layer_trigger_override_suppresses_analog() {
        let base = HashMap::from([(Button::L2, vec![slot(Binding::Gamepad(Button::L2))])]);
        let layers = [layer(Button::L1, one(Button::L2, Button::Cross))];

        let mut s = state(BTN_L2 | BTN_L1, DPAD_NEUTRAL);
        s.l2 = 200;
        apply_with_layers(&mut s, &base, &layers, true);
        assert_eq!(s.l2, 0);
        assert_eq!(s.buttons, BTN_CROSS | BTN_L1);
    }

    #[test]
    fn empty_layer_trigger_override_suppresses_analog() {
        let base = HashMap::from([(Button::L2, vec![slot(Binding::Gamepad(Button::L2))])]);
        let layers = [layer(Button::L1, HashMap::from([(Button::L2, Vec::new())]))];

        let mut s = state(BTN_L2 | BTN_L1, DPAD_NEUTRAL);
        s.l2 = 200;
        apply_with_layers(&mut s, &base, &layers, true);
        assert_eq!(s.l2, 0);
        assert_eq!(s.buttons, BTN_L1);
    }

    #[test]
    fn layer_stick_binding_suppresses_right_stick_analog() {
        let layers = [layer(
            Button::L1,
            HashMap::from([(Button::RightStickRight, vec![slot(key("D"))])]),
        )];
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();
        let mut s = state(BTN_L1, DPAD_NEUTRAL);
        s.right_x = 220;

        apply_engine_layers(
            &mut engine,
            &mut s,
            &HashMap::new(),
            &layers,
            true,
            &mut aux,
        );

        assert_eq!(aux.keys, HashSet::from([32]));
        assert_eq!((s.right_x, s.right_y), (128, 128));
    }

    #[test]
    fn layer_profile_action_fires_on_press_edge() {
        let layers = [layer(
            Button::L1,
            HashMap::from([(
                Button::Cross,
                vec![slot(Binding::Profile(ProfileAction::Cycle))],
            )]),
        )];
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut pressed = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers(
            &mut engine,
            &mut pressed,
            &HashMap::new(),
            &layers,
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, Some(ProfileAction::Cycle));

        let mut held = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers(
            &mut engine,
            &mut held,
            &HashMap::new(),
            &layers,
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, None);

        let mut released = state(BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers(
            &mut engine,
            &mut released,
            &HashMap::new(),
            &layers,
            true,
            &mut aux,
        );

        let mut pressed_again = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers(
            &mut engine,
            &mut pressed_again,
            &HashMap::new(),
            &layers,
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, Some(ProfileAction::Cycle));
    }

    #[test]
    fn resolve_binding_prefers_highest_active_layer_then_base() {
        let base = one(Button::Circle, Button::Square);
        let layers = [
            layer(Button::L1, one(Button::Cross, Button::Triangle)),
            layer(Button::R1, one(Button::Cross, Button::Circle)),
        ];

        let both = state(BTN_CROSS | BTN_L1 | BTN_R1, DPAD_NEUTRAL);
        assert_eq!(
            resolve_binding(&base, &layers, &both, Button::Cross, true),
            Some(&vec![slot(Binding::Gamepad(Button::Circle))])
        );

        let lower_only = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        assert_eq!(
            resolve_binding(&base, &layers, &lower_only, Button::Cross, true),
            Some(&vec![slot(Binding::Gamepad(Button::Triangle))])
        );

        assert_eq!(
            resolve_binding(&base, &layers, &both, Button::Circle, true),
            Some(&vec![slot(Binding::Gamepad(Button::Square))])
        );

        let idle = state(0, DPAD_NEUTRAL);
        assert_eq!(
            resolve_binding(&base, &layers, &idle, Button::Cross, true),
            None
        );
    }

    #[test]
    fn resolve_binding_returns_empty_layer_entry() {
        let base = one(Button::Cross, Button::Circle);
        let layers = [layer(
            Button::L1,
            HashMap::from([(Button::Cross, Vec::new())]),
        )];
        let s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);

        let resolved = resolve_binding(&base, &layers, &s, Button::Cross, true);
        assert_eq!(resolved, Some(&Vec::new()));
    }

    #[test]
    fn top_active_layer_skips_invalid_activators() {
        let layers = [
            layer(Button::L1, HashMap::new()),
            layer(Button::R1, HashMap::new()),
            layer(Button::LeftStickUp, HashMap::new()),
        ];

        let both = state(BTN_L1 | BTN_R1, DPAD_NEUTRAL);
        assert_eq!(top_active_layer(&layers, &both, true), Some(1));

        let lower = state(BTN_L1, DPAD_NEUTRAL);
        assert_eq!(top_active_layer(&layers, &lower, true), Some(0));

        assert_eq!(
            top_active_layer(&layers, &state(0, DPAD_NEUTRAL), true),
            None
        );

        let mut stick = state(0, DPAD_NEUTRAL);
        stick.left_y = 0;
        assert_eq!(top_active_layer(&layers, &stick, true), None);
    }

    #[test]
    fn touchpad_activator_requires_touchpad_allowed() {
        let layers = [layer(Button::Touchpad, HashMap::new())];
        let s = state(BTN_TOUCHPAD, DPAD_NEUTRAL);

        assert_eq!(top_active_layer(&layers, &s, false), None);
        assert_eq!(top_active_layer(&layers, &s, true), Some(0));
    }

    #[test]
    fn layer_serde_round_trip() {
        let layer = layer(
            Button::L1,
            HashMap::from([(
                Button::Cross,
                vec![slot(key("X")), slot(Binding::Mouse(MouseButton::Left))],
            )]),
        );

        let json = serde_json::to_string(&layer).expect("serialize");
        let back: Layer = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, layer);
    }

    #[test]
    fn long_press_key_waits_for_threshold() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![slot_press(key("X"), PressType::LongPress)],
        )]);
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());
        assert_eq!(s.buttons, 0, "explicit slots suppress identity output");

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(399),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([45]));

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(450),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());
    }

    #[test]
    fn double_tap_key_fires_on_second_press_only() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![slot_press(key("X"), PressType::DoubleTap)],
        )]);
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut pressed = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut pressed,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut released = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut released,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(50),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut second = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut second,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(200),
            true,
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([45]));

        let mut held = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut held,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(250),
            true,
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([45]));

        let mut released = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut released,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(300),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut late = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut late,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(650),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty(), "outside the double tap window");
    }

    #[test]
    fn regular_and_long_slots_are_additive() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot(Binding::Gamepad(Button::Circle)),
                slot_press(Binding::Gamepad(Button::Square), PressType::LongPress),
            ],
        )]);
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE);

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE | BTN_SQUARE);

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(450),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
    }

    #[test]
    fn long_wheel_fires_once_after_threshold() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![slot_press(
                Binding::Wheel(WheelDir::Up),
                PressType::LongPress,
            )],
        )]);
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, 0));

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, 1));

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(450),
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, 0));
    }

    #[test]
    fn long_profile_fires_once_after_threshold() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![slot_press(
                Binding::Profile(ProfileAction::Cycle),
                PressType::LongPress,
            )],
        )]);
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, None);

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, Some(ProfileAction::Cycle));

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(450),
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, None);
    }

    #[test]
    fn double_wheel_fires_once_on_second_press() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![slot_press(
                Binding::Wheel(WheelDir::Down),
                PressType::DoubleTap,
            )],
        )]);
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, 0));

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(40),
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, 0));

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(120),
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, -1));

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(160),
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, 0));
    }

    #[test]
    fn layer_long_slot_activates_after_threshold() {
        let layers = [layer(
            Button::L1,
            HashMap::from([(
                Button::Cross,
                vec![slot_press(key("X"), PressType::LongPress)],
            )]),
        )];
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &HashMap::new(),
            &layers,
            &timing,
            start,
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &HashMap::new(),
            &layers,
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([45]));
    }

    #[test]
    fn layer_double_slot_fires_on_second_press() {
        let layers = [layer(
            Button::L1,
            HashMap::from([(
                Button::Cross,
                vec![slot_press(key("X"), PressType::DoubleTap)],
            )]),
        )];
        let timing = PressTiming::default();
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &HashMap::new(),
            &layers,
            &timing,
            start,
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut s = state(BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &HashMap::new(),
            &layers,
            &timing,
            start + Duration::from_millis(50),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &HashMap::new(),
            &layers,
            &timing,
            start + Duration::from_millis(180),
            true,
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([45]));
    }

    #[test]
    fn press_timings_are_configurable() {
        assert_eq!(PressTiming::default().long_press_ms, 400);
        assert_eq!(PressTiming::default().double_tap_ms, 300);

        let bindings = HashMap::from([(
            Button::Cross,
            vec![slot_press(key("X"), PressType::LongPress)],
        )]);
        let timing = PressTiming {
            long_press_ms: 100,
            double_tap_ms: 50,
            slot_policy: HashMap::new(),
        };
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(99),
            true,
            &mut aux,
        );
        assert!(aux.keys.is_empty());

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(101),
            true,
            &mut aux,
        );
        assert_eq!(aux.keys, HashSet::from([45]));
    }

    #[test]
    fn binding_slot_deserializes_legacy_binding() {
        let slot: BindingSlot =
            serde_json::from_str(r#"{"Gamepad":"Cross"}"#).expect("legacy binding parses");
        assert_eq!(
            slot,
            BindingSlot {
                binding: Binding::Gamepad(Button::Cross),
                press: PressType::Regular,
            }
        );
    }

    #[test]
    fn binding_slot_round_trips_press_type() {
        let slot = BindingSlot {
            binding: key("E"),
            press: PressType::LongPress,
        };
        let json = serde_json::to_string(&slot).expect("serialize");
        assert_eq!(json, r#"{"binding":{"Key":"E"},"press":"LongPress"}"#);

        let back: BindingSlot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, slot);
    }

    #[test]
    fn layer_deserializes_legacy_binding_lists() {
        let json = r#"{
            "activator": "L1",
            "bindings": { "Cross": [{ "Gamepad": "Circle" }] }
        }"#;
        let layer: Layer = serde_json::from_str(json).expect("legacy layer parses");
        assert_eq!(
            layer.bindings.get(&Button::Cross),
            Some(&vec![slot(Binding::Gamepad(Button::Circle))])
        );
    }

    fn first_match_timing(button: Button) -> PressTiming {
        let mut timing = PressTiming::default();
        timing.set_policy(button, SlotPolicy::FirstMatch);
        timing
    }

    #[test]
    fn press_policy_defaults_to_combine_and_serde_is_backward_compatible() {
        let timing: PressTiming =
            serde_json::from_str(r#"{"long_press_ms":400,"double_tap_ms":300}"#)
                .expect("legacy timing parses");
        assert_eq!(timing.policy(Button::Cross), SlotPolicy::Combine);
        assert!(timing.slot_policy.is_empty());
    }

    #[test]
    fn press_policy_and_slot_order_round_trip() {
        let bindings = vec![
            slot_press(Binding::Gamepad(Button::Circle), PressType::LongPress),
            slot_press(Binding::Gamepad(Button::Square), PressType::Regular),
            slot_press(Binding::Gamepad(Button::Triangle), PressType::LongPress),
        ];
        let mut timing = first_match_timing(Button::Cross);
        timing.long_press_ms = 500;

        let json = serde_json::to_string(&(bindings.clone(), timing.clone())).expect("serialize");
        let (back_bindings, back_timing): (Vec<BindingSlot>, PressTiming) =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back_bindings, bindings);
        assert_eq!(back_timing.policy(Button::Cross), SlotPolicy::FirstMatch);
        assert_eq!(back_timing.long_press_ms, 500);
    }

    #[test]
    fn first_match_regular_first_blocks_later_hold() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot(Binding::Gamepad(Button::Circle)),
                slot_press(Binding::Gamepad(Button::Square), PressType::LongPress),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE);

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE, "the later Hold group is unreachable");
    }

    #[test]
    fn first_match_hold_first_quick_tap_pulses_regular_on_release() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::LongPress),
                slot(Binding::Gamepad(Button::Square)),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0, "nothing fires while Hold is still possible");

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(50),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_SQUARE, "quick release falls back to Press");

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(55),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_SQUARE, "pulse covers the next poll");

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(70),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0, "pulse ends without a stuck button");
    }

    #[test]
    fn first_match_hold_first_threshold_emits_only_hold() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::LongPress),
                slot(Binding::Gamepad(Button::Square)),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        for millis in [0_u64, 399] {
            let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
            apply_engine_layers_at(
                &mut engine,
                &mut s,
                &bindings,
                &[],
                &timing,
                start + Duration::from_millis(millis),
                true,
                &mut aux,
            );
            assert_eq!(s.buttons, 0, "no early Press while Hold can still fire");
        }

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE, "only the Hold group wins");

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(420),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
    }

    #[test]
    fn first_match_groups_keep_all_same_type_slots_and_first_occurrence_order() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::LongPress),
                slot(Binding::Gamepad(Button::Square)),
                slot_press(Binding::Gamepad(Button::Triangle), PressType::LongPress),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE | BTN_TRIANGLE);
        assert_eq!(s.buttons & BTN_SQUARE, 0, "lower Press group is suppressed");
    }

    #[test]
    fn first_match_double_first_single_tap_falls_back_after_window() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::DoubleTap),
                slot(Binding::Gamepad(Button::Square)),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(50),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0, "double-tap window is still open");

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(349),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(351),
            true,
            &mut aux,
        );
        assert_eq!(
            s.buttons, BTN_SQUARE,
            "single tap resolves after the window"
        );

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(370),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
    }

    #[test]
    fn first_match_double_first_second_press_wins() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::DoubleTap),
                slot(Binding::Gamepad(Button::Square)),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        for (millis, buttons) in [(0_u64, BTN_CROSS), (50, 0), (200, BTN_CROSS)] {
            let mut s = state(buttons, DPAD_NEUTRAL);
            apply_engine_layers_at(
                &mut engine,
                &mut s,
                &bindings,
                &[],
                &timing,
                start + Duration::from_millis(millis),
                true,
                &mut aux,
            );
            if millis == 200 {
                assert_eq!(s.buttons, BTN_CIRCLE);
            } else {
                assert_eq!(s.buttons, 0);
            }
        }

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(250),
            true,
            &mut aux,
        );
        assert_eq!(
            s.buttons, BTN_CIRCLE,
            "Double stays held on the second press"
        );
    }

    #[test]
    fn first_match_double_first_hold_falls_back_to_long_after_window() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::DoubleTap),
                slot_press(Binding::Gamepad(Button::Square), PressType::LongPress),
                slot(Binding::Gamepad(Button::Triangle)),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        for millis in [0_u64, 301, 399] {
            let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
            apply_engine_layers_at(
                &mut engine,
                &mut s,
                &bindings,
                &[],
                &timing,
                start + Duration::from_millis(millis),
                true,
                &mut aux,
            );
            assert_eq!(s.buttons, 0, "Double/Long defer lower and later groups");
        }

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_SQUARE, "Long is eligible at the threshold");
    }

    #[test]
    fn first_match_long_before_double_quick_double_fires_double() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::LongPress),
                slot_press(Binding::Gamepad(Button::Square), PressType::DoubleTap),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        for (millis, buttons) in [(0_u64, BTN_CROSS), (50, 0), (200, BTN_CROSS)] {
            let mut s = state(buttons, DPAD_NEUTRAL);
            apply_engine_layers_at(
                &mut engine,
                &mut s,
                &bindings,
                &[],
                &timing,
                start + Duration::from_millis(millis),
                true,
                &mut aux,
            );
            if millis == 200 {
                assert_eq!(s.buttons, BTN_SQUARE, "Double wins once Long is impossible");
            } else {
                assert_eq!(s.buttons, 0);
            }
        }
    }

    #[test]
    fn first_match_long_before_double_ignores_long_threshold_after_release() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::LongPress),
                slot_press(Binding::Gamepad(Button::Square), PressType::DoubleTap),
            ],
        )]);
        let mut timing = first_match_timing(Button::Cross);
        timing.long_press_ms = 100;
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        for (millis, buttons) in [(0_u64, BTN_CROSS), (50, 0), (200, BTN_CROSS)] {
            let mut s = state(buttons, DPAD_NEUTRAL);
            apply_engine_layers_at(
                &mut engine,
                &mut s,
                &bindings,
                &[],
                &timing,
                start + Duration::from_millis(millis),
                true,
                &mut aux,
            );
            if millis == 200 {
                assert_eq!(
                    s.buttons, BTN_SQUARE,
                    "Long cannot activate during the second press"
                );
            } else {
                assert_eq!(s.buttons, 0);
            }
        }
    }

    #[test]
    fn first_match_regular_before_double_makes_double_unreachable() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot(Binding::Gamepad(Button::Circle)),
                slot_press(Binding::Gamepad(Button::Square), PressType::DoubleTap),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE);

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(50),
            true,
            &mut aux,
        );

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(200),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE, "Regular always wins first");
    }

    #[test]
    fn first_match_wheel_and_profile_fire_once_per_gesture() {
        let wheel_bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Wheel(WheelDir::Up), PressType::DoubleTap),
                slot(key("X")),
            ],
        )]);
        let wheel_timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        for (millis, buttons) in [(0_u64, BTN_CROSS), (50, 0), (200, BTN_CROSS)] {
            let mut s = state(buttons, DPAD_NEUTRAL);
            apply_engine_layers_at(
                &mut engine,
                &mut s,
                &wheel_bindings,
                &[],
                &wheel_timing,
                start + Duration::from_millis(millis),
                true,
                &mut aux,
            );
        }
        assert_eq!(aux.wheel, (0, 1), "Double wheel fires once");
        assert!(aux.keys.is_empty());

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &wheel_bindings,
            &[],
            &wheel_timing,
            start + Duration::from_millis(250),
            true,
            &mut aux,
        );
        assert_eq!(aux.wheel, (0, 0), "wheel does not repeat while held");

        let profile_bindings = HashMap::from([(
            Button::Circle,
            vec![
                slot_press(Binding::Profile(ProfileAction::Cycle), PressType::LongPress),
                slot(key("X")),
            ],
        )]);
        let profile_timing = first_match_timing(Button::Circle);
        let mut engine = RemapEngine::new();

        let mut s = state(BTN_CIRCLE, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &profile_bindings,
            &[],
            &profile_timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, None);

        let mut s = state(BTN_CIRCLE, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &profile_bindings,
            &[],
            &profile_timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, Some(ProfileAction::Cycle));

        let mut s = state(BTN_CIRCLE, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &profile_bindings,
            &[],
            &profile_timing,
            start + Duration::from_millis(450),
            true,
            &mut aux,
        );
        assert_eq!(aux.profile, None, "profile action does not repeat");
    }

    #[test]
    fn first_match_latches_layer_bindings_at_press_start() {
        let base = HashMap::from([(Button::Cross, vec![slot(Binding::Gamepad(Button::Circle))])]);
        let layers = [layer(
            Button::L1,
            HashMap::from([(Button::Cross, vec![slot(Binding::Gamepad(Button::Square))])]),
        )];
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &base,
            &layers,
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE);

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &base,
            &layers,
            &timing,
            start + Duration::from_millis(10),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE | BTN_L1, "old latched binding wins");

        let mut s = state(BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &base,
            &layers,
            &timing,
            start + Duration::from_millis(20),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_L1);

        let mut s = state(BTN_CROSS | BTN_L1, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &base,
            &layers,
            &timing,
            start + Duration::from_millis(30),
            true,
            &mut aux,
        );
        assert_eq!(
            s.buttons,
            BTN_SQUARE | BTN_L1,
            "the next press latches the active layer"
        );
    }

    #[test]
    fn first_match_reset_pending_does_not_leave_outputs_stuck() {
        let bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot_press(Binding::Gamepad(Button::Circle), PressType::LongPress),
                slot(Binding::Gamepad(Button::Square)),
            ],
        )]);
        let timing = first_match_timing(Button::Cross);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, BTN_CIRCLE);
        assert!(!engine.first_match.is_empty());

        let mut s = state(BTN_CROSS, DPAD_NEUTRAL);
        engine.reset_pending();
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(410),
            true,
            &mut aux,
        );
        assert_eq!(
            s.buttons, BTN_CIRCLE,
            "the new gesture re-latches the held button"
        );

        let mut s = state(0, DPAD_NEUTRAL);
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(420),
            true,
            &mut aux,
        );
        assert_eq!(s.buttons, 0);
        assert!(engine.first_match.is_empty());
    }

    #[test]
    fn first_match_l2_analog_only_passes_for_winning_identity_group() {
        let bindings = HashMap::from([(
            Button::L2,
            vec![
                slot_press(Binding::Gamepad(Button::L2), PressType::LongPress),
                slot(Binding::Gamepad(Button::Cross)),
            ],
        )]);
        let timing = first_match_timing(Button::L2);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        let mut s = state(BTN_L2, DPAD_NEUTRAL);
        s.l2 = 200;
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start,
            true,
            &mut aux,
        );
        assert_eq!(s.l2, 0, "analog is suppressed while Hold is deferred");
        assert_eq!(s.buttons, 0);

        let mut s = state(BTN_L2, DPAD_NEUTRAL);
        s.l2 = 200;
        apply_engine_layers_at(
            &mut engine,
            &mut s,
            &bindings,
            &[],
            &timing,
            start + Duration::from_millis(401),
            true,
            &mut aux,
        );
        assert_eq!(s.l2, 200, "winning Long group carries the identity trigger");
        assert_eq!(s.buttons, BTN_L2);
    }

    #[test]
    fn first_match_l2_analog_ignores_identity_in_losing_group() {
        let bindings = HashMap::from([(
            Button::L2,
            vec![
                slot(Binding::Gamepad(Button::Cross)),
                slot_press(Binding::Gamepad(Button::L2), PressType::LongPress),
            ],
        )]);
        let timing = first_match_timing(Button::L2);
        let start = Instant::now();
        let mut engine = RemapEngine::new();
        let mut aux = AuxOutput::default();

        for millis in [0_u64, 401] {
            let mut s = state(BTN_L2, DPAD_NEUTRAL);
            s.l2 = 200;
            apply_engine_layers_at(
                &mut engine,
                &mut s,
                &bindings,
                &[],
                &timing,
                start + Duration::from_millis(millis),
                true,
                &mut aux,
            );
            assert_eq!(s.buttons, BTN_CROSS);
            assert_eq!(s.l2, 0, "the losing identity slot must not enable analog");
        }
    }
}
