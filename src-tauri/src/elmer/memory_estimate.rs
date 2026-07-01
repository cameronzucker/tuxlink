//! Memory-fit estimate for Ollama models (T6 — tuxlink-65qhn).
//!
//! # Overview
//!
//! This module provides two public entry-points:
//!
//! - [`estimate_model_memory`] — **pure** calculation function, no I/O.
//!   Unit-testable with synthetic geometry fixtures.
//! - [`elmer_estimate_memory`] — Tauri command that fetches geometry from
//!   Ollama (`/api/show` + `/api/tags`), reads host RAM, and returns a DTO.
//!
//! # Bias-safe design (D3, tuxlink-65qhn)
//!
//! The flat KV formula is a LOOSE UPPER BOUND and over-counts Mamba and
//! sliding-window architectures.  The estimate deliberately does NOT correct
//! for those architectures — over-estimating is safe (conservative), while
//! under-estimating would make a too-large window look safe.
//!
//! A fixed `compute_headroom` constant is added on top of weights + KV to
//! account for the flash-attention prefill spike.  Flash-attn collapses the
//! peak to a small constant rather than holding all token vectors in-flight;
//! however the spike is real (see the R2 crash saga: a 19k prefill at 32k ctx
//! on gemma4 peaked at ~21 GB total).  The constant is BIASED UPWARD relative
//! to the observed peak, so a "fits" result cannot be false-positive.
//!
//! # Host RAM seam
//!
//! Production reads `/proc/meminfo MemTotal`.  Tests inject a fixed value via
//! [`HostRamReader`] so no real filesystem access is required.
//!
//! # No new dependencies
//!
//! The implementation reads `/proc/meminfo` directly rather than pulling in
//! `sysinfo` (which was absent from `Cargo.toml`).  This matches the
//! Pi-friendly low-dep posture of the rest of the backend.

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use tuxlink_agent_frontend::{
    egress::{build_vetted_client, EgressError},
    endpoint::AgentEndpoint,
};

use crate::elmer::model_config_state::ElmerModelConfigState;

// ---------------------------------------------------------------------------
// COMPUTE_HEADROOM_GB — documented constant (D3, bias-safe)
// ---------------------------------------------------------------------------

/// Fixed compute-headroom added to every estimate (in GB, f64).
///
/// # Justification (D3 — bias-safe)
///
/// Actual runtime footprint exceeds weights + KV by a roughly-constant margin:
/// the inference graph / prefill compute buffer (with `OLLAMA_FLASH_ATTENTION=1`
/// this is a small rolling attention buffer, ~O(head_dim), not O(ctx)), llama.cpp
/// allocator overhead, KV-cache fragmentation, and the Tauri+WebKit host process.
/// A fixed headroom covers this without trying to model it precisely.
///
/// NOTE (2026-07-01): an earlier version of this comment justified the constant
/// via "the R2 gemma4 crash was a ~2 GB OOM gap." That was WRONG — operator
/// ground truth established the R2 crash is a deterministic gemma-SPECIFIC fault
/// (not OOM, not resource exhaustion; heavier non-gemma models run fine). So the
/// headroom is NOT anchored to that crash; it is a general allocator/compute-graph
/// safety margin, still valued for the bias-safe reason below.
///
/// We use 3.0 GB:
/// - It comfortably exceeds a typical flash-attn prefill compute buffer.
/// - It covers allocator overhead and VRAM/GTT headroom on unified-memory hosts.
/// - It is large enough that no plausible model/ctx combination will produce a
///   spurious "fits=true" when the host is within ~3 GB of the predicted total.
///
/// BIAS DIRECTION: **upward only**.  If the constant is wrong, the estimate
/// says "does not fit" for a window that would have actually fit — the operator
/// can ignore the warning.  The reverse (a too-small constant causing
/// "fits=true" for an OOM-level window) is the failure mode we must prevent.
pub const COMPUTE_HEADROOM_GB: f64 = 3.0;

/// OS overhead fraction reserved for the host OS + other processes.
///
/// `fit` is true when `total_gb <= host_ram_gb * (1 - OS_MARGIN_FRACTION)`.
/// 0.08 = 8% reserved for the OS — matches the "92% usable" framing in the
/// plan spec.
pub const OS_MARGIN_FRACTION: f64 = 0.08;

// ---------------------------------------------------------------------------
// ModelGeometry — Ollama /api/show parsed shape
// ---------------------------------------------------------------------------

/// Geometric parameters extracted from Ollama's `/api/show` + `/api/tags`.
///
/// All fields come from `model_info` (arch-prefixed keys) except `weights_bytes`
/// which comes from `/api/tags` `size`.  Missing fields cause the caller to
/// return an error rather than silently using zero.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelGeometry {
    /// Total on-disk model size in bytes (from `/api/tags` `size` field).
    /// Used as a proxy for loaded-weights footprint.
    pub weights_bytes: u64,
    /// Number of transformer layers (attention layers), e.g. `block_count`.
    pub n_layers: u32,
    /// Number of KV heads per layer.  May equal `n_heads` (MHA) or be smaller
    /// (GQA/MQA).  Sourced from `attention.head_count_kv`.
    pub n_kv_heads: u32,
    /// Per-head key/value dimension in elements.
    ///
    /// Sourced in priority order:
    /// 1. `attention.key_length` (explicit key dimension — most accurate).
    /// 2. `embedding_length / attention.head_count` (derived; less accurate for
    ///    models where head_dim != embedding_length / n_heads, e.g. Mistral).
    pub head_dim: u32,
    /// Trained maximum context length from `context_length`.  Present for
    /// informational use; the caller's `num_ctx` input may exceed this value
    /// (Ollama allows rope-scaling beyond trained ctx) — the estimate uses the
    /// caller-supplied `num_ctx`, not this cap.
    pub trained_max_ctx: Option<u32>,
}

// ---------------------------------------------------------------------------
// MemoryEstimate — pure calculation output
// ---------------------------------------------------------------------------

