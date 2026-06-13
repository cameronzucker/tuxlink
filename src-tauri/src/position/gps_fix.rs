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

/// Whether pkexec exists at all — drives "Fix it for me" button visibility.
/// AppImage / minimal installs (no pkexec or no registered policy) fall back to
/// the always-available "Show command" copy-paste path.
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
}
