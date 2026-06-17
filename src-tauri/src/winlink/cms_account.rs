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

use std::time::Duration;

/// Base URL for the Winlink Web Services account API. TLS is mandatory here
/// (unlike the telnet CMS) — cert validation MUST stay on. Every op is a form-POST
/// to `<base>/<path>?format=json` (verified live contract, 2026-06-17).
const API_BASE: &str = "https://api.winlink.org";

/// Environment variable supplying the Winlink access key. See the module docs:
/// injected at runtime so no literal is committed to the open repo. (The shared
/// WLE code is rejected by the current server; a Tuxlink-issued key is required.)
const ACCESS_CODE_ENV: &str = "TUXLINK_WINLINK_ACCESS_CODE";

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Serializes ALL account-mutating ops (create/change/set-recovery/remove) so the
/// server call + the keyring write/delete can never interleave across two
/// mutations and desync the keyring from the CMS (Codex adrev 2026-06-17 P1). A
/// single global lock is coarser than per-callsign but trivially correct; these
/// are operator-initiated, so contention is effectively nil. Read ops do not lock.
static ACCOUNT_MUTATION_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
    std::sync::OnceLock::new();

fn mutation_lock() -> &'static tokio::sync::Mutex<()> {
    ACCOUNT_MUTATION_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Why a password change did not complete. Serialized to the wizard; the
/// `Rejected` message is the CMS's own `ErrorMessage`, surfaced verbatim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum AccountApiError {
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
    /// A mutating op succeeded at the CMS but the keyring write/delete failed.
    /// The server state and the local keyring are out of sync — the UI must tell
    /// the operator (e.g. re-enter the credential to resync).
    KeyringDesync { reason: String },
    /// The input failed local validation before any network call (e.g. a
    /// tactical/hyphenated identifier on a full-account-only op, or a missing
    /// required field). `field` names what was rejected.
    InvalidInput { field: String },
    /// A mutating call's outcome is indeterminate — the request was sent but the
    /// response was lost (timeout/transport drop after send), so whether the
    /// server committed the change is unknown. The caller must reconcile (e.g.
    /// `account_exists`) before assuming success or failure.
    UnknownOutcome,
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

/// Op-specific form params for the password-change POST. The shared `Key` auth
/// param is appended by [`post_account_form`]; these builders carry only the
/// operation's own fields (verified live contract, 2026-06-17).
pub fn password_change_form(
    account_callsign: &str,
    old_password: &str,
    new_password: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("Callsign", account_callsign.to_string()),
        ("OldPassword", old_password.to_string()),
        ("NewPassword", new_password.to_string()),
    ]
}

/// Op-specific form params for `/account/add` (create). `RecoveryEmail` is a
/// direct param of the create call (live contract) — account creation is one
/// atomic request, and the recovery email is mandatory per the wizard flow.
fn account_create_form(account: &str, password: &str, recovery_email: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Callsign", account.to_string()),
        ("Password", password.to_string()),
        ("RecoveryEmail", recovery_email.to_string()),
    ]
}

/// Op-specific form params for `/account/password/recovery/email/set`.
fn account_set_recovery_form(account: &str, password: &str, recovery_email: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Callsign", account.to_string()),
        ("Password", password.to_string()),
        ("RecoveryEmail", recovery_email.to_string()),
    ]
}

