//! Tauri commands for Elmer model-config read/write and model detection.
//!
//! # Overview
//!
//! Three Tauri-only commands are exposed here:
//!
//! - [`elmer_config_read`] — returns `{agent_endpoint, agent_model, key_status}`.
//!   **Never returns the key value.**
//! - [`elmer_config_set`] — transactional write: endpoint validation → key action →
//!   config-file write → in-memory snapshot advance, all under the model-config lock.
//! - [`elmer_detect_models`] — probes `<derived-models-url>` through the vetted
//!   egress client and returns the list of available model IDs, or a typed short
//!   reason string on failure.  **Never echoes an upstream body or the key.**
//!
//! # Registration
//!
//! These are Tauri UI commands ONLY. They are NOT registered as MCP tools.
//! D3 registers them in `lib.rs`'s `invoke_handler`; F1 adds the boundary test.
//!
//! # Transactional ordering (key-first)
//!
//! `elmer_config_set` writes the keyring **before** writing the config file.  If the
//! config-file write fails the key may already be persisted — this is intentional:
//! the next successful `set` overwrites it.  The alternative (config-first) would
//! leave the config file pointing at an endpoint with no stored key, which is
//! harder to recover from (the UI would show the endpoint without any indication
//! a key exists).  Key-first is the safer default ordering.
//!
//! # Inner-helper pattern (testability)
//!
//! The `#[tauri::command]` wrappers simply forward `State<'_, Arc<T>> → &T` and
//! delegate to `config_set_inner` / `config_read_inner` / `detect_inner`.  Tests
//! call the inner helpers directly with concrete references — no Tauri `State`
//! machinery needed.
//!
//! # Detect-URL derivation (pinned convention)
//!
//! [`derive_models_url`] implements the pinned convention: if the configured
//! endpoint path ends with `/chat/completions`, replace that suffix with `/models`
//! (preserving any prefix, e.g. `/api/v1/chat/completions` → `/api/v1/models`).
//! Otherwise fall back to `<origin>/v1/models` (the OpenAI-standard path).  The
//! derived URL is re-validated through [`AgentEndpoint::parse`] before use.
//!
//! # Value-scrub (defence-in-depth)
//!
//! Any error string produced by the detect path is scrubbed of the just-sent key
//! via [`scrub_detect_key`] before being returned to the renderer, so a 401 body
//! that echoes the bearer cannot leak even if downstream mapping code changes.
//! `tuxlink_agent_frontend::provider::scrub_key` is `pub(crate)` there and not
//! re-exported; the local inline here is intentionally equivalent.

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;

use tuxlink_agent_frontend::{
    egress::{build_vetted_client, EgressError},
    endpoint::AgentEndpoint,
    provider::ApiKey,
};

use crate::elmer::{
    keyring::{ElmerKeyring, KeyStatus},
    model_config_state::ElmerModelConfigState,
};

// ---------------------------------------------------------------------------
// Public re-export
// ---------------------------------------------------------------------------

pub use crate::elmer::keyring::KeyStatus as KeyStatusDto;

// ---------------------------------------------------------------------------
// KeySource — how the caller supplies the API key for detect
// ---------------------------------------------------------------------------

/// How the caller supplies the API key for [`elmer_detect_models`].
///
/// Deserializes as `{ "source": "useStored" | "inline" | "none", "value"?: string }`.
///
/// `ApiKey` does not implement `Deserialize` (it is intentionally opaque), so
/// `Inline` carries a plain `String` at the boundary; `detect_inner` wraps it
/// in `ApiKey::new` at the point of use, mirroring `SetKey::Set`.
#[derive(Deserialize)]
#[serde(tag = "source", rename_all = "camelCase")]
pub enum KeySource {
    /// Read the key from the keyring for the derived endpoint origin.
    UseStored,
    /// Use the supplied key value (never touches the keyring).
    Inline {
        #[serde(rename = "value")]
        value: String,
    },
    /// No key — probe without authentication.
    None,
}

/// Manual `Debug` impl that NEVER prints the inline key value.
///
/// The `Inline` variant carries a raw `String` at the Tauri boundary.  A
/// derived `Debug` would format it as `Inline { value: "sk-secret" }`,
/// leaking the credential to any log subscriber.  This type-level redaction
/// is the primary guarantee; `#[instrument(skip(key_source, ...))]` on the
/// Tauri wrapper is defence-in-depth.
impl std::fmt::Debug for KeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeySource::UseStored => write!(f, "UseStored"),
            KeySource::Inline { .. } => f
                .debug_struct("Inline")
                .field("value", &"<redacted>")
                .finish(),
            KeySource::None => write!(f, "None"),
        }
    }
}

// ---------------------------------------------------------------------------
// DetectError — typed failure reasons for elmer_detect_models
// ---------------------------------------------------------------------------

/// Typed failure reasons for [`elmer_detect_models`].
///
/// Serialised as a SHORT human-readable string via [`DetectError::to_reason`] —
/// NEVER an upstream body or the raw key value.
#[derive(Debug)]
pub enum DetectError {
    /// No server listening at the derived URL (transport / connection error).
    NoServer { host: String },
    /// 401 or 403 — the key was rejected.  Fixed reason; body is NEVER echoed.
    Auth { provider: String },
    /// An unexpected non-2xx HTTP status (not 401/403).
    Status(u16),
    /// A transport or network error not caused by "no server".
    Network(String),
    /// The derived models URL failed `AgentEndpoint::parse`.
    BadUrl(String),
    /// The `/v1/models` response contained an empty `data` array.
    ZeroModels,
    /// `KeySource::UseStored` was requested but the keyring backend returned a
    /// non-`NoEntry` error (locked / unavailable daemon).  NEVER collapses to a
    /// keyless probe — that would silently send an unauthenticated request.
    UnreadableKey,
}

impl DetectError {
    /// Convert to the short typed string that is returned to the renderer.
    ///
    /// These strings are the UI-visible reason text.  They MUST NOT contain the
    /// API key, the upstream response body, or any other secret.
    pub fn to_reason(&self) -> String {
        match self {
            DetectError::NoServer { host } => {
                format!("no server: could not connect to {host}")
            }
            DetectError::Auth { provider } => {
                // FIXED reason — the body is never read or echoed here.
                format!("auth error: check the API key for {provider}")
            }
            DetectError::Status(code) => format!("server error: HTTP {code}"),
            DetectError::Network(msg) => format!("network error: {msg}"),
            DetectError::BadUrl(msg) => format!("bad URL: {msg}"),
            DetectError::ZeroModels => "no models: the server returned an empty model list".into(),
            DetectError::UnreadableKey => {
                "the saved key couldn't be read (keyring locked) \
                 — check Connect an AI Agent settings"
                    .into()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SetKey — the three possible key operations
// ---------------------------------------------------------------------------

/// What to do with the API key during a config write.
///
/// Deserializes as `{ "action": "keep" | "set" | "clear", "value"?: string }`.
///
/// `ApiKey` does not implement `Deserialize` (it is intentionally opaque), so
/// `Set` carries a plain `String` at the boundary; `config_set_inner` wraps it
/// in `ApiKey::new` at the point of use.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum SetKey {
    /// Leave the stored key unchanged.
    Keep,
    /// Store (overwrite) the key with the given value.
    Set {
        #[serde(rename = "value")]
        value: String,
    },
    /// Remove the stored key for this origin.
    Clear,
}

/// Manual `Debug` impl that NEVER prints the raw key value.
///
/// A derived `Debug` on `SetKey` would format `Set { value: "sk-secret" }`,
/// leaking the credential to any log subscriber — including the tracing span
/// recorded by `#[instrument]` before the skip list is applied at call time.
/// Making redaction TYPE-LEVEL ensures the secret cannot appear in `{:?}`
/// output regardless of how the caller formats or logs this enum.
///
/// The `#[instrument(skip(key, ...))]` guard on the Tauri wrapper is
/// defence-in-depth; this impl is the primary guarantee.
impl std::fmt::Debug for SetKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetKey::Keep => write!(f, "Keep"),
            SetKey::Set { .. } => f
                .debug_struct("Set")
                .field("value", &"<redacted>")
                .finish(),
            SetKey::Clear => write!(f, "Clear"),
        }
    }
}

// ---------------------------------------------------------------------------
// ConfigReadDto
// ---------------------------------------------------------------------------

/// The data returned by `elmer_config_read`.
///
/// **Never contains the key value** — `key_status` is the three-state
/// [`KeyStatus`] indicator only.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReadDto {
    pub agent_endpoint: String,
    pub agent_model: String,
    pub key_status: KeyStatus,
    /// Per-turn wall-clock timeout in SECONDS (tuxlink-1wi5w). Serialized as
    /// `agentTurnTimeoutSecs` on the boundary. Read off the live model-config
    /// snapshot, so it reflects any in-session `elmer_config_set` advance
    /// (already clamped to `[30, 3600]`).
    pub agent_turn_timeout_secs: u32,
}

