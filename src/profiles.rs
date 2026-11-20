use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    common::*,
    gyro::{GyroActivation, GyroConfig},
    inputs::{Button, ControllerKind},
    remap::{Binding, BindingSlot, Layer, PressTiming, PressType, StickConfig},
    stick_mouse::StickMouseConfig,
    trackpad_mouse::{SteamPadHapticStrength, SteamPadMouseConfig},
    transform::{GyroProcessor, InputTransform, TriggerDeadband},
};

pub const CURRENT_PROFILE_SCHEMA: u32 = 4;

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct TriggerConfig {
    pub mode: TriggerMode,
    pub start: u8,
    pub end: u8,
    pub strength: u8,
    pub frequency: u8,
    pub deadband: TriggerDeadband,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            mode: TriggerMode::Off,
            start: 2,
            end: 7,
            strength: 5,
            frequency: 30,
            deadband: TriggerDeadband::default(),
        }
    }
}

impl TriggerConfig {
    pub fn to_effect(&self) -> (u8, [u8; 10]) {
        match self.mode {
            TriggerMode::Off => (DS_TRIGGER_EFFECT_OFF, [0; 10]),
            TriggerMode::Feedback => {
                let start = self.start.min(9) as usize;
                let end = self.end.clamp(self.start, 9) as usize;
                let strength = self.strength.clamp(1, 8);
                let mut active_zones: u16 = 0;
                let mut strength_zones: u32 = 0;
                for i in start..=end {
                    let sv = ((strength - 1) & 0x07) as u32;
                    strength_zones |= sv << (3 * i);
                    active_zones |= 1 << i;
                }
                let mut p = [0u8; 10];
                p[0] = (active_zones & 0xff) as u8;
                p[1] = ((active_zones >> 8) & 0xff) as u8;
                p[2] = (strength_zones & 0xff) as u8;
                p[3] = ((strength_zones >> 8) & 0xff) as u8;
                p[4] = ((strength_zones >> 16) & 0xff) as u8;
                p[5] = ((strength_zones >> 24) & 0xff) as u8;
                (DS_TRIGGER_EFFECT_FEEDBACK, p)
            }
            TriggerMode::Weapon => {
                let start = self.start.clamp(2, 7);
                let end = self.end.clamp(start + 1, 8);
                let strength = self.strength.clamp(1, 8);
                let mut p = [0u8; 10];
                let positions: u16 = (1u16 << start) | (1u16 << end);
                p[0] = (positions & 0xff) as u8;
                p[1] = ((positions >> 8) & 0xff) as u8;
                p[2] = strength - 1;
                (DS_TRIGGER_EFFECT_WEAPON, p)
            }
            TriggerMode::Bow => {
                let start = self.start.min(8);
                let end = self.end.clamp(start + 1, 8);
                let strength = self.strength.clamp(1, 8);
                let snap = self.strength.clamp(1, 8);
                let mut p = [0u8; 10];
                let positions: u16 = (1u16 << start) | (1u16 << end);
                let force: u32 = ((strength - 1) as u32 & 0x07) | (((snap - 1) as u32 & 0x07) << 3);
                p[0] = (positions & 0xff) as u8;
                p[1] = ((positions >> 8) & 0xff) as u8;
                p[2] = (force & 0xff) as u8;
                p[3] = ((force >> 8) & 0xff) as u8;
                (DS_TRIGGER_EFFECT_BOW, p)
            }
            TriggerMode::Galloping => {
                let start = self.start.min(8);
                let end = self.end.clamp(start + 1, 9);
                let first_foot = self.strength.saturating_sub(1).min(7);
                let second_foot = first_foot.saturating_add(1).min(7);
                let freq = self.frequency.max(1);
                let mut p = [0u8; 10];
                p[0] = start;
                p[1] = end;
                p[2] = (second_foot << 4) | first_foot;
                p[3] = freq;
                (DS_TRIGGER_EFFECT_GALLOPING, p)
            }
            TriggerMode::Vibration => {
                let start = self.start.min(9) as usize;
                let end = self.end.clamp(self.start, 9) as usize;
                let strength = self.strength.clamp(1, 8);
                let freq = self.frequency.max(1);
                let mut active_zones: u16 = 0;
                let mut amp_zones: u32 = 0;
                for i in start..=end {
                    active_zones |= 1 << i;
                    amp_zones |= ((strength - 1) as u32 & 0x07) << (3 * i);
                }
                let mut p = [0u8; 10];
                p[0] = (active_zones & 0xff) as u8;
                p[1] = ((active_zones >> 8) & 0xff) as u8;
                p[2] = (amp_zones & 0xff) as u8;
                p[3] = ((amp_zones >> 8) & 0xff) as u8;
                p[4] = ((amp_zones >> 16) & 0xff) as u8;
                p[5] = ((amp_zones >> 24) & 0xff) as u8;
                p[6] = freq;
                (DS_TRIGGER_EFFECT_VIBRATION, p)
            }
            TriggerMode::Machine => {
                let start = self.start.min(8);
                let end = self.end.clamp(start + 1, 9);
                let amp_a = self.strength.clamp(1, 8);
                let amp_b = self.strength.clamp(1, 8);
                let freq = self.frequency.max(1);
                let mut p = [0u8; 10];
                p[0] = start;
                p[1] = end;
                p[2] = ((amp_b - 1) << 4) | (amp_a - 1);
                p[3] = freq;
                p[4] = 0;
                (DS_TRIGGER_EFFECT_MACHINE, p)
            }
        }
    }
}

