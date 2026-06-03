//! Banner-phrase parser — detect whether the remote end of a B2F session is
//! a CMS, an RMS Relay, or some other variant, by inspecting the human-
//! readable banner lines the remote emits AFTER the SID handshake and
//! BEFORE the `CMS>` prompt that begins the message turn.
//!
//! ## Why this is brittle and how we keep it less brittle
//!
//! WLE's B2 protocol layer (`B2Protocol.cs:2050-2079` in the
//! `RMS_Express_v11.0.0.0` decompile) distinguishes "talking to a relay"
//! at the wire layer entirely via plaintext English sentences in the
//! banner. There is no machine-readable header, no JSON envelope, no
//! length prefix — just byte-exact case-sensitive prefix or substring
//! matching on the operator-readable text. The exact wording matters,
//! including the *two spaces* after the period in
//! `"NETWORK POST OFFICE.  MESSAGES WILL BE STORED LOCALLY"`.
//!
//! Tuxlink replicates that parser because (a) it's the only mechanism
//! WLE itself uses to distinguish these classes of remote, and (b) any
//! relay that exists today was tested against WLE — so emitting these
//! exact phrases is the de-facto interop contract.
//!
//! See [`dev/scratch/winlink-re/findings/client-of-rms-relay.md`] §2.2
//! for the full decompile-grounded table.
//!
//! ## What we deliberately diverge on
//!
//! - Constant-time comparisons are NOT used. The banner text is public
//!   protocol content emitted in cleartext over a (usually) plaintext
//!   TCP connection. Side-channels are not a concern here.
//! - The WLE typo `"Messages will by stored on the hub"` (§10.4 of the
//!   deep-dive) is NOT carried forward. We use the correct
//!   `"Messages will be stored on the hub"` in our own modal text.
//!   The PARSER still matches WLE's intended phrases (which don't
//!   contain that typo).

/// What the post-connect banner says the remote IS.
///
/// Mirrors WLE's `B2RMSRelayState` enum (`B2Protocol.cs:25-35`), with
/// the addition of [`NotRelay`] which WLE represents implicitly via
/// `enmB2RMSRelayState == NotRMSRelay` (its default initial state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayState {
    /// No banner phrase matched a relay self-identifier; the remote
    /// looks like an ordinary CMS endpoint (the `CMS>` prompt, or the
    /// `CMS via <relay>` form when a relay is acting as a transparent
    /// proxy to an internet-reachable CMS).
    NotRelay,

    /// The remote is a local-database / store-and-forward post-office.
    /// Triggered by:
    /// - `"THIS IS AN RMS POST OFFICE"`
    /// - `"NETWORK POST OFFICE.  MESSAGES WILL BE STORED LOCALLY"`
    ///   (note: TWO spaces after the period — WLE's exact form)
    /// - `"THIS IS A RADIO-ONLY HUB. MESSAGES WILL NOT BE SENT"`
    LocalDatabase,

    /// The remote is a Radio-network hub (no internet leg).
    /// Triggered by `"THIS IS A RADIO NETWORK HUB"`.
    RadioNetwork,

    /// The remote is a hybrid Radio + internet hub.
    /// Triggered by `"THIS IS A RADIO/INTERNET NETWORK HUB"`.
    RadioNetworkAndInternet,

    /// Any banner phrase saying CMS routing is currently unavailable
    /// (the relay is holding messages until internet returns).
    /// Triggered by any of:
    /// - `"NO CMS CONNECTION IS CURRENTLY AVAILABLE"` (substring)
    /// - `"MESSAGES WILL BE HELD UNTIL"` (substring)
    /// - `"MESSAGES WILL BE FORWARDED"` (substring)
    NoCmsConnectionAvailable,
}