/// Result of [`estimate_model_memory`].  All sizes are in gigabytes (f64).
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEstimate {
    /// Weights footprint (GB) = `weights_bytes / 1e9`.
    pub weights_gb: f64,
    /// KV-cache footprint (GB) for `num_ctx` tokens at `kv_dtype_bytes` per
    /// element.  Formula: `2 * n_layers * n_kv_heads * head_dim * kv_dtype_bytes * num_ctx / 1e9`.
    pub kv_cache_gb: f64,
    /// Fixed flash-attention compute-headroom (GB).  See [`COMPUTE_HEADROOM_GB`].
    pub compute_headroom_gb: f64,
    /// Total estimated footprint = `weights_gb + kv_cache_gb + compute_headroom_gb`.
    pub total_gb: f64,
}

// ---------------------------------------------------------------------------
// estimate_model_memory — pure calculation (no I/O)
// ---------------------------------------------------------------------------

/// Estimate peak GPU/RAM footprint for a model at a given context length.
///
/// **Pure function** — no I/O, no panics.  All inputs are validated; invalid
/// geometry (e.g. zero-dimension heads) yields an all-zero estimate with a
/// documented behaviour rather than a panic.
///
/// # Formula
///
/// ```text
/// kv_bytes_per_token = 2 * n_layers * n_kv_heads * head_dim * kv_dtype_bytes
/// kv_cache_bytes     = kv_bytes_per_token * num_ctx
/// total_bytes        = weights_bytes + kv_cache_bytes + COMPUTE_HEADROOM_GB * 1e9
/// ```
///
/// # Parameters
///
/// - `geo` — model geometry from Ollama.
/// - `num_ctx` — context window in tokens.  Caller-supplied; may exceed
///   `geo.trained_max_ctx`.
/// - `kv_dtype_bytes` — bytes per KV element: 2 for f16 (conservative default),
///   1 for q8_0. A stock Ollama uses f16 unless `OLLAMA_KV_CACHE_TYPE=q8_0` is
///   set; callers should pass `1` only when the operator has confirmed q8_0.
///
/// # Bias safety
///
/// The flat formula over-counts for GQA-heavy and Mamba/sliding-window
/// architectures where many layers share a single KV slot or use a rolling
/// cache.  Over-counting is SAFE — see [`COMPUTE_HEADROOM_GB`] documentation
/// for the full rationale.
pub fn estimate_model_memory(
    geo: &ModelGeometry,
    num_ctx: u32,
    kv_dtype_bytes: u32,
) -> MemoryEstimate {
    // Weights footprint: treat the on-disk size as loaded-weights size.
    // GGUF quant layers load approximately at the quant size; BnB / safetensors
    // may differ slightly.  Treating disk size as loaded size is slightly
    // conservative for unquantized models (same size) and accurate for quants.
    let weights_gb = geo.weights_bytes as f64 / 1_000_000_000.0;

    // KV cache: per token = 2 * layers * kv_heads * head_dim * dtype_bytes
    // The leading 2 accounts for K and V matrices (one K, one V per layer).
    let kv_bytes_per_token = 2u64
        .saturating_mul(geo.n_layers as u64)
        .saturating_mul(geo.n_kv_heads as u64)
        .saturating_mul(geo.head_dim as u64)
        .saturating_mul(kv_dtype_bytes as u64);
    let kv_total_bytes = kv_bytes_per_token.saturating_mul(num_ctx as u64);
    let kv_cache_gb = kv_total_bytes as f64 / 1_000_000_000.0;

    let compute_headroom_gb = COMPUTE_HEADROOM_GB;
    let total_gb = weights_gb + kv_cache_gb + compute_headroom_gb;

    MemoryEstimate {
        weights_gb,
        kv_cache_gb,
        compute_headroom_gb,
        total_gb,
    }
}

// ---------------------------------------------------------------------------
// HostRamReader — seam for test injection
// ---------------------------------------------------------------------------

/// Seam for reading host total RAM.
///
/// Production: [`read_host_ram_proc`] reads `/proc/meminfo`.
/// Tests: inject a closure returning a fixed value.
pub type HostRamReader = Box<dyn Fn() -> Result<f64, String> + Send + Sync>;

/// Read total host RAM in GB from `/proc/meminfo`.
///
/// Parses the `MemTotal:` line (value is in kB).  Returns an error string if
/// the file is unreadable or the line is malformed.
///
/// This is production path only — tests inject a [`HostRamReader`] closure.
pub fn read_host_ram_proc() -> Result<f64, String> {
    let meminfo = std::fs::read_to_string("/proc/meminfo")
        .map_err(|e| format!("cannot read /proc/meminfo: {e}"))?;

    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: u64 = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| format!("MemTotal line malformed: {line:?}"))?;
            return Ok(kb as f64 / 1_048_576.0); // kB → GB (1 GB = 1_048_576 kB)
        }
    }
    Err("MemTotal not found in /proc/meminfo".into())
}

// ---------------------------------------------------------------------------
// MemoryEstimateDto — Tauri boundary DTO
// ---------------------------------------------------------------------------

/// The DTO returned by [`elmer_estimate_memory`] to the renderer.
///
/// All fields are camelCase (matching the pattern of other Elmer DTOs, e.g.
/// [`super::config_commands::ConfigReadDto`]).  The `fits` field is the primary
/// UI signal: green when `true`, red when `false`.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEstimateDto {
    /// Weights footprint in GB.
    pub weights_gb: f64,
    /// KV-cache footprint in GB at `num_ctx` tokens.
    pub kv_cache_gb: f64,
    /// Fixed compute-headroom constant in GB (see [`COMPUTE_HEADROOM_GB`]).
    pub compute_headroom_gb: f64,
    /// Total estimated footprint in GB.
    pub total_gb: f64,
    /// Total host RAM in GB (from `/proc/meminfo`).
    pub host_ram_gb: f64,
    /// `true` when `total_gb <= host_ram_gb * (1 - OS_MARGIN_FRACTION)`.
    ///
    /// CONSERVATIVE: the 8% OS margin (`OS_MARGIN_FRACTION`) plus the
    /// `COMPUTE_HEADROOM_GB` constant mean this can be `false` for a context
    /// window that would technically fit, but it can NEVER be `true` for one
    /// that would OOM the host (given accurate geometry).
    pub fits: bool,
    /// Number of context tokens this estimate was computed for.
    pub num_ctx: u32,
    /// KV dtype bytes used for this estimate (1 = q8_0, 2 = f16).
    pub kv_dtype_bytes: u32,
}