fn default_touchpad_sensitivity() -> f32 {
    1.0
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub device: ControllerKind,
    pub lightbar_r: f32,
    pub lightbar_g: f32,
    pub lightbar_b: f32,
    pub lightbar_brightness: f32,
    pub player_leds: u8,
    pub mic_enabled: bool,
    pub stick_left_curve: SensitivityCurve,
    pub stick_right_curve: SensitivityCurve,
    pub trigger_mode: TriggerMode,
    pub trigger_feedback_position: u8,
    pub haptic_intensity: u8,
    pub gyro_sensetivity: f32,
    pub touchpad_enabled: bool,
    #[serde(default)]
    pub touchpad_mode: TouchpadMode,
    #[serde(default)]
    pub touchpad_tap_to_click: bool,
    #[serde(default)]
    pub touchpad_natural_scrolling: bool,
    #[serde(default = "default_touchpad_sensitivity")]
    pub touchpad_sensitivity: f32,
    pub button_remapping: HashMap<Button, Button>,
    pub disabled_buttons: HashSet<Button>,
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub button_bindings: HashMap<Button, Vec<BindingSlot>>,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub press_timing: PressTiming,
    #[serde(default)]
    pub stick_digital: StickConfig,
    pub stick_left_deadzone: f32,
    pub stick_right_deadzone: f32,
    pub trigger_left_deadband: TriggerDeadband,
    pub trigger_right_deadband: TriggerDeadband,
    #[serde(default)]
    pub stick_left_outer_deadzone: f32,
    #[serde(default)]
    pub stick_right_outer_deadzone: f32,
    #[serde(default)]
    pub stick_left_invert_x: bool,
    #[serde(default)]
    pub stick_left_invert_y: bool,
    #[serde(default)]
    pub stick_right_invert_x: bool,
    #[serde(default)]
    pub stick_right_invert_y: bool,
    #[serde(default)]
    pub stick_swap: bool,

    #[serde(default)]
    pub trigger_left_config: TriggerConfig,
    #[serde(default)]
    pub trigger_right_config: TriggerConfig,

    #[serde(default)]
    pub gyro: GyroProcessor,

    #[serde(default)]
    pub gyro_mouse: GyroConfig,

    #[serde(default)]
    pub stick_mouse: StickMouseConfig,

    #[serde(default)]
    pub steam_left_pad_mouse: SteamPadMouseConfig,
    #[serde(default)]
    pub steam_right_pad_mouse: SteamPadMouseConfig,

    #[serde(default)]
    pub haptic_pattern: HapticPattern,
    #[serde(default)]
    pub haptic_strength: u8, // 0-7
    #[serde(default)]
    pub haptic_speed: f32, // Hz

    #[serde(default)]
    pub touchpad_show_overlay: bool,
}

impl Profile {
    pub fn matches_device(&self, kind: ControllerKind) -> bool {
        self.device.is_steam() == kind.is_steam()
    }

    /// Convert a profile's device family while preserving every mapping and
    /// configuration field.
    pub fn convert_to_device(&mut self, kind: ControllerKind) {
        self.device = kind;
    }

    pub fn default_for_device(name: String, kind: ControllerKind) -> Self {
        let mut profile = Self {
            name,
            device: kind,
            ..Default::default()
        };
        if kind.is_steam() {
            profile.gyro_mouse.activation = GyroActivation::RightTrackpad;
            profile.steam_left_pad_mouse.haptic_strength = SteamPadHapticStrength::Medium;
            profile.steam_right_pad_mouse.haptic_strength = SteamPadHapticStrength::Medium;
        }
        profile
    }

    pub fn migrate_legacy(&mut self) {
        if self.schema_version >= CURRENT_PROFILE_SCHEMA {
            return;
        }
        if self.schema_version < 2 && self.button_bindings.is_empty() {
            for (from, to) in &self.button_remapping {
                self.button_bindings
                    .insert(*from, vec![BindingSlot::from(Binding::Gamepad(*to))]);
            }
            for btn in &self.disabled_buttons {
                self.button_bindings.insert(*btn, Vec::new());
            }
        }
        if self.schema_version < 3
            && matches!(
                self.touchpad_mode,
                TouchpadMode::Mouse | TouchpadMode::PassThrough
            )
        {
            self.touchpad_mode = TouchpadMode::Disabled;
        }
        self.schema_version = CURRENT_PROFILE_SCHEMA;
    }

