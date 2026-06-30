//! Atomic `{endpoint, model}` configuration guard for the Elmer agent.
//!
//! [`ElmerModelConfigState`] wraps a `tokio::sync::Mutex<ModelConfigSnapshot>`
//! so that a turn's config snapshot and a concurrent `elmer_config_set` write
//! are serialised w.r.t. each other.  Neither a torn read (endpoint from the
//! new config, model from the old) nor a torn write is possible.
//!
//! # Why `tokio::sync::Mutex` and not `std::sync::Mutex`
//!
//! Task E2 (and D1 later) must hold this lock *across* async keyring calls.
//! `std::sync::Mutex` cannot be held across an `.await` point — the guard does
//! not implement `Send`, so the future becomes `!Send` and the Tokio runtime
//! refuses to schedule it.  `tokio::sync::Mutex` is the only sound choice here.
//!
//! # Key is NOT stored here
//!
//! `ModelConfigSnapshot` holds only the non-secret `{endpoint, model}` pair.
//! The API key is fetched fresh from the OS keyring (keyed by `endpoint.origin()`)
//! by the consumer (E2 / D1) while holding this same lock, so the key and the
//! config they belong to are always atomically consistent.

use tokio::sync::{Mutex, MutexGuard};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of the Elmer model configuration.
///
/// Intentionally cheap to clone — both fields are short strings.  Does NOT
/// contain the API key; the key is fetched from the OS keyring under the same
/// [`ElmerModelConfigState`] lock by the consumer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelConfigSnapshot {
    pub endpoint: String,
    pub model: String,
    /// Live-applied per-turn wall-clock timeout, in SECONDS (tuxlink-1wi5w).
    /// `ElmerSession::send` reads this off the snapshot to build the run
    /// `Limits`, so an `elmer_config_set` that advances this value takes effect
    /// on the NEXT turn. Always already clamped to `[30, 3600]` by
    /// `config_set_inner` before it reaches the snapshot.
    pub turn_timeout_secs: u32,
}

