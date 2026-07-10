//! Shared callsign normalization + validation.
//!
//! Four DISTINCT concerns (spec §1, [R5-10]) — do not merge them:
//! - `canonical_base` — the peers-store dedup anchor. NEVER a wire source.
//! - `validate_wire_callsign` — transport grammar for MYCALL/CONNECT targets.
//! - `validate_presented_callsign` — stored presented callsign with portable
//!   suffix (write boundary, later tasks 8/18).
//! - `sanitize_display` — broad injection floor for anything crossing the
//!   agent DTO or a render boundary. Rejects '/' outright as a path separator.

/// SSID-ish tails stripped by [`canonical_base`]: `-0`..`-15`, `-T`, `-R`,
/// and WLE's off-doc `-L` (post-office). Anything else is NOT an SSID.
fn is_ssid_tail(tail: &str) -> bool {
    matches!(tail, "T" | "R" | "L")
        || tail.parse::<u8>().map(|n| n <= 15).unwrap_or(false)
}

/// Dedup anchor: uppercase, trim, take the substring before the first `/`,
/// then strip one trailing SSID tail. Spec §1: "Never used to derive a wire
/// target."
pub fn canonical_base(presented: &str) -> String {
    let up = presented.trim().to_ascii_uppercase();
    let before_slash = up.split('/').next().unwrap_or("");
    if let Some((head, tail)) = before_slash.rsplit_once('-') {
        if is_ssid_tail(tail) && !head.is_empty() {
            return head.to_string();
        }
    }
    before_slash.to_string()
}

/// Transport wire grammar [R3-9]: base 3-7 chars A-Z0-9, optional SSID
/// `-1..-15` / `-T` / `-R`. Rejects 8-char bases and `-16`. Applied before
/// any MYCALL / CONNECT send.
pub fn validate_wire_callsign(s: &str) -> Result<(), String> {
    let s = s.trim().to_ascii_uppercase();
    let (base, ssid) = match s.split_once('-') {
        Some((b, t)) => (b, Some(t)),
        None => (s.as_str(), None),
    };
    if !(3..=7).contains(&base.len()) {
        return Err(format!("callsign base must be 3-7 chars, got {:?}", base));
    }
    if !base.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("callsign base must be A-Z0-9, got {:?}", base));
    }
    match ssid {
        None => Ok(()),
        Some("T") | Some("R") => Ok(()),
        Some(t) => match t.parse::<u8>() {
            Ok(n) if (1..=15).contains(&n) => Ok(()),
            _ => Err(format!("invalid SSID {:?} (allowed: -1..-15, -T, -R)", t)),
        },
    }
}

/// Stored presented callsign validator: base 3-7 chars A-Z0-9, optional
/// portable suffix `/SUFFIX`, and optional SSID `-1..-15` / `-T` / `-R` / `-L`.
/// Applied at write boundaries (tasks 8/18) to preserve legitimate portable
/// forms like `W6ABC/P`, `W6ABC/MM`, `W6ABC/P-7`.
pub fn validate_presented_callsign(s: &str) -> Result<(), String> {
    let s_upper = s.trim().to_ascii_uppercase();

    // Split on '-' from right to separate SSID
    let (base_and_suffix, ssid) = match s_upper.rsplit_once('-') {
        Some((bas, sid)) => (bas, Some(sid)),
        None => (s_upper.as_str(), None),
    };

    // Split on '/' to separate base from suffix
    let (base, suffix) = match base_and_suffix.split_once('/') {
        Some((b, suf)) => (b, Some(suf)),
        None => (base_and_suffix, None),
    };

    // Validate base
    if !(3..=7).contains(&base.len()) {
        return Err(format!("callsign base must be 3-7 chars, got {:?}", base));
    }
    if !base.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("callsign base must be A-Z0-9, got {:?}", base));
    }

    // Validate suffix if present
    if let Some(suf) = suffix {
        if suf.is_empty() || !suf.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("portable suffix must be alphanumeric, got {:?}", suf));
        }
    }

    // Validate SSID if present
    match ssid {
        None => Ok(()),
        Some("T") | Some("R") | Some("L") => Ok(()),
        Some(t) => match t.parse::<u8>() {
            Ok(n) if (1..=15).contains(&n) => Ok(()),
            _ => Err(format!("invalid SSID {:?} (allowed: -1..-15, -T, -R, -L)", t)),
        },
    }
}

