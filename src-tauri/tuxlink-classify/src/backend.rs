//! The embedding-backend seam (ADR 0030 deployment matrix): in-process
//! candle is the mandatory offline floor; "any OpenAI-compat embeddings
//! endpoint is a configurable alternative" arrives as an adapter implemented
//! by the integrating app over its existing HTTP stack (ports-and-adapters,
//! same pattern as tuxlink-mcp-core's ports) — this crate deliberately
//! carries no HTTP client.

use crate::ClassifyError;

/// How a sentence vector is produced from token states. Getting this wrong
/// per model family silently costs accuracy while still "working":
/// bge uses the CLS token; MiniLM/e5 mean-pool over the attention mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pooling {
    /// First-token pooling (bge family).
    Cls,
    /// Attention-masked mean over token positions (MiniLM / e5 families).
    Mean,
}

pub trait EmbeddingBackend {
    /// Embed a batch of texts into L2-NORMALIZED vectors (all the cosine
    /// math downstream assumes unit vectors: dot == cosine).
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ClassifyError>;

    /// Model identifier for the threshold-calibration key
    /// (e.g. `bge-small-en-v1.5`).
    fn model_id(&self) -> &str;
}
