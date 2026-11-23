use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    inputs::{ControllerFamily, ProfileGroup},
    profiles::{
        APEX5_DEFAULT_PROFILE, EIGHTBITDO_DEFAULT_PROFILE, LEGACY_EIGHTBITDO_DEFAULT_PROFILE,
        ProfileManager, TARANTULA_DEFAULT_PROFILE, default_profile_name_for_group,
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControllerPreference {
    #[default]
    Auto,
    DualSense,
    Steam,
    EightBitDo,
    Flydigi,
    GameSir,
}

impl ControllerPreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::DualSense => "DualSense",
            Self::Steam => "Steam Controller",
            Self::EightBitDo => "8BitDo",
            Self::Flydigi => "Flydigi",
            Self::GameSir => "GameSir",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme_id: String,
    /// Legacy single-profile field. Kept in sync for older builds and the CLI.
    pub profile: String,
    #[serde(default)]
    pub dualsense_profile: String,
    #[serde(default)]
    pub steam_profile: String,
    #[serde(default)]
    pub eightbitdo_profile: String,
    #[serde(default)]
    pub flydigi_profile: String,
    #[serde(default)]
    pub vader5_profile: String,
    #[serde(default)]
    pub gamesir_profile: String,
    #[serde(default = "default_true")]
    pub auto_start_daemon: bool,
    #[serde(default)]
    pub controller_preference: ControllerPreference,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_id: "default".into(),
            profile: String::new(),
            dualsense_profile: "Default".into(),
            steam_profile: "SteamDefault".into(),
            eightbitdo_profile: EIGHTBITDO_DEFAULT_PROFILE.into(),
            flydigi_profile: APEX5_DEFAULT_PROFILE.into(),
            vader5_profile: crate::profiles::VADER5_DEFAULT_PROFILE.into(),
            gamesir_profile: TARANTULA_DEFAULT_PROFILE.into(),
            auto_start_daemon: true,
            controller_preference: ControllerPreference::Auto,
        }
    }
}

fn group_default_profile(group: ProfileGroup) -> &'static str {
    default_profile_name_for_group(group)
}

fn family_profile_group(family: ControllerFamily) -> ProfileGroup {
    match family {
        ControllerFamily::DualSense => ProfileGroup::DualSense,
        ControllerFamily::Steam => ProfileGroup::Steam,
        ControllerFamily::EightBitDo => ProfileGroup::EightBitDo,
        ControllerFamily::Flydigi => ProfileGroup::FlydigiApex5,
        ControllerFamily::GameSir => ProfileGroup::GameSirTarantulaPro,
    }
}

impl Settings {
    /// Fill legacy per-family profile slots from the single `profile` field and
    /// seed model-specific slots with their defaults. Returns true when
    /// something changed and the settings need saving.
    pub fn migrate_profile_memory(&mut self, legacy_family: Option<ControllerFamily>) -> bool {
        let mut changed = false;
        if self.eightbitdo_profile == LEGACY_EIGHTBITDO_DEFAULT_PROFILE {
            self.eightbitdo_profile = EIGHTBITDO_DEFAULT_PROFILE.to_string();
            changed = true;
        }
        if self.profile == LEGACY_EIGHTBITDO_DEFAULT_PROFILE {
            self.profile = EIGHTBITDO_DEFAULT_PROFILE.to_string();
            changed = true;
        }
        for family in [
            ControllerFamily::DualSense,
            ControllerFamily::Steam,
            ControllerFamily::EightBitDo,
            ControllerFamily::Flydigi,
            ControllerFamily::GameSir,
        ] {
            let slot = match family {
                ControllerFamily::DualSense => &mut self.dualsense_profile,
                ControllerFamily::Steam => &mut self.steam_profile,
                ControllerFamily::EightBitDo => &mut self.eightbitdo_profile,
                ControllerFamily::Flydigi => &mut self.flydigi_profile,
                ControllerFamily::GameSir => &mut self.gamesir_profile,
            };
            if slot.is_empty() {
                *slot = if legacy_family == Some(family) && !self.profile.is_empty() {
                    self.profile.clone()
                } else {
                    group_default_profile(family_profile_group(family)).to_string()
                };
                changed = true;
            }
        }
        if self.vader5_profile.is_empty() {
            self.vader5_profile = group_default_profile(ProfileGroup::FlydigiVader5Pro).to_string();
            changed = true;
        }
        changed
    }