// ---------------------------------------------------------------------------
// Turn-timeout clamp bounds (tuxlink-1wi5w)
// ---------------------------------------------------------------------------

/// Minimum operator-settable per-turn timeout (seconds) = 0.5 min.
pub const MIN_TURN_TIMEOUT_SECS: u32 = 30;
/// Maximum operator-settable per-turn timeout (seconds) = 60 min.
pub const MAX_TURN_TIMEOUT_SECS: u32 = 3600;

/// Clamp a requested per-turn timeout into `[MIN_TURN_TIMEOUT_SECS,
/// MAX_TURN_TIMEOUT_SECS]` (tuxlink-1wi5w).
///
/// Per the pinned contract a value below 30 or above 3600 is NOT rejected —
/// it is clamped into range and used. This keeps the boundary lenient (a UI
/// that sends `0` from an empty field, or an out-of-range value, still yields a
/// usable timeout) while guaranteeing the runtime never builds a degenerate
/// (sub-second) or unbounded-feeling (>1h) per-turn ceiling.
pub fn clamp_turn_timeout_secs(requested: u32) -> u32 {
    requested.clamp(MIN_TURN_TIMEOUT_SECS, MAX_TURN_TIMEOUT_SECS)
}

// ---------------------------------------------------------------------------
// Pure inner helpers (testable without Tauri State)
// ---------------------------------------------------------------------------

/// Inner implementation of `elmer_config_set`.
///
/// Callers pass `&ElmerModelConfigState` / `&ElmerKeyring` directly; the Tauri
/// command wrapper simply dereferences the `State<'_, Arc<T>>` handles before
/// delegating here.
///
/// # Transactional ordering
///
/// 1. Acquire the model-config lock (held for the full transaction).
/// 2. Validate the endpoint string — any error aborts with nothing persisted.
/// 3. Apply the key action (`Set` | `Clear` | `Keep`):
///    - `Set(k)`: reject an empty key value as a validation error (never write
///      an empty credential); then `keyring.set` — any error returns
///      `"couldn't save the key — nothing was changed"`, aborting the
///      transaction before the config file is touched.
///    - `Clear`: `keyring.clear` (idempotent — missing entry is OK).
///    - `Keep`: no keyring operation.
/// 4. Write the config file atomically (read → patch elmer section → write).
///    On failure the function returns an error; the key may already be written
///    (key-first ordering), but the next successful `set` overwrites it.
/// 5. Advance the in-memory snapshot via `guard` mutation so the next read
///    sees the new values without re-acquiring the lock.
///
/// # Errors
///
/// Returns a `String` error on any validation or I/O failure.
pub async fn config_set_inner(
    agent_endpoint: String,
    agent_model: String,
    agent_turn_timeout_secs: u32,
    key: SetKey,
    state: &ElmerModelConfigState,
    keyring: &ElmerKeyring,
) -> Result<(), String> {
    // Clamp the requested per-turn timeout into [30, 3600] (tuxlink-1wi5w).
    // Out-of-range values are clamped, NOT rejected (pinned contract).
    let turn_timeout_secs = clamp_turn_timeout_secs(agent_turn_timeout_secs);

    // Step 1: acquire lock — held across the whole transaction.
    let mut guard = state.lock().await;

    // Step 2: validate endpoint.
    let endpoint = AgentEndpoint::parse(&agent_endpoint)
        .map_err(|e| e.to_string())?;
    let origin = endpoint.origin();

    // Step 3: apply key action (key-first).
    match key {
        SetKey::Set { value } => {
            if value.is_empty() {
                return Err("API key must not be empty".into());
            }
            let k = ApiKey::new(value);
            keyring
                .set(&origin, &k)
                .map_err(|_| "couldn't save the key — nothing was changed".to_string())?;
        }
        SetKey::Clear => {
            // Idempotent: missing entry is not an error.
            keyring
                .clear(&origin)
                .map_err(|e| format!("couldn't clear the key: {e}"))?;
        }
        SetKey::Keep => {
            // No keyring operation.
        }
    }

    // Step 4: patch and write the config file.
    let mut config = crate::config::read_config()
        .map_err(|e| format!("couldn't read config before saving: {e}"))?;
    config.elmer.agent_endpoint = agent_endpoint.clone();
    config.elmer.agent_model = agent_model.clone();
    config.elmer.agent_turn_timeout_secs = turn_timeout_secs;
    crate::config::write_config_atomic(&config)
        .map_err(|e| format!("couldn't save config: {e}"))?;

    // Step 5: advance in-memory snapshot (still under lock).
    guard.endpoint = agent_endpoint;
    guard.model = agent_model;
    guard.turn_timeout_secs = turn_timeout_secs;
    // Lock is released here when `guard` drops.

    Ok(())
}

/// Inner implementation of `elmer_config_read`.
///
/// Reads the endpoint + model from the in-memory snapshot, then performs a
/// **fail-closed** presence check on the keyring — the key value is NEVER
/// returned or logged.
///
/// # Loopback shortcut
///
/// Loopback endpoints (ollama / llama.cpp shims) never carry a key.  The key
/// field is hidden in the UI for loopback, so `key_status` is always
/// [`KeyStatus::Absent`] for them.  More importantly, calling the keyring for a
/// loopback endpoint is wasteful and wrong: a locked / unavailable D-Bus daemon
/// would needlessly return `Unreadable` for an endpoint that cannot have a key.
/// The guard mirrors `build_turn_provider_from_parts` in `session.rs`.
///
/// # Blocking I/O
///
/// `ElmerKeyring::status` calls `keyring::Entry::get_password`, a blocking
/// D-Bus round-trip.  Running it directly on the async task thread parks the
/// thread inside the Tokio executor and blocks any other task waiting on that
/// thread.  The call is moved to `tokio::task::spawn_blocking` so the executor
/// can yield while the D-Bus round-trip completes.
///
/// # Errors
///
/// Returns a `String` error only when the in-memory endpoint fails validation
/// (this should not happen in practice because `config_set_inner` validates
/// before persisting, but the defensive parse is the only way to call
/// `endpoint.is_loopback()` + `endpoint.origin()` without a stored `Url`).
pub async fn config_read_inner(
    state: &ElmerModelConfigState,
    keyring: &Arc<ElmerKeyring>,
) -> Result<ConfigReadDto, String> {
    let snapshot = state.snapshot().await;
    let endpoint =
        AgentEndpoint::parse(&snapshot.endpoint).map_err(|e| e.to_string())?;

    let key_status = if endpoint.is_loopback() {
        // Loopback endpoints never carry a key; skip the keyring entirely.
        // A locked / unavailable keyring must not report Unreadable for an
        // endpoint that the UI never shows a key field for.
        KeyStatus::Absent
    } else {
        let origin = endpoint.origin();
        let keyring = Arc::clone(keyring);
        // `keyring.status` is a blocking D-Bus call; run it off the async
        // reactor via spawn_blocking.  `spawn_blocking` requires `'static`,
        // hence the owned `origin` string and cloned `Arc`.
        tokio::task::spawn_blocking(move || keyring.status(&origin))
            .await
            // JoinError (the blocking task panicked) maps to Unreadable —
            // fail-closed: never report Absent when the keyring is broken.
            .unwrap_or(KeyStatus::Unreadable)
    };

    Ok(ConfigReadDto {
        agent_endpoint: snapshot.endpoint,
        agent_model: snapshot.model,
        key_status,
        agent_turn_timeout_secs: snapshot.turn_timeout_secs,
    })
}

// ---------------------------------------------------------------------------
// derive_models_url — pinned detect-URL derivation (pure, testable)
// ---------------------------------------------------------------------------

