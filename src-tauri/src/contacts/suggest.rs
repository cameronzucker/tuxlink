//! Suggest-from-history derivation — Task A3.
//!
//! Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → "### Task A3".
//!
//! The Contacts surface offers one-click "+ Add" cards for correspondents seen
//! in the mailbox that are NOT yet saved as contacts. This module owns the PURE
//! derivation ([`derive_suggestions`]); the mailbox enumeration + config read
//! live in the [`commands`](super::commands) layer (the `contacts_suggestions`
//! command).
//!
//! **Tuxlink NEVER auto-creates a contact.** This is suggest-only: the output
//! is a ranked list the UI renders as add-cards; nothing is written to the
//! store here.
//!
//! **Identity-normalization rules (all adrev-hardened — H11):**
//! - A correspondent is suppressed if it matches an EXISTING contact OR the
//!   OPERATOR's own callsign.
//! - Matching is on a NORMALIZED wire key: case-insensitive, and the
//!   `<callsign>@winlink.org` email form normalizes to the bare callsign (so a
//!   contact `W6ABC` suppresses suggestions for `w6abc@winlink.org` AND
//!   `W6ABC`).
//! - **SSID is identity — NEVER stripped.** `W6ABC` and `W6ABC-7` are DIFFERENT
//!   identities; only the `@winlink.org` suffix normalizes (to the
//!   bare-WITH-SSID callsign). A non-`@winlink.org` email (e.g.
//!   `w6abc@gmail.com`) is a distinct foreign SMTP identity and is NOT
//!   normalized to the bare callsign.
//! - Output sorted by `message_count` DESC; ties broken alphabetically (by the
//!   ORIGINAL correspondent string, case-insensitive) for deterministic order.

use serde::{Deserialize, Serialize};

use super::store::Contact;

/// A suggest-from-history card: an un-saved correspondent the UI can offer to
/// add as a contact, annotated with how many messages mention it. snake_case
/// serde (the codebase has no `rename_all`); mirrors the frontend DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// The correspondent string AS SEEN in the mailbox (callsign or address).
    /// Verbatim — the UI shows it and pre-fills the add-card with it; the store
    /// is NOT mutated here.
    pub callsign: String,
    /// How many messages this correspondent appeared in (From + To tally).
    pub message_count: u32,
}

/// Normalize a correspondent/contact string to a comparison key:
/// - trim surrounding whitespace,
/// - drop a trailing `@winlink.org` (case-insensitive) → bare callsign-with-SSID,
/// - uppercase the remainder for case-insensitive comparison.
///
/// SSID is preserved (`W6ABC-7` stays `W6ABC-7`). A non-`@winlink.org` address
/// is uppercased verbatim (no `@`-stripping), so `w6abc@gmail.com` →
/// `W6ABC@GMAIL.COM` — a distinct key from the bare callsign `W6ABC`.
fn normalize_key(raw: &str) -> String {
    let trimmed = raw.trim();
    let bare = match trimmed
        .rfind('@')
        .map(|idx| (&trimmed[..idx], &trimmed[idx..]))
    {
        Some((local, domain)) if domain.eq_ignore_ascii_case("@winlink.org") => local,
        _ => trimmed,
    };
    bare.to_ascii_uppercase()
}

