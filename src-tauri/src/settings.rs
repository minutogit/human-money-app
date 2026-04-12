// src-tauri/src/settings.rs
use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "app_settings";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AppSettings {
    pub bundle_retention_days: u64,
    // NEU: 0 = Immer fragen, >0 = Session Dauer in Sekunden
    pub session_timeout_seconds: u64,
    pub last_used_directory: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            bundle_retention_days: 30,
            session_timeout_seconds: 600, // Standard: 10 Minuten
            last_used_directory: None,
        }
    }
}