/// Broad display/injection sanitizer [R5-10][R2-S2][R2-S10]: the floor for
/// every peer-derived string crossing the agent DTO or a render/keyring
/// boundary. Rejects '/' outright as a path separator. Returns the trimmed
/// string, or `None` = reject/drop.
pub fn sanitize_display(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    if t.contains("..") {
        return None; // path traversal
    }
    for c in t.chars() {
        if c.is_control()
            || c.is_whitespace()
            || matches!(c, ':' | '\\' | '<' | '>' | '"' | '\'' | '`' | '/')
        {
            return None;
        }
    }
    Some(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_base_strips_ssid_and_portable_suffix() {
        assert_eq!(canonical_base("w6abc-7"), "W6ABC");
        assert_eq!(canonical_base("W6ABC/P"), "W6ABC");
        assert_eq!(canonical_base("W6ABC/P-7"), "W6ABC"); // slash first, then SSID
        assert_eq!(canonical_base("N0DAJ-T"), "N0DAJ");
        assert_eq!(canonical_base("N0DAJ-R"), "N0DAJ");
        assert_eq!(canonical_base("N0DAJ-L"), "N0DAJ"); // WLE off-doc post-office suffix
        assert_eq!(canonical_base("N0DAJ-0"), "N0DAJ");
        assert_eq!(canonical_base("N0DAJ-15"), "N0DAJ");
        assert_eq!(canonical_base("  n0daj "), "N0DAJ");
    }

    #[test]
    fn canonical_base_does_not_strip_non_ssid_tails() {
        // "-16" is not a valid SSID; a tactical hyphenated name keeps its tail.
        assert_eq!(canonical_base("N0DAJ-16"), "N0DAJ-16");
        assert_eq!(canonical_base("CAMP-OPS"), "CAMP-OPS");
    }

    #[test]
    fn wire_grammar_accepts_valid_and_rejects_invalid() {
        assert!(validate_wire_callsign("W6ABC").is_ok());
        assert!(validate_wire_callsign("W6ABC-7").is_ok());
        assert!(validate_wire_callsign("W6ABC-15").is_ok());
        assert!(validate_wire_callsign("W6ABC-T").is_ok());
        assert!(validate_wire_callsign("W6ABC-R").is_ok());
        assert!(validate_wire_callsign("AB1").is_ok());       // 3-char base
        assert!(validate_wire_callsign("AB1CDEF").is_ok());   // 7-char base
        assert!(validate_wire_callsign("AB1CDEFG").is_err()); // 8-char base [R3-9]
        assert!(validate_wire_callsign("W6ABC-16").is_err()); // SSID > 15 [R3-9]
        assert!(validate_wire_callsign("W6").is_err());       // too short
        assert!(validate_wire_callsign("W6:ABC").is_err());   // charset
        assert!(validate_wire_callsign("").is_err());
    }

    #[test]
    fn presented_grammar_accepts_portable_suffix_and_ssid() {
        // Valid presented forms with portable suffix
        assert!(validate_presented_callsign("W6ABC/P").is_ok());
        assert!(validate_presented_callsign("W6ABC/M").is_ok());
        assert!(validate_presented_callsign("W6ABC/MM").is_ok());
        // With SSID
        assert!(validate_presented_callsign("W6ABC/P-7").is_ok());
        assert!(validate_presented_callsign("W6ABC-T").is_ok());
        assert!(validate_presented_callsign("W6ABC-L").is_ok());
        // Must still reject invalid bases
        assert!(validate_presented_callsign("AB1CDEFG").is_err()); // 8-char base
        assert!(validate_presented_callsign("W6").is_err());       // too short
    }

    #[test]
    fn sanitize_display_rejects_injection_shapes() {
        // Broad display/injection floor [R5-10]: control chars, ':', path
        // separators, whitespace, angle brackets are rejected outright.
        assert_eq!(sanitize_display("W6ABC-7"), Some("W6ABC-7".to_string()));
        assert_eq!(sanitize_display("W6ABC/P"), None); // '/' is path separator; rejected
        assert_eq!(sanitize_display("<img src=x>"), None);
        assert_eq!(sanitize_display("A:B"), None);
        assert_eq!(sanitize_display("A B"), None);
        assert_eq!(sanitize_display("A\u{0}B"), None);
        assert_eq!(sanitize_display("..\\x"), None);
        assert_eq!(sanitize_display("a/../b"), None);
        assert_eq!(sanitize_display(""), None);
        assert_eq!(sanitize_display(&"X".repeat(65)), None); // length cap 64
    }

    #[test]
    fn division_of_labor_presented_vs_wire_vs_display() {
        // Portable suffix: accepted by presented, rejected by wire, rejected by display
        assert!(validate_presented_callsign("W6ABC/P").is_ok());
        assert!(validate_wire_callsign("W6ABC/P").is_err());
        assert_eq!(sanitize_display("W6ABC/P"), None);
    }
}
