//! App-wide backend handle held in Tauri managed state.
//!
//! Spec: docs/superpowers/specs/2026-05-20-pat-spawn-bootstrap-design.md §3.4
//! bd issue: tuxlink-22l (Task D — single BackendState + three-state status)
//! Supersedes: the `AppBackend(RwLock<Option<Arc<...>>>)` shape from Task 12
//! (tuxlink-zsm, spec 2026-05-19 §1.1).
//!
//! [`BackendState`] is the SINGLE Tauri-managed state every UI command and the
//! `backend_status` ribbon consume. It replaces the prior `AppBackend`, which
//! could only answer "is there a backend?" (`Some`/`None`) and so could not
//! distinguish "configured but Pat failed" from "offline / not connected".
//!
//! **Why one state, not two (adrev #9 — no torn read):** the bootstrap needs to
//! express a *phase* (`Spawning`/`Ready`/`Failed`/`ConfigError`/`NotConfigured`)
//! AND hold the live backend `Arc` once `Ready`. Holding the phase and the
//! `Option<Arc<…>>` behind TWO independent locks would let a reader observe a
//! torn pair — e.g. backend `Some` while the phase still reads `Spawning`, or
//! phase `Ready` a beat before the `Arc` is installed. `BackendState` keeps both
//! behind ONE `RwLock` and exposes [`BackendState::snapshot`] for an atomic read
//! of the pair under a single guard.
//!
//! **Concurrency invariant (unchanged from Task 12):** the trait is
//! `Send + Sync`, so a command MUST clone the `Arc`, DROP the `RwLock` read
//! guard, and ONLY THEN `.await` the trait method — never hold the guard across
//! an await point. [`BackendState::current`] enforces the clone-and-drop half
//! of that contract (it returns an owned `Arc`, holding no guard).

use std::sync::{Arc, RwLock};

use crate::winlink_backend::WinlinkBackend;

/// Lifecycle phase of the app's Winlink backend. Drives the three-state ribbon
/// (`backend_status`) per spec §2 / §3.4:
///
/// - [`BackendPhase::NotConfigured`] → "not connected" empty state (pre-wizard
///   OR offline `connect_to_cms = false`). Maps to `backend_status` → `None`.
/// - [`BackendPhase::Spawning`] → "connecting" (the bootstrap is launching Pat).
/// - [`BackendPhase::Ready`] → a live backend is installed; `backend_status`
///   projects the backend's own `status()`.
/// - [`BackendPhase::Failed`] → CMS configured but Pat spawn/health failed; the
///   ribbon shows an explicit error + reason (NOT a benign empty state).
/// - [`BackendPhase::ConfigError`] → a config file exists but is unusable
///   (`Serde`/`Validation`/`Io` from `read_config`); also an explicit error.
///
/// `Failed` vs `ConfigError` both project to `BackendStatus::Error { reason }`
/// at the ribbon, but are kept distinct so the reason string and any future
/// remediation differ (spec adrev #15: a malformed config is NOT "offline").
#[derive(Clone, Debug)]
pub enum BackendPhase {
    NotConfigured,
    Spawning,
    Ready,
    Failed { reason: String },
    ConfigError { reason: String },
}

/// Tauri-managed handle to the active Winlink backend AND its lifecycle phase,
/// behind one lock (adrev #9).
///
/// The pair starts `(NotConfigured, None)`. The bootstrap (`lib.rs` `.setup()`)
/// drives the phase: `set_phase(Spawning)` → on success `install(backend)`
/// (atomically `(Ready, Some(backend))`), on failure `set_phase(Failed{..})` /
/// `set_phase(ConfigError{..})`. Offline / pre-wizard launches end at
/// `set_phase(NotConfigured)`.
pub struct BackendState {
    inner: RwLock<(BackendPhase, Option<Arc<dyn WinlinkBackend>>)>,
}

impl BackendState {
    /// Construct an empty handle: `(NotConfigured, None)`. The bootstrap
    /// transitions it once `read_config` is classified (spec §3.3).
    pub fn new() -> Self {
        BackendState {
            inner: RwLock::new((BackendPhase::NotConfigured, None)),
        }
    }

    /// Set the phase. If `phase` is anything OTHER than [`BackendPhase::Ready`],
    /// the backend `Arc` is cleared — a non-`Ready` phase must never leave a
    /// stale backend installed (the invariant `backend.is_some() ⇒ Ready`).
    /// Installing a `Ready` backend is done via [`BackendState::install`], not
    /// here (calling `set_phase(Ready)` would clear the backend — see the
    /// `set_phase(Ready)` test). A poisoned lock is a no-op (degrades to "no
    /// transition" rather than panicking the setup thread).
    pub fn set_phase(&self, phase: BackendPhase) {
        if let Ok(mut guard) = self.inner.write() {
            let is_ready = matches!(phase, BackendPhase::Ready);
            guard.0 = phase;
            if !is_ready {
                guard.1 = None;
            }
        }
    }