/// Classify a parsed ServiceStack response (live contract, verified 2026-06-17).
/// Success ⇔ no error (`ResponseStatus` absent or its `ErrorCode` empty). Failure
/// ⇔ a non-empty `ResponseStatus.ErrorCode`; `InvalidAccessKey` maps to
/// `InvalidKey`, anything else to `Rejected` with the server's `Message` verbatim
/// (coded fallback when the server gives none). The error text is `Message`, NOT
/// the decompile's `ErrorMessage`; there is no `HasError` field.
fn account_error_from_value(v: &serde_json::Value) -> Result<(), AccountApiError> {
    let rs = v.get("ResponseStatus");
    let code = rs
        .and_then(|r| r.get("ErrorCode"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    if code.trim().is_empty() {
        return Ok(());
    }
    if code == "InvalidAccessKey" {
        return Err(AccountApiError::InvalidKey);
    }
    let msg = rs
        .and_then(|r| r.get("Message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let message = if msg.trim().is_empty() {
        format!("request rejected (code {code})")
    } else {
        msg.to_string()
    };
    Err(AccountApiError::Rejected { message })
}

/// Parse a Winlink account-API response body. A body that is not valid JSON (a
/// proxy HTML error page, garbage, a truncated stream) is a transport error,
/// NEVER a success — so it can never trigger a keyring write the CMS did not
/// confirm. Error classification is delegated to [`account_error_from_value`].
pub fn parse_password_change_response(body: &str) -> Result<(), AccountApiError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| AccountApiError::Network {
        reason: format!("unparseable account API response: {e}"),
    })?;
    account_error_from_value(&v)
}

/// Reject tactical/hyphenated identifiers on these full-account-only ops, and
/// return the base account callsign (SSID-stripped, uppercased) otherwise. The
/// base-callsign strip is destructive for tactical addresses (`EOC-1` -> `EOC`),
/// so these commands accept only real callsigns. Heuristic: ASCII-alphanumeric,
/// 3–10 chars, with at least one letter AND one digit (catches the common
/// tactical/word identifiers; tactical account ops are a non-goal of this layer).
fn normalize_account_callsign(raw: &str) -> Result<String, AccountApiError> {
    let base = account_callsign(raw);
    let looks_like_callsign = (3..=10).contains(&base.len())
        && base.chars().all(|c| c.is_ascii_alphanumeric())
        && base.chars().any(|c| c.is_ascii_digit())
        && base.chars().any(|c| c.is_ascii_alphabetic());
    if looks_like_callsign {
        Ok(base)
    } else {
        Err(AccountApiError::InvalidInput {
            field: "callsign (a licensed amateur callsign is required, not a tactical address)"
                .to_string(),
        })
    }
}

/// The TLS-mandatory reqwest client shared by every account op.
fn account_client() -> Result<reqwest::Client, AccountApiError> {
    reqwest::Client::builder()
        .https_only(true) // account API is genuinely TLS — never relax cert validation
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| AccountApiError::Network { reason: e.to_string() })
}

/// POST an account-API form (op params + the shared `Key`) and return the parsed
/// JSON value on success. ServiceStack returns errors as HTTP 400 with the detail
/// in the body, so the body is read + classified REGARDLESS of status; an
/// unexplained non-2xx is a transport error. Returning the value lets read ops
/// extract their top-level payload fields through this single guarded path.
async fn post_account_form(
    path: &str,
    mut params: Vec<(&'static str, String)>,
) -> Result<serde_json::Value, AccountApiError> {
    let code = access_code().ok_or(AccountApiError::NotConfigured)?;
    params.push(("Key", code));

    let url = format!("{API_BASE}{path}?format=json");
    let resp = account_client()?
        .post(&url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AccountApiError::Network { reason: e.to_string() })?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| AccountApiError::Network { reason: e.to_string() })?;

    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| AccountApiError::Network {
            reason: format!("unparseable account API response: {e}"),
        })?;

    // A ResponseStatus error (incl. on HTTP 400) returns here with the detail.
    account_error_from_value(&value)?;

    // No parsed error but an unsuccessful HTTP status ⇒ ambiguous; never a
    // confirmed success that would write/delete the keyring.
    if !status.is_success() {
        return Err(AccountApiError::Network {
            reason: format!("account API returned HTTP {status} with no error detail"),
        });
    }

    Ok(value)
}

/// Result of [`account_exists`]: whether a CMS account is registered for the
/// callsign, and whether it is blocked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AccountExistsResult {
    pub exists: bool,
    pub blocked: bool,
}

/// Change the CMS account password (current -> new). The caller supplies the
/// CURRENT password (the `OldPassword` proof — collected from the operator, not
/// read from the keyring), and on a confirmed-success response the keyring is
/// updated atomically. Any pre-success failure leaves the keyring untouched. The
/// mutation lock serializes this against other account mutations on the install.
///
/// RADIO-1 note: internet HTTPS to the account API, not a transmission — no on-air
/// consent gate. DOES mutate a real account → operator-run, never exercised in CI.
pub async fn change_password(
    raw_callsign: &str,
    old_password: &str,
    new_password: &str,
) -> Result<(), AccountApiError> {
    let account = normalize_account_callsign(raw_callsign)?;
    let _guard = mutation_lock().lock().await;
    post_account_form(
        "/account/password/change",
        password_change_form(&account, old_password, new_password),
    )
    .await?;
    // CMS accepted the new password; bring the keyring into lockstep. A failure
    // here means the CMS holds NEW while the keyring holds OLD — surfaced as a
    // distinct desync error so the operator knows to re-enter manually.
    crate::winlink::credentials::write_password(&account, new_password)
        .map_err(|e| AccountApiError::KeyringDesync { reason: e.to_string() })?;
    Ok(())
}

