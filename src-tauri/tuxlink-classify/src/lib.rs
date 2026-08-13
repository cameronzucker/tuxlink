//! T1 request classifier — embedding retrieval over the enriched Winlink
//! catalog (ADR 0030, ch3e9 step 2).
//!
//! The pipeline this crate implements, per the ADR's tier model:
//!
//! - **T0 (deterministic, always)**: the enriched-index schema, structured
//!   geo facts parsed at index time, the two plain-number thresholds
//!   ([`Thresholds`]) that turn scores into verdicts, and the verdict
//!   mapping itself. All owned by config/code, never by a model.
//! - **T1 (CPU encoder floor)**: [`CandleBert`] runs bge-small-en-v1.5
//!   natively (candle); [`EmbeddingBackend`] is the seam through which an
//!   OpenAI-compatible embeddings endpoint can substitute at integration
//!   time. The encoder supplies SCORES ONLY.
//!
//! The output is the ADR's corpus-generic advisory DTO
//! (`{corpus, item_ref, score, verdict}` — [`Advisory`]) plus the top-k
//! candidate shortlist the agent actually reads (the 2026-08-10 spike proved
//! the JSONL entry shape parseable by the served backend). Verdicts are
//! **advisory context to a capable model, never a gate on it**; they fully
//! drive behavior only in flows with no capable model (Routines, degraded
//! tiers). That policy lives with the caller — nothing here enforces it.

pub mod backend;
#[cfg(feature = "t1-candle")]
pub mod candle_bert;
pub mod dto;
pub mod enriched;
pub mod hosting;
pub mod inbox;
pub mod index;
pub mod pins;
pub mod thresholds;

pub use backend::{EmbeddingBackend, Pooling};
#[cfg(feature = "t1-candle")]
pub use candle_bert::CandleBert;
pub use dto::{Advisory, AdvisoryVerdict, Candidate, ClassifyResult};
pub use enriched::{embed_text, EnrichedEntry, GeoFacts, EMBED_TEMPLATE_VERSION};
pub use hosting::{
    Integrity, Located, MalformedFile, ModelLocator, ModelStatus, Reason, Rejected, SizeMismatch,
    REQUIRED_FILES,
};
pub use inbox::{
    convert, Callsign, Conversion, ConvertedMessage, Grid, Payload, RawMessage, Summary150,
    Triage, TriageClass,
};
pub use index::CatalogIndex;
pub use thresholds::{ThresholdKey, ThresholdTable, Thresholds};

/// Corpus name for the Winlink catalog instance (ADR 0030: the catalog is
/// instance #1 of the corpus-generic schema, not the schema's shape).
pub const WINLINK_CATALOG_CORPUS: &str = "winlink-catalog";

#[derive(Debug, thiserror::Error)]
pub enum ClassifyError {
    #[error("enriched index parse: {0}")]
    Parse(String),
    #[error("embedding backend: {0}")]
    Backend(String),
    #[error(
        "no calibrated thresholds for {0} — run the calibration eval and add \
         the entry; verdicts must not be minted from uncalibrated numbers"
    )]
    Uncalibrated(String),
    #[error("empty index")]
    EmptyIndex,
}