    pub fn rebuild_legacy_bindings(&mut self) {
        self.button_remapping.clear();
        self.disabled_buttons.clear();
        for (from, list) in &self.button_bindings {
            if from.is_stick_dir() {
                continue;
            }
            match list.as_slice() {
                [
                    BindingSlot {
                        binding: Binding::Gamepad(to),
                        press: PressType::Regular,
                    },
                ] => {
                    self.button_remapping.insert(*from, *to);
                }
                [] => {
                    self.disabled_buttons.insert(*from);
                }
                _ => {}
            }
        }
    }

    pub fn to_input_transform(&self) -> InputTransform {
        let mut transform = InputTransform {
            left_curve: self.stick_left_curve.clone(),
            right_curve: self.stick_right_curve.clone(),
            left_deadzone: self.stick_left_deadzone,
            right_deadzone: self.stick_right_deadzone,
            left_outer_deadzone: self.stick_left_outer_deadzone,
            right_outer_deadzone: self.stick_right_outer_deadzone,
            left_invert_x: self.stick_left_invert_x,
            left_invert_y: self.stick_left_invert_y,
            right_invert_x: self.stick_right_invert_x,
            right_invert_y: self.stick_right_invert_y,
            stick_swap: self.stick_swap,
            trigger_left: self.trigger_left_config.deadband.clone(),
            trigger_right: self.trigger_right_config.deadband.clone(),
            button_remap: self.button_remapping.clone(),
            disabled_buttons: self.disabled_buttons.clone(),
            button_bindings: self.button_bindings.clone(),
            layers: self.layers.clone(),
            press_timing: self.press_timing.clone(),
            stick_digital: self.stick_digital.clone(),
            touchpad_enabled: self.touchpad_enabled,
            touchpad_mode: self.touchpad_mode,
            gyro_mouse: self.gyro_mouse.clone(),
            stick_mouse: self.stick_mouse.clone(),
            steam_left_pad_mouse: self.steam_left_pad_mouse.clone(),
            steam_right_pad_mouse: self.steam_right_pad_mouse.clone(),
        };
        transform.normalize_bindings();
        transform
    }

    pub fn to_gyro_processor(&self) -> GyroProcessor {
        GyroProcessor {
            enabled: self.gyro.enabled,
            smoothing: self.gyro.smoothing,
            sensitivity: self.gyro.sensitivity * self.gyro_sensetivity,
            ..Default::default()
        }
    }

