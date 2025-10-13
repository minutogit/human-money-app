// src-tauri/src/settings.rs
use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "app_settings";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    pub bundle_retention_days: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            // Standard-Aufbewahrungsfrist: 30 Tage.
            // Ein guter Kompromiss zwischen Verfügbarkeit und Speicherplatz.
            bundle_retention_days: 30,
        }
    }
}