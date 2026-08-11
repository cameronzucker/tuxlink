//! The catalog vector index and the classify entry point.
//!
//! Brute-force cosine over ~1.5k unit vectors (384-dim ≈ 2.3MB) — scan cost
//! is microseconds, an ANN structure would be pure complexity. The index is
//! serde-serializable so the integrating app can persist it and skip the
//! embed pass on startup (rebuild + threshold recalibration are triggered by
//! catalog/model/template changes, per ADR 0030).

use serde::{Deserialize, Serialize};

use crate::backend::EmbeddingBackend;
use crate::dto::{Advisory, AdvisoryVerdict, Candidate, ClassifyResult};
use crate::enriched::{embed_text, EnrichedEntry, GeoFacts, EMBED_TEMPLATE_VERSION};
use crate::thresholds::{threshold_key, ThresholdTable};
use crate::ClassifyError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedItem {
    pub id: String,
    pub section: String,
    pub title: String,
    /// T0 structured facts (never embedded); deterministic callers filter or
    /// re-rank on these — e.g. nearest-buoy by lat/lon, state-scoped narrowing.
    pub geo: Option<GeoFacts>,
    /// L2-normalized embedding of [`embed_text`].
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogIndex {
    pub corpus: String,
    /// Model + template the vectors were produced under — the calibration
    /// key parts. A loaded index whose model/template disagree with the
    /// runtime backend is stale and must be rebuilt (the caller checks).
    pub model_id: String,
    pub template: String,
    pub items: Vec<IndexedItem>,
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

impl CatalogIndex {
    /// Embed every enriched entry through `backend` and build the index.
    pub fn build(
        corpus: &str,
        entries: &[EnrichedEntry],
        backend: &dyn EmbeddingBackend,
    ) -> Result<Self, ClassifyError> {
        if entries.is_empty() {
            return Err(ClassifyError::EmptyIndex);
        }
        let texts: Vec<String> = entries.iter().map(embed_text).collect();
        let vectors = backend.embed(&texts)?;
        let items = entries
            .iter()
            .zip(vectors)
            .map(|(e, vector)| IndexedItem {
                id: e.id.clone(),
                section: e.section.clone(),
                title: e.title.clone(),
                geo: e.geo.clone(),
                vector,
            })
            .collect();
        Ok(Self {
            corpus: corpus.to_string(),
            model_id: backend.model_id().to_string(),
            template: EMBED_TEMPLATE_VERSION.to_string(),
            items,
        })
    }

    /// Score-only ranking: the top-k shortlist and the top1−top2 margin, no
    /// thresholds involved. This is what calibration runs on (thresholds
    /// don't exist yet there), and what a capable-model flow may browse
    /// directly per ADR 0030 (verdicts advise; they never gate the model).
    pub fn rank(
        &self,
        query: &str,
        locality: Option<&str>,
        k: usize,
        backend: &dyn EmbeddingBackend,
    ) -> Result<(Vec<Candidate>, Option<f32>), ClassifyError> {
        let text = match locality {
            Some(l) => format!("{query} (operator location: {l})"),
            None => query.to_string(),
        };
        let qv = backend
            .embed(&[text])?
            .into_iter()
            .next()
            .ok_or_else(|| ClassifyError::Backend("empty embed result".into()))?;

        let mut scored: Vec<(f32, &IndexedItem)> = self
            .items
            .iter()
            .map(|it| (dot(&qv, &it.vector), it))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        let candidates: Vec<Candidate> = scored
            .iter()
            .take(k.max(1))
            .map(|(score, it)| Candidate {
                id: it.id.clone(),
                section: it.section.clone(),
                title: it.title.clone(),
                score: *score,
            })
            .collect();
        let margin = scored.get(1).map(|(s2, _)| candidates[0].score - s2);
        Ok((candidates, margin))
    }

    /// Classify a plain-language request against the corpus.
    ///
    /// `locality` is the station-locality context ADR 0030 appends to
    /// queries ("local area" alone is a tight ambiguous cluster; grid+state
    /// context resolves the section — quantified in the T1 spike). The
    /// caller supplies it from operator state; `None` leaves the query bare.
    ///
    /// Uncalibrated (corpus, model, template) combinations error loudly
    /// rather than minting verdicts from invented numbers.
    pub fn classify(
        &self,
        query: &str,
        locality: Option<&str>,
        k: usize,
        table: &ThresholdTable,
        backend: &dyn EmbeddingBackend,
    ) -> Result<ClassifyResult, ClassifyError> {
        let key = threshold_key(&self.corpus, &self.model_id, &self.template);
        let thresholds = table
            .lookup(&self.corpus, &self.model_id, &self.template)
            .ok_or(ClassifyError::Uncalibrated(key))?;
        let (candidates, margin) = self.rank(query, locality, k, backend)?;

        let top1 = candidates[0].score;
        let verdict = if top1 < thresholds.reject_floor {
            AdvisoryVerdict::NoMatch
        } else if matches!(margin, Some(m) if m < thresholds.ask_margin) {
            AdvisoryVerdict::Ambiguous
        } else {
            AdvisoryVerdict::Match
        };
        let advisory = Advisory {
            corpus: self.corpus.clone(),
            item_ref: match verdict {
                AdvisoryVerdict::NoMatch => None,
                _ => Some(candidates[0].id.clone()),
            },
            score: top1,
            verdict,
        };
        Ok(ClassifyResult {
            advisory,
            candidates,
            margin,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Deterministic backend: text → fixed unit vector; records queries so
    /// tests can assert the locality append reached the encoder.
    struct FakeBackend {
        map: HashMap<String, Vec<f32>>,
        seen: RefCell<Vec<String>>,
    }

    impl EmbeddingBackend for FakeBackend {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ClassifyError> {
            texts
                .iter()
                .map(|t| {
                    self.seen.borrow_mut().push(t.clone());
                    self.map
                        .get(t)
                        .cloned()
                        .ok_or_else(|| ClassifyError::Backend(format!("no fake for {t:?}")))
                })
                .collect()
        }
        fn model_id(&self) -> &str {
            "fake-model"
        }
    }

    fn entry(id: &str, section: &str) -> EnrichedEntry {
        EnrichedEntry {
            id: id.into(),
            section: section.into(),
            title: format!("{id} title"),
            intent: "intent".into(),
            synonyms: vec!["syn".into()],
            geo: None,
        }
    }

    /// Unit vectors at known angles: cos(a·b) is exact by construction.
    fn vec2(theta_deg: f32) -> Vec<f32> {
        let t = theta_deg.to_radians();
        vec![t.cos(), t.sin()]
    }

    fn setup(query_vec: Vec<f32>, query_text: &str) -> (CatalogIndex, FakeBackend, ThresholdTable) {
        let entries = vec![entry("A", "S1"), entry("B", "S1"), entry("C", "S2")];
        let mut map = HashMap::new();
        map.insert(embed_text(&entries[0]), vec2(0.0)); // A at 0°
        map.insert(embed_text(&entries[1]), vec2(30.0)); // B at 30°
        map.insert(embed_text(&entries[2]), vec2(90.0)); // C at 90°
        map.insert(query_text.to_string(), query_vec);
        let backend = FakeBackend {
            map,
            seen: RefCell::new(vec![]),
        };
        let index = CatalogIndex::build("test-corpus", &entries, &backend).unwrap();
        let mut table = ThresholdTable::default();
        table.0.insert(
            "test-corpus/fake-model/enriched-v1".into(),
            crate::Thresholds {
                reject_floor: 0.5,
                ask_margin: 0.05,
            },
        );
        (index, backend, table)
    }

    #[test]
    fn clear_winner_is_match() {
        // Query at 2°: cos to A ≈ 0.999, to B(30°) ≈ 0.883 — clear margin.
        let (index, backend, table) = setup(vec2(2.0), "q");
        let r = index.classify("q", None, 3, &table, &backend).unwrap();
        assert_eq!(r.advisory.verdict, AdvisoryVerdict::Match);
        assert_eq!(r.advisory.item_ref.as_deref(), Some("A"));
        assert_eq!(r.candidates.len(), 3);
        assert!(r.margin.unwrap() > 0.05);
    }

    #[test]
    fn near_tie_is_ambiguous_with_item_ref_kept() {
        // Query at 15°: equidistant from A(0°) and B(30°) — margin ~0.
        let (index, backend, table) = setup(vec2(15.0), "q");
        let r = index.classify("q", None, 2, &table, &backend).unwrap();
        assert_eq!(r.advisory.verdict, AdvisoryVerdict::Ambiguous);
        assert!(r.advisory.item_ref.is_some(), "advisory keeps the top ref");
        assert!(r.margin.unwrap() < 0.05);
    }

    #[test]
    fn under_floor_is_no_match_with_no_item_ref() {
        // Query at 150°: best cosine is C at cos(60°)=0.5... below floor via
        // 170°: cos to C(90°) = cos(80°) ≈ 0.17 — everything under 0.5.
        let (index, backend, table) = setup(vec2(170.0), "q");
        let r = index.classify("q", None, 2, &table, &backend).unwrap();
        assert_eq!(r.advisory.verdict, AdvisoryVerdict::NoMatch);
        assert!(r.advisory.item_ref.is_none());
        assert!(!r.candidates.is_empty(), "shortlist still returned");
    }

    #[test]
    fn uncalibrated_key_errors_loudly() {
        let (index, backend, _) = setup(vec2(0.0), "q");
        let err = index
            .classify("q", None, 2, &ThresholdTable::default(), &backend)
            .unwrap_err();
        assert!(matches!(err, ClassifyError::Uncalibrated(_)));
    }

    #[test]
    fn locality_context_reaches_the_encoder() {
        let (index, backend, table) =
            setup(vec2(2.0), "weather (operator location: DM43 Arizona)");
        let r = index
            .classify("weather", Some("DM43 Arizona"), 2, &table, &backend)
            .unwrap();
        assert_eq!(r.advisory.verdict, AdvisoryVerdict::Match);
        assert!(backend
            .seen
            .borrow()
            .iter()
            .any(|t| t == "weather (operator location: DM43 Arizona)"));
    }

    #[test]
    fn index_serde_roundtrip() {
        let (index, _, _) = setup(vec2(0.0), "q");
        let json = serde_json::to_string(&index).unwrap();
        let back: CatalogIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.items.len(), index.items.len());
        assert_eq!(back.model_id, "fake-model");
        assert_eq!(back.template, EMBED_TEMPLATE_VERSION);
    }
}
