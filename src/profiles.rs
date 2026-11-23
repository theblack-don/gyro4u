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
    inputs::{AbxyLayout, Button, ControllerFamily, ControllerKind, ProfileGroup},
    remap::{Binding, BindingSlot, Layer, PressTiming, PressType, StickConfig},
    stick_mouse::StickMouseConfig,
    trackpad_mouse::{SteamPadHapticStrength, SteamPadMouseConfig},
    transform::{GyroProcessor, InputTransform, TriggerDeadband},
};

pub const CURRENT_PROFILE_SCHEMA: u32 = 4;

pub const EIGHTBITDO_DEFAULT_PROFILE: &str = "8BitDoDefault";
pub const LEGACY_EIGHTBITDO_DEFAULT_PROFILE: &str = "EightBitDoDefault";
pub const APEX5_DEFAULT_PROFILE: &str = "Apex5Default";
pub const VADER5_DEFAULT_PROFILE: &str = "Vader5Default";
pub const TARANTULA_DEFAULT_PROFILE: &str = "TarantulaProDefault";

pub fn default_profile_name_for_group(group: ProfileGroup) -> &'static str {
    match group {
        ProfileGroup::DualSense => "Default",
        ProfileGroup::Steam => "SteamDefault",
        ProfileGroup::EightBitDo => EIGHTBITDO_DEFAULT_PROFILE,
        ProfileGroup::FlydigiApex5 => APEX5_DEFAULT_PROFILE,
        ProfileGroup::FlydigiVader5Pro => VADER5_DEFAULT_PROFILE,
        ProfileGroup::GameSirTarantulaPro => TARANTULA_DEFAULT_PROFILE,
    }
}

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

    #[serde(default)]
    pub eightbitdo_abxy_layout: AbxyLayout,
}

impl Profile {
    pub fn matches_device(&self, kind: ControllerKind) -> bool {
        self.device.same_profile_group(kind)
    }

    /// Convert a profile's device while preserving every mapping and
    /// configuration field except gyro center calibration, which is specific
    /// to the physical controller and resets when the profile group changes.
    pub fn convert_to_device(&mut self, kind: ControllerKind) {
        if !self.device.same_profile_group(kind) {
            self.gyro_mouse.bias_dps = [0.0; 3];
        }
        self.device = kind;
    }

