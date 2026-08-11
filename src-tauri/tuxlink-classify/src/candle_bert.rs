//! Native BERT-family embedding backend (candle) — the ADR 0030 T1 offline
//! floor. Loads a HuggingFace-format model directory (`config.json`,
//! `tokenizer.json`, `model.safetensors`); nothing here touches the network.
//!
//! Thread note (T1 spike): 2–4 intra-op threads is the knee on the target
//! hardware; `RAYON_NUM_THREADS` governs candle's pool.

use std::path::Path;

use candle_core::{DType, Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

use crate::backend::{EmbeddingBackend, Pooling};
use crate::ClassifyError;

/// Batch chunk for [`EmbeddingBackend::embed`]: bounds peak memory on the
/// full-catalog index build (1,477 texts) without measurably hurting
/// throughput at this model size.
const CHUNK: usize = 32;

/// BERT max sequence length; catalog embed-texts run well under it, the
/// truncation is a guard for pathological queries.
const MAX_LEN: usize = 512;

pub struct CandleBert {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    pooling: Pooling,
    model_id: String,
}

fn be(e: impl std::fmt::Display) -> ClassifyError {
    ClassifyError::Backend(e.to_string())
}

impl CandleBert {
    /// Load from a model directory. `model_id` becomes part of the
    /// threshold-calibration key, so name the actual weights (e.g.
    /// `bge-small-en-v1.5`), not the family.
    pub fn load(
        dir: &Path,
        pooling: Pooling,
        model_id: impl Into<String>,
    ) -> Result<Self, ClassifyError> {
        let device = Device::Cpu;
        let config: Config = serde_json::from_str(
            &std::fs::read_to_string(dir.join("config.json")).map_err(be)?,
        )
        .map_err(be)?;
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(be)?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_LEN,
                ..Default::default()
            }))
            .map_err(be)?;
        // Batch-longest padding: every encoding in one batch gets equal
        // length, which is what the (batch, seq) tensor construction needs.
        tokenizer.with_padding(Some(PaddingParams::default()));
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[dir.join("model.safetensors")],
                DType::F32,
                &device,
            )
            .map_err(be)?
        };
        let model = BertModel::load(vb, &config).map_err(be)?;
        Ok(Self {
            model,
            tokenizer,
            device,
            pooling,
            model_id: model_id.into(),
        })
    }

    fn embed_chunk(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ClassifyError> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(be)?;
        let (batch, seq) = (encodings.len(), encodings[0].get_ids().len());
        let mut ids = Vec::with_capacity(batch * seq);
        let mut mask = Vec::with_capacity(batch * seq);
        for enc in &encodings {
            ids.extend_from_slice(enc.get_ids());
            mask.extend(enc.get_attention_mask().iter().copied());
        }
        let input_ids = Tensor::from_vec(ids, (batch, seq), &self.device).map_err(be)?;
        let attention =
            Tensor::from_vec(mask, (batch, seq), &self.device).map_err(be)?;
        let token_type_ids = input_ids.zeros_like().map_err(be)?;
        let hidden = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention))
            .map_err(be)?; // (batch, seq, hidden)

        let pooled = match self.pooling {
            // bge family: the CLS (first) token IS the sentence vector.
            Pooling::Cls => hidden.narrow(1, 0, 1).map_err(be)?.squeeze(1).map_err(be)?,
            // MiniLM/e5: attention-masked mean over token positions.
            Pooling::Mean => {
                let m = attention
                    .to_dtype(DType::F32)
                    .map_err(be)?
                    .unsqueeze(2)
                    .map_err(be)?; // (batch, seq, 1)
                let summed = hidden.broadcast_mul(&m).map_err(be)?.sum(1).map_err(be)?;
                let counts = m.sum(1).map_err(be)?; // (batch, 1)
                summed.broadcast_div(&counts).map_err(be)?
            }
        };
        // L2 normalize so downstream dot products are cosines.
        let norm = pooled
            .sqr()
            .map_err(be)?
            .sum_keepdim(D::Minus1)
            .map_err(be)?
            .sqrt()
            .map_err(be)?;
        let unit = pooled.broadcast_div(&norm).map_err(be)?;
        unit.to_vec2::<f32>().map_err(be)
    }
}

impl EmbeddingBackend for CandleBert {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ClassifyError> {
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(CHUNK) {
            out.extend(self.embed_chunk(chunk)?);
        }
        Ok(out)
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}