/// Derive suggest-from-history cards from a tallied correspondent list.
///
/// `correspondents` is `(callsign_or_address, message_count)` already tallied by
/// the caller (the `contacts_suggestions` command, over mailbox From + To
/// headers). `existing` is the current contacts (their primary `callsign` AND
/// any `email` are both treated as known identities). `operator_callsign` is
/// the station's own callsign (the operator is the `From` on Sent/Outbox — H11).
///
/// Returns the un-saved correspondents as [`Suggestion`]s, sorted by
/// `message_count` DESC with an alphabetical (case-insensitive) tie-break for
/// determinism. NEVER writes the store; output is suggestions only.
pub fn derive_suggestions(
    correspondents: &[(String, u32)],
    existing: &[Contact],
    operator_callsign: &str,
) -> Vec<Suggestion> {
    // Build the set of known normalized keys: the operator + every contact's
    // primary callsign and (if present) email.
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    let op = operator_callsign.trim();
    if !op.is_empty() {
        known.insert(normalize_key(op));
    }
    for c in existing {
        if !c.callsign.trim().is_empty() {
            known.insert(normalize_key(&c.callsign));
        }
        if let Some(email) = c.email.as_deref() {
            if !email.trim().is_empty() {
                known.insert(normalize_key(email));
            }
        }
    }

    let mut out: Vec<Suggestion> = correspondents
        .iter()
        .filter(|(call, _)| {
            let c = call.trim();
            !c.is_empty() && !known.contains(&normalize_key(c))
        })
        .map(|(call, count)| Suggestion {
            callsign: call.trim().to_string(),
            message_count: *count,
        })
        .collect();

    // Sort by count DESC, tie-break alphabetical (case-insensitive) for a
    // deterministic, stable order.
    out.sort_by(|a, b| {
        b.message_count
            .cmp(&a.message_count)
            .then_with(|| {
                a.callsign
                    .to_ascii_uppercase()
                    .cmp(&b.callsign.to_ascii_uppercase())
            })
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(callsign: &str, email: Option<&str>) -> Contact {
        Contact {
            id: format!("id-{callsign}"),
            name: "Test".to_string(),
            callsign: callsign.to_string(),
            email: email.map(|e| e.to_string()),
            tactical: None,
            notes: None,
            tier: crate::contacts::reachability::ContactTier::Confirmed,
            origin: crate::contacts::reachability::Origin::Manual,
            grid: None,
            channels: vec![],
            endpoints: vec![],
            created_at: "2026-06-07T12:00:00+00:00".to_string(),
            updated_at: "2026-06-07T12:00:00+00:00".to_string(),
        }
    }

    fn corr(pairs: &[(&str, u32)]) -> Vec<(String, u32)> {
        pairs.iter().map(|(c, n)| (c.to_string(), *n)).collect()
    }

    #[test]
    fn empty_input_yields_empty() {
        let out = derive_suggestions(&[], &[], "W1OP");
        assert!(out.is_empty());
    }

    #[test]
    fn operator_own_callsign_is_excluded() {
        // H11: the operator is the From on Sent/Outbox; never suggest self.
        let c = corr(&[("W1OP", 9), ("W6ABC", 2)]);
        let out = derive_suggestions(&c, &[], "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(calls, vec!["W6ABC"], "operator's own callsign must not be suggested");
    }

    #[test]
    fn operator_callsign_excluded_case_insensitively_and_winlink_variant() {
        // The operator's @winlink.org and lowercase forms are also excluded.
        let c = corr(&[
            ("w1op", 5),
            ("W1OP@winlink.org", 3),
            ("KE7VAR", 1),
        ]);
        let out = derive_suggestions(&c, &[], "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(calls, vec!["KE7VAR"]);
    }

    #[test]
    fn existing_contact_excludes_bare_and_winlink_email_variants() {
        // H11: contact W6ABC suppresses a suggestion for w6abc@winlink.org AND W6ABC.
        let existing = vec![contact("W6ABC", None)];
        let c = corr(&[
            ("W6ABC", 4),
            ("w6abc@winlink.org", 3),
            ("KE7VAR", 1),
        ]);
        let out = derive_suggestions(&c, &existing, "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(calls, vec!["KE7VAR"], "both W6ABC forms must be suppressed");
    }

    #[test]
    fn existing_contact_email_field_also_excludes_correspondent() {
        // A contact whose EMAIL is w6abc@winlink.org suppresses a bare W6ABC
        // correspondent (and vice-versa), since the email normalizes to W6ABC.
        let existing = vec![contact("Some Name Callsign", Some("w6abc@winlink.org"))];
        let c = corr(&[("W6ABC", 4), ("KE7VAR", 1)]);
        let out = derive_suggestions(&c, &existing, "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(calls, vec!["KE7VAR"]);
    }

    #[test]
    fn ssid_variant_is_not_merged() {
        // M5/SSID-is-identity: W6ABC and W6ABC-7 are DIFFERENT identities. An
        // existing contact W6ABC must NOT suppress a W6ABC-7 correspondent.
        let existing = vec![contact("W6ABC", None)];
        let c = corr(&[("W6ABC-7", 5), ("W6ABC", 2)]);
        let out = derive_suggestions(&c, &existing, "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(
            calls,
            vec!["W6ABC-7"],
            "SSID variant must remain a distinct, still-suggested identity"
        );
    }

    #[test]
    fn winlink_email_normalizes_with_ssid_intact() {
        // The @winlink.org form of an SSID-bearing callsign normalizes to the
        // bare-WITH-SSID callsign — so a contact W6ABC-7 suppresses
        // w6abc-7@winlink.org but NOT w6abc@winlink.org.
        let existing = vec![contact("W6ABC-7", None)];
        let c = corr(&[
            ("w6abc-7@winlink.org", 6),
            ("w6abc@winlink.org", 2),
        ]);
        let out = derive_suggestions(&c, &existing, "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(
            calls,
            vec!["w6abc@winlink.org"],
            "only the matching-SSID winlink form is suppressed"
        );
    }

    #[test]
    fn non_winlink_email_is_distinct_from_bare_callsign() {
        // A foreign SMTP address (w6abc@gmail.com) is NOT normalized to the
        // bare callsign — it is a distinct identity. A contact W6ABC does NOT
        // suppress it.
        let existing = vec![contact("W6ABC", None)];
        let c = corr(&[("w6abc@gmail.com", 3)]);
        let out = derive_suggestions(&c, &existing, "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(calls, vec!["w6abc@gmail.com"]);
    }

    #[test]
    fn sorted_by_count_desc_then_alphabetical() {
        let c = corr(&[
            ("KE7VAR", 1),
            ("W6ABC", 5),
            ("N0CALL", 5), // tie with W6ABC at count 5 → alphabetical: N0CALL before W6ABC
            ("AA1AA", 3),
        ]);
        let out = derive_suggestions(&c, &[], "W1OP");
        let pairs: Vec<(&str, u32)> = out
            .iter()
            .map(|s| (s.callsign.as_str(), s.message_count))
            .collect();
        assert_eq!(
            pairs,
            vec![("N0CALL", 5), ("W6ABC", 5), ("AA1AA", 3), ("KE7VAR", 1)]
        );
    }

    #[test]
    fn preserves_message_count_and_verbatim_callsign() {
        let c = corr(&[("ke7var", 7)]);
        let out = derive_suggestions(&c, &[], "W1OP");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].callsign, "ke7var", "callsign preserved verbatim");
        assert_eq!(out[0].message_count, 7);
    }

    #[test]
    fn blank_correspondents_are_skipped() {
        let c = corr(&[("   ", 4), ("W6ABC", 1)]);
        let out = derive_suggestions(&c, &[], "W1OP");
        let calls: Vec<&str> = out.iter().map(|s| s.callsign.as_str()).collect();
        assert_eq!(calls, vec!["W6ABC"]);
    }

    #[test]
    fn empty_operator_callsign_does_not_over_exclude() {
        // An unset operator callsign (offline/pre-wizard) must not silently
        // exclude correspondents via a blank normalized key.
        let c = corr(&[("W6ABC", 2), ("KE7VAR", 1)]);
        let out = derive_suggestions(&c, &[], "");
        assert_eq!(out.len(), 2);
    }
}
