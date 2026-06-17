//! CMS account password change (tuxlink-vfb3).
//!
//! The Winlink *account* lifecycle (register / change password / recovery) is a
//! separate HTTPS JSON API at `https://api.winlink.org` — NOT the telnet CMS
//! (`:8772`/`:8773`) and NOT the secure-login (`;PQ`/`;PR`) challenge. Password
//! change is a single form-encoded POST + JSON parse. On a confirmed-success
//! response the new secret is written to the OS keyring atomically (via
//! [`crate::winlink::credentials::write_password`]); any failure leaves the
//! prior credential untouched.
//!
//! ## Access code (injected — never a committed literal)
//!
//! These account ops are gated by a client-shared `WebServiceAccesscode` (the
//! same one every Winlink-family client uses). It is NOT a per-user secret, but
//! tuxlink does not republish another application's key into open source, and a
//! tuxlink-issued key is the sanctioned path (keys are issued per-application by
//! a Winlink administrator). The code is therefore read at runtime from the
//! `TUXLINK_WINLINK_ACCESS_CODE` environment variable; when it is absent the
//! feature reports itself unavailable and the wizard control is gated off, so
//! the open source ships no literal and source-builders never hit a dead form.
//!
//! ## Testing
//!
//! The pure request-encode + response-parse are unit-tested here. The live POST
//! mutates a real Winlink account, so it is an operator-run integration check —
//! never exercised in CI.

use serde::Deserialize;
use std::time::Duration;

/// The account password-change endpoint. TLS is mandatory here (unlike the
/// telnet CMS) — cert validation MUST stay on.
const PASSWORD_CHANGE_URL: &str = "https://api.winlink.org/account/password/change?format=json";

/// Environment variable supplying the client-shared Winlink access code. See the
/// module docs: injected at runtime so no literal is committed to the open repo.
const ACCESS_CODE_ENV: &str = "TUXLINK_WINLINK_ACCESS_CODE";

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a password change did not complete. Serialized to the wizard; the
/// `Rejected` message is the CMS's own `ErrorMessage`, surfaced verbatim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum PasswordChangeError {
    /// `TUXLINK_WINLINK_ACCESS_CODE` is unset/empty — the feature is not
    /// configured on this build. The UI hides the control in this case; this
    /// variant is the defensive backstop if the command is invoked anyway.
    NotConfigured,
    /// The HTTPS request itself failed (DNS, TLS, timeout, transport).
    Network { reason: String },
    /// The access key in `TUXLINK_WINLINK_ACCESS_CODE` is missing/invalid for this
    /// operation — the server returned `InvalidAccessKey`. Distinct from a normal
    /// `Rejected` so the UI can tell the operator the key (not the password) is the
    /// problem. (Verified against the live API 2026-06-17: the shared WLE code is
    /// rejected; a Tuxlink-issued key is required — see the account-API spec.)
    InvalidKey,
    /// The CMS rejected the request. `message` is the server's
    /// `ResponseStatus.Message`, surfaced verbatim.
    Rejected { message: String },
    /// The change succeeded at the CMS but the keyring write failed. The CMS now
    /// holds the NEW password while the keyring may still hold the OLD — the
    /// wizard must tell the operator their stored credential is out of sync.
    KeyringDesync { reason: String },
}

/// Read the injected access code, treating empty/whitespace as absent.
pub fn access_code() -> Option<String> {
    match std::env::var(ACCESS_CODE_ENV) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

/// Whether the password-change feature is configured on this build (i.e. an
/// access code is present). The wizard uses this to show/enable its control.
pub fn is_available() -> bool {
    access_code().is_some()
}

/// The account `Callsign` parameter: uppercase, drop any `.`-qualifier, strip the
/// SSID (WLE `GetBaseCallsign`). Reuses the non-local base-callsign algorithm.
pub fn account_callsign(raw: &str) -> String {
    crate::winlink::telnet::base_callsign_for_post_office(raw, false)
}

/// Build the form-encoded body for the change POST, matching the LIVE
/// `api.winlink.org` contract (verified 2026-06-17): the auth parameter is `Key`
/// (uniform across every account op), and there is no `Requester` param. The
/// decompile's `WebServiceAccesscode`/`Requester` shape is stale and rejected by
/// the current server.
pub fn password_change_form(
    account_callsign: &str,
    old_password: &str,
    new_password: &str,
    access_code: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("Callsign", account_callsign.to_string()),
        ("OldPassword", old_password.to_string()),
        ("NewPassword", new_password.to_string()),
        ("Key", access_code.to_string()),
    ]
}