    pub fn default_for_device(name: String, kind: ControllerKind) -> Self {
        let mut profile = Self {
            name,
            device: kind,
            ..Default::default()
        };
        match kind.family() {
            ControllerFamily::Steam => {
                profile.gyro_mouse.activation = GyroActivation::RightTrackpad;
                profile.steam_left_pad_mouse.haptic_strength = SteamPadHapticStrength::Medium;
                profile.steam_right_pad_mouse.haptic_strength = SteamPadHapticStrength::Medium;
            }
            ControllerFamily::EightBitDo => {
                profile.gyro_mouse.activation = GyroActivation::Always;
                profile.eightbitdo_abxy_layout = AbxyLayout::Switch;
            }
            ControllerFamily::Flydigi => {
                profile.gyro_mouse.activation = GyroActivation::Always;
            }
            ControllerFamily::GameSir => {
                profile.gyro_mouse.activation = GyroActivation::Always;
            }
            ControllerFamily::DualSense => {}
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

/// Profile filenames are global; classify a name collision by the profile
/// group the existing profile belongs to.
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

    /// Rename the legacy `EightBitDoDefault.json` file to `8BitDoDefault.json`.
    /// Returns true when the profile was migrated.
    pub fn migrate_eightbitdo_default_name(&self) -> bool {
        let old_path = self
            .profiles_dir
            .join(format!("{LEGACY_EIGHTBITDO_DEFAULT_PROFILE}.json"));
        let new_path = self
            .profiles_dir
            .join(format!("{EIGHTBITDO_DEFAULT_PROFILE}.json"));
        if !old_path.exists() || new_path.exists() {
            return false;
        }
        let Ok(mut profile) = self.load_profile(LEGACY_EIGHTBITDO_DEFAULT_PROFILE) else {
            return false;
        };
        profile.name = EIGHTBITDO_DEFAULT_PROFILE.to_string();
        if self.save_profile(&profile).is_err() {
            return false;
        }
        fs::remove_file(old_path).is_ok()
    }

    pub fn default_profile_name(kind: ControllerKind) -> &'static str {
        default_profile_name_for_group(kind.profile_group())
    }

    pub fn is_default_profile_name(name: &str) -> bool {
        [
            "Default",
            "SteamDefault",
            LEGACY_EIGHTBITDO_DEFAULT_PROFILE,
            EIGHTBITDO_DEFAULT_PROFILE,
            APEX5_DEFAULT_PROFILE,
            VADER5_DEFAULT_PROFILE,
            TARANTULA_DEFAULT_PROFILE,
        ]
        .contains(&name)
    }

    /// Pick a deterministic fallback name when the canonical default is owned
    /// by another profile group. Returns the name and an existing same-group
    /// profile to reuse, if one already exists.
    fn fallback_default(&self, canonical: &str, kind: ControllerKind) -> (String, Option<Profile>) {
        let suffix = Self::sanitize_filename(match kind.profile_group() {
            ProfileGroup::DualSense => "DualSense",
            ProfileGroup::Steam => "Steam",
            ProfileGroup::EightBitDo => "8BitDo",
            ProfileGroup::FlydigiApex5 => "Flydigi",
            ProfileGroup::FlydigiVader5Pro => "Vader5",
            ProfileGroup::GameSirTarantulaPro => "Tarantula",
        });
        let base = format!("{canonical}_{suffix}");
        let mut index = 1;
        loop {
            let candidate = if index == 1 {
                base.clone()
            } else {
                format!("{base}_{index}")
            };
            if !self.profile_exists(&candidate) {
                return (candidate, None);
            }
            if let Ok(profile) = self.load_profile(&candidate)
                && profile.matches_device(kind)
            {
                return (candidate, Some(profile));
            }
            index += 1;
        }
    }

    /// Normalize the shared Flydigi default into an Apex 5 profile and seed a
    /// Vader 5 Pro default from it. Idempotent; the GUI and daemon may both run
    /// it. A missing or unreadable Apex5Default is left for the normal default
    /// path, and an existing Vader5Default is never overwritten.
    pub fn migrate_flydigi_defaults(&self) {
        let Ok(mut apex) = self.load_profile(APEX5_DEFAULT_PROFILE) else {
            return;
        };
        if apex.device.profile_group() != ProfileGroup::FlydigiApex5 {
            apex.convert_to_device(ControllerKind::FlydigiApex5);
            let _ = self.save_profile(&apex);
        }
        if !self.profile_exists(VADER5_DEFAULT_PROFILE) {
            let mut vader = apex.clone();
            vader.name = VADER5_DEFAULT_PROFILE.to_string();
            vader.convert_to_device(ControllerKind::FlydigiVader5Pro);
            vader.schema_version = CURRENT_PROFILE_SCHEMA;
            let _ = self.save_profile(&vader);
        }
    }

    pub fn ensure_default_for(&self, kind: ControllerKind) -> Profile {
        let group = kind.profile_group();
        if matches!(
            group,
            ProfileGroup::FlydigiApex5 | ProfileGroup::FlydigiVader5Pro
        ) {
            self.migrate_flydigi_defaults();
        }
        let name = Self::default_profile_name(kind);
        if kind.family() == ControllerFamily::EightBitDo {
            self.migrate_eightbitdo_default_name();
        }
        if let Ok(profile) = self.load_profile(name)
            && profile.matches_device(kind)
        {
            return profile;
        }
        // Prefer an unmigrated legacy default over creating a fresh empty one.
        if kind.family() == ControllerFamily::EightBitDo
            && !self.profile_exists(name)
            && let Ok(mut profile) = self.load_profile(LEGACY_EIGHTBITDO_DEFAULT_PROFILE)
            && profile.matches_device(kind)
        {
            profile.name = name.to_string();
            return profile;
        }
        // Never overwrite the canonical default of another profile group.
        let name = if self.profile_exists(name) {
            let (fallback, existing) = self.fallback_default(name, kind);
            if let Some(profile) = existing {
                return profile;
            }
            fallback
        } else {
            name.to_string()
        };
        let mut profile = Profile::default_for_device(name.clone(), kind);
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

    pub fn rename_profile(&self, old: &str, new: &str) -> Result<()> {
        let new = new.trim();
        Self::validate_profile_name(new)?;
        let mut profile = self.load_profile(old)?;
        let canonical = Self::default_profile_name(profile.device);
        if canonical == old || canonical == profile.name {
            bail!("Cannot rename the default profile for this device");
        }
        if Self::is_default_profile_name(new) {
            bail!("Profile name '{}' is reserved for a default profile", new);
        }
        if new == old {
            return Ok(());
        }
        if self.profile_exists(new) {
            bail!("A profile named '{}' already exists", new);
        }

        profile.name = new.to_string();
        self.save_profile(&profile)?;

        let old_path = self
            .profiles_dir
            .join(format!("{}.json", Self::sanitize_filename(old)));
        if let Err(e) = fs::remove_file(&old_path) {
            bail!(
                "Profile renamed to '{}' but the old file '{}' could not be removed: {}",
                new,
                old,
                e
            );
        }
        Ok(())
    }

    pub fn delete_profile(&self, name: &str) -> Result<()> {
        if let Ok(profile) = self.load_profile(name) {
            let canonical = Self::default_profile_name(profile.device);
            if canonical == name || canonical == profile.name {
                bail!("Cannot delete the default profile for this device");
            }
        } else if Self::is_default_profile_name(name) {
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
            .filter(|profile| profile.matches_device(kind))
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

            eightbitdo_abxy_layout: AbxyLayout::Xbox,
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
    fn profiles_without_abxy_layout_default_to_xbox() {
        let p = legacy();
        assert_eq!(p.eightbitdo_abxy_layout, AbxyLayout::Xbox);
        assert_eq!(Profile::default().eightbitdo_abxy_layout, AbxyLayout::Xbox);

        let mut value = serde_json::to_value(Profile::default()).expect("serialize");
        value
            .as_object_mut()
            .expect("profile object")
            .remove("eightbitdo_abxy_layout");
        let back: Profile = serde_json::from_value(value).expect("legacy profile parses");
        assert_eq!(back.eightbitdo_abxy_layout, AbxyLayout::Xbox);
    }

    #[test]
    fn eightbitdo_abxy_layout_round_trips() {
        let mut p = legacy();
        p.eightbitdo_abxy_layout = AbxyLayout::Switch;

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.eightbitdo_abxy_layout, AbxyLayout::Switch);
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

        let eightbitdo = Profile::default_for_device(
            EIGHTBITDO_DEFAULT_PROFILE.to_string(),
            ControllerKind::EightBitDoPro3,
        );
        assert_eq!(eightbitdo.gyro_mouse.activation, GyroActivation::Always);
        assert_eq!(eightbitdo.gyro_mouse.dots_per_360, 6545.0);
        assert_eq!(eightbitdo.eightbitdo_abxy_layout, AbxyLayout::Switch);

        let apex = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiApex5,
        );
        assert_eq!(apex.gyro_mouse.activation, GyroActivation::Always);
        assert!(apex.matches_device(ControllerKind::FlydigiApex5));

        let vader = Profile::default_for_device(
            VADER5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );
        assert_eq!(vader.gyro_mouse.activation, GyroActivation::Always);
        assert!(vader.matches_device(ControllerKind::FlydigiVader5Pro));
        assert!(!vader.matches_device(ControllerKind::FlydigiApex5));

        let gamesir = Profile::default_for_device(
            TARANTULA_DEFAULT_PROFILE.to_string(),
            ControllerKind::GameSirTarantulaPro,
        );
        assert_eq!(gamesir.gyro_mouse.activation, GyroActivation::Always);
        assert!(gamesir.matches_device(ControllerKind::GameSirTarantulaPro));
        assert!(!gamesir.matches_device(ControllerKind::FlydigiApex5));
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
        assert_eq!(
            ProfileManager::default_profile_name(ControllerKind::EightBitDoPro3),
            EIGHTBITDO_DEFAULT_PROFILE
        );
        assert_eq!(
            ProfileManager::default_profile_name(ControllerKind::FlydigiApex5),
            APEX5_DEFAULT_PROFILE
        );
        assert_eq!(
            ProfileManager::default_profile_name(ControllerKind::FlydigiVader5Pro),
            VADER5_DEFAULT_PROFILE
        );
        assert_eq!(
            ProfileManager::default_profile_name(ControllerKind::GameSirTarantulaPro),
            TARANTULA_DEFAULT_PROFILE
        );
        for kind in [
            ControllerKind::DualSense,
            ControllerKind::DualSenseEdge,
            ControllerKind::SteamController,
            ControllerKind::EightBitDoPro3,
            ControllerKind::FlydigiApex5,
            ControllerKind::FlydigiVader5Pro,
            ControllerKind::GameSirTarantulaPro,
        ] {
            ProfileManager::validate_profile_name(ProfileManager::default_profile_name(kind))
                .expect("default profile names must be valid filenames");
        }
    }

    #[test]
    fn default_profile_name_detection_covers_canonical_and_legacy_names() {
        for name in [
            "Default",
            "SteamDefault",
            LEGACY_EIGHTBITDO_DEFAULT_PROFILE,
            EIGHTBITDO_DEFAULT_PROFILE,
            APEX5_DEFAULT_PROFILE,
            VADER5_DEFAULT_PROFILE,
            TARANTULA_DEFAULT_PROFILE,
        ] {
            assert!(
                ProfileManager::is_default_profile_name(name),
                "{name} must be treated as a default profile"
            );
        }
        for name in [
            "default",
            "Default_DualSense",
            "SteamDefault_Steam",
            "MyDefault",
            "",
        ] {
            assert!(
                !ProfileManager::is_default_profile_name(name),
                "{name} must not be treated as a default profile"
            );
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
        assert!(!parsed.matches_device(ControllerKind::EightBitDoPro3));
    }

    #[test]
    fn eightbitdo_profiles_match_by_family_and_round_trip() {
        let mut p = Profile::default_for_device(
            EIGHTBITDO_DEFAULT_PROFILE.to_string(),
            ControllerKind::EightBitDoPro3,
        );
        p.button_bindings.insert(
            Button::EightBitDoPL,
            vec![slot(Binding::Gamepad(Button::L1))],
        );

        assert!(p.matches_device(ControllerKind::EightBitDoPro3));
        assert!(!p.matches_device(ControllerKind::DualSense));
        assert!(!p.matches_device(ControllerKind::SteamController));

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.device, ControllerKind::EightBitDoPro3);
        assert_eq!(
            back.button_bindings.get(&Button::EightBitDoPL),
            p.button_bindings.get(&Button::EightBitDoPL)
        );
    }

    #[test]
    fn apex5_profiles_match_by_profile_group_and_round_trip_extras() {
        let mut p = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiApex5,
        );
        p.button_bindings
            .insert(Button::ApexLM, vec![slot(Binding::Gamepad(Button::L1))]);

        assert!(p.matches_device(ControllerKind::FlydigiApex5));
        assert!(!p.matches_device(ControllerKind::FlydigiVader5Pro));
        assert!(!p.matches_device(ControllerKind::DualSense));
        assert!(!p.matches_device(ControllerKind::EightBitDoPro3));

        let json = serde_json::to_string(&p).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.device, ControllerKind::FlydigiApex5);
        assert_eq!(
            back.button_bindings.get(&Button::ApexLM),
            p.button_bindings.get(&Button::ApexLM)
        );
    }

