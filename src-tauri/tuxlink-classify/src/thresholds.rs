//! The two T0-owned thresholds (ADR 0030): plain numbers in deterministic
//! config, calibrated per (corpus, model, template). The ML supplies scores,
//! never verdicts; calibration is a config REGENERATION when a corpus or
//! model changes — never retraining.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The two numbers. Both come from a calibration run's measured
/// distributions (reject gap / margin classes), never from intuition.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Top-1 cosine similarity below this ⇒ `NoMatch` (the "not actually a
    /// catalog request" floor; bge-small's zero-overlap reject gap is what
    /// makes this workable per the T1 spike).
    pub reject_floor: f32,
    /// top1 − top2 under this ⇒ `Ambiguous` (genuinely-close candidates —
    /// ask one clarifying question instead of free-soloing the tie-break;
    /// this is the upstream absorber for the parseability spike's residual
    /// confident-pick-on-tie class).
    pub ask_margin: f32,
}

/// Calibration table key: `corpus/model/template`.
pub type ThresholdKey = String;

pub fn threshold_key(corpus: &str, model: &str, template: &str) -> ThresholdKey {
    format!("{corpus}/{model}/{template}")
}

/// The on-disk table (JSON object keyed by [`threshold_key`]). A missing
/// entry is a LOUD error at classify time, not a silent default — verdicts
/// minted from uncalibrated numbers are the failure mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThresholdTable(pub BTreeMap<ThresholdKey, Thresholds>);

impl ThresholdTable {
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn lookup(&self, corpus: &str, model: &str, template: &str) -> Option<Thresholds> {
        self.0.get(&threshold_key(corpus, model, template)).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped calibration asset. Provenance for the values:
    /// dev/evals/2026-08-10-ch3e9-t1-floor-calibration.md (R2 run, measured
    /// reject gap + margin classes — never invented numbers).
    const SHIPPED: &str =
        include_str!("../../resources/catalog/classify-thresholds.json");

    /// Recalibration tooth: the shipped table must carry an entry for the
    /// CURRENT (corpus, model, template) triple, for BOTH shipped corpora.
    /// Bumping EMBED_TEMPLATE_VERSION (or swapping the model) without
    /// running the calibration evals and updating the asset fails here —
    /// ADR 0030's "threshold rot" watched failure mode, enforced.
    #[test]
    fn shipped_table_is_calibrated_for_current_template() {
        let table = ThresholdTable::from_json(SHIPPED).expect("asset parses");
        for corpus in ["winlink-catalog", "tuxlink-tools"] {
            let th = table
                .lookup(
                    corpus,
                    "bge-small-en-v1.5",
                    crate::enriched::EMBED_TEMPLATE_VERSION,
                )
                .unwrap_or_else(|| {
                    panic!("no calibration entry for {corpus} at the current template")
                });
            // Sanity bounds, not re-derivation: a floor outside the measured
            // reject gap or a non-positive margin means the asset was mangled.
            assert!(th.reject_floor > 0.0 && th.reject_floor < 1.0);
            assert!(th.ask_margin > 0.0 && th.ask_margin < 0.5);
        }
    }

    #[test]
    fn table_roundtrip_and_lookup() {
        let json = r#"{"winlink-catalog/bge-small-en-v1.5/enriched-v1":
                        {"reject_floor":0.68,"ask_margin":0.04}}"#;
        let t = ThresholdTable::from_json(json).unwrap();
        let th = t
            .lookup("winlink-catalog", "bge-small-en-v1.5", "enriched-v1")
            .unwrap();
        assert_eq!(th.reject_floor, 0.68);
        assert_eq!(th.ask_margin, 0.04);
        assert!(t.lookup("winlink-catalog", "minilm-l6", "enriched-v1").is_none());
    }
}