/// Derive the `/v1/models` URL from a configured endpoint string.
///
/// # Pinned convention
///
/// 1. Parse the endpoint via `AgentEndpoint::parse` (rejects invalid URLs,
///    metadata ranges, credentials-in-URL).
/// 2. Inspect the URL path:
///    - If it ends with `/chat/completions`, replace ONLY that suffix with
///      `/models`, preserving any prefix
///      (`/api/v1/chat/completions` → `/api/v1/models`).
///    - Otherwise use `<origin>/v1/models` (the OpenAI-standard path).
/// 3. Re-validate the derived URL through `AgentEndpoint::parse` so it goes
///    through the egress gate like any other endpoint.
///
/// Returns a validated `AgentEndpoint` for the models URL, or a [`DetectError`]
/// if either parse step fails.
///
/// This is a pure function — no I/O, no network.  Both branches are unit-tested
/// explicitly (see `tests::detect::derive_models_url_*`).
pub fn derive_models_url(agent_endpoint: &str) -> Result<AgentEndpoint, DetectError> {
    // Step 1: parse the configured endpoint to access origin + path.
    let ep = AgentEndpoint::parse(agent_endpoint)
        .map_err(|e| DetectError::BadUrl(e.to_string()))?;

    let origin = ep.origin();
    let path = ep.url().path();

    // Step 2: derive the models path.
    const CHAT_COMPLETIONS: &str = "/chat/completions";
    let models_url = if let Some(prefix) = path.strip_suffix(CHAT_COMPLETIONS) {
        // Replace the trailing /chat/completions with /models, keeping any prefix.
        format!("{origin}{prefix}/models")
    } else {
        // Fallback: OpenAI-standard <origin>/v1/models.
        format!("{origin}/v1/models")
    };

    // Step 3: re-validate through the egress gate.
    AgentEndpoint::parse(&models_url)
        .map_err(|e| DetectError::BadUrl(format!("derived models URL rejected: {e}")))
}

// ---------------------------------------------------------------------------
// scrub_detect_key — inline value-scrub for detect error strings
// ---------------------------------------------------------------------------

/// Scrub every occurrence of `key.expose()` from `snippet` and return the
/// cleaned string.
///
/// `tuxlink_agent_frontend::provider::scrub_key` implements the same logic but
/// is `pub(crate)` there and therefore not accessible here.  This local copy is
/// intentionally equivalent and covers the detect error path.
///
/// When `key` is `None` (unauthenticated probe) the snippet is returned
/// unchanged.
fn scrub_detect_key(snippet: String, key: Option<&ApiKey>) -> String {
    match key {
        Some(k) if !k.expose().is_empty() => snippet.replace(k.expose(), "<redacted>"),
        _ => snippet,
    }
}

// ---------------------------------------------------------------------------
// map_models_response — pure response→result mapping (testable without network)
// ---------------------------------------------------------------------------

/// Map an HTTP status + response body to `Ok(Vec<String>)` or a [`DetectError`].
///
/// This is a pure function extracted from `detect_inner` so the response-mapping
/// logic can be unit-tested directly (the actual HTTP GET is correct-by-inspection
/// for D2 — the same pattern used in A3 and D1 for command delegation).
///
/// ## Contract
///
/// - **200 + valid `{data:[{id},…]}` JSON** → `Ok(ids)`.
/// - **200 + `data: []`** → `Err(DetectError::ZeroModels)`.
/// - **401 or 403** → `Err(DetectError::Auth{provider})`.  The `body` parameter
///   is NEVER included in the reason string — the FIXED text is "check the API
///   key for `<provider>`".  `key` is also NEVER echoed.
/// - **Other non-2xx** → `Err(DetectError::Status(code))`.
/// - **200 + non-JSON / wrong shape** → `Err(DetectError::Network(...))`.
///
/// # Value-scrub
///
/// The `key` parameter is passed only so that any error string produced by the
/// JSON-parse branch can be scrubbed of the bearer value.  The 401/403 branch
/// never reads `body` at all — it uses the fixed reason text.
pub fn map_models_response(
    status: u16,
    body: &str,
    provider: &str,
    key: Option<&ApiKey>,
) -> Result<Vec<String>, DetectError> {
    if status == 401 || status == 403 {
        // FIXED reason — body is NEVER read or echoed.
        return Err(DetectError::Auth {
            provider: provider.to_string(),
        });
    }
    if !(200..300).contains(&status) {
        return Err(DetectError::Status(status));
    }
    // 2xx: parse the OpenAI `/v1/models` shape `{data:[{id:string},…]}`.
    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|e| {
            let msg = scrub_detect_key(format!("response is not JSON: {e}"), key);
            DetectError::Network(msg)
        })?;

    let data = parsed
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            let msg = scrub_detect_key("response missing `data` array".into(), key);
            DetectError::Network(msg)
        })?;

    if data.is_empty() {
        return Err(DetectError::ZeroModels);
    }

    let ids: Vec<String> = data
        .iter()
        .filter_map(|entry| {
            entry.get("id").and_then(|v| v.as_str()).map(String::from)
        })
        .collect();

    if ids.is_empty() {
        // data array had entries but none had an `id` string field.
        return Err(DetectError::ZeroModels);
    }

    Ok(ids)
}

// ---------------------------------------------------------------------------
// detect_inner — testable core of elmer_detect_models
// ---------------------------------------------------------------------------

/// System resolver: resolve `host:port` to a list of `SocketAddr` via the
/// platform resolver (Tokio async DNS).  Mirrors `elmer::provider::system_resolver`.
async fn detect_system_resolver(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    let target = format!("{host}:{port}");
    tokio::net::lookup_host(target).await.map(|it| it.collect())
}

/// Core of `elmer_detect_models` with injectable resolver seam.
///
/// Separated from the Tauri command wrapper so tests can inject a loopback
/// resolver and point at a mockito server without real DNS or real egress.
///
/// The production caller ([`elmer_detect_models`]) injects
/// [`detect_system_resolver`]; tests inject a fixed resolver.
pub(crate) async fn detect_inner<R, Fut>(
    agent_endpoint: String,
    key_source: KeySource,
    keyring: &Arc<ElmerKeyring>,
    resolve: R,
) -> Result<Vec<String>, DetectError>
where
    R: Fn(String, u16) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
{
    // Step 1: derive and validate the models URL.
    let models_ep = derive_models_url(&agent_endpoint)?;
    let origin = models_ep.origin();
    // Extract strings from `models_ep` before the egress gate consumes it via
    // reference: `build_vetted_client` takes `&models_ep` so `models_ep` is not
    // moved, but we need owned strings for the error closures and the GET call.
    let models_host = models_ep.url().host_str().unwrap_or("unknown").to_string();
    let models_url_str = models_ep.url().to_string();

    // Step 2: build the vetted egress client.
    let client = {
        // Clone `models_host` into the closure so the outer binding remains
        // available after `map_err` completes.
        let h = models_host.clone();
        build_vetted_client(&models_ep, resolve).await.map_err(|e| match e {
            EgressError::HostDenied(msg) => DetectError::BadUrl(msg),
            EgressError::BadUrl(msg) => DetectError::BadUrl(msg),
            EgressError::Network(msg) => {
                DetectError::NoServer { host: format!("{h}: {msg}") }
            }
            EgressError::Redirect => {
                DetectError::NoServer { host: format!("{h}: redirect on connect") }
            }
        })?
    };

    // Step 3: resolve the key.
    //
    // `UseStored` reads the keyring via `spawn_blocking` (keyring::Entry
    // calls D-Bus — a blocking round-trip that must not run on the async
    // reactor thread).  On a backend error (Err from `keyring.read`) the
    // function FAILS CLOSED with `DetectError::UnreadableKey` — never
    // collapses to a keyless probe, because silently sending an
    // unauthenticated request to a cloud provider is worse than reporting
    // the error.  `Ok(None)` (NoEntry — no key stored) remains a legitimate
    // keyless probe.
    let key: Option<ApiKey> = match key_source {
        KeySource::UseStored => {
            let kr = Arc::clone(keyring);
            let origin_owned = origin.clone();
            let read = tokio::task::spawn_blocking(move || kr.read(&origin_owned))
                .await
                // JoinError (blocking task panicked) → fail-closed.
                .map_err(|_| DetectError::UnreadableKey)?;
            match read {
                Ok(Some(k)) => Some(k),
                Ok(None) => Option::None, // No key stored — keyless probe is fine.
                // Backend error (locked / unavailable keyring) → fail-closed.
                // NEVER collapses to None (which would silently send keyless).
                Err(_) => return Err(DetectError::UnreadableKey),
            }
        }
        KeySource::Inline { value } => {
            if value.is_empty() {
                Option::None
            } else {
                Some(ApiKey::new(value))
            }
        }
        KeySource::None => Option::None,
    };

    // Step 4: issue the GET request.
    let mut req = client.get(&models_url_str);
    if let Some(ref k) = key {
        req = req.bearer_auth(k.expose());
    }

    let resp = req.send().await.map_err(|e| {
        // Transport error — "no server" at the host.  Clone `models_host` into
        // the closure; `key` is moved into the error-scrub call below.
        let msg = scrub_detect_key(e.to_string(), key.as_ref());
        DetectError::NoServer { host: format!("{models_host}: {msg}") }
    })?;

    // Step 5: map the response.
    let status = resp.status().as_u16();

    // 401/403 — do NOT read the body; map to fixed Auth reason.
    if status == 401 || status == 403 {
        return Err(DetectError::Auth { provider: models_host });
    }

    // Other non-2xx — do NOT echo the body.
    if !(200u16..300).contains(&status) {
        return Err(DetectError::Status(status));
    }

    // 2xx: read and parse the body.  Scrub the key out of any parse-error string.
    let body = resp.text().await.unwrap_or_default();
    map_models_response(status, &body, &models_host, key.as_ref())
}

