use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControllerPreference {
    #[default]
    Auto,
    DualSense,
    Steam,
}

impl ControllerPreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::DualSense => "DualSense",
            Self::Steam => "Steam Controller",
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
            auto_start_daemon: true,
            controller_preference: ControllerPreference::Auto,
        }
    }
}

impl Settings {
    /// Fill per-family profile slots from the legacy single `profile` field.
    /// Returns true when something changed and the settings need saving.
    pub fn migrate_profile_memory(&mut self, legacy_is_steam: Option<bool>) -> bool {
        if self.dualsense_profile.is_empty() && self.steam_profile.is_empty() {
            let is_steam = legacy_is_steam.unwrap_or(false);
            if is_steam {
                self.steam_profile = if self.profile.is_empty() {
                    "SteamDefault".into()
                } else {
                    self.profile.clone()
                };
                self.dualsense_profile = "Default".into();
            } else {
                self.dualsense_profile = if self.profile.is_empty() {
                    "Default".into()
                } else {
                    self.profile.clone()
                };
                self.steam_profile = "SteamDefault".into();
            }
            return true;
        }

        let mut changed = false;
        if self.dualsense_profile.is_empty() {
            self.dualsense_profile = if legacy_is_steam == Some(false) && !self.profile.is_empty() {
                self.profile.clone()
            } else {
                "Default".into()
            };
            changed = true;
        }
        if self.steam_profile.is_empty() {
            self.steam_profile = if legacy_is_steam == Some(true) && !self.profile.is_empty() {
                self.profile.clone()
            } else {
                "SteamDefault".into()
            };
            changed = true;
        }
        changed
    }

    pub fn remembered_profile(&self, is_steam: bool) -> &str {
        let name = if is_steam {
            &self.steam_profile
        } else {
            &self.dualsense_profile
        };
        if name.is_empty() {
            if is_steam { "SteamDefault" } else { "Default" }
        } else {
            name
        }
    }

    pub fn remember_profile(&mut self, is_steam: bool, name: &str) {
        if is_steam {
            self.steam_profile = name.to_string();
        } else {
            self.dualsense_profile = name.to_string();
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
        let Ok(json) = fs::read_to_string(&self.path) else {
            return Settings::default();
        };

        let mut settings: Settings = serde_json::from_str(&json).unwrap_or_default();
        let legacy_is_steam = if settings.profile.is_empty() {
            None
        } else {
            crate::profiles::ProfileManager::new()
                .load_profile(&settings.profile)
                .ok()
                .map(|profile| profile.device.is_steam())
        };
        if settings.migrate_profile_memory(legacy_is_steam) {
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

        assert!(parsed.migrate_profile_memory(Some(false)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamDefault");
        assert_eq!(parsed.profile, "Main");
        assert_eq!(parsed.remembered_profile(false), "Main");
        assert_eq!(parsed.remembered_profile(true), "SteamDefault");
    }

    #[test]
    fn old_steam_profile_migrates_into_steam_slot() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":"TheFinals"}"#).expect("parse");

        assert!(parsed.migrate_profile_memory(Some(true)));

        assert_eq!(parsed.dualsense_profile, "Default");
        assert_eq!(parsed.steam_profile, "TheFinals");
        assert_eq!(parsed.remembered_profile(true), "TheFinals");
    }

    #[test]
    fn old_settings_with_empty_profile_use_family_defaults() {
        let mut parsed: Settings =
            serde_json::from_str(r#"{"theme_id":"default","profile":""}"#).expect("parse");

        assert!(parsed.migrate_profile_memory(None));

        assert_eq!(parsed.remembered_profile(false), "Default");
        assert_eq!(parsed.remembered_profile(true), "SteamDefault");
    }

    #[test]
    fn migration_does_not_overwrite_existing_family_memory() {
        let mut parsed: Settings = serde_json::from_str(
            r#"{"theme_id":"default","profile":"Main","dualsense_profile":"Main","steam_profile":"SteamMain"}"#,
        )
        .expect("parse");

        assert!(!parsed.migrate_profile_memory(Some(true)));

        assert_eq!(parsed.dualsense_profile, "Main");
        assert_eq!(parsed.steam_profile, "SteamMain");
    }

    #[test]
    fn per_family_profile_memory_is_independent() {
        let mut settings = Settings::default();
        settings.remember_profile(false, "Main");
        settings.remember_profile(true, "SteamMain");

        assert_eq!(settings.remembered_profile(false), "Main");
        assert_eq!(settings.remembered_profile(true), "SteamMain");
        assert_eq!(settings.profile, "SteamMain");

        settings.remember_profile(false, "Other");
        assert_eq!(settings.remembered_profile(false), "Other");
        assert_eq!(settings.remembered_profile(true), "SteamMain");
    }

    #[test]
    fn new_settings_have_family_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.remembered_profile(false), "Default");
        assert_eq!(settings.remembered_profile(true), "SteamDefault");
    }

    #[test]
    fn settings_with_family_slots_round_trip() {
        let mut settings = Settings {
            theme_id: "noctalia".into(),
            ..Settings::default()
        };
        settings.remember_profile(false, "Main");
        settings.remember_profile(true, "TheFinals");

        let json = serde_json::to_string(&settings).expect("serialize");
        let back: Settings = serde_json::from_str(&json).expect("parse");

        assert_eq!(back.remembered_profile(false), "Main");
        assert_eq!(back.remembered_profile(true), "TheFinals");
        assert_eq!(back.profile, "TheFinals");
        assert_eq!(back.controller_preference, ControllerPreference::Auto);
    }

    #[test]
    fn preference_labels_are_user_facing() {
        assert_eq!(ControllerPreference::Auto.label(), "Automatic");
        assert_eq!(ControllerPreference::Steam.label(), "Steam Controller");
    }
}
