//! App-wide backend handle held in Tauri managed state.
//!
//! Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §1.1
//! bd issue: tuxlink-zsm (Task 12 — main-UI cluster ROOT)
//!
//! `AppBackend` is the single instance of the [`WinlinkBackend`] trait that
//! every UI command consumes (see `ui_commands.rs`). It is set ONCE at app
//! bootstrap (in `lib.rs`'s `run()`) and read by every command — hence
//! `RwLock`, not `Mutex`. The wrapped value is `Option` because an offline
//! install (or a pre-connect launch) leaves it unset; commands that find
//! `None` project `UiError::NotConfigured` to the UI as a "not connected"
//! empty state, NOT an error.
//!
//! **Concurrency invariant (spec §1.1 + `winlink_backend.rs` trait doc):**
//! the trait is `Send + Sync`, so commands MUST clone the `Arc`, DROP the
//! `RwLock` read guard, and ONLY THEN `.await` the trait method — never hold
//! the guard across an await point. The [`AppBackend::current`] helper
//! enforces the clone-and-drop half of that contract.

use std::sync::{Arc, RwLock};

use crate::winlink_backend::WinlinkBackend;

/// Tauri-managed handle to the active Winlink backend.
///
/// `None` until the bootstrap installs a backend (offline installs stay
/// `None` for the whole session). Set via [`AppBackend::set`].
pub struct AppBackend(pub RwLock<Option<Arc<dyn WinlinkBackend>>>);

impl AppBackend {
    /// Construct an empty handle (no backend yet). The bootstrap calls
    /// [`AppBackend::set`] once Pat is spawned and a `PatBackend` exists.
    pub fn new() -> Self {
        AppBackend(RwLock::new(None))
    }

    /// Install the active backend. Called once at bootstrap. A poisoned
    /// lock is treated as "no backend" (the set is dropped) rather than
    /// panicking the setup hook — a degenerate case that only arises if a
    /// prior holder panicked while writing, which cannot happen here (the
    /// only writers are `set`/`clear`, neither of which panics under the
    /// guard).
    pub fn set(&self, backend: Arc<dyn WinlinkBackend>) {
        if let Ok(mut guard) = self.0.write() {
            *guard = Some(backend);
        }
    }

    /// Clear the active backend (e.g., teardown). Mirror of [`set`].
    #[allow(dead_code)] // used by teardown paths / future disconnect flows
    pub fn clear(&self) {
        if let Ok(mut guard) = self.0.write() {
            *guard = None;
        }
    }

    /// Snapshot the current backend by cloning the `Arc` and immediately
    /// dropping the read guard. Returns `None` when unset OR when the lock
    /// is poisoned. Callers `.await` on the returned `Arc` with NO lock
    /// held — the mandated pattern (spec §1.1). A poisoned lock degrades to
    /// `None` (→ `NotConfigured`) rather than panicking a command handler.
    pub fn current(&self) -> Option<Arc<dyn WinlinkBackend>> {
        self.0.read().ok().and_then(|guard| guard.clone())
    }
}

impl Default for AppBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink_backend::PatBackend;

    // Task-12 test (8), backend half: a fresh AppBackend has no backend, so
    // `current()` is None — the command layer maps this to NotConfigured.
    #[test]
    fn empty_app_backend_current_is_none() {
        let app = AppBackend::new();
        assert!(app.current().is_none(), "fresh AppBackend must be None → NotConfigured");
    }

    #[test]
    fn set_then_current_returns_a_backend() {
        let app = AppBackend::new();
        app.set(Arc::new(PatBackend::from_url("http://127.0.0.1:9")));
        assert!(app.current().is_some(), "after set, current() yields the backend");
    }

    #[test]
    fn clear_resets_to_none() {
        let app = AppBackend::new();
        app.set(Arc::new(PatBackend::from_url("http://127.0.0.1:9")));
        app.clear();
        assert!(app.current().is_none(), "clear() returns to None");
    }

    #[test]
    fn current_clones_arc_without_holding_lock() {
        // Two sequential current() calls each acquire + release the read
        // guard; a second call after the first's Arc is still alive must not
        // deadlock (proves the guard is dropped, not held in the returned
        // value). RwLock allows concurrent readers anyway, but this asserts
        // the clone-and-drop contract the async commands depend on.
        let app = AppBackend::new();
        app.set(Arc::new(PatBackend::from_url("http://127.0.0.1:9")));
        let a = app.current();
        let b = app.current();
        assert!(a.is_some() && b.is_some());
    }
}
