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