// ---------------------------------------------------------------------------
// Tauri command wrappers
// ---------------------------------------------------------------------------

/// Read the current Elmer model configuration.
///
/// Returns `{agentEndpoint, agentModel, keyStatus}` — **never returns the key
/// value**.
#[tauri::command]
pub async fn elmer_config_read(
    state: State<'_, Arc<ElmerModelConfigState>>,
    keyring: State<'_, Arc<ElmerKeyring>>,
) -> Result<ConfigReadDto, String> {
    // Dereference `State<'_, Arc<T>>` to `Arc<T>` by cloning so that
    // `config_read_inner` can move the Arc into the `spawn_blocking` closure.
    config_read_inner(&state, &Arc::clone(&keyring)).await
}

/// Write the Elmer model configuration.
///
/// Transactional: endpoint validation → key action → config-file write →
/// in-memory snapshot advance, all under the model-config lock.
#[instrument(skip(key, keyring, state))]
#[tauri::command]
pub async fn elmer_config_set(
    agent_endpoint: String,
    agent_model: String,
    agent_turn_timeout_secs: u32,
    key: SetKey,
    state: State<'_, Arc<ElmerModelConfigState>>,
    keyring: State<'_, Arc<ElmerKeyring>>,
) -> Result<(), String> {
    config_set_inner(
        agent_endpoint,
        agent_model,
        agent_turn_timeout_secs,
        key,
        &state,
        &keyring,
    )
    .await
}