/// Classify one banner line into a [`RelayState`] transition.
///
/// Returns [`None`] when the line is not a recognized banner phrase
/// (most banner lines aren't — the parser must be called per-line and
/// the caller threads the state across the whole banner block).
///
/// ## Matching rules
///
/// - `CMS>` and `"CMS via "` use prefix (`starts_with`) matching per
///   `B2Protocol.cs:1879-1883`.
/// - The "POST OFFICE" / "RADIO-ONLY HUB" / "RADIO NETWORK HUB" /
///   "RADIO/INTERNET NETWORK HUB" phrases use prefix matching (the
///   relay emits them as the first chars of a line).
/// - `"NO CMS CONNECTION IS CURRENTLY AVAILABLE"`,
///   `"MESSAGES WILL BE HELD UNTIL"`, and `"MESSAGES WILL BE FORWARDED"`
///   use substring (`contains`) matching per L2068-2079.
/// - All matching is case-sensitive. The decompile uses
///   `String.StartsWith`/`Contains` without `StringComparison.OrdinalIgnoreCase`.
pub fn classify_banner_line(line: &str) -> Option<RelayState> {
    // Prefix matches — order matters for the "POST OFFICE" group because
    // `"NETWORK POST OFFICE.  MESSAGES WILL BE STORED LOCALLY"` shares a
    // substring with the generic POST OFFICE prefix.
    if line.starts_with("CMS>") || line.starts_with("CMS via ") {
        return Some(RelayState::NotRelay);
    }
    if line.starts_with("THIS IS AN RMS POST OFFICE")
        || line.starts_with("NETWORK POST OFFICE.  MESSAGES WILL BE STORED LOCALLY")
        || line.starts_with("THIS IS A RADIO-ONLY HUB. MESSAGES WILL NOT BE SENT")
    {
        return Some(RelayState::LocalDatabase);
    }
    // The two RADIO NETWORK forms differ by one slash — order them so the
    // more-specific RADIO/INTERNET phrase wins. `starts_with("THIS IS A RADIO NETWORK HUB")`
    // would also match the start of `"THIS IS A RADIO/INTERNET NETWORK HUB"` if
    // the `/` weren't there, so verify the discriminator survives.
    if line.starts_with("THIS IS A RADIO/INTERNET NETWORK HUB") {
        return Some(RelayState::RadioNetworkAndInternet);
    }
    if line.starts_with("THIS IS A RADIO NETWORK HUB") {
        return Some(RelayState::RadioNetwork);
    }
    // Substring matches.
    if line.contains("NO CMS CONNECTION IS CURRENTLY AVAILABLE")
        || line.contains("MESSAGES WILL BE HELD UNTIL")
        || line.contains("MESSAGES WILL BE FORWARDED")
    {
        return Some(RelayState::NoCmsConnectionAvailable);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cms_prompt_classifies_as_not_relay() {
        assert_eq!(
            classify_banner_line("CMS>"),
            Some(RelayState::NotRelay),
        );
    }

    #[test]
    fn cms_via_form_classifies_as_not_relay() {
        // `B2Protocol.cs:1882` — `CMS via <relay-call>` means the relay is
        // acting as a transparent proxy to a reachable CMS; the local
        // mailbox should treat the session as CMS.
        assert_eq!(
            classify_banner_line("CMS via W7RMS"),
            Some(RelayState::NotRelay),
        );
    }

    #[test]
    fn post_office_prefix_classifies_as_local_database() {
        assert_eq!(
            classify_banner_line("THIS IS AN RMS POST OFFICE"),
            Some(RelayState::LocalDatabase),
        );
    }

    #[test]
    fn network_post_office_two_spaces_classifies_as_local_database() {
        // The exact byte-sensitive phrase from B2Protocol.cs:2050. Two
        // spaces after the period. If a reviewer "normalizes" this to one
        // space, the parser will silently miss real-world banners.
        let exact = "NETWORK POST OFFICE.  MESSAGES WILL BE STORED LOCALLY";
        assert!(exact.contains(".  "), "test fixture lost its two-space form");
        assert_eq!(classify_banner_line(exact), Some(RelayState::LocalDatabase));
    }

    #[test]
    fn network_post_office_one_space_does_not_classify() {
        // Defends the byte-exact match: a banner with just one space
        // after the period would NOT be WLE-emitted, and our parser
        // should not classify it.
        let one_space = "NETWORK POST OFFICE. MESSAGES WILL BE STORED LOCALLY";
        assert_eq!(classify_banner_line(one_space), None);
    }

    #[test]
    fn radio_only_hub_classifies_as_local_database() {
        assert_eq!(
            classify_banner_line("THIS IS A RADIO-ONLY HUB. MESSAGES WILL NOT BE SENT"),
            Some(RelayState::LocalDatabase),
        );
    }

    #[test]
    fn radio_network_hub_classifies_as_radio_network() {
        assert_eq!(
            classify_banner_line("THIS IS A RADIO NETWORK HUB"),
            Some(RelayState::RadioNetwork),
        );
    }

    #[test]
    fn radio_internet_hub_classifies_as_radio_network_and_internet() {
        assert_eq!(
            classify_banner_line("THIS IS A RADIO/INTERNET NETWORK HUB"),
            Some(RelayState::RadioNetworkAndInternet),
        );
    }

    #[test]
    fn radio_internet_hub_does_not_collide_with_radio_network_hub() {
        // Both prefixes start with `THIS IS A RADIO` — verify the more-
        // specific `RADIO/INTERNET` variant wins on its own line and
        // the less-specific `RADIO NETWORK HUB` does not also fire.
        let result =
            classify_banner_line("THIS IS A RADIO/INTERNET NETWORK HUB OPERATED BY W7RMS");
        assert_eq!(result, Some(RelayState::RadioNetworkAndInternet));
    }

    #[test]
    fn no_cms_substring_classifies_as_no_cms_available() {
        // Substring match, not prefix — this can appear anywhere in the
        // banner text, often after a station-id prefix.
        assert_eq!(
            classify_banner_line(
                "RMS Relay 1.4.5: NO CMS CONNECTION IS CURRENTLY AVAILABLE at this hub",
            ),
            Some(RelayState::NoCmsConnectionAvailable),
        );
    }

    #[test]
    fn messages_held_until_substring_classifies_as_no_cms_available() {
        assert_eq!(
            classify_banner_line("MESSAGES WILL BE HELD UNTIL CMS connection returns"),
            Some(RelayState::NoCmsConnectionAvailable),
        );
    }

    #[test]
    fn messages_forwarded_substring_classifies_as_no_cms_available() {
        assert_eq!(
            classify_banner_line("This is an RMS Relay: MESSAGES WILL BE FORWARDED to CMS"),
            Some(RelayState::NoCmsConnectionAvailable),
        );
    }

    #[test]
    fn unrelated_banner_line_does_not_classify() {
        // Most lines of a banner block are not relay self-identifiers
        // (version strings, MOTD, etc.). The parser MUST return None
        // for them so the caller can scan past without state changes.
        assert_eq!(classify_banner_line("Welcome to W7RMS"), None);
        assert_eq!(classify_banner_line(""), None);
        assert_eq!(classify_banner_line("> "), None);
    }

    #[test]
    fn case_sensitivity_is_preserved() {
        // WLE matches without OrdinalIgnoreCase, so lowercase variants
        // do NOT classify. This pins that behavior — if a relay starts
        // emitting lowercase, the matchers won't fire, mirroring WLE.
        assert_eq!(classify_banner_line("cms>"), None);
        assert_eq!(
            classify_banner_line("this is a radio network hub"),
            None,
        );
    }
}