    #[test]
    fn vader5_profiles_do_not_match_apex5() {
        let p = Profile::default_for_device(
            VADER5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );

        assert!(p.matches_device(ControllerKind::FlydigiVader5Pro));
        assert!(!p.matches_device(ControllerKind::FlydigiApex5));
        assert!(!p.matches_device(ControllerKind::DualSense));
    }

    #[test]
    fn list_profiles_for_separates_flydigi_models() {
        let dir = temp_profiles_dir("list-groups");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let apex = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiApex5,
        );
        let vader = Profile::default_for_device(
            VADER5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );
        let edge = Profile::default_for_device("Edge".to_string(), ControllerKind::DualSenseEdge);
        pm.save_profile(&apex).expect("save Apex profile");
        pm.save_profile(&vader).expect("save Vader profile");
        pm.save_profile(&edge).expect("save Edge profile");

        let apex_names: Vec<_> = pm
            .list_profiles_for(ControllerKind::FlydigiApex5)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(apex_names, vec![APEX5_DEFAULT_PROFILE.to_string()]);
        let vader_names: Vec<_> = pm
            .list_profiles_for(ControllerKind::FlydigiVader5Pro)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(vader_names, vec![VADER5_DEFAULT_PROFILE.to_string()]);
        let sony_names: Vec<_> = pm
            .list_profiles_for(ControllerKind::DualSense)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(sony_names, vec!["Edge".to_string()]);

