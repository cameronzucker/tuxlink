//! Keyring key/account-string format for identity activation secrets.
//!
//! Activation secrets live in the OS keyring under the canonical `tuxlink`
//! service (matching `winlink::credentials::SERVICE`) with a per-callsign
//! account string built by [`activation_account`]. This is the single source
//! of truth for the `tuxlink-identity-activation:<CALLSIGN>` format the spec's
//! resolved design decision #2 fixes.

/// Canonical keyring service name (must match `winlink::credentials`' `"tuxlink"`).
#[allow(dead_code)]
pub(crate) const SERVICE: &str = "tuxlink";

/// Build the keyring account string for a FULL identity's activation secret.
///
/// Format: `tuxlink-identity-activation:<CALLSIGN-UPPER>`. Uppercasing the
/// callsign keeps case variants from minting duplicate keyring entries (same
/// discipline as `credentials::p2p_peer_account`).
#[allow(dead_code)]
pub(crate) fn activation_account(callsign: &str) -> String {
    format!("tuxlink-identity-activation:{}", callsign.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_account_uses_documented_prefix_and_uppercases() {
        assert_eq!(
            activation_account("w1abc"),
            "tuxlink-identity-activation:W1ABC",
            "activation account must be tuxlink-identity-activation:<CALLSIGN-UPPER>"
        );
        // Already-upper input is idempotent.
        assert_eq!(
            activation_account("KK7XYZ"),
            "tuxlink-identity-activation:KK7XYZ"
        );
    }
}
