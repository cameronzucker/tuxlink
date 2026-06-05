//! Field-name blocklist for the redacting Visit (spec §5.2).
//!
//! Discipline: name-based blocklist for fields that CARRY SECRETS BY NAME.
//! `error`/`err`/`error_msg`/`err_msg` are diagnostic code fields and are NOT
//! in this blocklist — blanket-redacting them would destroy diagnostic value.
//! Error types use type-based redaction (manual Display/Debug impls on
//! credential structs). See logging_blocklist_corpus.rs for the curated
//! MUST_PASS list and Codex impl-adrev P2 #6 for the rationale.
//!
//! Credential-shaped compound error field names (`error_password`,
//! `err_token`, etc.) ARE in the blocklist: their SUFFIX unambiguously
//! identifies secret material even though they are error-flavoured.

use once_cell::sync::Lazy;
use regex::Regex;

static FIELD_BLOCKLIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?ix)
        ^(
            # Generic password-class
            password | passwd | pwd | password_input | peer_password
            | station_password | secure_response
            # Token-class
            | token | auth_token | access_token | refresh_token | oauth_token
            | bearer | bearer_token
            | consent_token
            # Secret/key-class
            | secret | client_secret | private_key | privatekey
            | api_key | apikey | api[_-]key
            # Auth-class
            | auth | authorization | auth_header | authheader
            | credential | credentials
            # Challenge/response
            | secure_login_response | secure_login_challenge
            | challenge_response | challenge | response
            # Session/cookie
            | session_cookie | sessioncookie | sessionid | session_id
            | cookie
            # Cryptographic primitives that might carry secret material
            | signature | nonce | hmac | salt
            # Keyring-internal
            | keyring_value | keyring_secret
            # Credential-shaped compound error field names (Codex impl-adrev P2 #6).
            # These carry secrets by their SUFFIX even though they are error-flavoured.
            # Do NOT add `error` or `err` here — those are diagnostic identifiers.
            | error_password | err_password
            | error_token | err_token
            | error_value | err_value
            | error_body | err_body
            | error_secret | err_secret
            | error_credential | err_credential
        )$
    ",
    )
    .expect("redaction blocklist regex must compile")
});

/// Returns true if a tracing field's NAME matches the credential blocklist.
/// Match → the value is replaced with `<redacted>` in the redacted event.
pub fn should_redact_field(name: &str) -> bool {
    FIELD_BLOCKLIST.is_match(name)
}

#[cfg(test)]
mod tests {
    use super::should_redact_field;

    #[test]
    fn matches_password_class() {
        for name in [
            "password",
            "passwd",
            "pwd",
            "password_input",
            "peer_password",
            "station_password",
            "secure_response",
        ] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    #[test]
    fn matches_token_class() {
        for name in [
            "token",
            "auth_token",
            "access_token",
            "refresh_token",
            "oauth_token",
            "bearer",
            "bearer_token",
            "consent_token",
        ] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    #[test]
    fn matches_secret_class() {
        for name in [
            "secret",
            "client_secret",
            "private_key",
            "privatekey",
            "api_key",
            "apikey",
            "api-key",
        ] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    #[test]
    fn matches_auth_and_credential() {
        for name in [
            "auth",
            "authorization",
            "auth_header",
            "authheader",
            "credential",
            "credentials",
        ] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    #[test]
    fn matches_challenge_response() {
        for name in [
            "secure_login_response",
            "secure_login_challenge",
            "challenge_response",
            "challenge",
            "response",
        ] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    #[test]
    fn matches_session_and_cookie() {
        for name in [
            "session_cookie",
            "sessioncookie",
            "sessionid",
            "session_id",
            "cookie",
        ] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    #[test]
    fn matches_crypto_primitives() {
        for name in ["signature", "nonce", "hmac", "salt"] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    #[test]
    fn matches_keyring_internal() {
        for name in ["keyring_value", "keyring_secret"] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    /// Control cases — plausibly benign field names that the anchored regex
    /// must NOT match.
    #[test]
    fn does_not_match_benign_field_names() {
        for name in [
            "password_hint_index",
            "challenge_round_number",
            "nonce_count_total",
            "key_event_handler",
            "cookie_jar_path",
            "auth_required_count",
            "token_count",
            "signature_validation_disabled",
            "salt_buffer_size",
            "credential_provider_name",
            "session_id_format_version",
        ] {
            assert!(!should_redact_field(name), "{name} should NOT be redacted");
        }
    }

    #[test]
    fn is_case_insensitive() {
        assert!(should_redact_field("PASSWORD"));
        assert!(should_redact_field("Token"));
        assert!(should_redact_field("API_KEY"));
    }

    /// Credential-shaped compound error field names must be blocked.
    /// These carry secrets by their suffix even though they are error-flavoured.
    /// Codex impl-adrev P2 #6: a callsite `tracing::error!(error_password = %e, ...)`
    /// where `e` displays a credential string must be blocked by field name.
    #[test]
    fn matches_credential_shaped_error_field_names() {
        for name in [
            "error_password", "err_password",
            "error_token", "err_token",
            "error_value", "err_value",
            "error_body", "err_body",
            "error_secret", "err_secret",
            "error_credential", "err_credential",
        ] {
            assert!(should_redact_field(name), "{name} should be redacted");
        }
    }

    /// Plain `error`/`err`/`error_msg`/`err_msg` must NOT be redacted.
    /// These are the most common diagnostic field names; blanket-redacting them
    /// would destroy diagnostic value. Type-based redaction on Display impls is
    /// the correct mechanism for error types that carry credentials.
    #[test]
    fn error_diagnostic_fields_pass_through() {
        for name in ["error", "err", "error_msg", "err_msg", "error_kind", "error_count"] {
            assert!(!should_redact_field(name), "{name} should NOT be redacted (diagnostic field)");
        }
    }
}
