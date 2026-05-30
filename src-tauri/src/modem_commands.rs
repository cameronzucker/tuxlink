//! Tauri commands for modem (ARDOP) operations.
//!
//! RADIO-1: `modem_ardop_connect` requires a per-session consent token issued
//! by the frontend's RADIO-1 modal. The backend rejects any connect attempt
//! whose token doesn't match the current session token. See Phase 6.

use crate::config::{self, ArdopUiConfig};

/// Return the persisted ARDOP configuration, or the struct default if nothing
/// has been written yet (first run) or the config file is absent.
#[tauri::command]
pub fn config_get_ardop() -> ArdopUiConfig {
    config::read_config()
        .map(|cfg| cfg.modem_ardop.unwrap_or_default())
        .unwrap_or_default()
}

/// Persist a new ARDOP configuration. Reads the current config, replaces
/// `modem_ardop`, and writes atomically. Returns an error if the config file
/// cannot be read (e.g. wizard has not been completed) or the write fails.
#[tauri::command]
pub fn config_set_ardop(value: ArdopUiConfig) -> Result<(), String> {
    let mut cfg = config::read_config().map_err(|e| format!("read failed: {e}"))?;
    cfg.modem_ardop = Some(value);
    config::write_config_atomic(&cfg).map_err(|e| format!("save failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONFIG_SCHEMA_VERSION;

    #[test]
    fn round_trip_persists_through_config() {
        // Isolate this test from the operator's real config by pointing
        // TUXLINK_CONFIG_DIR at a fresh tempdir. `config_path()` will resolve
        // to `<tmpdir>/config.json` (per config.rs §294).
        //
        // Because `config_set_ardop` calls `read_config()` before writing, the
        // config file must exist first. We pre-seed a minimal valid config that
        // satisfies `deny_unknown_fields` + semantic validation (offline path:
        // no callsign). `config_set_ardop` will then read it, inject `modem_ardop`,
        // and write it back atomically.
        //
        // NOTE: std::env::set_var is not thread-safe under parallel test
        // execution. This test must run serially (--test-threads=1 or via the
        // `modem_commands::tests` filter). The existing `config.rs` tests avoid
        // this race by using pure serde deserialization; this test exercises the
        // file I/O path, so TUXLINK_CONFIG_DIR isolation is the correct approach.
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded test; no concurrent env reads within this block.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        // Seed a minimal valid config (offline path: connect_to_cms=false, no callsign).
        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let initial = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
        };
        config_set_ardop(initial.clone()).expect("config_set_ardop must succeed");
        let read = config_get_ardop();
        assert_eq!(read, initial);

        // Restore env (best-effort).
        // SAFETY: symmetric with the set_var above; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }
}
