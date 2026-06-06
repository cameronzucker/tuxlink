//! Keyring environment probe — Secret Service / KWallet / KeePassXC /
//! Flatpak portal detection (spec §9.3).
//!
//! RADIO-1: read-only. No keyring writes; only state queries.

use crate::logging::env_probes::{run_with_deadline, safe_env_value, ProbeGate, ProbeSnapshot};
use chrono::Utc;
use serde_json::json;
use std::path::Path;

pub static GATE: ProbeGate = ProbeGate::new();

pub fn run(trigger: &str) -> ProbeSnapshot {
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let dbus_addr = safe_env_value("DBUS_SESSION_BUS_ADDRESS");
    let xdg_runtime = safe_env_value("XDG_RUNTIME_DIR");
    let home = safe_env_value("HOME").unwrap_or_default();

    let dbus_reachable = dbus_addr.is_some()
        && run_with_deadline(
            "dbus-send",
            &[
                "--session",
                "--print-reply",
                "--dest=org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus.ListNames",
            ],
        )
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    let gnome_keyring_active = systemd_active_user("gnome-keyring-daemon.service");
    let kwallet_active = systemd_active_user("kwalletd5.service")
        || systemd_active_user("kwalletd6.service");
    let keepassxc_running = process_running("keepassxc");
    let secret_service_owner = dbus_owner_of("org.freedesktop.secrets");

    let keyrings_dir = format!("{}/.local/share/keyrings", home);
    let keyrings_exists = Path::new(&keyrings_dir).exists();
    let login_keyring_exists =
        Path::new(&format!("{}/login.keyring", keyrings_dir)).exists();

    let result = json!({
        "trigger": trigger,
        "compile_features": "sync-secret-service+crypto-rust",
        "dbus_session_bus_address_set": dbus_addr.is_some(),
        "dbus_session_bus_reachable": dbus_reachable,
        "xdg_runtime_dir": xdg_runtime,
        "secret_service_owner": secret_service_owner,
        "gnome_keyring_daemon_systemd_active": gnome_keyring_active,
        "kwallet_systemd_active": kwallet_active,
        "keepassxc_running": keepassxc_running,
        "keyrings_dir_exists": keyrings_exists,
        "login_keyring_file_exists": login_keyring_exists,
    });

    ProbeSnapshot {
        probe: "keyring".into(),
        timestamp,
        trigger: trigger.into(),
        result,
    }
}

fn systemd_active_user(unit: &str) -> bool {
    run_with_deadline("systemctl", &["--user", "is-active", unit])
        .map(|s| s.trim() == "active")
        .unwrap_or(false)
}

fn process_running(name: &str) -> bool {
    run_with_deadline("pgrep", &["-x", name])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

fn dbus_owner_of(service: &str) -> Option<String> {
    let service_arg = format!("string:{service}");
    let out = run_with_deadline(
        "dbus-send",
        &[
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.GetNameOwner",
            &service_arg,
        ],
    )?;
    if out.contains("ServiceUnknown") || out.trim().is_empty() {
        None
    } else {
        out.lines()
            .find_map(|l| l.trim().strip_prefix("string \""))
            .map(|s| s.trim_end_matches('"').to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_produces_snapshot_with_expected_fields() {
        let snap = run("startup");
        assert_eq!(snap.probe, "keyring");
        let r = &snap.result;
        assert!(r.get("dbus_session_bus_address_set").is_some());
        assert!(r.get("gnome_keyring_daemon_systemd_active").is_some());
        assert!(r.get("kwallet_systemd_active").is_some());
    }
}