/// Guards the `{endpoint, model}` pair so reads and writes are atomic.
///
/// All public methods are `async` because the underlying mutex is a
/// `tokio::sync::Mutex`.  Callers on the Tokio runtime pay a negligible
/// contention cost in the uncontended case.
pub struct ElmerModelConfigState {
    inner: Mutex<ModelConfigSnapshot>,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl ElmerModelConfigState {
    /// Create a new state guard with an initial `{endpoint, model,
    /// turn_timeout_secs}`.
    pub fn new(endpoint: String, model: String, turn_timeout_secs: u32) -> Self {
        Self {
            inner: Mutex::new(ModelConfigSnapshot {
                endpoint,
                model,
                turn_timeout_secs,
            }),
        }
    }

    /// Return a clone of the current snapshot.
    ///
    /// The clone is taken under the mutex so the pair is always internally
    /// consistent — consumers never see endpoint-A with model-B.
    pub async fn snapshot(&self) -> ModelConfigSnapshot {
        self.inner.lock().await.clone()
    }

    /// Atomically replace the stored `{endpoint, model, turn_timeout_secs}`.
    pub async fn set(&self, endpoint: String, model: String, turn_timeout_secs: u32) {
        let mut guard = self.inner.lock().await;
        guard.endpoint = endpoint;
        guard.model = model;
        guard.turn_timeout_secs = turn_timeout_secs;
    }

    /// Acquire the underlying mutex guard for a transactional write.
    ///
    /// Intended for Task D1's live-apply path, which needs to hold the lock
    /// across a keyring write + config-file write + snapshot replace so that
    /// no concurrent reader can observe a half-applied update.
    pub async fn lock(&self) -> MutexGuard<'_, ModelConfigSnapshot> {
        self.inner.lock().await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Barrier;

    // -----------------------------------------------------------------------
    // snapshot_returns_current
    // -----------------------------------------------------------------------

    /// A freshly constructed state returns the exact values passed to `new`.
    #[tokio::test]
    async fn snapshot_returns_current() {
        let state = ElmerModelConfigState::new(
            "http://127.0.0.1:11434/v1/chat/completions".to_owned(),
            "llama3".to_owned(),
            900,
        );

        let snap = state.snapshot().await;

        assert_eq!(snap.endpoint, "http://127.0.0.1:11434/v1/chat/completions");
        assert_eq!(snap.model, "llama3");
        assert_eq!(snap.turn_timeout_secs, 900);
    }

    // -----------------------------------------------------------------------
    // set_then_snapshot_reflects_change
    // -----------------------------------------------------------------------

    /// After a `set`, `snapshot` returns the new values.
    #[tokio::test]
    async fn set_then_snapshot_reflects_change() {
        let state = ElmerModelConfigState::new(
            "http://127.0.0.1:11434/v1/chat/completions".to_owned(),
            "llama3".to_owned(),
            900,
        );

        state
            .set(
                "https://api.openai.com/v1/chat/completions".to_owned(),
                "gpt-4o".to_owned(),
                120,
            )
            .await;

        let snap = state.snapshot().await;
        assert_eq!(snap.endpoint, "https://api.openai.com/v1/chat/completions");
        assert_eq!(snap.model, "gpt-4o");
        assert_eq!(snap.turn_timeout_secs, 120);
    }

    // -----------------------------------------------------------------------
    // concurrent_set_and_snapshot_are_atomic
    // -----------------------------------------------------------------------

    /// A concurrent `set` and `snapshot` must produce an internally consistent
    /// pair — either *both* old values or *both* new values.  A torn read such
    /// as `{endpoint_new, model_old}` is a concurrency bug.
    ///
    /// How the concurrency is exercised:
    ///   - Two tasks are spawned onto the Tokio runtime via `tokio::spawn`.
    ///   - A `tokio::sync::Barrier(2)` ensures both tasks reach their
    ///     critical-section entry point at the *same moment* before either
    ///     proceeds.  This maximises the probability of true interleaving.
    ///   - The mutex then serialises the two; the test asserts that whichever
    ///     order they ran in, the snapshot is one of the two known consistent
    ///     states.
    ///
    /// See testing-pitfalls §5 "Concurrency & TOCTOU": barrier-gate ensures
    /// simultaneity; `tokio::spawn` ensures two real runtime tasks, not a
    /// sequential call sequence that hides the race.
    #[tokio::test]
    async fn concurrent_set_and_snapshot_are_atomic() {
        const OLD_ENDPOINT: &str = "http://127.0.0.1:11434/v1/chat/completions";
        const OLD_MODEL: &str = "llama3";
        const NEW_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
        const NEW_MODEL: &str = "gpt-4o";

        let state = Arc::new(ElmerModelConfigState::new(
            OLD_ENDPOINT.to_owned(),
            OLD_MODEL.to_owned(),
            900,
        ));

        // A barrier of 2 holds both tasks until both have reached wait().
        let barrier = Arc::new(Barrier::new(2));

        // Task A: waits at the barrier, then performs `set`.
        let state_a = Arc::clone(&state);
        let barrier_a = Arc::clone(&barrier);
        let handle_a = tokio::spawn(async move {
            barrier_a.wait().await;
            state_a
                .set(NEW_ENDPOINT.to_owned(), NEW_MODEL.to_owned(), 900)
                .await;
        });

        // Task B: waits at the barrier, then takes a snapshot.
        let state_b = Arc::clone(&state);
        let barrier_b = Arc::clone(&barrier);
        let handle_b = tokio::spawn(async move {
            barrier_b.wait().await;
            state_b.snapshot().await
        });

        handle_a.await.expect("set task panicked");
        let snap = handle_b.await.expect("snapshot task panicked");

        // The pair must be one of the two known consistent states.
        let old_state = (OLD_ENDPOINT, OLD_MODEL);
        let new_state = (NEW_ENDPOINT, NEW_MODEL);

        let snap_pair = (snap.endpoint.as_str(), snap.model.as_str());

        assert!(
            snap_pair == old_state || snap_pair == new_state,
            "torn read detected: got endpoint={:?} model={:?} — \
             expected either ({:?},{:?}) or ({:?},{:?})",
            snap.endpoint,
            snap.model,
            OLD_ENDPOINT,
            OLD_MODEL,
            NEW_ENDPOINT,
            NEW_MODEL,
        );
    }

    // -----------------------------------------------------------------------
    // lock_gives_mutable_guard
    // -----------------------------------------------------------------------

    /// `lock()` returns a guard that can be used to mutate the snapshot
    /// in-place — the D1 transactional-write pattern.
    #[tokio::test]
    async fn lock_gives_mutable_guard() {
        let state = ElmerModelConfigState::new(
            "http://127.0.0.1:11434/v1/chat/completions".to_owned(),
            "llama3".to_owned(),
            900,
        );

        {
            let mut guard = state.lock().await;
            guard.endpoint = "https://api.openai.com/v1/chat/completions".to_owned();
            guard.model = "gpt-4o".to_owned();
            guard.turn_timeout_secs = 600;
            // guard dropped here, releasing the lock
        }

        let snap = state.snapshot().await;
        assert_eq!(snap.endpoint, "https://api.openai.com/v1/chat/completions");
        assert_eq!(snap.model, "gpt-4o");
        assert_eq!(snap.turn_timeout_secs, 600);
    }
}