    pub fn to_trigger_effect(&self) -> Option<(u8, [u8; 10])> {
        match self.trigger_mode {
            TriggerMode::Off => None,
            _ => Some(self.trigger_left_config.to_effect()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileNameConflict {
    None,
    SameFamily,
    OtherFamily,
}

/// Profile filenames are global; classify a name collision by the family the
/// existing profile belongs to.
pub fn profile_name_conflict(
    existing: Option<&Profile>,
    kind: ControllerKind,
) -> ProfileNameConflict {
    match existing {
        None => ProfileNameConflict::None,
        Some(profile) if profile.matches_device(kind) => ProfileNameConflict::SameFamily,
        Some(_) => ProfileNameConflict::OtherFamily,
    }
}

pub struct ProfileManager {
    profiles_dir: PathBuf,
}

impl ProfileManager {
    pub fn new() -> Self {
        let profiles_dir = Self::get_profiles_dir();
        if !profiles_dir.exists() {
            let _ = fs::create_dir_all(&profiles_dir);
        }
        Self { profiles_dir }
    }

    fn get_profiles_dir() -> PathBuf {
        crate::common::config_dir().join("profiles")
    }

    pub fn default_profile_name(kind: ControllerKind) -> &'static str {
        if kind.is_steam() {
            "SteamDefault"
        } else {
            "Default"
        }
    }

    pub fn ensure_default_for(&self, kind: ControllerKind) -> Profile {
        let name = Self::default_profile_name(kind);
        if let Ok(profile) = self.load_profile(name)
            && profile.device.is_steam() == kind.is_steam()
        {
            return profile;
        }
        let mut profile = Profile::default_for_device(name.to_string(), kind);
        profile.schema_version = CURRENT_PROFILE_SCHEMA;
        let _ = self.save_profile(&profile);
        profile
    }

    fn sanitize_filename(name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub fn validate_profile_name(name: &str) -> Result<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            bail!("Profile name cannot be empty");
        }

        if trimmed.len() > 64 {
            bail!("Profile name must be less than 64 characters");
        }

        if Self::sanitize_filename(trimmed) != trimmed {
            bail!(
                "Profile name '{}' contains invalid characters. Allowed: letters, digits, '-', '_' (no spaces, slashes, or punctuation)",
                name
            );
        }
        Ok(())
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<()> {
        Self::validate_profile_name(&profile.name)?;
        let filename = format!("{}.json", Self::sanitize_filename(&profile.name));
        let path = self.profiles_dir.join(filename);
        let mut profile = profile.clone();
        profile.schema_version = CURRENT_PROFILE_SCHEMA;
        profile.rebuild_legacy_bindings();
        let json = serde_json::to_string_pretty(&profile)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_profile(&self, name: &str) -> Result<Profile> {
        let filename = format!("{}.json", Self::sanitize_filename(name));
        let path = self.profiles_dir.join(filename);
        if !path.exists() {
            bail!("Profile '{}' not found", name);
        }
        let json = fs::read_to_string(path)?;
        let mut profile: Profile = serde_json::from_str(&json)?;
        profile.migrate_legacy();
        Ok(profile)
    }

    pub fn delete_profile(&self, name: &str) -> Result<()> {
        if name == "Default" || name == "SteamDefault" {
            bail!("Cannot delete the default profile for this device");
        }
        let filename = format!("{}.json", Self::sanitize_filename(name));
        let path = self.profiles_dir.join(filename);
        if !path.exists() {
            bail!("Profile '{}' not found", name);
        }
        fs::remove_file(path)?;
        Ok(())
    }

    pub fn profile_exists(&self, name: &str) -> bool {
        let filename = format!("{}.json", Self::sanitize_filename(name));
        self.profiles_dir.join(filename).exists()
    }

    pub fn list_profiles(&self) -> Vec<Profile> {
        self.load_all_profiles()
    }

    pub fn list_profiles_for(&self, kind: ControllerKind) -> Vec<Profile> {
        self.load_all_profiles()
            .into_iter()
            .filter(|profile| profile.device.is_steam() == kind.is_steam())
            .collect()
    }

    fn load_all_profiles(&self) -> Vec<Profile> {
        let mut profiles = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.profiles_dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json")
                    && let Ok(json) = fs::read_to_string(&path)
                    && let Ok(mut profile) = serde_json::from_str::<Profile>(&json)
                {
                    profile.migrate_legacy();
                    profiles.push(profile);
                }
            }
        }
        profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        profiles
    }
}

impl Clone for ProfileManager {
    fn clone(&self) -> Self {
        ProfileManager::new()
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            device: ControllerKind::DualSense,
            lightbar_r: 0.0,
            lightbar_g: 0.5,
            lightbar_b: 1.0,
            lightbar_brightness: 255.0,
            player_leds: 1,
            mic_enabled: false,
            stick_left_curve: SensitivityCurve::Default,
            stick_right_curve: SensitivityCurve::Default,
            trigger_mode: TriggerMode::Off,
            trigger_feedback_position: 0,
            haptic_intensity: 0,
            gyro_sensetivity: 1.0,
            touchpad_enabled: true,
            touchpad_mode: TouchpadMode::Disabled,
            touchpad_tap_to_click: true,
            touchpad_natural_scrolling: false,
            touchpad_sensitivity: 1.0,
            button_remapping: HashMap::new(),
            disabled_buttons: HashSet::new(),
            schema_version: CURRENT_PROFILE_SCHEMA,
            button_bindings: HashMap::new(),
            layers: Vec::new(),
            press_timing: PressTiming::default(),
            stick_digital: StickConfig::default(),
            stick_left_deadzone: 0.0,
            stick_right_deadzone: 0.0,
            trigger_left_deadband: TriggerDeadband::default(),
            trigger_right_deadband: TriggerDeadband::default(),

            stick_left_outer_deadzone: 1.0,
            stick_right_outer_deadzone: 1.0,
            stick_left_invert_x: false,
            stick_left_invert_y: false,
            stick_right_invert_x: false,
            stick_right_invert_y: false,
            stick_swap: false,

            trigger_left_config: TriggerConfig::default(),
            trigger_right_config: TriggerConfig::default(),

            gyro: GyroProcessor::default(),

            gyro_mouse: GyroConfig::default(),

            stick_mouse: StickMouseConfig::default(),
            steam_left_pad_mouse: SteamPadMouseConfig::default(),
            steam_right_pad_mouse: SteamPadMouseConfig::default(),

            haptic_pattern: HapticPattern::None,
            haptic_strength: 0,
            haptic_speed: 1.0,

            touchpad_show_overlay: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remap::SlotPolicy;

    const LEGACY: &str = include_str!("../tests/fixtures/profile_v1_legacy.json");

    fn slot(binding: Binding) -> BindingSlot {
        BindingSlot::from(binding)
    }

    fn legacy() -> Profile {
        serde_json::from_str(LEGACY).expect("legacy fixture parses")
    }

    #[test]
    fn legacy_profile_migrates_to_bindings() {
        let mut p = legacy();
        assert_eq!(p.schema_version, 0);
        assert!(p.button_bindings.is_empty());
        assert_eq!(p.stick_digital, StickConfig::default());

        p.migrate_legacy();

        assert_eq!(p.schema_version, CURRENT_PROFILE_SCHEMA);
        assert_eq!(
            p.button_bindings.get(&Button::Cross),
            Some(&vec![slot(Binding::Gamepad(Button::Circle))])
        );
        assert_eq!(
            p.button_bindings.get(&Button::L1),
            Some(&vec![slot(Binding::Gamepad(Button::R1))])
        );
        assert!(
            p.button_bindings
                .get(&Button::L3)
                .is_some_and(Vec::is_empty)
        );
        assert_eq!(p.touchpad_mode, TouchpadMode::Disabled);
    }

    #[test]
    fn schema_2_legacy_touchpad_modes_migrate_to_disabled() {
        for mode in [TouchpadMode::Mouse, TouchpadMode::PassThrough] {
            let mut p = legacy();
            p.schema_version = 2;
            p.touchpad_mode = mode;
            p.button_bindings =
                HashMap::from([(Button::Cross, vec![slot(Binding::Gamepad(Button::Circle))])]);

            p.migrate_legacy();

            assert_eq!(p.schema_version, CURRENT_PROFILE_SCHEMA);
            assert_eq!(p.touchpad_mode, TouchpadMode::Disabled);
            assert_eq!(
                p.button_bindings.get(&Button::Cross),
                Some(&vec![slot(Binding::Gamepad(Button::Circle))])
            );
            assert_eq!(p.button_bindings.len(), 1);
        }
    }

    #[test]
    fn schema_2_supported_touchpad_modes_are_preserved() {
        for mode in [TouchpadMode::GesturesOnly, TouchpadMode::Disabled] {
            let mut p = legacy();
            p.schema_version = 2;
            p.touchpad_mode = mode;

            p.migrate_legacy();

            assert_eq!(p.schema_version, CURRENT_PROFILE_SCHEMA);
            assert_eq!(p.touchpad_mode, mode);
        }
    }

    #[test]
    fn migrated_profile_round_trips() {
        let mut p = legacy();
        p.migrate_legacy();

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.schema_version, CURRENT_PROFILE_SCHEMA);
        assert_eq!(back.button_bindings, p.button_bindings);
    }

    #[test]
    fn rebuild_legacy_bindings_restores_simple_bindings() {
        let mut p = legacy();
        p.migrate_legacy();

        p.rebuild_legacy_bindings();

        assert_eq!(p.button_remapping.len(), 2);
        assert_eq!(
            p.button_remapping.get(&Button::Cross),
            Some(&Button::Circle)
        );
        assert_eq!(p.button_remapping.get(&Button::L1), Some(&Button::R1));
        assert_eq!(p.disabled_buttons.len(), 1);
        assert!(p.disabled_buttons.contains(&Button::L3));
    }

    #[test]
    fn rebuild_skips_multi_bindings() {
        let mut p = legacy();
        p.button_bindings = HashMap::from([(
            Button::Cross,
            vec![
                slot(Binding::Gamepad(Button::Circle)),
                slot(Binding::Gamepad(Button::Square)),
            ],
        )]);

        p.rebuild_legacy_bindings();

        assert!(p.button_remapping.is_empty());
        assert!(p.disabled_buttons.is_empty());
    }

    #[test]
    fn rebuild_skips_profile_bindings() {
        use crate::remap::ProfileAction;

        let mut p = legacy();
        let binding = Binding::Profile(ProfileAction::Switch("Main".to_string()));
        p.button_bindings = HashMap::from([(Button::Cross, vec![slot(binding.clone())])]);

        p.rebuild_legacy_bindings();

        assert!(p.button_remapping.is_empty());
        assert!(p.disabled_buttons.is_empty());
        assert_eq!(
            p.button_bindings.get(&Button::Cross),
            Some(&vec![slot(binding)])
        );
    }

    #[test]
    fn rebuild_skips_stick_direction_bindings() {
        let mut p = legacy();
        p.button_bindings = HashMap::from([
            (Button::Cross, vec![slot(Binding::Gamepad(Button::Circle))]),
            (
                Button::LeftStickUp,
                vec![slot(Binding::Gamepad(Button::DPadUp))],
            ),
        ]);

        p.rebuild_legacy_bindings();

        assert_eq!(p.button_remapping.len(), 1);
        assert_eq!(
            p.button_remapping.get(&Button::Cross),
            Some(&Button::Circle)
        );
        assert!(!p.button_remapping.contains_key(&Button::LeftStickUp));
    }

    #[test]
    fn rebuild_skips_non_regular_slots() {
        let mut p = legacy();
        p.button_bindings = HashMap::from([
            (
                Button::Cross,
                vec![BindingSlot {
                    binding: Binding::Gamepad(Button::Circle),
                    press: PressType::LongPress,
                }],
            ),
            (
                Button::Circle,
                vec![BindingSlot {
                    binding: Binding::Gamepad(Button::Square),
                    press: PressType::DoubleTap,
                }],
            ),
        ]);

        p.rebuild_legacy_bindings();

        assert!(p.button_remapping.is_empty());
        assert!(p.disabled_buttons.is_empty());
    }

    #[test]
    fn press_timing_copies_into_transform_and_round_trips() {
        let mut p = Profile::default();
        p.press_timing = PressTiming {
            long_press_ms: 500,
            double_tap_ms: 250,
            slot_policy: HashMap::new(),
        };

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.press_timing, p.press_timing);
        assert_eq!(p.to_input_transform().press_timing, p.press_timing);
    }

    #[test]
    fn profile_round_trips_slot_policy_and_order() {
        let mut p = Profile::default();
        p.press_timing
            .set_policy(Button::Cross, SlotPolicy::FirstMatch);
        p.button_bindings.insert(
            Button::Cross,
            vec![
                BindingSlot {
                    binding: Binding::Gamepad(Button::Circle),
                    press: PressType::LongPress,
                },
                BindingSlot::from(Binding::Gamepad(Button::Square)),
                BindingSlot {
                    binding: Binding::Gamepad(Button::Triangle),
                    press: PressType::LongPress,
                },
            ],
        );
        p.layers.push(Layer {
            activator: Button::L1,
            bindings: HashMap::from([(
                Button::Cross,
                vec![BindingSlot::from(Binding::Gamepad(Button::DPadUp))],
            )]),
        });

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            back.press_timing.policy(Button::Cross),
            SlotPolicy::FirstMatch
        );
        assert_eq!(
            p.to_input_transform().press_timing.policy(Button::Cross),
            SlotPolicy::FirstMatch
        );
        assert_eq!(
            back.button_bindings.get(&Button::Cross),
            p.button_bindings.get(&Button::Cross),
            "slot order survives the round trip"
        );
        assert_eq!(back.layers, p.layers);
        assert_eq!(back.schema_version, CURRENT_PROFILE_SCHEMA);
    }