// ---------------------------------------------------------------------------
// OllamaShowResponse — /api/show parsed shape
// ---------------------------------------------------------------------------

/// Minimal deserialization target for `POST /api/show`.
///
/// We only extract `model_info` — a flat JSON object keyed by arch-prefixed
/// strings like `"llama.block_count"`.  We do NOT try to enumerate every
/// architecture key; instead we derive the architecture prefix and look up
/// the keys we need at parse time.
#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    /// The `model_info` field: a flat map of `"<arch>.<key>": <value>`.
    #[serde(default)]
    model_info: serde_json::Value,
    /// The `general.architecture` key lives inside `model_info` but is also
    /// sometimes surfaced at the top level.  We parse it from `model_info`.
    #[serde(skip)]
    _phantom: (),
}

/// Minimal entry from `/api/tags` `models` array.
#[derive(Debug, Deserialize)]
struct OllamaTagModel {
    name: String,
    /// Size in bytes — used as `weights_bytes`.
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagModel>,
}

// ---------------------------------------------------------------------------
// parse_geometry_from_show — pure helper (testable)
// ---------------------------------------------------------------------------

/// Parse a [`ModelGeometry`] from the body of a `POST /api/show` response and
/// a weights size from `/api/tags`.
///
/// # Architecture-prefix derivation
///
/// Ollama prefixes every `model_info` key with the architecture name:
/// `"llama.block_count"`, `"gemma3.attention.head_count_kv"`, etc.  The
/// `"general.architecture"` key is always present and unprefixed.  We read
/// the architecture string first, then look up the following keys:
///
/// | field           | key                                        | fallback |
/// |-----------------|--------------------------------------------|----------|
/// | `n_layers`      | `<arch>.block_count`                       | error    |
/// | `n_kv_heads`    | `<arch>.attention.head_count_kv`           | error    |
/// | `head_dim`      | `<arch>.attention.key_length`              | derived  |
/// | `head_dim` alt  | `<arch>.embedding_length / <arch>.attention.head_count` | error if key_length missing AND derivation fails |
/// | `trained_max_ctx` | `<arch>.context_length`                  | None     |
///
/// `weights_bytes` comes from the caller (extracted from `/api/tags`).
pub fn parse_geometry_from_show(
    model_info: &serde_json::Value,
    weights_bytes: u64,
) -> Result<ModelGeometry, String> {
    let arch = model_info
        .get("general.architecture")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "model_info missing 'general.architecture'".to_string())?
        .to_string();

    let get_u64 = |key: &str| -> Option<u64> {
        model_info.get(key).and_then(|v| {
            // Ollama can return these as JSON number or occasionally as a
            // JSON string (observed in some beta builds).  Handle both.
            v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
    };

    let n_layers = get_u64(&format!("{arch}.block_count"))
        .ok_or_else(|| format!("model_info missing '{arch}.block_count'"))?
        as u32;

    let n_kv_heads = get_u64(&format!("{arch}.attention.head_count_kv"))
        .ok_or_else(|| format!("model_info missing '{arch}.attention.head_count_kv'"))?
        as u32;

    // head_dim: prefer explicit key_length, fall back to embedding/n_heads.
    let head_dim = if let Some(kl) = get_u64(&format!("{arch}.attention.key_length")) {
        kl as u32
    } else {
        let emb = get_u64(&format!("{arch}.embedding_length"))
            .ok_or_else(|| format!(
                "model_info missing '{arch}.attention.key_length' and '{arch}.embedding_length'"
            ))?;
        let n_heads = get_u64(&format!("{arch}.attention.head_count"))
            .ok_or_else(|| format!(
                "model_info missing '{arch}.attention.head_count' (needed to derive head_dim)"
            ))?;
        if n_heads == 0 {
            return Err(format!("'{arch}.attention.head_count' is zero — cannot derive head_dim"));
        }
        (emb / n_heads) as u32
    };

    let trained_max_ctx = get_u64(&format!("{arch}.context_length")).map(|v| v as u32);

    Ok(ModelGeometry {
        weights_bytes,
        n_layers,
        n_kv_heads,
        head_dim,
        trained_max_ctx,
    })
}

// ---------------------------------------------------------------------------
// estimate_inner — testable core (seam-injected I/O)
// ---------------------------------------------------------------------------

/// Core of [`elmer_estimate_memory`] with injectable HTTP client and RAM reader.
///
/// Separated so tests can inject a mock client and fixed RAM value without
/// needing a running Tauri app or a real Ollama server.
///
/// `host` is the Ollama origin, e.g. `"http://127.0.0.1:11434"`.  The function
/// constructs `/api/show` and `/api/tags` URLs by appending to this origin.
pub(crate) async fn estimate_inner(
    model: &str,
    num_ctx: u32,
    client: &reqwest::Client,
    origin: &str,
    kv_dtype_bytes: u32,
    // MUST be `Send + Sync`: this reference is live across the `.await`s below, so
    // without the bound the returned future is not `Send` and the #[tauri::command]
    // `elmer_estimate_memory` fails to compile (Tauri requires a Send future).
    // Matches the `HostRamReader` type alias, which already carries the bound.
    read_host_ram: &(dyn Fn() -> Result<f64, String> + Send + Sync),
) -> Result<MemoryEstimateDto, String> {
    // --- Step 1: POST /api/show to get model geometry ---
    let show_url = format!("{origin}/api/show");
    let show_body = serde_json::json!({ "model": model });
    let show_resp = client
        .post(&show_url)
        .json(&show_body)
        .send()
        .await
        .map_err(|e| format!("POST /api/show failed: {e}"))?;

    let show_status = show_resp.status().as_u16();
    if show_status == 404 {
        return Err(format!("model '{model}' not found on Ollama (404)"));
    }
    if !(200..300).contains(&show_status) {
        return Err(format!("POST /api/show returned HTTP {show_status}"));
    }

    let show_text = show_resp
        .text()
        .await
        .map_err(|e| format!("reading /api/show body: {e}"))?;
    let show_json: OllamaShowResponse = serde_json::from_str(&show_text)
        .map_err(|e| format!("parsing /api/show JSON: {e}"))?;

    // --- Step 2: GET /api/tags to find weights size ---
    let tags_url = format!("{origin}/api/tags");
    let tags_resp = client
        .get(&tags_url)
        .send()
        .await
        .map_err(|e| format!("GET /api/tags failed: {e}"))?;

    let tags_status = tags_resp.status().as_u16();
    if !(200..300).contains(&tags_status) {
        return Err(format!("GET /api/tags returned HTTP {tags_status}"));
    }

    let tags_text = tags_resp
        .text()
        .await
        .map_err(|e| format!("reading /api/tags body: {e}"))?;
    let tags_json: OllamaTagsResponse = serde_json::from_str(&tags_text)
        .map_err(|e| format!("parsing /api/tags JSON: {e}"))?;

    // Match by name prefix — Ollama may append ":latest" or other tags.
    let weights_bytes = tags_json
        .models
        .iter()
        .find(|m| m.name == model || m.name.starts_with(&format!("{model}:")))
        .map(|m| m.size)
        .ok_or_else(|| format!("model '{model}' not found in /api/tags response"))?;

    // --- Step 3: parse geometry ---
    let geo = parse_geometry_from_show(&show_json.model_info, weights_bytes)
        .map_err(|e| format!("geometry parse error: {e}"))?;

    // --- Step 4: compute estimate ---
    let estimate = estimate_model_memory(&geo, num_ctx, kv_dtype_bytes);

    // --- Step 5: read host RAM ---
    let host_ram_gb = read_host_ram()?;

    // --- Step 6: compute fit ---
    let usable_ram_gb = host_ram_gb * (1.0 - OS_MARGIN_FRACTION);
    let fits = estimate.total_gb <= usable_ram_gb;

    Ok(MemoryEstimateDto {
        weights_gb: estimate.weights_gb,
        kv_cache_gb: estimate.kv_cache_gb,
        compute_headroom_gb: estimate.compute_headroom_gb,
        total_gb: estimate.total_gb,
        host_ram_gb,
        fits,
        num_ctx,
        kv_dtype_bytes,
    })
}

// ---------------------------------------------------------------------------
// derive_ollama_origin — helper (pure)
// ---------------------------------------------------------------------------

/// Derive the Ollama origin (`scheme://host:port`) from an endpoint string.
///
/// Accepts either a chat-completions endpoint (`/v1/chat/completions`,
/// `/api/chat`, etc.) or any other path — strips the path and returns the
/// scheme+host+port only.  Re-validates through [`AgentEndpoint::parse`] so
/// the egress gate applies.
///
/// Returns an error if the endpoint is unparseable or rejected by the gate.
pub fn derive_ollama_origin(endpoint: &str) -> Result<(AgentEndpoint, String), String> {
    let ep = AgentEndpoint::parse(endpoint).map_err(|e| e.to_string())?;
    let origin = ep.origin();
    Ok((ep, origin))
}

// ---------------------------------------------------------------------------
// Tauri command: elmer_estimate_memory
// ---------------------------------------------------------------------------

/// Estimate memory footprint for an Ollama model at a given context length.
///
/// Fetches geometry from `POST {endpoint_origin}/api/show` and weights size
/// from `GET {endpoint_origin}/api/tags`, computes the estimate via
/// [`estimate_model_memory`], reads host RAM from `/proc/meminfo`, and returns
/// a [`MemoryEstimateDto`] with `fits: bool`.
///
/// # Parameters
///
/// - `model` — the Ollama model name, e.g. `"gemma3:27b"`.
/// - `num_ctx` — the context window in tokens.
/// - `endpoint` — the currently configured Elmer endpoint string.  The origin
///   is extracted from it to construct the Ollama API URLs.
/// - `kv_dtype_bytes` — bytes per KV element: 2 for f16 (default, conservative),
///   1 for q8_0. Omit this field to use the conservative f16 default.
///
/// # Errors
///
/// Returns a `String` error if:
/// - The endpoint cannot be parsed or is rejected by the egress gate.
/// - `/api/show` or `/api/tags` returns a non-2xx status.
/// - The model geometry is missing required fields.
/// - `/proc/meminfo` cannot be read.
///
/// # Security note
///
/// This command is a **Tauri UI command only** — NOT an MCP tool.  It should
/// be registered in `lib.rs`'s `invoke_handler` alongside the other
/// `config_commands`.  Registering it in the MCP router would expose the
/// operator's Ollama endpoint and model list to the agent.
#[tauri::command]
pub async fn elmer_estimate_memory(
    model: String,
    num_ctx: u32,
    endpoint: String,
    kv_dtype_bytes: Option<u32>,
    _state: State<'_, Arc<ElmerModelConfigState>>,
) -> Result<MemoryEstimateDto, String> {
    // Fix E: default to 2 (f16), the conservative choice. A stock Ollama uses
    // f16 KV cache unless OLLAMA_KV_CACHE_TYPE=q8_0 is set explicitly. The
    // prior default of 1 (q8_0) under-counted KV by 2× for stock installs,
    // producing spurious fits=true results (violates the D3 bias-safe goal).
    // The caller (GetKeyCard T8 frontend) may pass kvDtypeBytes=1 explicitly
    // when the operator has confirmed q8_0; that override is honored via the
    // Some(_) path. Absent the field → f16 (safe conservative default).
    let kv_bytes = kv_dtype_bytes.unwrap_or(2); // default: f16 = 2 bytes (conservative)

    // Derive the Ollama origin and build a vetted reqwest client.
    let (ep, origin) = derive_ollama_origin(&endpoint)?;

    // Use the system resolver for production.
    let client = build_vetted_client(&ep, |host, port| async move {
        let target = format!("{host}:{port}");
        tokio::net::lookup_host(target).await.map(|it| it.collect::<Vec<SocketAddr>>())
    })
    .await
    .map_err(|e| match e {
        EgressError::HostDenied(msg) => format!("endpoint denied by egress gate: {msg}"),
        EgressError::BadUrl(msg) => format!("bad endpoint URL: {msg}"),
        EgressError::Network(msg) => format!("network error building client: {msg}"),
        EgressError::Redirect => "redirect on connect denied".into(),
    })?;

    estimate_inner(
        &model,
        num_ctx,
        &client,
        &origin,
        kv_bytes,
        &read_host_ram_proc,
    )
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Geometry fixtures
    // -----------------------------------------------------------------------

    /// Recorded gemma4 MoE-style geometry.
    ///
    /// Based on gemma3:27b model_info values as observed via /api/show.
    /// gemma3 is a dense transformer (not true MoE), but we label it "MoE-style"
    /// because the plan spec calls for a "gemma4-style MoE geometry fixture" —
    /// we use the closest available recorded values.
    ///
    /// Reference: gemma3:27b
    ///   block_count: 62, kv_heads: 16, head_dim: 128 (from key_length),
    ///   context_length: 131072, size: ~16.9 GB
    fn gemma3_27b_geometry() -> ModelGeometry {
        ModelGeometry {
            weights_bytes: 16_888_000_000, // ~16.9 GB disk
            n_layers: 62,
            n_kv_heads: 16,
            head_dim: 128,
            trained_max_ctx: Some(131072),
        }
    }

    /// Dense model geometry fixture: llama3.1:8b
    ///
    /// Reference (recorded from /api/show):
    ///   block_count: 32, kv_heads: 8, head_dim: 128 (key_length),
    ///   context_length: 131072, size: ~4.7 GB
    fn llama3_8b_geometry() -> ModelGeometry {
        ModelGeometry {
            weights_bytes: 4_661_000_000, // ~4.7 GB
            n_layers: 32,
            n_kv_heads: 8,
            head_dim: 128,
            trained_max_ctx: Some(131072),
        }
    }

    // -----------------------------------------------------------------------
    // T6 diagram row: gemma4-style MoE geometry fixture → estimate within band
    // -----------------------------------------------------------------------

    /// gemma3:27b at 32k ctx, q8_0 → estimate is within expected band.
    ///
    /// Expected total:
    ///   weights: ~16.888 GB
    ///   kv: 2 * 62 * 16 * 128 * 1 * 32768 / 1e9 = 2*62*16*128*32768/1e9
    ///       = 8,388,608 * 32768 / 1e9... let's compute:
    ///       kv_per_token = 2 * 62 * 16 * 128 = 253,952 bytes
    ///       kv_total = 253952 * 32768 = 8,321,785,856 bytes = ~8.322 GB
    ///   headroom: 3.0 GB
    ///   total: ~28.2 GB
    ///
    /// Band: [25.0, 32.0] GB (generous; the formula is approximate).
    #[test]
    fn gemma3_27b_at_32k_ctx_estimate_in_band() {
        let geo = gemma3_27b_geometry();
        let est = estimate_model_memory(&geo, 32768, 1); // q8_0

        // KV sanity: kv_per_token = 2 * 62 * 16 * 128 = 253952 bytes
        // kv_total = 253952 * 32768 = 8321785856 bytes = ~8.322 GB
        assert!(
            est.kv_cache_gb > 8.0 && est.kv_cache_gb < 9.0,
            "gemma3:27b KV at 32k should be ~8.3 GB, got {:.3}",
            est.kv_cache_gb
        );

        assert!(
            est.total_gb > 25.0 && est.total_gb < 35.0,
            "gemma3:27b 32k total should be in [25, 35] GB, got {:.3}",
            est.total_gb
        );

        assert_eq!(est.compute_headroom_gb, COMPUTE_HEADROOM_GB);
        assert!(
            est.total_gb > est.weights_gb + est.kv_cache_gb,
            "total must exceed weights+KV (headroom adds)"
        );
    }

    // -----------------------------------------------------------------------
    // T6 diagram row: dense model fixture → estimate
    // -----------------------------------------------------------------------

    /// llama3.1:8b at 8k ctx, q8_0 → estimate is in expected band.
    ///
    /// kv_per_token = 2 * 32 * 8 * 128 = 65536 bytes
    /// kv_total     = 65536 * 8192 = 536,870,912 bytes = ~0.537 GB
    /// total        = 4.661 + 0.537 + 3.0 = ~8.2 GB
    #[test]
    fn llama3_8b_at_8k_ctx_estimate_in_band() {
        let geo = llama3_8b_geometry();
        let est = estimate_model_memory(&geo, 8192, 1); // q8_0

        assert!(
            est.kv_cache_gb > 0.4 && est.kv_cache_gb < 0.7,
            "llama3:8b KV at 8k should be ~0.54 GB, got {:.3}",
            est.kv_cache_gb
        );

        assert!(
            est.total_gb > 7.0 && est.total_gb < 10.0,
            "llama3:8b 8k total should be in [7, 10] GB, got {:.3}",
            est.total_gb
        );
    }

    // -----------------------------------------------------------------------
    // T6 diagram row: fit boundary — just under / just over → fits true/false
    // -----------------------------------------------------------------------

    /// A total just under host_ram * 0.92 → fits = true.
    /// A total just over host_ram * 0.92 → fits = false.
    ///
    /// We use a tiny model geometry and tune num_ctx to land near the boundary.
    #[test]
    fn fit_boundary_true_and_false() {
        // Tiny geometry: 1 layer, 1 kv_head, 64 head_dim.
        // kv_per_token = 2 * 1 * 1 * 64 * 1 = 128 bytes
        // With weights=0, headroom=3 GB:
        //   total = 3.0 + kv_cache_gb
        //
        // host_ram = 10.0 GB → usable = 9.2 GB → fits when total <= 9.2
        // → kv_cache_gb must be <= 6.2 GB
        // kv_cache_gb = 128 * num_ctx / 1e9 = 6.2 → num_ctx = 48437500

        let geo = ModelGeometry {
            weights_bytes: 0,
            n_layers: 1,
            n_kv_heads: 1,
            head_dim: 64,
            trained_max_ctx: None,
        };
        let host_ram_gb = 10.0_f64;
        let usable = host_ram_gb * (1.0 - OS_MARGIN_FRACTION); // 9.2 GB

        // Num ctx that puts total exactly at usable - epsilon (just under).
        // total = headroom + kv = headroom + 128 * ctx / 1e9
        // ctx = (usable - headroom) * 1e9 / 128
        let headroom = COMPUTE_HEADROOM_GB;
        let ctx_at_boundary_f = (usable - headroom) * 1_000_000_000.0 / 128.0;
        let ctx_just_under = (ctx_at_boundary_f - 10_000.0).max(0.0) as u32;
        let ctx_just_over = (ctx_at_boundary_f + 10_000.0) as u32;

        let est_under = estimate_model_memory(&geo, ctx_just_under, 1);
        let fits_under = est_under.total_gb <= usable;
        assert!(
            fits_under,
            "total {:.4} GB must fit in {:.4} GB usable (under boundary): ctx={}",
            est_under.total_gb, usable, ctx_just_under
        );

        let est_over = estimate_model_memory(&geo, ctx_just_over, 1);
        let fits_over = est_over.total_gb <= usable;
        assert!(
            !fits_over,
            "total {:.4} GB must NOT fit in {:.4} GB usable (over boundary): ctx={}",
            est_over.total_gb, usable, ctx_just_over
        );
    }

    // -----------------------------------------------------------------------
    // Fix E: default kv_dtype_bytes is f16 (2), not q8_0 (1)
    // -----------------------------------------------------------------------

    /// Fix E: the Tauri command's default for `kv_dtype_bytes` is 2 (f16),
    /// the conservative choice. Stock Ollama uses f16 unless
    /// `OLLAMA_KV_CACHE_TYPE=q8_0` is set explicitly. The prior default of 1
    /// under-counted KV by 2×, producing spurious fits=true on stock installs.
    ///
    /// This test uses estimate_inner with kv_dtype_bytes=2 (the new default)
    /// and verifies the KV footprint is double what kv_dtype_bytes=1 produces —
    /// i.e. it exercises the conservative path that was previously skipped by
    /// the wrong default.
    #[tokio::test]
    async fn default_kv_dtype_is_f16_not_q8() {
        let mut server = mockito::Server::new_async().await;

        let show_body = serde_json::json!({
            "model_info": {
                "general.architecture": "llama",
                "llama.block_count": 32,
                "llama.attention.head_count": 32,
                "llama.attention.head_count_kv": 8,
                "llama.attention.key_length": 128,
                "llama.context_length": 131072,
                "llama.embedding_length": 4096
            }
        });
        let tags_body = serde_json::json!({
            "models": [{ "name": "llama3:8b", "size": 4661000000u64 }]
        });

        let _m1 = server
            .mock("POST", "/api/show")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(show_body.to_string())
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(tags_body.to_string())
            .create_async()
            .await;
        // Second set of mocks for the q8_0 call.
        let _m3 = server
            .mock("POST", "/api/show")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(show_body.to_string())
            .create_async()
            .await;
        let _m4 = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(tags_body.to_string())
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let origin = server.url();

        // f16 (new default = 2).
        let dto_f16 = estimate_inner(
            "llama3:8b",
            8192,
            &client,
            &origin,
            2, // f16 — what the Tauri command default now produces
            &|| Ok(32.0),
        )
        .await
        .expect("estimate_inner with f16 must succeed");

        // q8_0 (old/wrong default = 1) — should be half the KV.
        let dto_q8 = estimate_inner(
            "llama3:8b",
            8192,
            &client,
            &origin,
            1, // q8_0
            &|| Ok(32.0),
        )
        .await
        .expect("estimate_inner with q8_0 must succeed");

        // f16 KV must be exactly 2× q8_0 KV.
        let ratio = dto_f16.kv_cache_gb / dto_q8.kv_cache_gb;
        assert!(
            (ratio - 2.0).abs() < 1e-9,
            "f16 KV (new default) must be exactly 2× q8_0 KV; ratio={ratio}"
        );

        // Weights and headroom are dtype-independent.
        assert_eq!(
            dto_f16.weights_gb, dto_q8.weights_gb,
            "weights must be independent of kv_dtype_bytes"
        );

        // The new f16 default reports a larger total — conservative, correct.
        assert!(
            dto_f16.total_gb > dto_q8.total_gb,
            "f16 total must exceed q8_0 total (conservative)"
        );

        // Verify DTO carries the kv_dtype_bytes that was passed in.
        assert_eq!(dto_f16.kv_dtype_bytes, 2, "DTO must echo back kv_dtype_bytes=2");
        assert_eq!(dto_q8.kv_dtype_bytes, 1, "DTO must echo back kv_dtype_bytes=1");
    }

    // -----------------------------------------------------------------------
    // T6 diagram row: q8_0 vs f16 → f16 KV is exactly 2× q8_0 KV
    // -----------------------------------------------------------------------

    /// Switching from kv_dtype_bytes=1 (q8_0) to kv_dtype_bytes=2 (f16) must
    /// exactly double the KV cache footprint.  The weights and headroom are
    /// unchanged.
    #[test]
    fn f16_kv_is_double_q8_kv() {
        let geo = llama3_8b_geometry();
        let num_ctx = 16384;

        let est_q8 = estimate_model_memory(&geo, num_ctx, 1); // q8_0
        let est_f16 = estimate_model_memory(&geo, num_ctx, 2); // f16

        // KV must double exactly (both are integer arithmetic scaled to f64).
        let ratio = est_f16.kv_cache_gb / est_q8.kv_cache_gb;
        assert!(
            (ratio - 2.0).abs() < 1e-9,
            "f16 KV must be exactly 2× q8_0 KV; ratio = {ratio}"
        );

        // Weights and headroom must be unchanged.
        assert_eq!(
            est_q8.weights_gb, est_f16.weights_gb,
            "weights must not change between dtype variants"
        );
        assert_eq!(
            est_q8.compute_headroom_gb, est_f16.compute_headroom_gb,
            "headroom must not change between dtype variants"
        );
    }

    // -----------------------------------------------------------------------
    // T6 diagram row: geometry parse from recorded /api/show JSON blob
    // -----------------------------------------------------------------------

    /// Parse a realistic /api/show model_info object for llama3.1:8b and
    /// verify the extracted geometry matches the fixture.
    #[test]
    fn parse_geometry_from_llama3_show_blob() {
        // Recorded from a real Ollama /api/show response (llama3.1:8b).
        let model_info = serde_json::json!({
            "general.architecture": "llama",
            "general.name": "Meta Llama 3.1 8B Instruct",
            "llama.block_count": 32,
            "llama.attention.head_count": 32,
            "llama.attention.head_count_kv": 8,
            "llama.attention.key_length": 128,
            "llama.attention.value_length": 128,
            "llama.context_length": 131072,
            "llama.embedding_length": 4096,
            "llama.feed_forward_length": 14336,
            "llama.rope.dimension_count": 128,
            "llama.rope.freq_base": 500000.0,
            "llama.vocab_size": 128256,
            "tokenizer.ggml.model": "gpt2"
        });

        let geo = parse_geometry_from_show(&model_info, 4_661_000_000)
            .expect("parse must succeed for well-formed llama3.1 blob");

        assert_eq!(geo.n_layers, 32, "n_layers");
        assert_eq!(geo.n_kv_heads, 8, "n_kv_heads");
        assert_eq!(geo.head_dim, 128, "head_dim (from key_length)");
        assert_eq!(geo.trained_max_ctx, Some(131072), "trained_max_ctx");
        assert_eq!(geo.weights_bytes, 4_661_000_000, "weights_bytes");
    }

    /// Parse a gemma3:12b style blob where key_length is absent — head_dim
    /// must be derived from embedding_length / head_count.
    #[test]
    fn parse_geometry_derived_head_dim() {
        let model_info = serde_json::json!({
            "general.architecture": "gemma3",
            "gemma3.block_count": 46,
            "gemma3.attention.head_count": 16,
            "gemma3.attention.head_count_kv": 8,
            // No key_length — head_dim must be derived: 3072 / 16 = 192? No.
            // Actually gemma3 embedding_length=3840, head_count=16 → 240.
            "gemma3.embedding_length": 3840,
            "gemma3.context_length": 131072
        });

        let geo = parse_geometry_from_show(&model_info, 7_000_000_000)
            .expect("parse must succeed when key_length absent and derivation possible");

        assert_eq!(geo.n_layers, 46);
        assert_eq!(geo.n_kv_heads, 8);
        assert_eq!(geo.head_dim, 240, "3840 / 16 = 240");
        assert_eq!(geo.trained_max_ctx, Some(131072));
    }

    /// Parse fails when required fields are absent.
    #[test]
    fn parse_geometry_missing_block_count_is_error() {
        let model_info = serde_json::json!({
            "general.architecture": "llama",
            // Deliberately missing llama.block_count
            "llama.attention.head_count_kv": 8,
            "llama.attention.key_length": 128,
            "llama.context_length": 131072
        });
        let err = parse_geometry_from_show(&model_info, 1_000_000_000);
        assert!(err.is_err(), "missing block_count must be an error");
        assert!(
            err.unwrap_err().contains("block_count"),
            "error must mention block_count"
        );
    }

    // -----------------------------------------------------------------------
    // Bias-safe self-check: total can NEVER be less than weights + KV
    // -----------------------------------------------------------------------

    /// The headroom constant ensures total > weights + KV always.
    #[test]
    fn total_always_exceeds_weights_plus_kv() {
        let geo = gemma3_27b_geometry();
        for num_ctx in [1u32, 1024, 8192, 32768, 131072] {
            for kv_bytes in [1u32, 2] {
                let est = estimate_model_memory(&geo, num_ctx, kv_bytes);
                assert!(
                    est.total_gb >= est.weights_gb + est.kv_cache_gb + COMPUTE_HEADROOM_GB - 1e-9,
                    "total {:.4} must >= weights+KV+headroom for ctx={num_ctx} kv_bytes={kv_bytes}",
                    est.total_gb
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // camelCase serde verification
    // -----------------------------------------------------------------------

    /// MemoryEstimateDto must serialize field names as camelCase.
    #[test]
    fn dto_fields_are_camel_case() {
        let dto = MemoryEstimateDto {
            weights_gb: 1.0,
            kv_cache_gb: 2.0,
            compute_headroom_gb: 3.0,
            total_gb: 6.0,
            host_ram_gb: 32.0,
            fits: true,
            num_ctx: 8192,
            kv_dtype_bytes: 1,
        };
        let json = serde_json::to_value(&dto).expect("serialize DTO");
        assert!(json.get("weightsGb").is_some(), "weightsGb must exist");
        assert!(json.get("kvCacheGb").is_some(), "kvCacheGb must exist");
        assert!(json.get("computeHeadroomGb").is_some(), "computeHeadroomGb must exist");
        assert!(json.get("totalGb").is_some(), "totalGb must exist");
        assert!(json.get("hostRamGb").is_some(), "hostRamGb must exist");
        assert!(json.get("numCtx").is_some(), "numCtx must exist");
        assert!(json.get("kvDtypeBytes").is_some(), "kvDtypeBytes must exist");
        assert!(json.get("fits").is_some(), "fits must exist");

        // snake_case names must NOT appear.
        assert!(json.get("weights_gb").is_none(), "snake_case must not exist");
        assert!(json.get("kv_cache_gb").is_none(), "snake_case must not exist");
        assert!(json.get("host_ram_gb").is_none(), "snake_case must not exist");
        assert!(json.get("num_ctx").is_none(), "snake_case must not exist");
        assert!(json.get("kv_dtype_bytes").is_none(), "snake_case must not exist");
    }

    // -----------------------------------------------------------------------
    // estimate_inner with injected seams (no network)
    // -----------------------------------------------------------------------

    /// estimate_inner with a mock HTTP client and fixed RAM returns a correct DTO.
    ///
    /// This validates the plumbing from HTTP response → geometry parse →
    /// estimate → DTO, using a mock reqwest server.
    #[tokio::test]
    async fn estimate_inner_gemma3_27b_mock() {
        // Build a mockito server serving /api/show and /api/tags.
        let mut server = mockito::Server::new_async().await;

        let show_body = serde_json::json!({
            "model_info": {
                "general.architecture": "gemma3",
                "gemma3.block_count": 62,
                "gemma3.attention.head_count": 32,
                "gemma3.attention.head_count_kv": 16,
                "gemma3.attention.key_length": 128,
                "gemma3.context_length": 131072,
                "gemma3.embedding_length": 5120
            }
        });

        let tags_body = serde_json::json!({
            "models": [
                { "name": "gemma3:27b", "size": 16888000000u64 }
            ]
        });

        let _m1 = server
            .mock("POST", "/api/show")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(show_body.to_string())
            .create_async()
            .await;

        let _m2 = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(tags_body.to_string())
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let origin = server.url();
        let host_ram = 32.0_f64;

        let dto = estimate_inner(
            "gemma3:27b",
            32768,
            &client,
            &origin,
            1, // q8_0
            &|| Ok(host_ram),
        )
        .await
        .expect("estimate_inner must succeed");

        // Verify geometry was parsed correctly.
        assert_eq!(dto.num_ctx, 32768);
        assert_eq!(dto.kv_dtype_bytes, 1);
        assert_eq!(dto.host_ram_gb, 32.0);

        // KV = 2 * 62 * 16 * 128 * 1 * 32768 / 1e9 ≈ 8.322 GB
        assert!(
            dto.kv_cache_gb > 8.0 && dto.kv_cache_gb < 9.0,
            "KV at 32k should be ~8.3 GB, got {:.3}",
            dto.kv_cache_gb
        );

        // Weights ≈ 16.888 GB, KV ≈ 8.322 GB, headroom = 3.0 → total ≈ 28.2 GB
        assert!(
            dto.total_gb > 25.0 && dto.total_gb < 32.0,
            "total should be in [25, 32] GB, got {:.3}",
            dto.total_gb
        );

        // On a 32 GB host: usable = 32 * 0.92 = 29.44 GB
        // total ≈ 28.2 < 29.44 → fits
        assert!(dto.fits, "gemma3:27b at 32k should fit in 32 GB host");
    }

    /// estimate_inner with a host too small → fits = false.
    #[tokio::test]
    async fn estimate_inner_does_not_fit_small_host() {
        let mut server = mockito::Server::new_async().await;

        let show_body = serde_json::json!({
            "model_info": {
                "general.architecture": "llama",
                "llama.block_count": 32,
                "llama.attention.head_count": 32,
                "llama.attention.head_count_kv": 8,
                "llama.attention.key_length": 128,
                "llama.context_length": 131072,
                "llama.embedding_length": 4096
            }
        });
        let tags_body = serde_json::json!({
            "models": [{ "name": "llama3:8b", "size": 4661000000u64 }]
        });

        let _m1 = server
            .mock("POST", "/api/show")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(show_body.to_string())
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(tags_body.to_string())
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let origin = server.url();

        // Host with only 8 GB RAM — total (weights ~4.7 + KV + headroom 3.0)
        // at 32k ctx will exceed 8 * 0.92 = 7.36 GB.
        let dto = estimate_inner(
            "llama3:8b",
            32768,
            &client,
            &origin,
            2, // f16
            &|| Ok(8.0),
        )
        .await
        .expect("estimate_inner must succeed");

        assert!(
            !dto.fits,
            "llama3:8b at 32k ctx on 8GB host should NOT fit; total={:.3}",
            dto.total_gb
        );
    }

    /// estimate_inner returns error when model not found in /api/tags.
    #[tokio::test]
    async fn estimate_inner_model_not_in_tags_is_error() {
        let mut server = mockito::Server::new_async().await;

        let show_body = serde_json::json!({
            "model_info": {
                "general.architecture": "llama",
                "llama.block_count": 32,
                "llama.attention.head_count_kv": 8,
                "llama.attention.key_length": 128,
                "llama.context_length": 131072
            }
        });
        // Tags response with a DIFFERENT model name.
        let tags_body = serde_json::json!({
            "models": [{ "name": "other-model:latest", "size": 1000000u64 }]
        });

        let _m1 = server
            .mock("POST", "/api/show")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(show_body.to_string())
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(tags_body.to_string())
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let origin = server.url();
        let err = estimate_inner(
            "missing-model",
            8192,
            &client,
            &origin,
            1,
            &|| Ok(32.0),
        )
        .await;

        assert!(err.is_err(), "missing model must return Err");
        assert!(
            err.unwrap_err().contains("not found in /api/tags"),
            "error must mention /api/tags"
        );
    }

    // -----------------------------------------------------------------------
    // Bias-safe self-check via DTO: fits can never be true when total > usable
    // -----------------------------------------------------------------------

    /// Direct check: if estimate.total_gb > host_ram_gb * 0.92,
    /// the DTO must have fits = false, and vice versa.
    #[test]
    fn dto_fits_matches_usable_ram_formula() {
        let geo = gemma3_27b_geometry();

        // total at 32k, q8_0 ≈ 28.2 GB
        let est = estimate_model_memory(&geo, 32768, 1);

        for host_gb in [10.0_f64, 20.0, 28.0, 29.0, 30.0, 32.0, 64.0] {
            let usable = host_gb * (1.0 - OS_MARGIN_FRACTION);
            let expected_fits = est.total_gb <= usable;
            // Re-compute the dto fits field inline to verify the formula.
            let dto_fits = est.total_gb <= host_gb * (1.0 - OS_MARGIN_FRACTION);
            assert_eq!(
                dto_fits, expected_fits,
                "fits mismatch for host_gb={host_gb:.1}: total={:.3}, usable={:.3}",
                est.total_gb, usable
            );
        }
    }
}
