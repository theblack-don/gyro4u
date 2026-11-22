use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    inputs::ControllerFamily,
    profiles::{EIGHTBITDO_DEFAULT_PROFILE, LEGACY_EIGHTBITDO_DEFAULT_PROFILE, ProfileManager},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControllerPreference {
    #[default]
    Auto,
    DualSense,
    Steam,
    EightBitDo,
}

impl ControllerPreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::DualSense => "DualSense",
            Self::Steam => "Steam Controller",
            Self::EightBitDo => "8BitDo",
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
            auto_start_daemon: true,
            controller_preference: ControllerPreference::Auto,
        }
    }
}

fn family_default_profile(family: ControllerFamily) -> &'static str {
    match family {
        ControllerFamily::DualSense => "Default",
        ControllerFamily::Steam => "SteamDefault",
        ControllerFamily::EightBitDo => EIGHTBITDO_DEFAULT_PROFILE,
    }
}

impl Settings {
    /// Fill per-family profile slots from the legacy single `profile` field.
    /// Returns true when something changed and the settings need saving.
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
        ] {
            let slot = match family {
                ControllerFamily::DualSense => &mut self.dualsense_profile,
                ControllerFamily::Steam => &mut self.steam_profile,
                ControllerFamily::EightBitDo => &mut self.eightbitdo_profile,
            };
            if slot.is_empty() {
                *slot = if legacy_family == Some(family) && !self.profile.is_empty() {
                    self.profile.clone()
                } else {
                    family_default_profile(family).to_string()
                };
                changed = true;
            }
        }
        changed
    }

    pub fn remembered_profile(&self, family: ControllerFamily) -> &str {
        let name = match family {
            ControllerFamily::DualSense => &self.dualsense_profile,
            ControllerFamily::Steam => &self.steam_profile,
            ControllerFamily::EightBitDo => &self.eightbitdo_profile,
        };
        if name.is_empty() {
            family_default_profile(family)
        } else {
            name
        }
    }

    pub fn remember_profile(&mut self, family: ControllerFamily, name: &str) {
        match family {
            ControllerFamily::DualSense => self.dualsense_profile = name.to_string(),
            ControllerFamily::Steam => self.steam_profile = name.to_string(),
            ControllerFamily::EightBitDo => self.eightbitdo_profile = name.to_string(),
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

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::DualSense)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamDefault");
        assert_eq!(parsed.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
        assert_eq!(parsed.profile, "Main");
        assert_eq!(
            parsed.remembered_profile(ControllerFamily::DualSense),
            "Main"
        );
        assert_eq!(
            parsed.remembered_profile(ControllerFamily::Steam),
            "SteamDefault"
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
        assert_eq!(
            parsed.remembered_profile(ControllerFamily::Steam),
            "TheFinals"
        );
    }

    #[test]
    fn old_eightbitdo_profile_migrates_into_eightbitdo_slot() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":"Pro3"}"#).expect("parse");

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::EightBitDo)));

        assert_eq!(parsed.dualsense_profile, "Default");
        assert_eq!(parsed.steam_profile, "SteamDefault");
        assert_eq!(parsed.eightbitdo_profile, "Pro3");
        assert_eq!(
            parsed.remembered_profile(ControllerFamily::EightBitDo),
            "Pro3"
        );
    }

    #[test]
    fn old_settings_with_empty_profile_use_family_defaults() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":""}"#).expect("parse");

        assert!(parsed.migrate_profile_memory(None));

        assert_eq!(
            parsed.remembered_profile(ControllerFamily::DualSense),
            "Default"
        );
        assert_eq!(
            parsed.remembered_profile(ControllerFamily::Steam),
            "SteamDefault"
        );
        assert_eq!(
            parsed.remembered_profile(ControllerFamily::EightBitDo),
            EIGHTBITDO_DEFAULT_PROFILE
        );
    }

    #[test]
    fn migration_does_not_overwrite_existing_family_memory() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"Main","dualsense_profile":"Main","steam_profile":"SteamMain","eightbitdo_profile":"Pro3"}"#,
        )
        .expect("parse");

        assert!(!parsed.migrate_profile_memory(Some(ControllerFamily::Steam)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamMain");
        assert_eq!(parsed.eightbitdo_profile, "Pro3");
    }

    #[test]
    fn migration_adds_missing_eightbitdo_slot_only() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"Main","dualsense_profile":"Main","steam_profile":"SteamMain"}"#,
        )
        .expect("parse");

        assert!(parsed.migrate_profile_memory(Some(ControllerFamily::DualSense)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamMain");
        assert_eq!(parsed.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
    }

    #[test]
    fn legacy_eightbitdo_default_name_is_renamed_and_saved() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"EightBitDoDefault","eightbitdo_profile":"EightBitDoDefault"}"#,
        )
        .expect("parse");
        assert_eq!(parsed.eightbitdo_profile, LEGACY_EIGHTBITDO_DEFAULT_PROFILE);

        assert!(parsed.migrate_profile_memory(None));

        assert_eq!(parsed.eightbitdo_profile, EIGHTBITDO_DEFAULT_PROFILE);
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
    fn per_family_profile_memory_is_independent() {
        let mut settings = Settings::default();
        settings.remember_profile(ControllerFamily::DualSense, "Main");
        settings.remember_profile(ControllerFamily::Steam, "SteamMain");

        assert_eq!(
            settings.remembered_profile(ControllerFamily::DualSense),
            "Main"
        );
        assert_eq!(
            settings.remembered_profile(ControllerFamily::Steam),
            "SteamMain"
        );
        assert_eq!(settings.profile, "SteamMain");

        settings.remember_profile(ControllerFamily::EightBitDo, "Pro3");
        assert_eq!(
            settings.remembered_profile(ControllerFamily::DualSense),
            "Main"
        );
        assert_eq!(
            settings.remembered_profile(ControllerFamily::Steam),
            "SteamMain"
        );
        assert_eq!(
            settings.remembered_profile(ControllerFamily::EightBitDo),
            "Pro3"
        );
        assert_eq!(settings.profile, "Pro3");
    }

    #[test]
    fn new_settings_have_family_defaults() {
        let settings = Settings::default();
        assert_eq!(
            settings.remembered_profile(ControllerFamily::DualSense),
            "Default"
        );
        assert_eq!(
            settings.remembered_profile(ControllerFamily::Steam),
            "SteamDefault"
        );
        assert_eq!(
            settings.remembered_profile(ControllerFamily::EightBitDo),
            EIGHTBITDO_DEFAULT_PROFILE
        );
    }

    #[test]
    fn settings_with_family_slots_round_trip() {
        let mut settings = Settings {
            theme_id: "noctalia".into(),
            ..Settings::default()
        };
        settings.remember_profile(ControllerFamily::DualSense, "Main");
        settings.remember_profile(ControllerFamily::Steam, "TheFinals");
        settings.remember_profile(ControllerFamily::EightBitDo, "Pro3");

        let json = serde_json::to_string(&settings).expect("serialize");
        let back: Settings = serde_json::from_str(&json).expect("parse");

        assert_eq!(back.remembered_profile(ControllerFamily::DualSense), "Main");
        assert_eq!(
            back.remembered_profile(ControllerFamily::Steam),
            "TheFinals"
        );
        assert_eq!(
            back.remembered_profile(ControllerFamily::EightBitDo),
            "Pro3"
        );
        assert_eq!(back.profile, "Pro3");
        assert_eq!(back.controller_preference, ControllerPreference::Auto);
    }

    #[test]
    fn preference_labels_are_user_facing() {
        assert_eq!(ControllerPreference::Auto.label(), "Automatic");
        assert_eq!(ControllerPreference::Steam.label(), "Steam Controller");
        assert_eq!(ControllerPreference::EightBitDo.label(), "8BitDo");
    }
}