    #[test]
    fn old_profiles_without_slot_policy_default_to_combine() {
        let mut p = Profile::default();
        p.schema_version = 3;
        let mut json: serde_json::Value = serde_json::to_value(&p).expect("serialize");
        json.as_object_mut()
            .expect("profile object")
            .get_mut("press_timing")
            .and_then(|timing| timing.as_object_mut())
            .expect("press timing object")
            .remove("slot_policy");

        let back: Profile = serde_json::from_value(json).expect("legacy profile parses");
        assert!(back.press_timing.slot_policy.is_empty());
        assert_eq!(back.press_timing.policy(Button::Cross), SlotPolicy::Combine);
    }

    #[test]
    fn gyro_mouse_copies_into_transform() {
        let mut p = legacy();
        p.gyro_mouse.enabled = true;
        p.gyro_mouse.dots_per_360 = 1234.0;

        let transform = p.to_input_transform();

        assert_eq!(transform.gyro_mouse, p.gyro_mouse);
    }

    #[test]
    fn stick_mouse_copies_into_transform() {
        let mut p = legacy();
        assert_eq!(p.stick_mouse, StickMouseConfig::default());
        p.stick_mouse.enabled = true;
        p.stick_mouse.speed = 900.0;

        let transform = p.to_input_transform();

        assert_eq!(transform.stick_mouse, p.stick_mouse);
    }