    /// Install the active backend, atomically setting `(Ready, Some(backend))`
    /// under one write lock. Called once by the bootstrap after
    /// `PatBackend::spawn` succeeds. A poisoned lock is a no-op (the install is
    /// dropped rather than panicking; the only writers are the bootstrap's own
    /// phase transitions, none of which panic under the guard).
    pub fn install(&self, backend: Arc<dyn WinlinkBackend>) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = (BackendPhase::Ready, Some(backend));
        }
    }

    /// Atomic snapshot of `(phase, backend)` under ONE read lock (adrev #9 —
    /// no torn read). Both are cloned and the guard is dropped before return,
    /// so callers `.await` (if at all) holding no lock. A poisoned lock
    /// degrades to `(NotConfigured, None)` (→ ribbon "not connected") rather
    /// than panicking a command handler.
    pub fn snapshot(&self) -> (BackendPhase, Option<Arc<dyn WinlinkBackend>>) {
        match self.inner.read() {
            Ok(guard) => (guard.0.clone(), guard.1.clone()),
            Err(_) => (BackendPhase::NotConfigured, None),
        }
    }

    /// The active backend for command consumers: clone the `Arc` and drop the
    /// guard (the mandated clone-and-drop pattern — see the module doc's
    /// concurrency invariant). Returns `None` unless the phase is `Ready` (a
    /// non-`Ready` phase never holds a backend, by the `set_phase` invariant),
    /// OR when the lock is poisoned. Commands map `None` → `NotConfigured`
    /// (the "not connected" empty state, not an error).
    pub fn current(&self) -> Option<Arc<dyn WinlinkBackend>> {
        self.snapshot().1
    }
}

impl Default for BackendState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink_backend::NativeBackend;

    // A fresh BackendState is (NotConfigured, None): `current()` is None — the
    // command layer maps this to NotConfigured. (Migrated from Task-12
    // `empty_app_backend_current_is_none`.)
    #[test]
    fn empty_backend_state_current_is_none() {
        let state = BackendState::new();
        assert!(
            state.current().is_none(),
            "fresh BackendState must be None → NotConfigured"
        );
        assert!(
            matches!(state.snapshot().0, BackendPhase::NotConfigured),
            "fresh BackendState phase is NotConfigured"
        );
    }

    // install() yields a backend AND a Ready phase, atomically. (Migrated from
    // Task-12 `set_then_current_returns_a_backend`; `set` → `install`.)
    #[test]
    fn install_then_current_returns_a_backend_and_ready() {
        let state = BackendState::new();
        state.install(Arc::new(NativeBackend::test_fixture()));
        let (phase, backend) = state.snapshot();
        assert!(backend.is_some(), "after install, current() yields the backend");
        assert!(matches!(phase, BackendPhase::Ready), "install sets phase Ready");
    }

    // set_phase to a non-Ready phase clears any installed backend (the
    // backend.is_some() ⇒ Ready invariant). (Migrated from Task-12
    // `clear_resets_to_none`: a Failed transition is the new "clear".)
    #[test]
    fn set_phase_failed_clears_backend() {
        let state = BackendState::new();
        state.install(Arc::new(NativeBackend::test_fixture()));
        state.set_phase(BackendPhase::Failed {
            reason: "spawn failed".to_string(),
        });
        let (phase, backend) = state.snapshot();
        assert!(backend.is_none(), "Failed phase clears the backend");
        assert!(
            matches!(phase, BackendPhase::Failed { .. }),
            "phase is Failed after set_phase"
        );
    }

    // set_phase(Ready) WITHOUT a backend clears the Option — Ready must be
    // reached via install(), not set_phase(Ready). This documents the
    // intentional asymmetry (set_phase(Ready) is not how you install a backend).
    #[test]
    fn set_phase_ready_without_install_has_no_backend() {
        let state = BackendState::new();
        state.set_phase(BackendPhase::Ready);
        assert!(
            state.current().is_none(),
            "set_phase(Ready) does not conjure a backend; use install()"
        );
    }

    // ConfigError is a distinct phase carrying its reason (adrev #15).
    #[test]
    fn set_phase_config_error_carries_reason() {
        let state = BackendState::new();
        state.set_phase(BackendPhase::ConfigError {
            reason: "bad json".to_string(),
        });
        match state.snapshot().0 {
            BackendPhase::ConfigError { reason } => assert_eq!(reason, "bad json"),
            other => panic!("expected ConfigError, got {other:?}"),
        }
        assert!(state.current().is_none(), "ConfigError holds no backend");
    }

    // The clone-and-drop contract: two sequential current() calls each acquire
    // + release the read guard; a second call while the first Arc is alive must
    // not deadlock (proves the guard is dropped, not held in the returned
    // value). (Migrated from Task-12 `current_clones_arc_without_holding_lock`.)
    #[test]
    fn current_clones_arc_without_holding_lock() {
        let state = BackendState::new();
        state.install(Arc::new(NativeBackend::test_fixture()));
        let a = state.current();
        let b = state.current();
        assert!(a.is_some() && b.is_some());
    }
}
