//! The corpus-generic advisory DTO (ADR 0030, verbatim shape).

use serde::{Deserialize, Serialize};

/// What the deterministic threshold mapping says about the top hit.
///
/// Advisory only. A capable model may overrule `Ambiguous` from context or
/// browse the raw catalog past a `NoMatch`; deterministic flows (Routines,
/// degraded tiers) treat it as the decision input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvisoryVerdict {
    /// Top hit clears the reject floor with a clear margin over #2.
    Match,
    /// Top hit clears the floor but #2 is within the ask margin — the
    /// honest move is one clarifying question between the close candidates.
    Ambiguous,
    /// Top hit is under the reject floor — likely not a catalog request.
    NoMatch,
}

/// `{corpus, item_ref, score, verdict}` — the ADR 0030 advisory record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Advisory {
    /// Which corpus this advisory is about (e.g. `winlink-catalog`).
    pub corpus: String,
    /// Top item reference; `None` when the verdict is `NoMatch`.
    pub item_ref: Option<String>,
    /// Cosine similarity of the top hit (vectors are L2-normalized, so
    /// dot product == cosine; range is [-1, 1], useful mass ~[0.3, 1]).
    pub score: f32,
    pub verdict: AdvisoryVerdict,
}

/// One shortlist row — the agent-facing narrowing surface. Field names match
/// the enriched-index JSONL the parseability spike validated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub section: String,
    pub title: String,
    pub score: f32,
}

/// Advisory + the top-k shortlist it was computed from, plus the margin the
/// ask trigger used (top1 − top2; `None` with fewer than two candidates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyResult {
    pub advisory: Advisory,
    pub candidates: Vec<Candidate>,
    pub margin: Option<f32>,
}