    pub fn remembered_profile(&self, group: ProfileGroup) -> &str {
        let name = match group {
            ProfileGroup::DualSense => &self.dualsense_profile,
            ProfileGroup::Steam => &self.steam_profile,
            ProfileGroup::EightBitDo => &self.eightbitdo_profile,
            ProfileGroup::FlydigiApex5 => &self.flydigi_profile,
            ProfileGroup::FlydigiVader5Pro => &self.vader5_profile,
            ProfileGroup::GameSirTarantulaPro => &self.gamesir_profile,
        };
        if name.is_empty() {
            group_default_profile(group)
        } else {
            name
        }
    }

    pub fn remember_profile(&mut self, group: ProfileGroup, name: &str) {
        match group {
            ProfileGroup::DualSense => self.dualsense_profile = name.to_string(),
            ProfileGroup::Steam => self.steam_profile = name.to_string(),
            ProfileGroup::EightBitDo => self.eightbitdo_profile = name.to_string(),
            ProfileGroup::FlydigiApex5 => self.flydigi_profile = name.to_string(),
            ProfileGroup::FlydigiVader5Pro => self.vader5_profile = name.to_string(),
            ProfileGroup::GameSirTarantulaPro => self.gamesir_profile = name.to_string(),
        }
        self.profile = name.to_string();
    }
}

pub struct SettingsManager {
    path: PathBuf,
}

impl SettingsManager {
    pub fn new() -> Self {
        let path = crate::common::config_dir().join("settings.json");

        Self { path }
    }

    pub fn load(&self) -> Settings {
        let profile_manager = ProfileManager::new();
        profile_manager.migrate_eightbitdo_default_name();

        let Ok(json) = fs::read_to_string(&self.path) else {
            return Settings::default();
        };

        let mut settings: Settings = serde_json::from_str(&json).unwrap_or_default();
        let legacy_family = if settings.profile.is_empty() {
            None
        } else {
            profile_manager
                .load_profile(&settings.profile)
                .ok()
                .map(|profile| profile.device.family())
        };
        if settings.migrate_profile_memory(legacy_family) {
            self.save(&settings);
        }
        settings
    }

