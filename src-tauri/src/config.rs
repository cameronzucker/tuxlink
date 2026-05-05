use serde::{Deserialize, Deserializer, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u32,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub callsign: String,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub grid_square: String,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub pat_mbo_address: String,
    pub winlink_password_present: bool,
    pub wizard_completed: bool,
}

fn deserialize_schema_version<'de, D>(d: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let v = u32::deserialize(d)?;
    if v != CONFIG_SCHEMA_VERSION {
        return Err(serde::de::Error::custom(format!(
            "unsupported config schema_version {} (expected {})",
            v, CONFIG_SCHEMA_VERSION
        )));
    }
    Ok(v)
}

fn deserialize_nonempty_string<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    if s.is_empty() {
        return Err(serde::de::Error::custom("field must not be empty"));
    }
    Ok(s)
}

/// Resolve the config file path. Honors XDG_CONFIG_HOME, falls back to
/// ~/.config/tuxlink/config.json.
pub fn config_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").expect("HOME must be set");
            std::path::PathBuf::from(home).join(".config")
        });
    base.join("tuxlink").join("config.json")
}