/// The ServiceStack error block. Present (with a non-empty `ErrorCode`) only on
/// failure; on a clean success the live API omits `ResponseStatus` entirely or
/// leaves `ErrorCode` empty. The human-readable text is `Message` (NOT the
/// decompile's `ErrorMessage`). Unknown fields (StackTrace, Errors, Meta) ignored.
#[derive(Debug, Deserialize)]
struct ResponseStatus {
    #[serde(rename = "ErrorCode", default)]
    error_code: String,
    #[serde(rename = "Message", default)]
    message: String,
}

/// The common Winlink (ServiceStack) JSON envelope. Payload fields are top-level
/// and op-specific; the only field this layer needs for a write op is the
/// optional `ResponseStatus`. There is no `HasError` field in the live contract.
#[derive(Debug, Deserialize)]
struct AccountEnvelope {
    #[serde(rename = "ResponseStatus", default)]
    response_status: Option<ResponseStatus>,
}

/// Parse a Winlink account-API response (live ServiceStack contract, verified
/// 2026-06-17). Success ⇔ the body is a JSON object with no error
/// (`ResponseStatus` absent or its `ErrorCode` empty). Failure ⇔ a non-empty
/// `ResponseStatus.ErrorCode`; `InvalidAccessKey` maps to `InvalidKey`, anything
/// else to `Rejected` with the server's `Message` verbatim (coded fallback when
/// the server gives none).
///
/// Credential-safety guard: a body that is not valid JSON (a proxy HTML error
/// page, garbage, a truncated stream) is a transport error, NEVER a success — so
/// it can never trigger a keyring write the CMS did not confirm. (A bare `{}` on
/// an HTTP-200 is a legitimate empty success for write ops, so the HTTP status
/// check in the caller is the second half of this guard.)
pub fn parse_password_change_response(body: &str) -> Result<(), PasswordChangeError> {
    let env: AccountEnvelope = serde_json::from_str(body).map_err(|e| {
        PasswordChangeError::Network {
            reason: format!("unparseable account API response: {e}"),
        }
    })?;
    match env.response_status {
        Some(rs) if !rs.error_code.trim().is_empty() => {
            if rs.error_code == "InvalidAccessKey" {
                return Err(PasswordChangeError::InvalidKey);
            }
            let message = if rs.message.trim().is_empty() {
                format!("request rejected (code {})", rs.error_code)
            } else {
                rs.message
            };
            Err(PasswordChangeError::Rejected { message })
        }
        _ => Ok(()),
    }
}

/// Perform the password change against the live account API.
///
/// The caller supplies the CURRENT password (`old_password`, the `OldPassword`
/// proof — collected from the operator, not read from the keyring, so the change
/// is gated on the operator demonstrably knowing the current secret). POSTs the
/// change, and on a confirmed-success response writes the NEW password to the
/// keyring atomically. Any pre-success failure leaves the keyring untouched.
///
/// RADIO-1 note: this is an internet HTTPS call to the account API, not a
/// transmission — no on-air consent gate applies. It DOES mutate a real account,
/// so it is operator-run, never exercised in CI.
pub async fn change_password(
    raw_callsign: &str,
    old_password: &str,
    new_password: &str,
) -> Result<(), PasswordChangeError> {
    let code = access_code().ok_or(PasswordChangeError::NotConfigured)?;
    let account = account_callsign(raw_callsign);

    let form = password_change_form(&account, old_password, new_password, &code);

    let client = reqwest::Client::builder()
        .https_only(true) // account API is genuinely TLS — never relax cert validation
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| PasswordChangeError::Network { reason: e.to_string() })?;

    let resp = client
        .post(PASSWORD_CHANGE_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| PasswordChangeError::Network { reason: e.to_string() })?;

    // ServiceStack returns errors as HTTP 400 with a JSON body carrying
    // ResponseStatus.Message — so read + parse the body REGARDLESS of status, or
    // the server's actual error (e.g. InvalidAccessKey, "Old password incorrect")
    // is discarded. The parse maps any ResponseStatus error to Rejected/InvalidKey.
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| PasswordChangeError::Network { reason: e.to_string() })?;

    // Parse FIRST — only a confirmed success may touch the keyring. A parsed
    // rejection (incl. on HTTP 400) returns here with the server's message.
    parse_password_change_response(&body)?;

    // Parse said "no error", but if the HTTP status was itself unsuccessful the
    // outcome is ambiguous (a 4xx/5xx whose body lacked a ResponseStatus error) —
    // treat as a transport error, never a confirmed success that writes the keyring.
    if !status.is_success() {
        return Err(PasswordChangeError::Network {
            reason: format!("account API returned HTTP {status} with no error detail"),
        });
    }

    // CMS accepted the new password; bring the keyring into lockstep. A failure
    // here means the CMS holds NEW while the keyring holds OLD — surfaced as a
    // distinct desync error so the operator knows to re-enter manually.
    crate::winlink::credentials::write_password(&account, new_password)
        .map_err(|e| PasswordChangeError::KeyringDesync { reason: e.to_string() })?;

    Ok(())
}

