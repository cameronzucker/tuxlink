//! `@`-reference tokens and variable paths (spec §14 conventions).

use std::fmt;

use crate::types::StepId;

/// A named-entity reference: `@station-set:or-gateways`, `@preset:winlink-40m`.
/// These are what reference validation (plan 3) resolves.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub kind: String,
    pub name: String,
}

impl EntityRef {
    pub fn parse(s: &str) -> Option<Self> {
        let rest = s.strip_prefix('@')?;
        let (kind, name) = rest.split_once(':')?;
        if kind.is_empty() || name.is_empty() {
            return None;
        }
        Some(EntityRef {
            kind: kind.to_string(),
            name: name.to_string(),
        })
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}:{}", self.kind, self.name)
    }
}

/// A step-output path: `s1.connected`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarPath {
    pub step: StepId,
    pub output: String,
}

impl VarPath {
    pub fn parse(s: &str) -> Option<Self> {
        let (step, output) = s.split_once('.')?;
        if step.is_empty() || output.is_empty() {
            return None;
        }
        Some(VarPath {
            step: StepId(step.to_string()),
            output: output.to_string(),
        })
    }
}

/// Scan a string for embedded `$path` tokens (tuxlink-6epl8 second
/// absorption; battery S1: qwen wrote
/// `"connected=$s3.connected, station=$s3.station"` into a log message and
/// got literal text). `path = [a-z0-9_]+(\.[A-Za-z0-9_]+)*`, hand-scanned —
/// no regex dependency. Returns `(byte offset of the '$', path)` pairs in
/// order; shared by the executor (interpolation) and the validator
/// (EMBEDDED_REF_IGNORED) so the warning can never disagree with the
/// runtime. A `$` not followed by a path yields nothing, while `"$50"` DOES
/// scan (digits are path chars) — resolution simply fails downstream and the
/// text stays verbatim, so a dollar amount is never mangled.
pub fn scan_embedded_refs(s: &str) -> Vec<(usize, &str)> {
    fn is_first_seg(b: u8) -> bool {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
    }
    fn is_later_seg(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && is_first_seg(bytes[j]) {
            j += 1;
        }
        if j == start {
            i += 1;
            continue;
        }
        // Subsequent `.segment`s; a dot not followed by a segment char stays
        // outside the path ("see $s2.status." keeps its full stop).
        while j < bytes.len() && bytes[j] == b'.' {
            let seg = j + 1;
            let mut k = seg;
            while k < bytes.len() && is_later_seg(bytes[k]) {
                k += 1;
            }
            if k == seg {
                break;
            }
            j = k;
        }
        out.push((i, &s[start..j]));
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entity_refs() {
        let r = EntityRef::parse("@station-set:or-gateways").unwrap();
        assert_eq!(r.kind, "station-set");
        assert_eq!(r.name, "or-gateways");
        assert_eq!(r.to_string(), "@station-set:or-gateways");
    }

    #[test]
    fn non_refs_are_none() {
        assert!(EntityRef::parse("plain string").is_none());
        assert!(EntityRef::parse("@missing-colon").is_none());
        assert!(EntityRef::parse("@:empty-kind").is_none());
        assert!(EntityRef::parse("@kind:").is_none());
    }

    #[test]
    fn parses_var_paths() {
        let v = VarPath::parse("s1.connected").unwrap();
        assert_eq!(v.step.0, "s1");
        assert_eq!(v.output, "connected");
        assert!(VarPath::parse("nodot").is_none());
        assert!(VarPath::parse(".leading").is_none());
        assert!(VarPath::parse("trailing.").is_none());
    }

    /// tuxlink-6epl8: the embedded-token scanner — the qwen battery line,
    /// boundary punctuation, first-segment case rules, and the harmless
    /// dollar-amount scan.
    #[test]
    fn scans_embedded_ref_tokens() {
        assert_eq!(
            scan_embedded_refs("connected=$s3.connected, station=$s3.station"),
            vec![(10, "s3.connected"), (33, "s3.station")]
        );
        assert_eq!(scan_embedded_refs("$s1.connected"), vec![(0, "s1.connected")]);
        assert_eq!(scan_embedded_refs("no refs, lone $ sign"), vec![]);
        // Digits are path chars: "$50" scans, resolution fails downstream,
        // text stays verbatim.
        assert_eq!(scan_embedded_refs("$50 total"), vec![(0, "50")]);
        // Later segments allow uppercase; the FIRST segment does not.
        assert_eq!(scan_embedded_refs("x $s1.Alpha_2 y"), vec![(2, "s1.Alpha_2")]);
        assert_eq!(scan_embedded_refs("x $S1.alpha y"), vec![]);
        // A trailing dot is punctuation, not path.
        assert_eq!(scan_embedded_refs("see $s2.status."), vec![(4, "s2.status")]);
        // "$$…": the first '$' matches nothing; the second starts a token.
        assert_eq!(scan_embedded_refs("$$s1.x"), vec![(1, "s1.x")]);
        // Nested output paths keep every segment.
        assert_eq!(
            scan_embedded_refs("k=$s1.indices.k_index!"),
            vec![(2, "s1.indices.k_index")]
        );
    }
}