/// Whether a CMS account exists for `raw_callsign` (and whether it is blocked).
/// Read-only — no keyring, no mutation lock. Fails closed if the live response
/// omits the expected `CallsignExists` payload field (never a silent default).
pub async fn account_exists(raw_callsign: &str) -> Result<AccountExistsResult, AccountApiError> {
    let account = normalize_account_callsign(raw_callsign)?;
    let v = post_account_form("/account/exists", vec![("Callsign", account)]).await?;
    let exists = v
        .get("CallsignExists")
        .and_then(|x| x.as_bool())
        .ok_or_else(|| AccountApiError::Network {
            reason: "account/exists response missing CallsignExists".to_string(),
        })?;
    let blocked = v.get("Blocked").and_then(|x| x.as_bool()).unwrap_or(false);
    Ok(AccountExistsResult { exists, blocked })
}

/// Create a CMS account (callsign + password + MANDATORY recovery email). On a
/// confirmed-success response the password is written to the keyring atomically.
/// The recovery email is required by the wizard flow and re-checked here.
pub async fn account_create(
    raw_callsign: &str,
    password: &str,
    recovery_email: &str,
) -> Result<(), AccountApiError> {
    let account = normalize_account_callsign(raw_callsign)?;
    let email = recovery_email.trim();
    if email.is_empty() {
        return Err(AccountApiError::InvalidInput { field: "recovery_email".to_string() });
    }
    let _guard = mutation_lock().lock().await;
    post_account_form("/account/add", account_create_form(&account, password, email)).await?;
    crate::winlink::credentials::write_password(&account, password)
        .map_err(|e| AccountApiError::KeyringDesync { reason: e.to_string() })?;
    Ok(())
}

/// Set/replace the account's recovery email (requires the current password as
/// proof). No keyring effect.
pub async fn account_set_recovery_email(
    raw_callsign: &str,
    password: &str,
    recovery_email: &str,
) -> Result<(), AccountApiError> {
    let account = normalize_account_callsign(raw_callsign)?;
    let email = recovery_email.trim();
    if email.is_empty() {
        return Err(AccountApiError::InvalidInput { field: "recovery_email".to_string() });
    }
    let _guard = mutation_lock().lock().await;
    post_account_form(
        "/account/password/recovery/email/set",
        account_set_recovery_form(&account, password, email),
    )
    .await
    .map(|_| ())
}

/// Trigger the server to email the account password to its recovery address.
/// Read-class (no keyring, no lock). The server returns an error when no recovery
/// address is on file — surfaced as `Rejected` with the server's message so the
/// UI can guide the user to set one.
pub async fn account_send_recovery(raw_callsign: &str) -> Result<(), AccountApiError> {
    let account = normalize_account_callsign(raw_callsign)?;
    post_account_form("/account/password/send", vec![("Callsign", account)])
        .await
        .map(|_| ())
}

/// DELETE a CMS account. On a confirmed-success response the keyring entry is
/// removed (the account no longer exists, so the stored credential is dead).
/// DESTRUCTIVE — the UI gates this behind explicit typed confirmation, and the op
/// must be proven invocable with the issued key (its live metadata is
/// 403-privileged) before any UI wires it. The mutation lock serializes it.
pub async fn account_remove(raw_callsign: &str) -> Result<(), AccountApiError> {
    let account = normalize_account_callsign(raw_callsign)?;
    let _guard = mutation_lock().lock().await;
    post_account_form("/account/remove", vec![("Callsign", account.clone())]).await?;
    // Account is gone; drop the now-dead keyring credential. A delete failure here
    // is a desync (removed at CMS, stale secret locally), never a false success.
    crate::winlink::credentials::delete_password(&account)
        .map_err(|e| AccountApiError::KeyringDesync { reason: e.to_string() })?;
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
) -> Result<(), AccountApiError> {
    change_password(&raw_callsign, &old_password, &new_password).await
}

/// Whether the password-change feature is configured (an access code is present).
/// The wizard calls this to decide whether to show/enable the control.
#[tauri::command]
pub fn cms_password_change_available() -> bool {
    is_available()
}

/// Create a CMS account (callsign + password + mandatory recovery email).
/// Operator-run (mutates a real account); never exercised in CI.
#[tauri::command]
pub async fn cms_account_create(
    raw_callsign: String,
    password: String,
    recovery_email: String,
) -> Result<(), AccountApiError> {
    account_create(&raw_callsign, &password, &recovery_email).await
}

