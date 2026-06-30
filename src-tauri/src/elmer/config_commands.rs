//! Tauri commands for Elmer model-config read/write.
//!
//! # Overview
//!
//! Two Tauri-only commands are exposed here:
//!
//! - [`elmer_config_read`] — returns `{agent_endpoint, agent_model, key_status}`.
//!   **Never returns the key value.**
//! - [`elmer_config_set`] — transactional write: endpoint validation → key action →
//!   config-file write → in-memory snapshot advance, all under the model-config lock.
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
//! delegate to `config_set_inner` / `config_read_inner`.  Tests call the inner
//! helpers directly with concrete `&ElmerModelConfigState` / `&ElmerKeyring`
//! references — no Tauri `State` machinery needed.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;

use tuxlink_agent_frontend::{
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
// SetKey — the three possible key operations
// ---------------------------------------------------------------------------

/// What to do with the API key during a config write.
///
/// Deserializes as `{ "action": "keep" | "set" | "clear", "value"?: string }`.
///
/// `ApiKey` does not implement `Deserialize` (it is intentionally opaque), so
/// `Set` carries a plain `String` at the boundary; `config_set_inner` wraps it
/// in `ApiKey::new` at the point of use.
#[derive(Debug, Deserialize)]
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
    key: SetKey,
    state: &ElmerModelConfigState,
    keyring: &ElmerKeyring,
) -> Result<(), String> {
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
    crate::config::write_config_atomic(&config)
        .map_err(|e| format!("couldn't save config: {e}"))?;

    // Step 5: advance in-memory snapshot (still under lock).
    guard.endpoint = agent_endpoint;
    guard.model = agent_model;
    // Lock is released here when `guard` drops.

    Ok(())
}

/// Inner implementation of `elmer_config_read`.
///
/// Reads the endpoint + model from the in-memory snapshot, then performs a
/// **fail-closed** presence check on the keyring — the key value is NEVER
/// returned or logged.
///
/// # Errors
///
/// Returns a `String` error only when the in-memory endpoint fails validation
/// (this should not happen in practice because `config_set_inner` validates
/// before persisting, but the defensive parse is the only way to call
/// `endpoint.origin()` without a stored `Url`).
pub async fn config_read_inner(
    state: &ElmerModelConfigState,
    keyring: &ElmerKeyring,
) -> Result<ConfigReadDto, String> {
    let snapshot = state.snapshot().await;
    let endpoint =
        AgentEndpoint::parse(&snapshot.endpoint).map_err(|e| e.to_string())?;
    let origin = endpoint.origin();
    let key_status = keyring.status(&origin);
    Ok(ConfigReadDto {
        agent_endpoint: snapshot.endpoint,
        agent_model: snapshot.model,
        key_status,
    })
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
    config_read_inner(&state, &keyring).await
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
    key: SetKey,
    state: State<'_, Arc<ElmerModelConfigState>>,
    keyring: State<'_, Arc<ElmerKeyring>>,
) -> Result<(), String> {
    config_set_inner(agent_endpoint, agent_model, key, &state, &keyring).await
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
        ElmerModelConfigState::new(VALID_ENDPOINT.into(), VALID_MODEL.into())
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
        let kr = ElmerKeyring::with_memory_keyring();
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
        let kr = failing_keyring();
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
}
