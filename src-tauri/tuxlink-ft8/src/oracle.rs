//! Oracle-parity comparator (M3, plan T3.2): the permanent regression harness
//! that scores this crate's decode set against a reference decode list produced
//! by an independent decoder (WSJT-X `jt9 -8`, AP disabled).
//!
//! The harness is passive, reproducible, and TX-free: it parses a committed
//! `.jt9-ap-off.txt` reference log, compares it against [`crate::sync::decode_samples`]
//! output as a **multiset on normalized message identity**, and reports parity %
//! (recall against the reference) plus the false-decode count. The L0 exit gate
//! (plan §M3/M4) is `parity ≥ 85 %` with **zero false decodes** on each capture.
//!
//! # Match rules (plan T3.2)
//!
//! - **Normalized identity.** Fields are single-space-joined and trimmed
//!   ([`crate::message::normalize_message`]); jt9 emits the same spacing, so a
//!   standard / free-text / telemetry message matches its reference verbatim.
//! - **Hashed-callsign class.** A hashed callsign renders as `<...>` (unresolved)
//!   or `<CALL>` (resolved from the slot hash table). It is a truncated hash the
//!   passive decoder cannot verify against the reference by content, so every
//!   bracketed callsign token is collapsed to `<*>` before matching — all
//!   bracketed tokens form one equivalence class. The **non-hashed** fields must
//!   still match exactly, so this never masks a wrong callsign-pair, grid, or
//!   report.
//! - **Multiset.** Counts matter: a reference with the same message twice needs
//!   two of ours to fully match.
//!
//! # AP-disabled reference (why it is fair)
//!
//! The reference is `jt9 -8 <wav>` with no `--my-call`, so a-priori decoding is
//! inert (see `tests/fixtures/sdr/README.md`). AP decodes are messages WSJT-X
//! only recovers by assuming known bits mid-QSO, which a passive unaided decoder
//! structurally cannot reproduce; diffing against an AP-*enabled* reference would
//! rig the ≥85 % gate.

use crate::message::{message_identity, normalize_message};
use std::collections::HashMap;

/// The outcome of comparing a decode set against a reference decode list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParityResult {
    /// Number of reference (oracle) messages.
    pub reference_total: usize,
    /// Number of messages this crate decoded.
    pub decoded_total: usize,
    /// True positives: our decodes that matched a distinct reference message
    /// (multiset).
    pub matched: usize,
    /// Reference messages we did NOT recover (normalized, for reporting).
    pub missed: Vec<String>,
    /// Our decodes with no reference counterpart — the FALSE decodes the
    /// zero-false gate forbids (normalized, for reporting).
    pub false_decodes: Vec<String>,
}

impl ParityResult {
    /// Recall against the reference: `matched / reference_total`, as a percentage.
    /// An empty reference scores 100 % (nothing to recover).
    pub fn parity_pct(&self) -> f64 {
        if self.reference_total == 0 {
            100.0
        } else {
            self.matched as f64 / self.reference_total as f64 * 100.0
        }
    }

    /// The false-decode count (`false_decodes.len()`); the zero-false gate
    /// requires this to be 0.
    pub fn false_count(&self) -> usize {
        self.false_decodes.len()
    }

    /// The L0 exit gate: recall ≥ `min_parity` percent AND zero false decodes.
    pub fn passes(&self, min_parity: f64) -> bool {
        self.parity_pct() >= min_parity && self.false_count() == 0
    }

    /// A one-line human summary for test/CI output.
    pub fn summary(&self) -> String {
        format!(
            "parity {:.1}% ({} / {} matched), {} false, {} missed",
            self.parity_pct(),
            self.matched,
            self.reference_total,
            self.false_count(),
            self.missed.len()
        )
    }
}

/// The multiset match key for a message: [`message_identity`] — normalized with
/// every bracketed hashed-callsign token collapsed to `<*>`. This is the same key
/// the decoder's within-slot dedup uses, so "hashed callsign as its own class"
/// (module docs) and the dedup rule cannot drift apart.
fn match_key(msg: &str) -> String {
    message_identity(msg)
}

/// Parse a WSJT-X `jt9 -8` reference log into its list of decoded messages.
///
/// Each decode line is `"<SNR> <DT> <FREQ> ~ <MESSAGE>"`; the `~` is the FT-8
/// sync-mode marker and never appears inside a message (FT-8 messages are
/// upper-case alphanumerics plus `/ < > + - .` and spaces), so splitting on the
/// first `~` cleanly separates the metadata from the message. Blank lines and
/// lines without a `~` marker are ignored.
pub fn parse_reference_log(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (_meta, msg) = line.split_once('~')?;
            let msg = normalize_message(msg);
            if msg.is_empty() {
                None
            } else {
                Some(msg)
            }
        })
        .collect()
}

