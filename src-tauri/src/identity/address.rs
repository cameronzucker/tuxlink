//! `Callsign` newtype and the `Address` enum.
//!
//! `Callsign` reuses the existing loose-callsign validator
//! (`config::validate_identity_describe`: nonempty, ASCII-printable, no
//! whitespace, ≤32). `Address::Tactical` is a free-form label validated to
//! ≤24 chars + ASCII-printable (the master-plan contract).

use serde::{Deserialize, Serialize};

use super::IdentityError;
use crate::config::validate_identity_describe;

/// A validated FCC-format callsign. Construct via [`Callsign::parse`].
///
/// The inner string is private so a `Callsign` can only exist if it passed the
/// loose validator — there is no way to forge an unvalidated callsign value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Callsign(String);

impl Callsign {
    /// Parse + validate a callsign against the loose-callsign rules
    /// (`config::validate_identity_describe`). Stored verbatim (case preserved);
    /// callers that need case-insensitive matching uppercase at the comparison
    /// site (e.g. keyring account strings).
    pub fn parse(s: &str) -> Result<Self, IdentityError> {
        match validate_identity_describe(s) {
            Some(rule) => Err(IdentityError::InvalidCallsign(rule.to_string())),
            None => Ok(Callsign(s.to_string())),
        }
    }

    /// Borrow the validated callsign string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What an operation operates/addresses AS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Address {
    /// A licensed FCC callsign.
    Full(Callsign),
    /// A free-form tactical label (validated ≤24 chars, ASCII-printable).
    Tactical(String),
}

impl Address {
    /// Validate + build a `Tactical` address. Rules: nonempty, ASCII-printable
    /// (no control chars), no internal whitespace, ≤24 chars.
    pub fn tactical(label: &str) -> Result<Self, IdentityError> {
        if let Some(rule) = validate_tactical_describe(label) {
            return Err(IdentityError::InvalidTactical(rule.to_string()));
        }
        Ok(Address::Tactical(label.to_string()))
    }
}

/// Returns `Some(rule-slug)` for the FIRST tactical-label rule violated, else `None`.
/// Mirrors the order/shape of `config::validate_identity_describe` but caps at 24.
pub(crate) fn validate_tactical_describe(s: &str) -> Option<&'static str> {
    if s.is_empty() {
        return Some("must not be empty");
    }
    if s.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) {
        return Some("must be ASCII-printable");
    }
    if s.chars().any(char::is_whitespace) {
        return Some("must not contain whitespace");
    }
    if s.chars().count() > 24 {
        return Some("must be ≤24 chars");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callsign_parses_a_valid_call() {
        let c = Callsign::parse("W1ABC").expect("valid call");
        assert_eq!(c.as_str(), "W1ABC");
    }

    #[test]
    fn callsign_preserves_case() {
        // Stored verbatim; case-folding happens only at comparison sites.
        assert_eq!(Callsign::parse("kk7xyz").unwrap().as_str(), "kk7xyz");
    }

    #[test]
    fn callsign_rejects_empty_and_whitespace() {
        assert_eq!(
            Callsign::parse(""),
            Err(IdentityError::InvalidCallsign("must not be empty".into()))
        );
        assert_eq!(
            Callsign::parse("W1 ABC"),
            Err(IdentityError::InvalidCallsign("must not contain whitespace".into()))
        );
    }

    #[test]
    fn callsign_rejects_over_32_chars() {
        let long = "A".repeat(33);
        assert_eq!(
            Callsign::parse(&long),
            Err(IdentityError::InvalidCallsign("must be ≤32 chars".into()))
        );
    }

    #[test]
    fn tactical_parses_a_valid_label() {
        match Address::tactical("AIDSTATION-1").unwrap() {
            Address::Tactical(l) => assert_eq!(l, "AIDSTATION-1"),
            other => panic!("expected Tactical, got {other:?}"),
        }
    }

    #[test]
    fn tactical_rejects_over_24_chars() {
        let long = "T".repeat(25);
        assert_eq!(
            Address::tactical(&long),
            Err(IdentityError::InvalidTactical("must be ≤24 chars".into()))
        );
    }

    #[test]
    fn tactical_rejects_empty_and_nonascii() {
        assert_eq!(
            Address::tactical(""),
            Err(IdentityError::InvalidTactical("must not be empty".into()))
        );
        assert_eq!(
            Address::tactical("EOC-é"),
            Err(IdentityError::InvalidTactical("must be ASCII-printable".into()))
        );
    }

    #[test]
    fn address_full_round_trips_through_serde() {
        let addr = Address::Full(Callsign::parse("W1ABC").unwrap());
        let json = serde_json::to_string(&addr).unwrap();
        let back: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(addr, back);
    }
}