    pub fn save(&self, settings: &Settings) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let tmp = self
                .path
                .with_extension(format!("json.{}.tmp", std::process::id()));
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.path);
            } else {
                let _ = fs::remove_file(&tmp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_default_auto_start_daemon_to_true() {
        let parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":""}"#).expect("parse");
        assert!(parsed.auto_start_daemon);
    }

    #[test]
    fn old_settings_default_controller_preference_to_auto() {
        let parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":""}"#).expect("parse");
        assert_eq!(parsed.controller_preference, ControllerPreference::Auto);
    }

    #[test]
    fn controller_preference_round_trips() {
        for preference in [
            ControllerPreference::Auto,
            ControllerPreference::DualSense,
            ControllerPreference::Steam,
            ControllerPreference::EightBitDo,
            ControllerPreference::Flydigi,
            ControllerPreference::GameSir,
        ] {
            let json = serde_json::to_string(&preference).expect("serialize");
            let back: ControllerPreference = serde_json::from_str(&json).expect("parse");
            assert_eq!(back, preference);
        }
    }

    #[test]
    fn old_settings_without_family_slots_migrate() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":"Main"}"#).expect("parse");
        assert!(parsed.dualsense_profile.is_empty());
        assert!(parsed.steam_profile.is_empty());
        assert!(parsed.eightbitdo_profile.is_empty());
        assert!(parsed.flydigi_profile.is_empty());
        assert!(parsed.gamesir_profile.is_empty());

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::DualSense)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamDefault");
        assert_eq!(parsed.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(parsed.flydigi_profile, APEX5_DEFAULT_PROFILE);
        assert_eq!(parsed.gamesir_profile, TARANTULA_DEFAULT_PROFILE);
        assert_eq!(
            parsed.vader5_profile,
            crate::profiles::VADER5_DEFAULT_PROFILE
        );
        assert_eq!(parsed.profile, "Main");
        assert_eq!(parsed.remembered_profile(ProfileGroup::DualSense), "Main");
        assert_eq!(
            parsed.remembered_profile(ProfileGroup::Steam),
            "SteamDefault"
        );
        assert_eq!(
            parsed.remembered_profile(ProfileGroup::FlydigiApex5),
            APEX5_DEFAULT_PROFILE
        );
    }

    #[test]
    fn old_steam_profile_migrates_into_steam_slot() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":"TheFinals"}"#).expect("parse");

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::Steam)));

        assert_eq!(parsed.dualsense_profile, "Default");
        assert_eq!(parsed.steam_profile, "TheFinals");
        assert_eq!(parsed.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(parsed.flydigi_profile, APEX5_DEFAULT_PROFILE);
        assert_eq!(parsed.remembered_profile(ProfileGroup::Steam), "TheFinals");
    }

    #[test]
    fn old_eightbitdo_profile_migrates_into_eightbitdo_slot() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":"Pro3"}"#).expect("parse");

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::EightBitDo)));

        assert_eq!(parsed.dualsense_profile, "Default");
        assert_eq!(parsed.steam_profile, "SteamDefault");
        assert_eq!(parsed.eightbitdo_profile, "Pro3");
        assert_eq!(parsed.flydigi_profile, APEX5_DEFAULT_PROFILE);
        assert_eq!(parsed.remembered_profile(ProfileGroup::EightBitDo), "Pro3");
    }

    #[test]
    fn old_settings_with_empty_profile_use_family_defaults() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":""}"#).expect("parse");

        assert!(parsed.migrate_profile_memory(None));

        assert_eq!(
            parsed.remembered_profile(ProfileGroup::DualSense),
            "Default"
        );
        assert_eq!(
            parsed.remembered_profile(ProfileGroup::Steam),
            "SteamDefault"
        );
        assert_eq!(
            parsed.remembered_profile(ProfileGroup::EightBitDo),
            EIGHTBITDO_DEFAULT_PROFILE
        );
        assert_eq!(
            parsed.remembered_profile(ProfileGroup::FlydigiApex5),
            APEX5_DEFAULT_PROFILE
        );
        assert_eq!(
            parsed.remembered_profile(ProfileGroup::GameSirTarantulaPro),
            TARANTULA_DEFAULT_PROFILE
        );
    }

    #[test]
    fn migration_does_not_overwrite_existing_family_memory() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"Main","dualsense_profile":"Main","steam_profile":"SteamMain","eightbitdo_profile":"Pro3","flydigi_profile":"Apex","vader5_profile":"Vader","gamesir_profile":"Tarantula"}"#,
        )
        .expect("parse");

        assert!(!parsed.migrate_profile_memory(Some(ControllerFamily::Steam)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamMain");
        assert_eq!(parsed.eightbitdo_profile, "Pro3");
        assert_eq!(parsed.flydigi_profile, "Apex");
        assert_eq!(parsed.vader5_profile, "Vader");
        assert_eq!(parsed.gamesir_profile, "Tarantula");
    }

    #[test]
    fn migration_adds_missing_family_slots() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"Main","dualsense_profile":"Main","steam_profile":"SteamMain"}"#,
        )
        .expect("parse");

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::DualSense)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamMain");
        assert_eq!(parsed.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(parsed.flydigi_profile, APEX5_DEFAULT_PROFILE);
        assert_eq!(parsed.gamesir_profile, TARANTULA_DEFAULT_PROFILE);
        assert_eq!(
            parsed.vader5_profile,
            crate::profiles::VADER5_DEFAULT_PROFILE
        );
    }

    #[test]
    fn legacy_eightbitdo_default_name_is_renamed_and_saved() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"EightBitDoDefault","eightbitdo_profile":"EightBitDoDefault"}"#,
        )
        .expect("parse");
        assert_eq!(parsed.eightbitdo_profile, LEGACY_EIGHTBITDO_DEFAULT_PROFILE);
        assert!(parsed.vader5_profile.is_empty());

        assert!(parsed.migrate_profile_memory(None));

        assert_eq!(parsed.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(parsed.flydigi_profile, APEX5_DEFAULT_PROFILE);
        assert_eq!(parsed.profile, EIGHTBITDO_DEFAULT_PROFILE);
        assert!(!parsed.migrate_profile_memory(None));

        let path = std::env::temp_dir().join(format!(
            "gyro4u-settings-test-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        SettingsManager { path: path.clone() }.save(&parsed);
        let saved: Settings =
            serde_json::from_str(&fs::read_to_string(&path).expect("read saved settings"))
                .expect("parse saved settings");
        assert_eq!(saved.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(saved.profile, EIGHTBITDO_DEFAULT_PROFILE);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn per_group_profile_memory_is_independent() {
        let mut settings = Settings::default();
        settings.remember_profile(ProfileGroup::DualSense, "Main");
        settings.remember_profile(ProfileGroup::Steam, "SteamMain");

        assert_eq!(settings.remembered_profile(ProfileGroup::DualSense), "Main");
        assert_eq!(
            settings.remembered_profile(ProfileGroup::Steam),
            "SteamMain"
        );
        assert_eq!(settings.profile, "SteamMain");

        settings.remember_profile(ProfileGroup::EightBitDo, "Pro3");
        settings.remember_profile(ProfileGroup::FlydigiApex5, "Apex");
        settings.remember_profile(ProfileGroup::FlydigiVader5Pro, "Vader");
        assert_eq!(settings.remembered_profile(ProfileGroup::DualSense), "Main");
        assert_eq!(
            settings.remembered_profile(ProfileGroup::Steam),
            "SteamMain"
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::EightBitDo),
            "Pro3"
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::FlydigiApex5),
            "Apex"
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::FlydigiVader5Pro),
            "Vader"
        );
        assert_eq!(settings.profile, "Vader");
    }

    #[test]
    fn new_settings_have_group_defaults() {
        let settings = Settings::default();
        assert_eq!(
            settings.remembered_profile(ProfileGroup::DualSense),
            "Default"
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::Steam),
            "SteamDefault"
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::EightBitDo),
            EIGHTBITDO_DEFAULT_PROFILE
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::FlydigiApex5),
            APEX5_DEFAULT_PROFILE
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::FlydigiVader5Pro),
            crate::profiles::VADER5_DEFAULT_PROFILE
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::GameSirTarantulaPro),
            TARANTULA_DEFAULT_PROFILE
        );
    }

    #[test]
    fn old_settings_fill_vader_slot_without_touching_flydigi() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"Apex5Default","flydigi_profile":"Apex5Default"}"#,
        )
        .expect("parse");
        assert!(parsed.vader5_profile.is_empty());

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::Flydigi)));

        assert_eq!(parsed.flydigi_profile, APEX5_DEFAULT_PROFILE);
        assert_eq!(parsed.gamesir_profile, TARANTULA_DEFAULT_PROFILE);
        assert_eq!(
            parsed.vader5_profile,
            crate::profiles::VADER5_DEFAULT_PROFILE
        );
        assert_eq!(parsed.profile, APEX5_DEFAULT_PROFILE);
    }

    #[test]
    fn apex_and_vader_profile_memory_is_independent() {
        let mut settings = Settings::default();
        settings.remember_profile(ProfileGroup::FlydigiApex5, "Apex");
        settings.remember_profile(ProfileGroup::FlydigiVader5Pro, "Vader");

        assert_eq!(
            settings.remembered_profile(ProfileGroup::FlydigiApex5),
            "Apex"
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::FlydigiVader5Pro),
            "Vader"
        );
        assert_eq!(settings.flydigi_profile, "Apex");
        assert_eq!(settings.vader5_profile, "Vader");
    }

    #[test]
    fn dualsense_edge_uses_the_dualsense_slot() {
        let mut settings = Settings::default();
        settings.remember_profile(
            crate::inputs::ControllerKind::DualSenseEdge.profile_group(),
            "EdgeMain",
        );

        assert_eq!(settings.dualsense_profile, "EdgeMain");
        assert_eq!(
            settings.remembered_profile(crate::inputs::ControllerKind::DualSense.profile_group()),
            "EdgeMain"
        );
    }

    #[test]
    fn settings_with_family_slots_round_trip() {
        let mut settings = Settings {
            theme_id: "noctalia".into(),
            ..Settings::default()
        };
        settings.remember_profile(ProfileGroup::DualSense, "Main");
        settings.remember_profile(ProfileGroup::Steam, "TheFinals");
        settings.remember_profile(ProfileGroup::EightBitDo, "Pro3");
        settings.remember_profile(ProfileGroup::FlydigiApex5, "Apex");
        settings.remember_profile(ProfileGroup::FlydigiVader5Pro, "Vader");
        settings.remember_profile(ProfileGroup::GameSirTarantulaPro, "Tarantula");

        let json = serde_json::to_string(&settings).expect("serialize");
        let back: Settings = serde_json::from_str(&json).expect("parse");

        assert_eq!(back.remembered_profile(ProfileGroup::DualSense), "Main");
        assert_eq!(back.remembered_profile(ProfileGroup::Steam), "TheFinals");
        assert_eq!(back.remembered_profile(ProfileGroup::EightBitDo), "Pro3");
        assert_eq!(back.remembered_profile(ProfileGroup::FlydigiApex5), "Apex");
        assert_eq!(
            back.remembered_profile(ProfileGroup::FlydigiVader5Pro),
            "Vader"
        );
        assert_eq!(
            back.remembered_profile(ProfileGroup::GameSirTarantulaPro),
            "Tarantula"
        );
        assert_eq!(back.vader5_profile, "Vader");
        assert_eq!(back.gamesir_profile, "Tarantula");
        assert_eq!(back.profile, "Tarantula");
        assert_eq!(back.controller_preference, ControllerPreference::Auto);
    }

    #[test]
    fn gamesir_profile_memory_is_independent() {
        let mut settings = Settings::default();
        settings.remember_profile(ProfileGroup::GameSirTarantulaPro, "Tarantula");

        assert_eq!(
            settings.remembered_profile(ProfileGroup::GameSirTarantulaPro),
            "Tarantula"
        );
        assert_eq!(
            settings.remembered_profile(ProfileGroup::FlydigiApex5),
            APEX5_DEFAULT_PROFILE
        );
        assert_eq!(settings.gamesir_profile, "Tarantula");
        assert_eq!(settings.profile, "Tarantula");
    }

    #[test]
    fn preference_labels_are_user_facing() {
        assert_eq!(ControllerPreference::Auto.label(), "Automatic");
        assert_eq!(ControllerPreference::Steam.label(), "Steam Controller");
        assert_eq!(ControllerPreference::EightBitDo.label(), "8BitDo");
        assert_eq!(ControllerPreference::Flydigi.label(), "Flydigi");
        assert_eq!(ControllerPreference::GameSir.label(), "GameSir");
    }
}