/// Probe the `/v1/models` endpoint derived from `agent_endpoint` and return the
/// list of available model IDs.
///
/// The derive URL convention:
/// - If the configured path ends with `/chat/completions`, replace that suffix
///   with `/models` (preserving any prefix).
/// - Otherwise fall back to `<origin>/v1/models`.
///
/// The response is mapped to a typed short reason string on failure — the
/// upstream body and the API key are NEVER echoed back.
///
/// # Key resolution
///
/// `key_source` controls how the bearer token is selected:
/// - `UseStored` — read from keyring for the derived endpoint origin.
/// - `Inline(value)` — use the supplied value directly.
/// - `None` — probe without authentication.
#[instrument(skip(key_source, keyring))]
#[tauri::command]
pub async fn elmer_detect_models(
    agent_endpoint: String,
    key_source: KeySource,
    keyring: State<'_, Arc<ElmerKeyring>>,
) -> Result<Vec<String>, String> {
    detect_inner(
        agent_endpoint,
        key_source,
        // Clone the Arc so detect_inner can move it into the spawn_blocking
        // closure for UseStored key reads.
        &Arc::clone(&keyring),
        |host, port| async move { detect_system_resolver(&host, port).await },
    )
    .await
    .map_err(|e| e.to_reason())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elmer::{
        keyring::ElmerKeyring,
        model_config_state::ElmerModelConfigState,
    };
    use serial_test::serial;
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    const VALID_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
    const VALID_MODEL: &str = "gpt-4o";
    const VALID_ORIGIN: &str = "https://api.openai.com";

    fn valid_state() -> ElmerModelConfigState {
        // 900 = the default 15-min per-turn timeout (tuxlink-1wi5w).
        ElmerModelConfigState::new(VALID_ENDPOINT.into(), VALID_MODEL.into(), 900)
    }

    // -----------------------------------------------------------------------
    // FailingEntry — all writes fail with a non-NoEntry backend error.
    // -----------------------------------------------------------------------

    use crate::winlink::credentials::EntryLike;
    use crate::elmer::keyring::EntryFactory;

    struct FailingEntry;

    impl EntryLike for FailingEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            Err(keyring::Error::PlatformFailure(Box::new(
                std::io::Error::other("backend unavailable"),
            )))
        }
        fn set_password(&self, _password: &str) -> Result<(), keyring::Error> {
            Err(keyring::Error::PlatformFailure(Box::new(
                std::io::Error::other("backend unavailable"),
            )))
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            Err(keyring::Error::PlatformFailure(Box::new(
                std::io::Error::other("backend unavailable"),
            )))
        }
    }

    fn failing_keyring() -> ElmerKeyring {
        let factory: EntryFactory = Box::new(|_svc: &str, _account: &str| {
            Box::new(FailingEntry) as Box<dyn EntryLike>
        });
        ElmerKeyring::with_factory(factory)
    }

    // -----------------------------------------------------------------------
    // Config-file isolation
    //
    // config_set_inner calls read_config + write_config_atomic which resolve
    // config_path() at runtime.  We redirect them to a temp dir per-test by
    // setting TUXLINK_CONFIG_DIR.  Because tests run concurrently and
    // std::env::set_var is not thread-safe under Rust's multi-threaded test
    // runner, each test that exercises config I/O must:
    //   1. Use a per-test unique temp dir.
    //   2. Seed a minimal valid config.json in that dir before calling set.
    //   3. Restore / isolate the env var via the helper below.
    //
    // Tests that do NOT touch config file I/O (validation error paths that
    // abort before reaching Step 4) skip the seeding entirely.
    // -----------------------------------------------------------------------

    use std::path::PathBuf;

    struct TempConfigDir {
        dir: tempfile::TempDir,
    }

    impl TempConfigDir {
        /// Create a temp dir with a valid minimal config.json.
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("create temp dir");
            let config = minimal_config();
            let json =
                serde_json::to_string_pretty(&config).expect("serialize config");
            std::fs::write(dir.path().join("config.json"), json)
                .expect("write config.json");
            TempConfigDir { dir }
        }

        fn path(&self) -> PathBuf {
            self.dir.path().to_path_buf()
        }
    }

    fn minimal_config() -> crate::config::Config {
        // Build the minimal valid Config using serde round-trip from the known
        // valid JSON shape.  Using serde_json::from_str avoids depending on any
        // private constructor.
        let json = serde_json::json!({
            "schema_version": crate::config::CONFIG_SCHEMA_VERSION,
            "wizard_completed": true,
            "connect": { "connect_to_cms": false, "transport": "Telnet" },
            "identity": { "callsign": null, "identifier": null, "grid": null },
            "privacy": {
                "gps_state": "Off",
                "position_precision": "FourCharGrid"
            }
        });
        serde_json::from_value(json).expect("minimal config must deserialize")
    }

    // -----------------------------------------------------------------------
    // Test: set_keep_leaves_key
    // -----------------------------------------------------------------------

    /// Storing a key, then calling set with Keep, leaves the key present.
    #[tokio::test]
    #[serial]
    async fn set_keep_leaves_key() {
        let kr = ElmerKeyring::with_memory_keyring();
        let state = valid_state();
        let tmp = TempConfigDir::new();

        // Pre-store a key for the origin.
        kr.set(VALID_ORIGIN, &ApiKey::new("sk-existing"))
            .expect("pre-store key");

        // Set with Keep — must not touch the keyring.
        let dir_path = tmp.path().to_str().unwrap().to_string();
        std::env::set_var("TUXLINK_CONFIG_DIR", &dir_path);
        let result = config_set_inner(
            VALID_ENDPOINT.into(),
            VALID_MODEL.into(),
            900,
            SetKey::Keep,
            &state,
            &kr,
        )
        .await;
        std::env::remove_var("TUXLINK_CONFIG_DIR");

        assert!(result.is_ok(), "Keep should succeed: {result:?}");
        // Key must still be present.
        assert_eq!(kr.status(VALID_ORIGIN), KeyStatus::Present);
        let stored = kr.read(VALID_ORIGIN).expect("read").expect("some");
        assert_eq!(stored.expose(), "sk-existing");
    }

    // -----------------------------------------------------------------------
    // Test: set_set_writes_key
    // -----------------------------------------------------------------------

    /// SetKey::Set stores the key under the endpoint's origin.
    #[tokio::test]
    #[serial]
    async fn set_set_writes_key() {
        let kr = ElmerKeyring::with_memory_keyring();
        let state = valid_state();
        let tmp = TempConfigDir::new();

        let dir_path = tmp.path().to_str().unwrap().to_string();
        std::env::set_var("TUXLINK_CONFIG_DIR", &dir_path);
        let result = config_set_inner(
            VALID_ENDPOINT.into(),
            VALID_MODEL.into(),
            900,
            SetKey::Set { value: "sk-x".into() },
            &state,
            &kr,
        )
        .await;
        std::env::remove_var("TUXLINK_CONFIG_DIR");

        assert!(result.is_ok(), "Set should succeed: {result:?}");
        let stored = kr.read(VALID_ORIGIN).expect("read").expect("some");
        assert_eq!(stored.expose(), "sk-x");
    }

    // -----------------------------------------------------------------------
    // Test: set_empty_is_validation_error
    // -----------------------------------------------------------------------

    /// SetKey::Set with an empty value is a validation error; nothing is written.
    #[tokio::test]
    async fn set_empty_is_validation_error() {
        let kr = ElmerKeyring::with_memory_keyring();
        let state = valid_state();

        // No temp dir needed — this aborts before Step 4.
        let result = config_set_inner(
            VALID_ENDPOINT.into(),
            VALID_MODEL.into(),
            900,
            SetKey::Set { value: "".into() },
            &state,
            &kr,
        )
        .await;

        assert!(result.is_err(), "empty key must be rejected");
        // Nothing should have been written to the keyring.
        assert_eq!(kr.status(VALID_ORIGIN), KeyStatus::Absent);
    }

    // -----------------------------------------------------------------------
    // Test: clamp_turn_timeout_secs (pure)
    // -----------------------------------------------------------------------

    /// The clamp helper bounds the requested timeout into [30, 3600], NEVER
    /// rejecting an out-of-range value (tuxlink-1wi5w pinned contract).
    #[test]
    fn clamp_turn_timeout_bounds_into_range() {
        // Below the floor → clamped up to 30.
        assert_eq!(clamp_turn_timeout_secs(0), 30);
        assert_eq!(clamp_turn_timeout_secs(5), 30);
        assert_eq!(clamp_turn_timeout_secs(29), 30);
        // At the bounds → unchanged.
        assert_eq!(clamp_turn_timeout_secs(30), 30);
        assert_eq!(clamp_turn_timeout_secs(3600), 3600);
        // In range → unchanged.
        assert_eq!(clamp_turn_timeout_secs(600), 600);
        assert_eq!(clamp_turn_timeout_secs(900), 900);
        // Above the ceiling → clamped down to 3600.
        assert_eq!(clamp_turn_timeout_secs(5000), 3600);
        assert_eq!(clamp_turn_timeout_secs(u32::MAX), 3600);
    }

    // -----------------------------------------------------------------------
    // Test: set_persists_and_clamps_turn_timeout
    // -----------------------------------------------------------------------

    /// `config_set_inner` clamps the requested per-turn timeout into [30, 3600]
    /// and persists+advances the clamped value into BOTH the in-memory snapshot
    /// and the config file (tuxlink-1wi5w). A below-floor request clamps up, an
    /// above-ceiling request clamps down, and an in-range request is stored
    /// verbatim. The snapshot value is what the send-path reads to build the
    /// per-turn `Limits`.
    #[tokio::test]
    #[serial]
    async fn set_persists_and_clamps_turn_timeout() {
        // Each sub-case uses a fresh temp config dir + a fresh state seeded at
        // the default 900 so the assertion isolates the write under test.
        async fn run_case(requested: u32, expected: u32) {
            let kr = ElmerKeyring::with_memory_keyring();
            let state = valid_state();
            let tmp = TempConfigDir::new();

            let dir_path = tmp.path().to_str().unwrap().to_string();
            std::env::set_var("TUXLINK_CONFIG_DIR", &dir_path);
            let result = config_set_inner(
                VALID_ENDPOINT.into(),
                VALID_MODEL.into(),
                requested,
                SetKey::Keep,
                &state,
                &kr,
            )
            .await;
            let on_disk = crate::config::read_config().map(|c| c.elmer.agent_turn_timeout_secs);
            std::env::remove_var("TUXLINK_CONFIG_DIR");

            assert!(result.is_ok(), "set must succeed for {requested}: {result:?}");

            // In-memory snapshot carries the clamped value (the send-path read).
            let snap = state.snapshot().await;
            assert_eq!(
                snap.turn_timeout_secs, expected,
                "snapshot timeout for requested {requested} must clamp to {expected}"
            );

            // Persisted config file carries the same clamped value.
            assert_eq!(
                on_disk.expect("config must read back"),
                expected,
                "persisted timeout for requested {requested} must clamp to {expected}"
            );
        }

        run_case(5, 30).await; // below floor → 30
        run_case(5000, 3600).await; // above ceiling → 3600
        run_case(600, 600).await; // in range → verbatim
    }

    // -----------------------------------------------------------------------
    // Test: read_returns_turn_timeout_from_snapshot
    // -----------------------------------------------------------------------

    /// `config_read_inner` surfaces the live snapshot's per-turn timeout in the
    /// DTO (tuxlink-1wi5w). The state is seeded at a non-default value to prove
    /// the DTO reflects the snapshot, not a hardcoded default.
    #[tokio::test]
    async fn read_returns_turn_timeout_from_snapshot() {
        let kr = Arc::new(ElmerKeyring::with_memory_keyring());
        // Loopback endpoint so the read skips the keyring; timeout seeded to 240.
        let state = ElmerModelConfigState::new(
            "http://127.0.0.1:11434/v1/chat/completions".into(),
            "llama3".into(),
            240,
        );

        let dto = config_read_inner(&state, &kr)
            .await
            .expect("config_read_inner must succeed");

        assert_eq!(
            dto.agent_turn_timeout_secs, 240,
            "DTO timeout must reflect the snapshot value"
        );

        // The boundary field name is camelCase `agentTurnTimeoutSecs`.
        let json = serde_json::to_value(&dto).expect("serialize DTO");
        assert_eq!(
            json.get("agentTurnTimeoutSecs").and_then(|v| v.as_u64()),
            Some(240),
            "DTO must serialize the timeout as `agentTurnTimeoutSecs`; got: {json}"
        );
    }

    // -----------------------------------------------------------------------
    // Test: set_clear_removes_key
    // -----------------------------------------------------------------------

    /// SetKey::Clear removes a previously stored key.
    #[tokio::test]
    #[serial]
    async fn set_clear_removes_key() {
        let kr = ElmerKeyring::with_memory_keyring();
        let state = valid_state();
        let tmp = TempConfigDir::new();

        // Pre-store.
        kr.set(VALID_ORIGIN, &ApiKey::new("sk-to-clear"))
            .expect("pre-store");

        let dir_path = tmp.path().to_str().unwrap().to_string();
        std::env::set_var("TUXLINK_CONFIG_DIR", &dir_path);
        let result = config_set_inner(
            VALID_ENDPOINT.into(),
            VALID_MODEL.into(),
            900,
            SetKey::Clear,
            &state,
            &kr,
        )
        .await;
        std::env::remove_var("TUXLINK_CONFIG_DIR");

        assert!(result.is_ok(), "Clear should succeed: {result:?}");
        assert_eq!(kr.status(VALID_ORIGIN), KeyStatus::Absent);
    }

    // -----------------------------------------------------------------------
    // Test: set_invalid_endpoint_persists_nothing
    // -----------------------------------------------------------------------

    /// An invalid endpoint string causes early abort — keyring AND state unchanged.
    #[tokio::test]
    async fn set_invalid_endpoint_persists_nothing() {
        let kr = ElmerKeyring::with_memory_keyring();
        // Pre-store a key at a different origin to confirm it is not touched.
        kr.set("https://api.openai.com", &ApiKey::new("sk-safe"))
            .expect("pre-store");

        let state = valid_state();

        // No temp dir needed — aborts at Step 2.
        let result = config_set_inner(
            "not a url".into(),
            VALID_MODEL.into(),
            900,
            SetKey::Set { value: "sk-injected".into() },
            &state,
            &kr,
        )
        .await;

        assert!(result.is_err(), "invalid endpoint must be rejected");
        // The pre-stored key must be untouched.
        let stored = kr
            .read("https://api.openai.com")
            .expect("read")
            .expect("some");
        assert_eq!(stored.expose(), "sk-safe", "keyring must be unchanged");
        // In-memory snapshot must be unchanged.
        let snap = state.snapshot().await;
        assert_eq!(snap.endpoint, VALID_ENDPOINT);
        assert_eq!(snap.model, VALID_MODEL);
    }

    // -----------------------------------------------------------------------
    // Test: set_keyring_failure_is_transactional
    // -----------------------------------------------------------------------

    /// If the keyring write fails, the function returns an error containing
    /// "nothing was changed", and the in-memory state snapshot is NOT advanced.
    #[tokio::test]
    async fn set_keyring_failure_is_transactional() {
        let kr = failing_keyring();
        let state = valid_state();

        // No temp dir needed — aborts at Step 3 (keyring write failure).
        let result = config_set_inner(
            VALID_ENDPOINT.into(),
            "gpt-new".into(),
            900,
            SetKey::Set { value: "sk-never-stored".into() },
            &state,
            &kr,
        )
        .await;

        assert!(result.is_err(), "keyring failure must propagate as Err");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("nothing was changed"),
            "error must say 'nothing was changed', got: {msg:?}"
        );

        // In-memory snapshot must NOT have advanced.
        let snap = state.snapshot().await;
        assert_eq!(
            snap.model, VALID_MODEL,
            "model must not advance on keyring failure"
        );
    }

    // -----------------------------------------------------------------------
    // Test: read_returns_status_not_value
    // -----------------------------------------------------------------------

    /// After setting a key, config_read_inner returns key_status == Present
    /// and the DTO serialized to JSON must NOT contain the key string.
    #[tokio::test]
    async fn read_returns_status_not_value() {
        let kr = Arc::new(ElmerKeyring::with_memory_keyring());
        kr.set(VALID_ORIGIN, &ApiKey::new("sk-super-secret"))
            .expect("pre-store");

        let state = valid_state();
        let dto = config_read_inner(&state, &kr)
            .await
            .expect("config_read_inner must succeed");

        assert_eq!(
            dto.key_status,
            KeyStatus::Present,
            "key_status must be Present after setting a key"
        );

        // Serialize the whole DTO and assert the secret is absent.
        let json = serde_json::to_string(&dto).expect("serialize DTO");
        assert!(
            !json.contains("sk-super-secret"),
            "serialized DTO must NOT contain the key value; got: {json}"
        );
    }

    // -----------------------------------------------------------------------
    // Test: read_locked_keyring_is_unreadable
    // -----------------------------------------------------------------------

    /// A failing (locked) keyring produces key_status == Unreadable, not Absent.
    #[tokio::test]
    async fn read_locked_keyring_is_unreadable() {
        let kr = Arc::new(failing_keyring());
        let state = valid_state();

        let dto = config_read_inner(&state, &kr)
            .await
            .expect("config_read_inner must succeed even with failing keyring");

        assert_eq!(
            dto.key_status,
            KeyStatus::Unreadable,
            "a backend error must yield Unreadable, not Absent"
        );
    }

    // -----------------------------------------------------------------------
    // Test: instrument_skip_no_key_in_event
    // -----------------------------------------------------------------------

    /// Tracing events emitted by config_set_inner must NOT contain the raw API
    /// key value.
    ///
    /// This verifies the `#[instrument(skip(key, ...))]` guarantee on the
    /// `elmer_config_set` Tauri wrapper: the key operand is excluded from any
    /// span field.  We install a custom in-process capturing `Layer` that
    /// records every event field value, drive `config_set_inner` to completion
    /// on a dedicated single-threaded Tokio runtime (isolated from the
    /// `#[tokio::test]` runtime to avoid `block_on`-inside-async panics), then
    /// scan the captured strings for the secret.
    ///
    /// # Defence layers
    ///
    /// The PRIMARY guarantee is now TYPE-LEVEL: `SetKey` and `KeySource` have
    /// manual `Debug` impls that always emit `<redacted>` for the secret
    /// `value` field, regardless of the caller's skip list.  See the
    /// `setkey_debug_redacts_value` and `keysource_debug_redacts_value` tests.
    ///
    /// This `instrument_skip_no_key_in_event` test verifies the SECONDARY
    /// (defence-in-depth) guarantee: even if the `Debug` impl were somehow
    /// bypassed, the `#[instrument(skip(key, ...))]` guard prevents the key
    /// from ever entering a tracing span field.  Both layers together ensure
    /// the secret is never observable in structured logs or debug output.
    ///
    /// MSRV note: uses `tokio::runtime::Builder::new_current_thread().build()`
    /// which is stable on MSRV 1.75.
    #[test]
    #[serial]
    fn instrument_skip_no_key_in_event() {
        use std::sync::{Arc, Mutex};
        use tracing::{Event, Subscriber};
        use tracing_subscriber::{layer::Context, layer::SubscriberExt, Layer, Registry};

        const SECRET: &str = "sk-secret";

        // Collector layer — records every event's message + field values into
        // a shared Vec<String>.
        #[derive(Clone)]
        struct CapturingLayer {
            captured: Arc<Mutex<Vec<String>>>,
        }

        impl<S: Subscriber> Layer<S> for CapturingLayer {
            fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
                struct Visitor(Vec<String>);
                impl tracing::field::Visit for Visitor {
                    fn record_str(
                        &mut self,
                        _field: &tracing::field::Field,
                        value: &str,
                    ) {
                        self.0.push(value.to_string());
                    }
                    fn record_debug(
                        &mut self,
                        _field: &tracing::field::Field,
                        value: &dyn std::fmt::Debug,
                    ) {
                        self.0.push(format!("{value:?}"));
                    }
                }
                let mut visitor = Visitor(Vec::new());
                event.record(&mut visitor);
                self.captured.lock().unwrap().extend(visitor.0);
            }
        }

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let layer = CapturingLayer { captured: Arc::clone(&captured) };
        let subscriber = Registry::default().with(layer);

        let kr = Arc::new(ElmerKeyring::with_memory_keyring());
        let state = Arc::new(valid_state());
        let tmp = TempConfigDir::new();
        let dir_path = tmp.path().to_str().unwrap().to_string();

        // Build a dedicated single-threaded runtime so we can call block_on
        // without being inside an existing async context.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build rt");

        let result = tracing::subscriber::with_default(subscriber, || {
            rt.block_on(async {
                std::env::set_var("TUXLINK_CONFIG_DIR", &dir_path);
                let r = config_set_inner(
                    VALID_ENDPOINT.into(),
                    VALID_MODEL.into(),
                    900,
                    SetKey::Set { value: SECRET.into() },
                    &state,
                    &kr,
                )
                .await;
                std::env::remove_var("TUXLINK_CONFIG_DIR");
                r
            })
        });

        assert!(result.is_ok(), "config_set_inner must succeed: {result:?}");

        // Scan captured strings for the raw secret.
        let guard = captured.lock().unwrap();
        for s in guard.iter() {
            assert!(
                !s.contains(SECRET),
                "captured tracing event contains the raw API key!\n  \
                 event: {s:?}\n  \
                 The #[instrument(skip(key, ...))] guarantee is violated."
            );
        }
    }

    // -----------------------------------------------------------------------
    // Test: setkey_debug_redacts_value
    // -----------------------------------------------------------------------

    /// `SetKey::Set { value }` must NEVER appear in `{:?}` output.
    ///
    /// This is the primary type-level guarantee that the secret field value
    /// cannot leak through Debug formatting, regardless of context or callers.
    #[test]
    fn setkey_debug_redacts_value() {
        // Set variant: value must be redacted, not the literal key string.
        let set = SetKey::Set { value: "sk-secret123".into() };
        let formatted = format!("{:?}", set);
        assert!(
            !formatted.contains("sk-secret123"),
            "SetKey::Set Debug must NOT contain the raw key; got: {formatted:?}"
        );
        assert!(
            formatted.contains("<redacted>"),
            "SetKey::Set Debug must contain '<redacted>'; got: {formatted:?}"
        );

        // Keep and Clear must still render their variant names.
        let keep_fmt = format!("{:?}", SetKey::Keep);
        assert!(
            keep_fmt.contains("Keep"),
            "SetKey::Keep Debug must contain 'Keep'; got: {keep_fmt:?}"
        );

        let clear_fmt = format!("{:?}", SetKey::Clear);
        assert!(
            clear_fmt.contains("Clear"),
            "SetKey::Clear Debug must contain 'Clear'; got: {clear_fmt:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Test: keysource_debug_redacts_value
    // -----------------------------------------------------------------------

    /// `KeySource::Inline { value }` must NEVER appear in `{:?}` output.
    ///
    /// Mirrors `setkey_debug_redacts_value` for the detect path's key source.
    #[test]
    fn keysource_debug_redacts_value() {
        // Inline variant: value must be redacted, not the literal key string.
        let inline = KeySource::Inline { value: "sk-secret123".into() };
        let formatted = format!("{:?}", inline);
        assert!(
            !formatted.contains("sk-secret123"),
            "KeySource::Inline Debug must NOT contain the raw key; got: {formatted:?}"
        );
        assert!(
            formatted.contains("<redacted>"),
            "KeySource::Inline Debug must contain '<redacted>'; got: {formatted:?}"
        );

        // UseStored and None must still render their variant names.
        let use_stored_fmt = format!("{:?}", KeySource::UseStored);
        assert!(
            use_stored_fmt.contains("UseStored"),
            "KeySource::UseStored Debug must contain 'UseStored'; got: {use_stored_fmt:?}"
        );

        let none_fmt = format!("{:?}", KeySource::None);
        assert!(
            none_fmt.contains("None"),
            "KeySource::None Debug must contain 'None'; got: {none_fmt:?}"
        );
    }

    // =======================================================================
    // D2 tests — derive_models_url, map_models_response, detect_inner
    // =======================================================================

    mod detect {
        use super::*;
        // `ElmerKeyring`, `ApiKey`, `KeySource`, `DetectError`, `derive_models_url`,
        // `map_models_response`, `detect_inner` all come through `use super::*`.
        // `SocketAddr` is from std; `mockito` is in [dev-dependencies].
        use std::net::SocketAddr;

        // -------------------------------------------------------------------
        // Pure helper: derive_models_url
        // -------------------------------------------------------------------

        /// derive_models_url replaces /chat/completions suffix with /models,
        /// preserving any path prefix.
        ///
        /// Tests BOTH the suffix-replace branch AND the fallback branch
        /// explicitly, as required by the brief.
        #[test]
        fn derive_models_url_preserves_prefix() {
            // /api/v1/chat/completions → /api/v1/models
            let ep = derive_models_url("https://api.openai.com/api/v1/chat/completions")
                .expect("must derive OK");
            assert_eq!(
                ep.url().path(),
                "/api/v1/models",
                "/api/v1/chat/completions must yield /api/v1/models"
            );

            // /v1/chat/completions → /v1/models
            let ep2 = derive_models_url("http://127.0.0.1:11434/v1/chat/completions")
                .expect("must derive OK");
            assert_eq!(
                ep2.url().path(),
                "/v1/models",
                "/v1/chat/completions must yield /v1/models"
            );
        }

        /// derive_models_url falls back to <origin>/v1/models when the path
        /// does NOT end with /chat/completions.
        #[test]
        fn derive_models_url_no_chat_completions_fallback() {
            // A bare custom path — must not append /models to the custom path;
            // must use the OpenAI-standard <origin>/v1/models.
            let ep = derive_models_url("https://api.openai.com/custom/path")
                .expect("must derive OK");
            assert_eq!(
                ep.url().path(),
                "/v1/models",
                "non-chat-completions path must fall back to /v1/models"
            );
            assert_eq!(
                ep.origin(),
                "https://api.openai.com",
                "origin must be preserved"
            );
        }

        /// derive_models_url with a loopback endpoint (no /chat/completions) also
        /// falls back to <origin>/v1/models correctly.
        #[test]
        fn derive_models_url_loopback_fallback() {
            let ep = derive_models_url("http://127.0.0.1:11434/some/custom")
                .expect("must derive OK");
            assert_eq!(ep.url().path(), "/v1/models");
            assert_eq!(ep.origin(), "http://127.0.0.1:11434");
        }

        /// derive_models_url rejects an invalid endpoint string.
        #[test]
        fn derive_models_url_rejects_invalid() {
            let err = derive_models_url("not a url");
            assert!(
                matches!(err, Err(DetectError::BadUrl(_))),
                "invalid endpoint must yield BadUrl, got: {err:?}"
            );
        }

        // -------------------------------------------------------------------
        // Pure helper: map_models_response
        // -------------------------------------------------------------------

        /// 200 with a valid `{data:[{id},…]}` body returns Ok(ids).
        #[test]
        fn map_models_response_200_ok() {
            let body = r#"{"data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#;
            let result = map_models_response(200, body, "api.openai.com", None);
            assert_eq!(
                result.unwrap(),
                vec!["gpt-4o", "gpt-4o-mini"],
                "200 with valid data must parse to model IDs"
            );
        }

        /// 200 with `data: []` returns ZeroModels.
        #[test]
        fn map_models_response_200_empty_data_is_zero_models() {
            let body = r#"{"data":[]}"#;
            let result = map_models_response(200, body, "api.openai.com", None);
            assert!(
                matches!(result, Err(DetectError::ZeroModels)),
                "empty data array must yield ZeroModels, got: {result:?}"
            );
        }

        /// 401 with a body that contains the bearer token returns Auth — NEVER
        /// echoes the body or the key.
        #[test]
        fn map_models_response_401_auth_no_body_echo() {
            let secret = "sk-super-secret";
            let key = ApiKey::new(secret);
            // A 401 body that echoes the key — must not appear in the error.
            let body = format!("Unauthorized: Bearer {secret} is invalid");
            let result = map_models_response(401, &body, "api.openai.com", Some(&key));

            match result {
                Err(DetectError::Auth { provider }) => {
                    let reason = DetectError::Auth {
                        provider: provider.clone(),
                    }
                    .to_reason();
                    assert!(
                        !reason.contains(secret),
                        "reason must NOT contain the key; got: {reason:?}"
                    );
                    assert!(
                        !reason.contains("Unauthorized"),
                        "reason must NOT echo the body; got: {reason:?}"
                    );
                    assert!(
                        reason.contains("check the API key"),
                        "reason must mention 'check the API key'; got: {reason:?}"
                    );
                }
                other => panic!("expected Auth, got: {other:?}"),
            }
        }

        /// 403 maps to Auth (same as 401).
        #[test]
        fn map_models_response_403_auth() {
            let result = map_models_response(403, "forbidden", "api.openai.com", None);
            assert!(
                matches!(result, Err(DetectError::Auth { .. })),
                "403 must map to Auth, got: {result:?}"
            );
        }

        /// 500 maps to Status(500).
        #[test]
        fn map_models_response_500_status() {
            let result = map_models_response(500, "internal error", "api.openai.com", None);
            assert!(
                matches!(result, Err(DetectError::Status(500))),
                "500 must map to Status(500), got: {result:?}"
            );
        }

        // -------------------------------------------------------------------
        // Key-source resolution via detect_inner
        //
        // mockito IS a dev-dep (verified in Cargo.toml: `mockito = "1.5"`).
        // These tests use mockito to drive the actual HTTP path end-to-end.
        //
        // Note on the egress gate: build_vetted_client enforces the SSRF
        // policy.  mockito binds to 127.0.0.1 (an IP literal), so the egress
        // gate takes the IP-literal branch — it calls `elmer_ip_is_permitted`
        // directly and does NOT invoke the injected resolver.  We pass a
        // dummy never-called resolver because the generic signature requires
        // one.  The gate permits loopback literals because is_loopback() is
        // true on the derived 127.0.0.1 endpoint.
        // -------------------------------------------------------------------

        /// Dummy resolver — never called for IP-literal (127.x.x.x) endpoints.
        fn no_dns_resolver(
        ) -> impl Fn(String, u16) -> std::future::Ready<std::io::Result<Vec<SocketAddr>>> {
            |_host: String, _port: u16| {
                std::future::ready(Err(std::io::Error::other("no DNS in test")))
            }
        }

        /// UseStored reads the key from the keyring and sends it as the bearer.
        ///
        /// The mockito server asserts the exact Authorization header value, so if
        /// detect_inner fails to forward the stored key the mock assertion fails.
        #[tokio::test]
        async fn detect_use_stored_reads_keyring() {
            let mut server = mockito::Server::new_async().await;
            let server_url = server.url();
            let secret = "sk-stored-key";

            let _m = server
                .mock("GET", "/v1/models")
                .match_header("authorization", format!("Bearer {secret}").as_str())
                .with_status(200)
                .with_body(r#"{"data":[{"id":"gpt-4o"}]}"#)
                .create_async()
                .await;

            let kr = Arc::new(ElmerKeyring::with_memory_keyring());
            // The stored endpoint uses the mockito server URL + /v1/chat/completions.
            // origin() is the scheme+host+port part that becomes the keyring key.
            let endpoint_str = format!("{server_url}/v1/chat/completions");
            let ep = AgentEndpoint::parse(&endpoint_str).expect("must parse");
            let origin = ep.origin();
            kr.set(&origin, &ApiKey::new(secret)).expect("pre-store");

            let result = detect_inner(
                endpoint_str,
                KeySource::UseStored,
                &kr,
                no_dns_resolver(),
            )
            .await;

            _m.assert_async().await;
            assert_eq!(result.unwrap(), vec!["gpt-4o"]);
        }

        /// Inline key does not touch the keyring (empty keyring, key provided inline).
        #[tokio::test]
        async fn detect_inline_does_not_touch_keyring() {
            let mut server = mockito::Server::new_async().await;
            let server_url = server.url();
            let secret = "sk-inline-key";

            let _m = server
                .mock("GET", "/v1/models")
                .match_header("authorization", format!("Bearer {secret}").as_str())
                .with_status(200)
                .with_body(r#"{"data":[{"id":"text-ada-001"}]}"#)
                .create_async()
                .await;

            // Empty keyring — UseStored would find nothing and probe unauthenticated.
            let kr = Arc::new(ElmerKeyring::with_memory_keyring());
            let endpoint_str = format!("{server_url}/v1/chat/completions");

            let result = detect_inner(
                endpoint_str,
                KeySource::Inline { value: secret.into() },
                &kr,
                no_dns_resolver(),
            )
            .await;

            _m.assert_async().await;
            assert_eq!(result.unwrap(), vec!["text-ada-001"]);
        }

        /// 401 from the server maps to Auth — the fixed reason is returned, not
        /// the body, and the reason does NOT contain the sent key.
        #[tokio::test]
        async fn detect_maps_401_to_auth_no_body_echo() {
            let mut server = mockito::Server::new_async().await;
            let server_url = server.url();
            let secret = "sk-rejected-key";

            // The server echoes the bearer in the 401 body (adversarial).
            let body = format!("Bearer {secret} is invalid");
            let _m = server
                .mock("GET", "/v1/models")
                .with_status(401)
                .with_body(&body)
                .create_async()
                .await;

            let kr = Arc::new(ElmerKeyring::with_memory_keyring());
            let endpoint_str = format!("{server_url}/v1/chat/completions");

            let result = detect_inner(
                endpoint_str,
                KeySource::Inline { value: secret.into() },
                &kr,
                no_dns_resolver(),
            )
            .await;

            _m.assert_async().await;
            match result {
                Err(DetectError::Auth { provider }) => {
                    let reason = DetectError::Auth { provider }.to_reason();
                    // The FIXED reason is used; body must not appear.
                    assert!(
                        reason.contains("check the API key"),
                        "reason must say 'check the API key'; got: {reason:?}"
                    );
                    // Key must not appear in the reason either.
                    assert!(
                        !reason.contains(secret),
                        "reason must NOT contain the key; got: {reason:?}"
                    );
                }
                other => panic!("expected Auth, got: {other:?}"),
            }
        }

        /// A connection-refused (dead loopback port) maps to NoServer.
        ///
        /// Port 1 is traditionally reserved; nothing listens on it on a Pi.
        /// Since the endpoint is an IP literal (127.0.0.1:1), the egress gate
        /// takes the IP-literal branch (no DNS, no resolver call) and the client
        /// attempts to connect directly to port 1 — which is refused.
        #[tokio::test]
        async fn detect_maps_connection_refused_to_no_server() {
            let kr = Arc::new(ElmerKeyring::with_memory_keyring());
            // Endpoint on port 1 — connection will be refused (nothing listens).
            let result = detect_inner(
                "http://127.0.0.1:1/v1/chat/completions".into(),
                KeySource::None,
                &kr,
                // Resolver is never called for an IP-literal endpoint; supply a
                // dummy that would always fail if it were called.
                |_host: String, _port: u16| async move {
                    Err(std::io::Error::other("should not be called"))
                },
            )
            .await;

            assert!(
                matches!(result, Err(DetectError::NoServer { .. })),
                "connection refused must map to NoServer, got: {result:?}"
            );
        }

        /// 200 with valid data parses model IDs correctly.
        #[tokio::test]
        async fn detect_parses_model_ids() {
            let mut server = mockito::Server::new_async().await;
            let server_url = server.url();

            let _m = server
                .mock("GET", "/v1/models")
                .with_status(200)
                .with_body(r#"{"data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#)
                .create_async()
                .await;

            let kr = Arc::new(ElmerKeyring::with_memory_keyring());
            let endpoint_str = format!("{server_url}/v1/chat/completions");

            let result = detect_inner(endpoint_str, KeySource::None, &kr, no_dns_resolver()).await;

            _m.assert_async().await;
            assert_eq!(result.unwrap(), vec!["gpt-4o", "gpt-4o-mini"]);
        }

        /// 200 with `data: []` maps to ZeroModels.
        #[tokio::test]
        async fn detect_empty_data_is_zero_models() {
            let mut server = mockito::Server::new_async().await;
            let server_url = server.url();

            let _m = server
                .mock("GET", "/v1/models")
                .with_status(200)
                .with_body(r#"{"data":[]}"#)
                .create_async()
                .await;

            let kr = Arc::new(ElmerKeyring::with_memory_keyring());
            let endpoint_str = format!("{server_url}/v1/chat/completions");

            let result = detect_inner(endpoint_str, KeySource::None, &kr, no_dns_resolver()).await;

            _m.assert_async().await;
            let reason = result.unwrap_err().to_reason();
            assert!(
                reason.contains("no models") || reason.contains("empty model list"),
                "ZeroModels reason must mention empty list; got: {reason:?}"
            );
        }
        // -------------------------------------------------------------------
        // FIX 1 test: UseStored with an unreadable keyring must FAIL CLOSED
        // -------------------------------------------------------------------

        /// `KeySource::UseStored` + a locked/unavailable keyring must return
        /// `Err(DetectError::UnreadableKey)`, NOT perform a keyless probe.
        ///
        /// Prior to the fix, `keyring.read(&origin).unwrap_or(None)` on a
        /// backend error silently produced `None`, causing a keyless GET to a
        /// cloud provider with no bearer token.  The fixed path uses
        /// `spawn_blocking` and fails closed on any `Err` from `keyring.read`.
        ///
        /// This test uses `FailingEntry` (PlatformFailure on `get_password`),
        /// which is the same fake used in `set_keyring_failure_is_transactional`
        /// above.
        ///
        /// # Why mockito here?
        ///
        /// `detect_inner` Step 2 (`build_vetted_client`) must succeed BEFORE Step 3
        /// (key read) is reached.  For a loopback IP-literal endpoint mockito binds
        /// to, the egress gate takes the direct-IP branch and skips the resolver —
        /// so `no_dns_resolver` is safe here.  The mock server needs no response
        /// spec because `detect_inner` returns before issuing any GET (the key read
        /// in Step 3 fails first).
        #[tokio::test]
        async fn detect_use_stored_unreadable_keyring_is_error() {
            // Bind a mockito server so build_vetted_client (Step 2) can succeed
            // for the IP-literal loopback endpoint, letting us reach Step 3.
            let server = mockito::Server::new_async().await;
            let server_url = server.url();
            let endpoint_str = format!("{server_url}/v1/chat/completions");

            // failing_keyring() is visible via `use super::*`.
            let kr = Arc::new(failing_keyring());

            let result = detect_inner(
                endpoint_str,
                KeySource::UseStored,
                &kr,
                no_dns_resolver(),
            )
            .await;

            assert!(
                matches!(result, Err(DetectError::UnreadableKey)),
                "UseStored + locked keyring must be UnreadableKey (fail-closed), \
                 not a keyless probe; got: {result:?}"
            );

            // Confirm the to_reason() string mentions "keyring".
            let reason = DetectError::UnreadableKey.to_reason();
            assert!(
                reason.contains("keyring"),
                "UnreadableKey reason must mention 'keyring'; got: {reason:?}"
            );
        }
    } // mod detect

    // -----------------------------------------------------------------------
    // FIX 2 test: config_read_inner loopback skips the keyring entirely
    // -----------------------------------------------------------------------

    /// `config_read_inner` with a loopback endpoint must return
    /// `key_status == Absent` WITHOUT consulting the keyring.
    ///
    /// We use `failing_keyring()` (PlatformFailure on any read) as the
    /// injected keyring.  If `config_read_inner` calls the keyring for a
    /// loopback endpoint, it would return `Unreadable` — not `Absent`.
    /// Receiving `Absent` proves the keyring was never touched.
    ///
    /// This mirrors `build_turn_provider_loopback_no_keyring_read` in
    /// `session.rs`, which uses the same technique to prove the `!is_loopback`
    /// guard is in place.
    #[tokio::test]
    async fn config_read_loopback_skips_keyring() {
        // A panicking / failing keyring — any call to it returns Unreadable.
        // If the loopback guard is missing and the keyring IS consulted, the
        // test will receive Unreadable and the assertion below will fail,
        // proving the regression.
        let kr = Arc::new(failing_keyring());

        // Ollama-style loopback endpoint: no port reachability needed;
        // config_read_inner reads the snapshot only.
        let loopback_ep = "http://127.0.0.1:11434/v1/chat/completions";
        let state = ElmerModelConfigState::new(loopback_ep.into(), "llama3".into(), 900);

        let dto = config_read_inner(&state, &kr)
            .await
            .expect("config_read_inner must succeed for loopback even with failing keyring");

        assert_eq!(
            dto.key_status,
            KeyStatus::Absent,
            "loopback endpoint must yield Absent without consulting the keyring; \
             got {:?} — the loopback guard is missing or the keyring was called",
            dto.key_status,
        );
    }
}