// ──────────────────────────────────────────────────────────────
// Tauri commands
// ──────────────────────────────────────────────────────────────

/// Change the CMS account password (current -> new). The operator supplies both
/// the current and new passwords; on success the keyring is updated atomically.
/// Operator-run (mutates a real account); never exercised in CI.
#[tauri::command]
pub async fn cms_password_change(
    raw_callsign: String,
    old_password: String,
    new_password: String,
) -> Result<(), PasswordChangeError> {
    change_password(&raw_callsign, &old_password, &new_password).await
}

/// Whether the password-change feature is configured (an access code is present).
/// The wizard calls this to decide whether to show/enable the control.
#[tauri::command]
pub fn cms_password_change_available() -> bool {
    is_available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_callsign_uppercases_and_strips_ssid() {
        assert_eq!(account_callsign("n7cpz-10"), "N7CPZ");
        assert_eq!(account_callsign("W7XYZ"), "W7XYZ");
        assert_eq!(account_callsign("k1abc.p"), "K1ABC");
    }

    #[test]
    fn form_carries_live_contract_fields_and_base_callsign() {
        let form = password_change_form("N7CPZ", "oldpw", "newpw", "ACCESSKEY");
        let get = |k: &str| form.iter().find(|(name, _)| *name == k).map(|(_, v)| v.as_str());
        assert_eq!(get("Callsign"), Some("N7CPZ"));
        assert_eq!(get("OldPassword"), Some("oldpw"));
        assert_eq!(get("NewPassword"), Some("newpw"));
        // Live API auth param is `Key` (verified 2026-06-17), not the decompile's
        // `WebServiceAccesscode`; there is no `Requester`.
        assert_eq!(get("Key"), Some("ACCESSKEY"));
        assert_eq!(get("WebServiceAccesscode"), None);
        assert_eq!(get("Requester"), None);
        assert_eq!(form.len(), 4);
    }

    #[test]
    fn parse_accepts_live_success_shapes() {
        // ServiceStack success: ResponseStatus absent, or present with empty ErrorCode.
        for body in [
            "{}",
            r#"{"ResponseStatus":{}}"#,
            r#"{"ResponseStatus":{"ErrorCode":""}}"#,
        ] {
            assert_eq!(parse_password_change_response(body), Ok(()), "should be success: {body}");
        }
    }

    #[test]
    fn parse_rejects_with_verbatim_message_from_response_status() {
        let body = r#"{"ResponseStatus":{"ErrorCode":"AUTH","Message":"Old password is incorrect"}}"#;
        assert_eq!(
            parse_password_change_response(body),
            Err(PasswordChangeError::Rejected {
                message: "Old password is incorrect".to_string()
            })
        );
    }

    #[test]
    fn parse_maps_invalid_access_key_to_invalid_key() {
        // The live shape observed 2026-06-17 when the access key is missing/wrong.
        let body = r#"{"CallsignExists":false,"Blocked":false,"ResponseStatus":{"ErrorCode":"InvalidAccessKey","Message":"Invalid access key for this operation"}}"#;
        assert_eq!(
            parse_password_change_response(body),
            Err(PasswordChangeError::InvalidKey)
        );
    }

    #[test]
    fn parse_rejection_without_message_uses_code_fallback() {
        let body = r#"{"ResponseStatus":{"ErrorCode":"X1","Message":""}}"#;
        match parse_password_change_response(body) {
            Err(PasswordChangeError::Rejected { message }) => {
                assert!(message.contains("X1"), "fallback should cite the code: {message}");
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn parse_unparseable_body_is_network_error_not_success() {
        // A proxy HTML error page / garbage must be a transport error, never a
        // confirmed success that writes the keyring (credential-safety guard).
        for body in ["<html>502 Bad Gateway</html>", "not json", ""] {
            assert!(
                matches!(
                    parse_password_change_response(body),
                    Err(PasswordChangeError::Network { .. })
                ),
                "unparseable body must be a transport error: {body:?}"
            );
        }
    }

    #[test]
    fn access_code_treats_empty_as_absent() {
        // Note: not asserting the env-present case here to avoid mutating
        // process-global env in a parallel test run; the empty/absent mapping is
        // the behavior the availability gate depends on.
        std::env::remove_var(ACCESS_CODE_ENV);
        assert_eq!(access_code(), None);
        assert!(!is_available());
    }
}
