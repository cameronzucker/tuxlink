//! TOML-backed persistence for Detailed-mode + retention values (spec §4.3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DetailedMode {
    Off,
    On,
    Bounded { expires_at: DateTime<Utc> },
}

impl Default for DetailedMode {
    fn default() -> Self {
        DetailedMode::Off
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub detailed_mode: DetailedMode,
    pub retention_days: u32,
    pub retention_mb_cap: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            detailed_mode: DetailedMode::Off,
            retention_days: 14,
            retention_mb_cap: 500,
        }
    }
}

pub fn settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("tuxlink").join("logging.toml")
}

pub fn load() -> Settings {
    let path = settings_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create_dir_all: {e}"))?;
    }
    let toml_str =
        toml::to_string_pretty(settings).map_err(|e| format!("toml serialize: {e}"))?;
    std::fs::write(&path, toml_str).map_err(|e| format!("write {path:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let s = Settings {
            detailed_mode: DetailedMode::Bounded {
                expires_at: chrono::Utc::now() + chrono::Duration::hours(4),
            },
            retention_days: 30,
            retention_mb_cap: 1024,
        };
        let toml_str = toml::to_string(&s).unwrap();
        let s2: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(s2.retention_days, 30);
        assert_eq!(s2.retention_mb_cap, 1024);
        assert!(matches!(s2.detailed_mode, DetailedMode::Bounded { .. }));
    }
}