/// Whether a CMS account exists (and is blocked) for the callsign. Read-only.
#[tauri::command]
pub async fn cms_account_exists(raw_callsign: String) -> Result<AccountExistsResult, AccountApiError> {
    account_exists(&raw_callsign).await
}

/// Set/replace the account's recovery email (current password required as proof).
#[tauri::command]
pub async fn cms_account_set_recovery_email(
    raw_callsign: String,
    password: String,
    recovery_email: String,
) -> Result<(), AccountApiError> {
    account_set_recovery_email(&raw_callsign, &password, &recovery_email).await
}

/// Email the account password to its recovery address (forgot-password recovery).
#[tauri::command]
pub async fn cms_account_send_recovery(raw_callsign: String) -> Result<(), AccountApiError> {
    account_send_recovery(&raw_callsign).await
}

/// DELETE a CMS account (destructive). The UI must gate this behind explicit typed
/// confirmation, and the op must be proven invocable with the issued key before
/// any UI wires it (its live metadata is 403-privileged). Operator-run.
#[tauri::command]
pub async fn cms_account_remove(raw_callsign: String) -> Result<(), AccountApiError> {
    account_remove(&raw_callsign).await
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

    fn get<'a>(form: &'a [(&'static str, String)], k: &str) -> Option<&'a str> {
        form.iter().find(|(name, _)| *name == k).map(|(_, v)| v.as_str())
    }

    #[test]
    fn change_form_carries_op_fields_only_no_key() {
        // The shared `Key` is appended by post_account_form, NOT the op builder.
        // The decompile's `WebServiceAccesscode`/`Requester` are gone (stale).
        let form = password_change_form("N7CPZ", "oldpw", "newpw");
        assert_eq!(get(&form, "Callsign"), Some("N7CPZ"));
        assert_eq!(get(&form, "OldPassword"), Some("oldpw"));
        assert_eq!(get(&form, "NewPassword"), Some("newpw"));
        assert_eq!(get(&form, "Key"), None);
        assert_eq!(get(&form, "WebServiceAccesscode"), None);
        assert_eq!(get(&form, "Requester"), None);
        assert_eq!(form.len(), 3);
    }

    #[test]
    fn create_form_carries_callsign_password_recovery_email() {
        let form = account_create_form("W4PHS", "secretpw", "me@example.org");
        assert_eq!(get(&form, "Callsign"), Some("W4PHS"));
        assert_eq!(get(&form, "Password"), Some("secretpw"));
        // RecoveryEmail is a direct /account/add param (live contract).
        assert_eq!(get(&form, "RecoveryEmail"), Some("me@example.org"));
        assert_eq!(form.len(), 3);
    }

    #[test]
    fn set_recovery_form_carries_callsign_password_recovery_email() {
        let form = account_set_recovery_form("W4PHS", "secretpw", "new@example.org");
        assert_eq!(get(&form, "Callsign"), Some("W4PHS"));
        assert_eq!(get(&form, "Password"), Some("secretpw"));
        assert_eq!(get(&form, "RecoveryEmail"), Some("new@example.org"));
        assert_eq!(form.len(), 3);
    }

    #[test]
    fn normalize_accepts_real_callsigns_and_strips_ssid() {
        assert_eq!(normalize_account_callsign("n7cpz-10"), Ok("N7CPZ".to_string()));
        assert_eq!(normalize_account_callsign("W4PHS"), Ok("W4PHS".to_string()));
        assert_eq!(normalize_account_callsign("k1abc.p"), Ok("K1ABC".to_string()));
    }

    #[test]
    fn normalize_rejects_tactical_and_word_identifiers() {
        // No digit (word-like) or otherwise non-callsign → InvalidInput, never a
        // silently-mangled base callsign.
        for raw in ["EOC-1", "ARES", "EOC", "BAOFENG-FM"] {
            assert!(
                matches!(normalize_account_callsign(raw), Err(AccountApiError::InvalidInput { .. })),
                "should reject tactical/word input: {raw}"
            );
        }
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
            Err(AccountApiError::Rejected {
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
            Err(AccountApiError::InvalidKey)
        );
    }

    #[test]
    fn parse_rejection_without_message_uses_code_fallback() {
        let body = r#"{"ResponseStatus":{"ErrorCode":"X1","Message":""}}"#;
        match parse_password_change_response(body) {
            Err(AccountApiError::Rejected { message }) => {
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
                    Err(AccountApiError::Network { .. })
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