        let _ = fs::remove_dir_all(&dir);
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
        p.gyro_mouse.bias_dps = [0.5, -0.25, 1.0];
        let bindings = p.button_bindings.clone();
        let layers = p.layers.clone();

        p.convert_to_device(ControllerKind::SteamController);

        assert_eq!(p.device, ControllerKind::SteamController);
        assert_eq!(p.button_bindings, bindings);
        assert_eq!(p.layers, layers);
        assert_eq!(p.lightbar_r, 1.0);
        assert_eq!(p.gyro_mouse.dots_per_360, 1234.0);
        assert_eq!(p.gyro_mouse.bias_dps, [0.0; 3]);

        p.convert_to_device(ControllerKind::EightBitDoPro3);
        assert_eq!(p.device, ControllerKind::EightBitDoPro3);
        assert_eq!(p.button_bindings, bindings);
        assert_eq!(p.layers, layers);
        assert_eq!(p.gyro_mouse.bias_dps, [0.0; 3]);
    }

    #[test]
    fn conversion_resets_gyro_bias_only_across_profile_groups() {
        let mut cross_group =
            Profile::default_for_device("Cross".into(), ControllerKind::DualSense);
        cross_group.gyro_mouse.bias_dps = [0.6, -0.05, -0.4];

        cross_group.convert_to_device(ControllerKind::EightBitDoPro3);

        assert_eq!(cross_group.device, ControllerKind::EightBitDoPro3);
        assert_eq!(cross_group.gyro_mouse.bias_dps, [0.0; 3]);

        let mut same_group = Profile::default_for_device("Same".into(), ControllerKind::DualSense);
        same_group.gyro_mouse.bias_dps = [0.6, -0.05, -0.4];

        same_group.convert_to_device(ControllerKind::DualSenseEdge);

        assert_eq!(same_group.device, ControllerKind::DualSenseEdge);
        assert_eq!(same_group.gyro_mouse.bias_dps, [0.6, -0.05, -0.4]);

        let mut flydigi = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiApex5,
        );
        flydigi.gyro_mouse.bias_dps = [0.6, -0.05, -0.4];

        flydigi.convert_to_device(ControllerKind::FlydigiVader5Pro);

        assert_eq!(flydigi.device, ControllerKind::FlydigiVader5Pro);
        assert_eq!(flydigi.gyro_mouse.bias_dps, [0.0; 3]);
    }

    #[test]
    fn default_profile_names_cannot_be_deleted() {
        let pm = ProfileManager {
            profiles_dir: std::env::temp_dir().join("gyro4u-delete-guard-unused"),
        };
        for name in [
            "Default",
            "SteamDefault",
            LEGACY_EIGHTBITDO_DEFAULT_PROFILE,
            EIGHTBITDO_DEFAULT_PROFILE,
            APEX5_DEFAULT_PROFILE,
            VADER5_DEFAULT_PROFILE,
            TARANTULA_DEFAULT_PROFILE,
        ] {
            assert!(pm.delete_profile(name).is_err(), "{name} must be protected");
        }
    }

    #[test]
    fn rename_profile_preserves_content_and_moves_file() {
        let dir = temp_profiles_dir("rename-preserve");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let mut profile =
            Profile::default_for_device("Build".to_string(), ControllerKind::DualSense);
        profile.lightbar_r = 0.42;
        profile.gyro_mouse.dots_per_360 = 1234.0;
        profile
            .button_bindings
            .insert(Button::Cross, vec![slot(Binding::Gamepad(Button::Circle))]);
        profile.layers.push(Layer {
            activator: Button::L1,
            bindings: HashMap::from([(
                Button::Circle,
                vec![slot(Binding::Gamepad(Button::Square))],
            )]),
        });
        pm.save_profile(&profile).expect("save profile");
        let other = Profile::default_for_device("Other".to_string(), ControllerKind::DualSense);
        pm.save_profile(&other).expect("save other profile");

        pm.rename_profile("Build", "Renamed")
            .expect("rename profile");

        assert!(!pm.profile_exists("Build"));
        assert!(pm.profile_exists("Renamed"));
        assert!(pm.profile_exists("Other"));
        let renamed = pm.load_profile("Renamed").expect("load renamed profile");
        assert_eq!(renamed.name, "Renamed");
        assert_eq!(renamed.device, ControllerKind::DualSense);
        assert_eq!(renamed.lightbar_r, 0.42);
        assert_eq!(renamed.gyro_mouse.dots_per_360, 1234.0);
        assert_eq!(renamed.button_bindings, profile.button_bindings);
        assert_eq!(renamed.layers, profile.layers);
        assert_eq!(
            pm.load_profile("Other")
                .expect("other profile remains")
                .name,
            other.name
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_profile_rejects_existing_name() {
        let dir = temp_profiles_dir("rename-conflict");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let build = Profile::default_for_device("Build".to_string(), ControllerKind::DualSense);
        let other = Profile::default_for_device("Other".to_string(), ControllerKind::DualSense);
        pm.save_profile(&build).expect("save profile");
        pm.save_profile(&other).expect("save other profile");

        let err = pm
            .rename_profile("Build", "Other")
            .expect_err("conflicting name must be rejected");

        assert!(err.to_string().contains("already exists"));
        assert!(pm.profile_exists("Build"));
        assert_eq!(
            pm.load_profile("Build")
                .expect("original profile remains")
                .name,
            "Build"
        );
        assert_eq!(
            pm.load_profile("Other")
                .expect("conflicting profile remains")
                .name,
            other.name
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_profile_rejects_canonical_default() {
        let dir = temp_profiles_dir("rename-default");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let default = Profile::default_for_device("Default".to_string(), ControllerKind::DualSense);
        pm.save_profile(&default).expect("save default profile");

        let err = pm
            .rename_profile("Default", "Mine")
            .expect_err("canonical default must be protected");

        assert!(err.to_string().contains("default profile"));
        assert!(pm.profile_exists("Default"));
        assert!(!pm.profile_exists("Mine"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_profile_rejects_vader5_default() {
        let dir = temp_profiles_dir("rename-vader5-default");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let default = Profile::default_for_device(
            VADER5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );
        pm.save_profile(&default).expect("save Vader 5 default");

        let err = pm
            .rename_profile(VADER5_DEFAULT_PROFILE, "Mine")
            .expect_err("canonical Vader 5 default must be protected");

        assert!(err.to_string().contains("default profile"));
        assert!(pm.profile_exists(VADER5_DEFAULT_PROFILE));
        assert!(!pm.profile_exists("Mine"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_profile_rejects_reserved_default_name() {
        let dir = temp_profiles_dir("rename-reserved");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let build = Profile::default_for_device("Build".to_string(), ControllerKind::DualSense);
        pm.save_profile(&build).expect("save profile");

        let err = pm
            .rename_profile("Build", "Default")
            .expect_err("reserved name must be rejected");

        assert!(err.to_string().contains("reserved"));
        assert!(pm.profile_exists("Build"));
        assert!(!pm.profile_exists("Default"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_profile_allows_same_name_no_op() {
        let dir = temp_profiles_dir("rename-noop");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let build = Profile::default_for_device("Build".to_string(), ControllerKind::DualSense);
        pm.save_profile(&build).expect("save profile");

        pm.rename_profile("Build", "Build")
            .expect("same-name rename is a no-op");

        let loaded = pm.load_profile("Build").expect("profile remains");
        assert_eq!(loaded.name, "Build");
        assert_eq!(loaded.device, build.device);
        assert_eq!(loaded.button_bindings, build.button_bindings);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_profile_uses_loaded_device_for_default_guard() {
        let dir = temp_profiles_dir("delete-device-aware");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let converted =
            Profile::default_for_device("Default".to_string(), ControllerKind::EightBitDoPro3);
        let other = Profile::default_for_device("Other".to_string(), ControllerKind::DualSense);
        pm.save_profile(&converted).expect("save 8BitDo profile");
        pm.save_profile(&other).expect("save other profile");

        pm.delete_profile("Default")
            .expect("8BitDo profile named Default is not the 8BitDo default");

        assert!(!pm.profile_exists("Default"));
        assert!(pm.profile_exists("Other"));
        assert_eq!(
            pm.load_profile("Other")
                .expect("unrelated profile remains")
                .name,
            other.name
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_profile_rejects_dualsense_default() {
        let dir = temp_profiles_dir("delete-dualsense-default");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let default = Profile::default_for_device("Default".to_string(), ControllerKind::DualSense);
        pm.save_profile(&default).expect("save DualSense default");

        let err = pm
            .delete_profile("Default")
            .expect_err("DualSense default must be protected");

        assert!(err.to_string().contains("default profile"));
        assert!(pm.profile_exists("Default"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_profile_rejects_eightbitdo_default() {
        let dir = temp_profiles_dir("delete-eightbitdo-default");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let default = Profile::default_for_device(
            EIGHTBITDO_DEFAULT_PROFILE.to_string(),
            ControllerKind::EightBitDoPro3,
        );
        pm.save_profile(&default).expect("save 8BitDo default");

        let err = pm
            .delete_profile(EIGHTBITDO_DEFAULT_PROFILE)
            .expect_err("canonical 8BitDo default must be protected");

        assert!(err.to_string().contains("default profile"));
        assert!(pm.profile_exists(EIGHTBITDO_DEFAULT_PROFILE));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_profile_rejects_apex5_default() {
        let dir = temp_profiles_dir("delete-apex5-default");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let default = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiApex5,
        );
        pm.save_profile(&default).expect("save Apex 5 default");

        let err = pm
            .delete_profile(APEX5_DEFAULT_PROFILE)
            .expect_err("canonical Apex 5 default must be protected");

        assert!(err.to_string().contains("default profile"));
        assert!(pm.profile_exists(APEX5_DEFAULT_PROFILE));

        let _ = fs::remove_dir_all(&dir);
    }

    fn temp_profiles_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gyro4u-profile-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp profile dir");
        dir
    }

    #[test]
    fn eightbitdo_default_profile_file_is_migrated() {
        let dir = temp_profiles_dir("eightbitdo-migrate");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let mut old = Profile::default_for_device(
            LEGACY_EIGHTBITDO_DEFAULT_PROFILE.to_string(),
            ControllerKind::EightBitDoPro3,
        );
        old.lightbar_r = 0.75;
        old.button_bindings.insert(
            Button::EightBitDoPL,
            vec![slot(Binding::Gamepad(Button::L1))],
        );
        pm.save_profile(&old).expect("save legacy profile");

        assert!(pm.migrate_eightbitdo_default_name());

        assert!(!pm.profile_exists(LEGACY_EIGHTBITDO_DEFAULT_PROFILE));
        assert!(pm.profile_exists(EIGHTBITDO_DEFAULT_PROFILE));
        let migrated = pm
            .load_profile(EIGHTBITDO_DEFAULT_PROFILE)
            .expect("load migrated profile");
        assert_eq!(migrated.name, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(migrated.device, ControllerKind::EightBitDoPro3);
        assert_eq!(migrated.lightbar_r, 0.75);
        assert_eq!(
            migrated.button_bindings.get(&Button::EightBitDoPL),
            old.button_bindings.get(&Button::EightBitDoPL)
        );
        assert!(
            !pm.migrate_eightbitdo_default_name(),
            "migration is idempotent"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_default_for_does_not_overwrite_other_family_default() {
        let dir = temp_profiles_dir("default-collision");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let mut eightbitdo =
            Profile::default_for_device("Default".to_string(), ControllerKind::EightBitDoPro3);
        eightbitdo.lightbar_r = 0.42;
        eightbitdo.gyro_mouse.bias_dps = [0.1, 0.2, 0.3];
        pm.save_profile(&eightbitdo).expect("save 8BitDo default");
        let canonical_path = dir.join("Default.json");
        let original = fs::read_to_string(&canonical_path).expect("read canonical default");

        let dualsense = pm.ensure_default_for(ControllerKind::DualSense);

        assert_eq!(dualsense.name, "Default_DualSense");
        assert_eq!(dualsense.device, ControllerKind::DualSense);
        assert!(pm.profile_exists("Default_DualSense"));
        assert_eq!(
            fs::read_to_string(&canonical_path).expect("canonical default still present"),
            original,
            "the other family's default file must be untouched"
        );
        let kept = pm
            .load_profile("Default")
            .expect("original default remains loadable");
        assert_eq!(kept.device, ControllerKind::EightBitDoPro3);
        assert_eq!(kept.name, "Default");
        assert_eq!(kept.lightbar_r, 0.42);
        assert_eq!(kept.gyro_mouse.bias_dps, [0.1, 0.2, 0.3]);

        assert_eq!(
            pm.ensure_default_for(ControllerKind::DualSense).name,
            "Default_DualSense",
            "the fallback default is reused on later calls"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_fallback_name_avoids_occupied_fallback() {
        let dir = temp_profiles_dir("default-collision-suffix");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let eightbitdo =
            Profile::default_for_device("Default".to_string(), ControllerKind::EightBitDoPro3);
        pm.save_profile(&eightbitdo).expect("save 8BitDo default");
        let steam = Profile::default_for_device(
            "Default_DualSense".to_string(),
            ControllerKind::SteamController,
        );
        pm.save_profile(&steam).expect("save occupied fallback");

        let dualsense = pm.ensure_default_for(ControllerKind::DualSense);

        assert_eq!(dualsense.name, "Default_DualSense_2");
        assert!(pm.profile_exists("Default_DualSense_2"));
        assert_eq!(
            pm.load_profile("Default_DualSense")
                .expect("occupied fallback remains")
                .device,
            ControllerKind::SteamController
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_default_for_creates_canonical_defaults_on_clean_install() {
        let dir = temp_profiles_dir("default-clean");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };

        for (kind, name) in [
            (ControllerKind::DualSense, "Default"),
            (ControllerKind::DualSenseEdge, "Default"),
            (ControllerKind::SteamController, "SteamDefault"),
            (ControllerKind::EightBitDoPro3, EIGHTBITDO_DEFAULT_PROFILE),
            (ControllerKind::FlydigiApex5, APEX5_DEFAULT_PROFILE),
            (ControllerKind::FlydigiVader5Pro, VADER5_DEFAULT_PROFILE),
            (
                ControllerKind::GameSirTarantulaPro,
                TARANTULA_DEFAULT_PROFILE,
            ),
        ] {
            let profile = pm.ensure_default_for(kind);
            assert_eq!(profile.name, name);
            assert!(profile.matches_device(kind));
            assert!(pm.profile_exists(name));
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_flydigi_defaults_copies_shared_apex_profile_to_vader() {
        let dir = temp_profiles_dir("flydigi-copy");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let mut shared = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );
        shared.lightbar_r = 0.42;
        shared.gyro_mouse.dots_per_360 = 1234.0;
        shared.gyro_mouse.bias_dps = [0.5, -0.25, 1.0];
        shared.press_timing.long_press_ms = 450;
        shared.eightbitdo_abxy_layout = AbxyLayout::Switch;
        shared
            .button_bindings
            .insert(Button::ApexLM, vec![slot(Binding::Gamepad(Button::L1))]);
        shared.button_bindings.insert(
            Button::VaderC,
            vec![slot(Binding::Key(crate::remap::Key::new("C").unwrap()))],
        );
        shared.layers.push(Layer {
            activator: Button::L1,
            bindings: HashMap::from([(
                Button::Circle,
                vec![slot(Binding::Gamepad(Button::Square))],
            )]),
        });
        pm.save_profile(&shared).expect("save shared default");
        let bindings = shared.button_bindings.clone();
        let layers = shared.layers.clone();

        pm.migrate_flydigi_defaults();

        let apex = pm
            .load_profile(APEX5_DEFAULT_PROFILE)
            .expect("load normalized Apex default");
        assert_eq!(apex.name, APEX5_DEFAULT_PROFILE);
        assert_eq!(apex.device, ControllerKind::FlydigiApex5);
        assert_eq!(apex.gyro_mouse.bias_dps, [0.0; 3]);
        assert_eq!(apex.button_bindings, bindings);
        assert_eq!(apex.layers, layers);
        assert_eq!(apex.lightbar_r, 0.42);
        assert_eq!(apex.gyro_mouse.dots_per_360, 1234.0);
        assert_eq!(apex.press_timing.long_press_ms, 450);
        assert_eq!(apex.eightbitdo_abxy_layout, AbxyLayout::Switch);
        assert!(
            apex.button_bindings.contains_key(&Button::VaderC),
            "Vader-only bindings stay in the Apex clone"
        );

        let vader = pm
            .load_profile(VADER5_DEFAULT_PROFILE)
            .expect("load copied Vader default");
        assert_eq!(vader.name, VADER5_DEFAULT_PROFILE);
        assert_eq!(vader.device, ControllerKind::FlydigiVader5Pro);
        assert_eq!(vader.gyro_mouse.bias_dps, [0.0; 3]);
        assert_eq!(vader.button_bindings, bindings);
        assert_eq!(vader.layers, layers);
        assert_eq!(vader.lightbar_r, 0.42);
        assert_eq!(vader.gyro_mouse.dots_per_360, 1234.0);
        assert_eq!(vader.press_timing.long_press_ms, 450);
        assert_eq!(vader.eightbitdo_abxy_layout, AbxyLayout::Switch);
        assert!(vader.button_bindings.contains_key(&Button::VaderC));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn flipped_apex_default_is_normalized_and_seeds_vader() {
        let dir = temp_profiles_dir("flydigi-normalize");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let mut flipped = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );
        flipped.gyro_mouse.bias_dps = [0.1, 0.2, 0.3];
        pm.save_profile(&flipped).expect("save flipped default");

        let apex = pm.ensure_default_for(ControllerKind::FlydigiApex5);

        assert_eq!(apex.name, APEX5_DEFAULT_PROFILE);
        assert_eq!(apex.device, ControllerKind::FlydigiApex5);
        assert_eq!(apex.gyro_mouse.bias_dps, [0.0; 3]);
        let reloaded = pm
            .load_profile(APEX5_DEFAULT_PROFILE)
            .expect("normalized file persists");
        assert_eq!(reloaded.device, ControllerKind::FlydigiApex5);
        let vader = pm
            .load_profile(VADER5_DEFAULT_PROFILE)
            .expect("Vader default seeded from normalized Apex");
        assert_eq!(vader.device, ControllerKind::FlydigiVader5Pro);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_flydigi_defaults_is_idempotent_and_keeps_existing_vader() {
        let dir = temp_profiles_dir("flydigi-idempotent");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let apex = Profile::default_for_device(
            APEX5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiApex5,
        );
        let mut existing = Profile::default_for_device(
            VADER5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );
        existing.lightbar_g = 0.9;
        existing
            .button_bindings
            .insert(Button::VaderZ, vec![slot(Binding::Gamepad(Button::L2))]);
        pm.save_profile(&apex).expect("save Apex default");
        pm.save_profile(&existing)
            .expect("save existing Vader default");
        let apex_path = dir.join(format!("{APEX5_DEFAULT_PROFILE}.json"));
        let vader_path = dir.join(format!("{VADER5_DEFAULT_PROFILE}.json"));
        let apex_before = fs::read_to_string(&apex_path).expect("read Apex default");
        let vader_before = fs::read_to_string(&vader_path).expect("read Vader default");

        pm.migrate_flydigi_defaults();
        pm.migrate_flydigi_defaults();

        assert_eq!(
            fs::read_to_string(&apex_path).expect("read Apex default"),
            apex_before,
            "normalized Apex file is not rewritten"
        );
        assert_eq!(
            fs::read_to_string(&vader_path).expect("read Vader default"),
            vader_before,
            "an existing Vader default is never overwritten"
        );
        let kept = pm
            .load_profile(VADER5_DEFAULT_PROFILE)
            .expect("load preserved Vader default");
        assert_eq!(kept.lightbar_g, 0.9);
        assert_eq!(
            kept.button_bindings.get(&Button::VaderZ),
            existing.button_bindings.get(&Button::VaderZ)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_default_for_vader_creates_a_fresh_default_without_apex() {
        let dir = temp_profiles_dir("flydigi-fresh-vader");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };

        let vader = pm.ensure_default_for(ControllerKind::FlydigiVader5Pro);

        assert_eq!(vader.name, VADER5_DEFAULT_PROFILE);
        assert_eq!(vader.device, ControllerKind::FlydigiVader5Pro);
        assert!(pm.profile_exists(VADER5_DEFAULT_PROFILE));
        assert!(!pm.profile_exists(APEX5_DEFAULT_PROFILE));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_profile_rejects_vader5_default() {
        let dir = temp_profiles_dir("delete-vader5-default");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let default = Profile::default_for_device(
            VADER5_DEFAULT_PROFILE.to_string(),
            ControllerKind::FlydigiVader5Pro,
        );
        pm.save_profile(&default).expect("save Vader 5 default");

        let err = pm
            .delete_profile(VADER5_DEFAULT_PROFILE)
            .expect_err("canonical Vader 5 default must be protected");

        assert!(err.to_string().contains("default profile"));
        assert!(pm.profile_exists(VADER5_DEFAULT_PROFILE));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_default_for_keeps_apex_and_vader_defaults_separate() {
        let dir = temp_profiles_dir("flydigi-separate-defaults");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };

        let apex = pm.ensure_default_for(ControllerKind::FlydigiApex5);
        let vader = pm.ensure_default_for(ControllerKind::FlydigiVader5Pro);

        assert_eq!(apex.name, APEX5_DEFAULT_PROFILE);
        assert_eq!(vader.name, VADER5_DEFAULT_PROFILE);
        assert!(pm.profile_exists(APEX5_DEFAULT_PROFILE));
        assert!(pm.profile_exists(VADER5_DEFAULT_PROFILE));
        assert_eq!(
            pm.load_profile(APEX5_DEFAULT_PROFILE)
                .expect("load Apex default")
                .device,
            ControllerKind::FlydigiApex5
        );
        assert_eq!(
            pm.load_profile(VADER5_DEFAULT_PROFILE)
                .expect("load Vader default")
                .device,
            ControllerKind::FlydigiVader5Pro
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_new_eightbitdo_default_is_not_overwritten() {
        let dir = temp_profiles_dir("eightbitdo-migrate-existing");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let old = Profile::default_for_device(
            LEGACY_EIGHTBITDO_DEFAULT_PROFILE.to_string(),
            ControllerKind::EightBitDoPro3,
        );
        pm.save_profile(&old).expect("save legacy profile");
        let mut new = Profile::default_for_device(
            EIGHTBITDO_DEFAULT_PROFILE.to_string(),
            ControllerKind::EightBitDoPro3,
        );
        new.lightbar_g = 0.9;
        pm.save_profile(&new).expect("save current profile");

        assert!(!pm.migrate_eightbitdo_default_name());

        assert!(pm.profile_exists(LEGACY_EIGHTBITDO_DEFAULT_PROFILE));
        let kept = pm
            .load_profile(EIGHTBITDO_DEFAULT_PROFILE)
            .expect("load current profile");
        assert_eq!(kept.lightbar_g, 0.9);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_default_for_migrates_legacy_eightbitdo_profile() {
        let dir = temp_profiles_dir("eightbitdo-ensure");
        let pm = ProfileManager {
            profiles_dir: dir.clone(),
        };
        let mut old = Profile::default_for_device(
            LEGACY_EIGHTBITDO_DEFAULT_PROFILE.to_string(),
            ControllerKind::EightBitDoPro3,
        );
        old.lightbar_b = 0.6;
        pm.save_profile(&old).expect("save legacy profile");

        let profile = pm.ensure_default_for(ControllerKind::EightBitDoPro3);

        assert_eq!(profile.name, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(profile.lightbar_b, 0.6);
        assert!(pm.profile_exists(EIGHTBITDO_DEFAULT_PROFILE));
        assert!(!pm.profile_exists(LEGACY_EIGHTBITDO_DEFAULT_PROFILE));

        let _ = fs::remove_dir_all(&dir);
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