    #[test]
    fn stick_digital_copies_into_transform() {
        let mut p = legacy();
        p.stick_digital = StickConfig {
            left_deadzone: 0.55,
            right_deadzone: 0.25,
            left_analog: true,
            right_analog: false,
        };

        let transform = p.to_input_transform();

        assert_eq!(transform.stick_digital, p.stick_digital);
    }

    #[test]
    fn profile_layers_copy_into_transform_and_round_trip() {
        let mut p = Profile::default();
        assert!(p.layers.is_empty());

        let layer = Layer {
            activator: Button::L1,
            bindings: HashMap::from([(
                Button::Cross,
                vec![slot(Binding::Gamepad(Button::Triangle))],
            )]),
        };
        p.layers = vec![layer.clone()];

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.layers, vec![layer.clone()]);

        assert_eq!(p.to_input_transform().layers, vec![layer.clone()]);

        p.rebuild_legacy_bindings();
        assert_eq!(p.layers, vec![layer]);
    }

    #[test]
    fn schema_3_profiles_migrate_with_default_steam_fields() {
        let mut p = legacy();
        p.schema_version = 3;
        p.device = ControllerKind::DualSense;
        p.migrate_legacy();
        assert_eq!(p.schema_version, CURRENT_PROFILE_SCHEMA);
        assert_eq!(p.device, ControllerKind::DualSense);
        assert_eq!(p.steam_left_pad_mouse, SteamPadMouseConfig::default());
        assert_eq!(p.steam_right_pad_mouse, SteamPadMouseConfig::default());
    }

    #[test]
    fn old_profiles_default_to_dualsense() {
        let parsed: Profile = serde_json::from_str(LEGACY).expect("legacy fixture parses");
        assert_eq!(parsed.device, ControllerKind::DualSense);
        assert_eq!(parsed.steam_left_pad_mouse, SteamPadMouseConfig::default());
        assert_eq!(parsed.steam_right_pad_mouse, SteamPadMouseConfig::default());
    }

    #[test]
    fn steam_profile_round_trips_device_and_trackpad_settings() {
        let mut p = Profile {
            name: "Steam Main".to_string(),
            device: ControllerKind::SteamController,
            ..Default::default()
        };
        p.steam_left_pad_mouse.mode = crate::trackpad_mouse::SteamPadMouseMode::Mouse;
        p.steam_left_pad_mouse.sensitivity = 900.0;
        p.steam_left_pad_mouse.scroll_sensitivity = 250.0;
        p.steam_left_pad_mouse.scroll_natural = false;
        p.steam_left_pad_mouse.haptic_strength = SteamPadHapticStrength::High;
        p.steam_right_pad_mouse.invert_x = true;
        p.steam_right_pad_mouse.mode = crate::trackpad_mouse::SteamPadMouseMode::Scroll;
        p.button_bindings
            .insert(Button::L4, vec![slot(Binding::Gamepad(Button::Cross))]);

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.device, ControllerKind::SteamController);
        assert_eq!(back.steam_left_pad_mouse, p.steam_left_pad_mouse);
        assert_eq!(back.steam_right_pad_mouse, p.steam_right_pad_mouse);
        assert_eq!(
            back.button_bindings.get(&Button::L4),
            p.button_bindings.get(&Button::L4)
        );
        assert_eq!(
            p.to_input_transform().steam_left_pad_mouse,
            p.steam_left_pad_mouse
        );
    }

    #[test]
    fn new_steam_profiles_default_to_right_trackpad() {
        let steam = Profile::default_for_device(
            "SteamDefault".to_string(),
            ControllerKind::SteamController,
        );
        assert_eq!(steam.gyro_mouse.activation, GyroActivation::RightTrackpad);

        let dualsense =
            Profile::default_for_device("Default".to_string(), ControllerKind::DualSense);
        assert_eq!(dualsense.gyro_mouse.activation, GyroActivation::Touchpad);
    }

    #[test]
    fn new_steam_profiles_default_to_medium_pad_haptics() {
        let steam = Profile::default_for_device(
            "SteamDefault".to_string(),
            ControllerKind::SteamController,
        );
        assert_eq!(
            steam.steam_left_pad_mouse.haptic_strength,
            SteamPadHapticStrength::Medium
        );
        assert_eq!(
            steam.steam_right_pad_mouse.haptic_strength,
            SteamPadHapticStrength::Medium
        );

        let dualsense =
            Profile::default_for_device("Default".to_string(), ControllerKind::DualSense);
        assert_eq!(
            dualsense.steam_left_pad_mouse.haptic_strength,
            SteamPadHapticStrength::Off
        );
    }

    #[test]
    fn existing_profiles_keep_pad_haptics_off() {
        let parsed: Profile = serde_json::from_str(LEGACY).expect("legacy fixture parses");
        assert_eq!(
            parsed.steam_left_pad_mouse.haptic_strength,
            SteamPadHapticStrength::Off
        );
        assert_eq!(
            parsed.steam_right_pad_mouse.haptic_strength,
            SteamPadHapticStrength::Off
        );
    }

    #[test]
    fn existing_steam_touchpad_activation_is_preserved() {
        let mut p =
            Profile::default_for_device("Steam Main".to_string(), ControllerKind::SteamController);
        p.gyro_mouse.activation = GyroActivation::Touchpad;

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.gyro_mouse.activation, GyroActivation::Touchpad);
        assert_eq!(back.device, ControllerKind::SteamController);
    }

    #[test]
    fn default_profile_names_are_device_specific() {
        assert_eq!(
            ProfileManager::default_profile_name(ControllerKind::DualSense),
            "Default"
        );
        assert_eq!(
            ProfileManager::default_profile_name(ControllerKind::DualSenseEdge),
            "Default"
        );
        assert_eq!(
            ProfileManager::default_profile_name(ControllerKind::SteamController),
            "SteamDefault"
        );
        for kind in [
            ControllerKind::DualSense,
            ControllerKind::DualSenseEdge,
            ControllerKind::SteamController,
        ] {
            ProfileManager::validate_profile_name(ProfileManager::default_profile_name(kind))
                .expect("default profile names must be valid filenames");
        }
    }

    #[test]
    fn new_bindings_win_over_legacy_fields() {
        let mut p = legacy();
        p.button_bindings =
            HashMap::from([(Button::Cross, vec![slot(Binding::Gamepad(Button::Square))])]);

        p.migrate_legacy();

        assert_eq!(
            p.button_bindings.get(&Button::Cross),
            Some(&vec![slot(Binding::Gamepad(Button::Square))])
        );
        assert!(!p.button_bindings.contains_key(&Button::L1));
        assert_eq!(p.schema_version, CURRENT_PROFILE_SCHEMA);
    }

    #[test]
    fn legacy_profile_without_device_is_dualsense_only() {
        let parsed: Profile = serde_json::from_str(LEGACY).expect("legacy fixture parses");
        assert_eq!(parsed.device, ControllerKind::DualSense);
        assert!(parsed.matches_device(ControllerKind::DualSense));
        assert!(parsed.matches_device(ControllerKind::DualSenseEdge));
        assert!(!parsed.matches_device(ControllerKind::SteamController));
    }

    #[test]
    fn profile_name_conflicts_are_family_aware() {
        let dualsense = Profile::default_for_device("Build".into(), ControllerKind::DualSense);
        let steam = Profile::default_for_device("Build".into(), ControllerKind::SteamController);

        assert_eq!(
            profile_name_conflict(None, ControllerKind::DualSense),
            ProfileNameConflict::None
        );
        assert_eq!(
            profile_name_conflict(Some(&dualsense), ControllerKind::DualSenseEdge),
            ProfileNameConflict::SameFamily
        );
        assert_eq!(
            profile_name_conflict(Some(&dualsense), ControllerKind::SteamController),
            ProfileNameConflict::OtherFamily
        );
        assert_eq!(
            profile_name_conflict(Some(&steam), ControllerKind::DualSense),
            ProfileNameConflict::OtherFamily
        );
    }

    #[test]
    fn conversion_preserves_mappings_and_configuration() {
        let mut p = Profile::default_for_device("TheFinals".into(), ControllerKind::DualSense);
        p.button_bindings.insert(
            Button::Cross,
            vec![slot(Binding::Key(crate::remap::Key::new("C").unwrap()))],
        );
        p.layers.push(Layer {
            activator: Button::L1,
            bindings: HashMap::from([(
                Button::Circle,
                vec![slot(Binding::Gamepad(Button::Square))],
            )]),
        });
        p.lightbar_r = 1.0;
        p.gyro_mouse.dots_per_360 = 1234.0;
        let bindings = p.button_bindings.clone();
        let layers = p.layers.clone();

        p.convert_to_device(ControllerKind::SteamController);

        assert_eq!(p.device, ControllerKind::SteamController);
        assert_eq!(p.button_bindings, bindings);
        assert_eq!(p.layers, layers);
        assert_eq!(p.lightbar_r, 1.0);
        assert_eq!(p.gyro_mouse.dots_per_360, 1234.0);
    }

    #[test]
    fn new_steam_profile_round_trips_with_current_schema() {
        let mut p =
            Profile::default_for_device("Steam_New".into(), ControllerKind::SteamController);
        p.schema_version = CURRENT_PROFILE_SCHEMA;
        p.button_bindings
            .insert(Button::L4, vec![slot(Binding::Gamepad(Button::Cross))]);

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.name, "Steam_New");
        assert_eq!(back.device, ControllerKind::SteamController);
        assert_eq!(back.schema_version, CURRENT_PROFILE_SCHEMA);
        assert_eq!(
            back.button_bindings.get(&Button::L4),
            p.button_bindings.get(&Button::L4)
        );
    }

    #[test]
    fn steam_profile_file_round_trips_device() {
        let dir = std::env::temp_dir().join(format!(
            "gyro4u-profile-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp profile dir");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let profile =
            Profile::default_for_device("TheFinals".into(), ControllerKind::SteamController);

        pm.save_profile(&profile).expect("save profile");
        let loaded = pm.load_profile("TheFinals").expect("load profile");

        assert_eq!(loaded.device, ControllerKind::SteamController);
        assert_eq!(loaded.schema_version, CURRENT_PROFILE_SCHEMA);
        assert!(pm.profile_exists("TheFinals"));
        assert_eq!(
            pm.list_profiles_for(ControllerKind::SteamController).len(),
            1
        );
        assert!(pm.list_profiles_for(ControllerKind::DualSense).is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
