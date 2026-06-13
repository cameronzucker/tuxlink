//! GPS "Fix it for me" Tauri commands (tuxlink-m9ej). Invoke the privileged
//! `tuxlink-gps-fix` helper via `pkexec`, mapping exit codes to a small outcome
//! enum the frontend can branch on. The ACTION TOKEN comes from a fixed match —
//! operator-supplied text can never reach the helper's argv.

use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GpsFixOutcome {
    /// Helper ran and succeeded.
    Ok,
    /// The PolicyKit auth dialog was dismissed/cancelled (pkexec exit 126).
    AuthDismissed,
    /// pkexec is not installed / not authorized (exit 127, or spawn failure).
    PkexecMissing,
    /// Helper ran but the underlying command failed.
    Failed,
}

/// Pure exit-code → outcome mapping (the unit-tested core).
pub fn classify_exit(code: Option<i32>) -> GpsFixOutcome {
    match code {
        Some(0) => GpsFixOutcome::Ok,
        Some(126) => GpsFixOutcome::AuthDismissed,
        Some(127) => GpsFixOutcome::PkexecMissing,
        _ => GpsFixOutcome::Failed,
    }
}

/// Map the operator-facing action string to the fixed helper token. Returns
/// `None` for anything outside the allowlist so operator text can never reach
/// the helper's argv.
fn action_token(action: &str) -> Option<&'static str> {
    match action {
        "add-dialout" => Some("add-dialout"),
        "mask-modemmanager" => Some("mask-modemmanager"),
        "unmask-modemmanager" => Some("unmask-modemmanager"),
        _ => None,
    }
}

const PKEXEC_PATHS: [&str; 3] = ["/usr/bin/pkexec", "/usr/local/bin/pkexec", "/bin/pkexec"];

fn which_pkexec() -> Option<&'static str> {
    PKEXEC_PATHS.iter().copied().find(|p| std::path::Path::new(p).exists())
}

/// Run a fixed GPS-fix action through pkexec + the privileged helper.
#[tauri::command]
pub async fn gps_run_fix(action: String) -> Result<GpsFixOutcome, crate::ui_commands::UiError> {
    let token = action_token(&action).ok_or_else(|| crate::ui_commands::UiError::Internal {
        detail: format!("unknown gps fix action: {action}"),
    })?;
    let Some(pkexec) = which_pkexec() else {
        return Ok(GpsFixOutcome::PkexecMissing);
    };
    match std::process::Command::new(pkexec)
        .arg("/usr/libexec/tuxlink-gps-fix")
        .arg(token)
        .status()
    {
        Ok(s) => Ok(classify_exit(s.code())),
        Err(_) => Ok(GpsFixOutcome::PkexecMissing),
    }
}

/// Same safe-device-path shape the helper enforces (defense-in-depth; the device
/// comes from our own probe, never operator free-text, but validate anyway).
fn is_safe_device_path(p: &str) -> bool {
    if p.is_empty() || p.len() > 256 || !p.starts_with("/dev/") || p.contains("..") {
        return false;
    }
    if !p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | ':')) {
        return false;
    }
    p.starts_with("/dev/ttyACM")
        || p.starts_with("/dev/ttyUSB")
        || p.starts_with("/dev/ttyAMA")
        || p.starts_with("/dev/ttyS")
        || p.starts_with("/dev/serial/by-id/")
        || p.starts_with("/dev/gps")
}

/// One-click gpsd setup (tuxlink-n399): install + configure + enable in a single
/// privileged run (one pkexec prompt). `device` (optional) is the detected serial
/// device to pin in /etc/default/gpsd; omit for USB-hotplug-only.
#[tauri::command]
pub async fn gps_setup_gpsd(device: Option<String>) -> Result<GpsFixOutcome, crate::ui_commands::UiError> {
    if let Some(d) = device.as_deref() {
        if !is_safe_device_path(d) {
            return Err(crate::ui_commands::UiError::Rejected(
                "refusing to configure gpsd with an unsafe device path".into(),
            ));
        }
    }
    let Some(pkexec) = which_pkexec() else {
        return Ok(GpsFixOutcome::PkexecMissing);
    };
    let mut cmd = std::process::Command::new(pkexec);
    cmd.arg("/usr/libexec/tuxlink-gps-fix").arg("setup-gpsd");
    if let Some(d) = device.as_deref() {
        cmd.arg(d);
    }
    match cmd.status() {
        Ok(s) => Ok(classify_exit(s.code())),
        Err(_) => Ok(GpsFixOutcome::PkexecMissing),
    }
}

/// The system package manager, if one of the known ones is present — drives
/// whether the UI offers one-click ("apt") or copy-paste guidance.
#[tauri::command]
pub async fn gps_pkg_manager() -> Result<Option<String>, crate::ui_commands::UiError> {
    let candidates: [(&str, &[&str]); 3] = [
        ("apt", &["/usr/bin/apt-get", "/bin/apt-get"]),
        ("dnf", &["/usr/bin/dnf", "/bin/dnf"]),
        ("pacman", &["/usr/bin/pacman", "/bin/pacman"]),
    ];
    Ok(candidates
        .iter()
        .find(|(_, paths)| paths.iter().any(|p| std::path::Path::new(p).exists()))
        .map(|(name, _)| (*name).to_string()))
}

/// Whether the pkexec *binary* exists — drives "Fix it for me" button visibility.
/// This does NOT check that the PolicyKit policy is registered; on AppImage /
/// minimal installs the helper binary is absent too, so a click then resolves to
/// `PkexecMissing` (exit 127) → the UI falls back to the always-available "Show
/// command" copy-paste path. The button is a best-effort affordance, not a
/// guarantee the privileged path is wired.
#[tauri::command]
pub async fn gps_pkexec_available() -> Result<bool, crate::ui_commands::UiError> {
    Ok(which_pkexec().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_exit_codes_to_outcomes() {
        assert_eq!(classify_exit(Some(0)), GpsFixOutcome::Ok);
        assert_eq!(classify_exit(Some(126)), GpsFixOutcome::AuthDismissed);
        assert_eq!(classify_exit(Some(127)), GpsFixOutcome::PkexecMissing);
        assert_eq!(classify_exit(Some(1)), GpsFixOutcome::Failed);
        assert_eq!(classify_exit(Some(2)), GpsFixOutcome::Failed);
        assert_eq!(classify_exit(None), GpsFixOutcome::Failed);
    }

    #[test]
    fn action_token_allowlist_rejects_unknown() {
        assert_eq!(action_token("add-dialout"), Some("add-dialout"));
        assert_eq!(action_token("mask-modemmanager"), Some("mask-modemmanager"));
        assert_eq!(action_token("unmask-modemmanager"), Some("unmask-modemmanager"));
        assert_eq!(action_token("rm -rf /"), None);
        assert_eq!(action_token("add-dialout; reboot"), None);
        assert_eq!(action_token(""), None);
    }

    #[test]
    fn device_path_validation_rejects_injection() {
        assert!(is_safe_device_path("/dev/ttyACM0"));
        assert!(is_safe_device_path("/dev/serial/by-id/usb-u-blox-if00"));
        assert!(!is_safe_device_path("/etc/passwd"));
        assert!(!is_safe_device_path("/dev/ttyACM0; reboot"));
        assert!(!is_safe_device_path("/dev/../etc/shadow"));
        assert!(!is_safe_device_path(""));
    }
}
