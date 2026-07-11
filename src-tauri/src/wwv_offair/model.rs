//! STT model file resolution. Runtime is off-air; the model is provisioned at
//! setup (Task 17) into the data dir, or pointed at via config.
use std::path::PathBuf;

pub const MODEL_FILENAME: &str = "ggml-base.en-q5_1.bin";

/// `$XDG_DATA_HOME/tuxlink/models` or `$HOME/.local/share/tuxlink/models`.
fn models_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tuxlink/models")
}

/// Resolve the STT model path: config override → default data-dir file →
/// `Err(hint)`. Never downloads (runtime is off-air).
pub fn resolve_model_path(cfg: &crate::config::Config) -> Result<PathBuf, String> {
    if let Some(p) = cfg.wwv_offair.as_ref().and_then(|w| w.model_path.clone()) {
        let pb = PathBuf::from(p);
        return if pb.is_file() {
            Ok(pb)
        } else {
            Err(format!("configured STT model not found: {}", pb.display()))
        };
    }
    let base = models_dir().join(MODEL_FILENAME);
    if base.is_file() {
        Ok(base)
    } else {
        Err(format!(
            "STT model not installed. Download to {} (see setup) or set wwv_offair.model_path.",
            base.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hermetic Config construction: `config.rs` has NO `Config::default()` and its
    // `config_json` helper is private to its own test module, so build a minimal
    // valid Config from inline JSON (mirrors config.rs's own fixture) with
    // wwv_offair.model_path embedded. `CONFIG_SCHEMA_VERSION` is `pub`.
    fn cfg_with_model(model_path: Option<&str>) -> crate::config::Config {
        let mp = match model_path {
            Some(p) => format!("\"{p}\""),
            None => "null".to_string(),
        };
        let json = format!(
            r#"{{
                "schema_version": {},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }},
                "wwv_offair": {{ "capture_device": "", "model_path": {mp}, "auto_retry_next_window": true }}
            }}"#,
            crate::config::CONFIG_SCHEMA_VERSION
        );
        serde_json::from_str(&json).expect("minimal config with wwv_offair parses")
    }

    #[test]
    fn override_to_real_file_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("m.bin");
        std::fs::write(&f, b"x").unwrap();
        let cfg = cfg_with_model(Some(&f.to_string_lossy()));
        assert_eq!(resolve_model_path(&cfg).unwrap(), f);
    }

    #[test]
    fn missing_override_errors_with_hint() {
        let cfg = cfg_with_model(Some("/no/such/model.bin"));
        assert!(resolve_model_path(&cfg).unwrap_err().contains("not found"));
    }
}
