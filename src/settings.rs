use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme_id: String,
    pub profile: String,
    #[serde(default = "default_true")]
    pub auto_start_daemon: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_id: "default".into(),
            profile: String::new(),
            auto_start_daemon: true,
        }
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

        serde_json::from_str(&json).unwrap_or_default()
    }

    pub fn save(&self, settings: &Settings) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = fs::write(&self.path, json);
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
}