/// Compare a decode set against a reference decode list as a multiset on the
/// hash-class-aware match key. Returns true/false positives and the missed
/// reference messages.
pub fn compare(decoded: &[String], reference: &[String]) -> ParityResult {
    // Multiset pool of reference match keys, still holding an unmatched count.
    let mut pool: HashMap<String, usize> = HashMap::new();
    for m in reference {
        *pool.entry(match_key(m)).or_insert(0) += 1;
    }

    // Greedily consume each decode against the reference pool.
    let mut matched = 0usize;
    let mut false_decodes = Vec::new();
    for m in decoded {
        let key = match_key(m);
        match pool.get_mut(&key) {
            Some(c) if *c > 0 => {
                *c -= 1;
                matched += 1;
            }
            _ => false_decodes.push(normalize_message(m)),
        }
    }

    // Whatever remains in the pool is a reference message we missed; recover
    // representative strings by re-walking the reference in order.
    let mut residual = pool;
    let mut missed = Vec::new();
    for m in reference {
        let key = match_key(m);
        if let Some(c) = residual.get_mut(&key) {
            if *c > 0 {
                *c -= 1;
                missed.push(normalize_message(m));
            }
        }
    }

    ParityResult {
        reference_total: reference.len(),
        decoded_total: decoded.len(),
        matched,
        missed,
        false_decodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jt9_ap_off_lines() {
        let log = "\
-8 -0.5 2714 ~  RK0SK AE6CH CM97
-19 -0.5 1214 ~  <...> N2CUA EM95
";
        let msgs = parse_reference_log(log);
        assert_eq!(msgs, vec!["RK0SK AE6CH CM97", "<...> N2CUA EM95"]);
    }

    #[test]
    fn parse_ignores_blank_and_markerless_lines() {
        // A real leading blank line, a valid decode, then a marker-less line.
        let log = "\n-16 -1.1 2046 ~  CQ JE6HOG PM53\ngarbage line without a marker\n";
        assert_eq!(parse_reference_log(log), vec!["CQ JE6HOG PM53"]);
    }

    #[test]
    fn perfect_match_scores_100_zero_false() {
        let refs = vec!["CQ N5IF EM11".to_string(), "K0BQB WD8ASA +09".to_string()];
        let ours = vec!["K0BQB WD8ASA +09".to_string(), "CQ N5IF EM11".to_string()];
        let r = compare(&ours, &refs);
        assert_eq!(r.parity_pct(), 100.0);
        assert_eq!(r.false_count(), 0);
        assert!(r.missed.is_empty());
        assert!(r.passes(85.0));
    }

    #[test]
    fn partial_recall_and_false_decode() {
        let refs = vec![
            "CQ N5IF EM11".to_string(),
            "YD2BCR W7DGM DN37".to_string(),
            "9V1DX N7QT -06".to_string(),
        ];
        // Recover 2/3, and emit one message the reference does not contain.
        let ours = vec![
            "CQ N5IF EM11".to_string(),
            "9V1DX N7QT -06".to_string(),
            "BOGUS DECODE 73".to_string(),
        ];
        let r = compare(&ours, &refs);
        assert_eq!(r.matched, 2);
        assert!((r.parity_pct() - 66.6667).abs() < 0.01);
        assert_eq!(r.false_count(), 1);
        assert_eq!(r.false_decodes, vec!["BOGUS DECODE 73"]);
        assert_eq!(r.missed, vec!["YD2BCR W7DGM DN37"]);
        assert!(!r.passes(85.0));
    }

    #[test]
    fn hashed_callsign_is_its_own_class() {
        // Reference rendered the hash as <...>; we resolved it to <K1ABC>. The
        // non-hashed fields match, so it counts as a match (hash-class rule) —
        // not a miss + false pair.
        let refs = vec!["<...> N2CUA EM95".to_string()];
        let ours = vec!["<K1ABC> N2CUA EM95".to_string()];
        let r = compare(&ours, &refs);
        assert_eq!(r.matched, 1);
        assert_eq!(r.false_count(), 0);
        assert!(r.missed.is_empty());
    }

    #[test]
    fn hash_class_does_not_mask_a_wrong_verifiable_field() {
        // Same hashed slot, but the VERIFIABLE grid differs — must NOT match.
        let refs = vec!["<...> N2CUA EM95".to_string()];
        let ours = vec!["<...> N2CUA FN31".to_string()];
        let r = compare(&ours, &refs);
        assert_eq!(r.matched, 0);
        assert_eq!(r.false_count(), 1);
        assert_eq!(r.missed, vec!["<...> N2CUA EM95"]);
    }

    #[test]
    fn multiset_requires_matching_counts() {
        // Reference has the message once; we decoded it twice (a dedup escape).
        // One matches, the extra is a false decode.
        let refs = vec!["CQ AA7J DN30".to_string()];
        let ours = vec!["CQ AA7J DN30".to_string(), "CQ AA7J DN30".to_string()];
        let r = compare(&ours, &refs);
        assert_eq!(r.matched, 1);
        assert_eq!(r.false_count(), 1);
    }

    #[test]
    fn empty_reference_scores_100_but_false_decodes_still_fail_gate() {
        let refs: Vec<String> = vec![];
        let ours = vec!["CQ AA7J DN30".to_string()];
        let r = compare(&ours, &refs);
        assert_eq!(r.parity_pct(), 100.0);
        assert_eq!(r.false_count(), 1);
        assert!(!r.passes(85.0), "a false decode must fail the gate even at 100% recall");
    }
}
