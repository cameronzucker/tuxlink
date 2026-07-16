//! Dock registry — the runtime authority for surface dock state (spec §3).
//!
//! [`DockRegistry`] is Tauri managed state. It owns the in-memory
//! [`DockSnapshot`] and is the *single transition path* (spec §3):
//! mutate → best-effort persist → **always** emit `dock:changed` on an
//! effective transition. The registry is authoritative while the app runs;
//! the config write is write-through, and a failed persist never blocks the
//! emit and never lets two windows disagree (spec §3 "Runtime authority and
//! persist failure").
//!
//! The pure mutation core ([`apply_with_context`]) and the restoration
//! idempotence guard ([`RestorationGate`]) are unit-tested here without an
//! `AppHandle`; the `AppHandle`-touching persist + emit is the thin
//! [`DockRegistry::transition`] wrapper around them.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use crate::dock::{
    apply_transition, DockContext, DockMode, DockSnapshot, DockSurfaces, SurfaceId,
};

/// Pure transition core with continuity-token bookkeeping (spec §3, §7).
///
/// Returns `true` iff the transition is EFFECTIVE (the surface's mode
/// actually changed). On an effective transition the surface's continuity
/// token is REPLACED with `context` (a dock-back with `None` clears it). A
/// no-op transition (target already current) leaves state, context, and the
/// return all untouched — this suppression is what makes a concurrent double
/// dock-back safe (spec §3): the second one changes nothing and emits nothing.
pub fn apply_with_context(
    snap: &mut DockSnapshot,
    surface: SurfaceId,
    target: DockMode,
    context: Option<serde_json::Value>,
) -> bool {
    if !apply_transition(&mut snap.surfaces, surface, target) {
        // No-op: do NOT touch the stored token (spec §3 — the token belongs to
        // the most recent EFFECTIVE transition).
        return false;
    }
    snap.context.set(surface, context);
    true
}

/// Idempotence guard for launch restoration (spec §3). [`arm`](Self::arm)
/// returns `true` exactly once — the first `shell_mounted` proceeds, every
/// later arrival no-ops.
#[derive(Default)]
pub struct RestorationGate(AtomicBool);

impl RestorationGate {
    /// `true` on the first call, `false` on every subsequent call. `SeqCst`
    /// swap: the winning caller observes the prior `false` and flips it to
    /// `true`; all others observe `true`.
    pub fn arm(&self) -> bool {
        !self.0.swap(true, Ordering::SeqCst)
    }
}

/// The managed-state registry (spec §3). Owns the authoritative snapshot
/// behind a mutex; the persisted half seeds it at launch, the runtime
/// continuity tokens start empty (context is never persisted — spec §3, §7).
pub struct DockRegistry(Mutex<DockSnapshot>);

impl DockRegistry {
    /// Seed the registry from the persisted `surfaces` map (config `dock`
    /// section, config v8). Continuity tokens start empty.
    pub fn new(persisted: DockSurfaces) -> Self {
        DockRegistry(Mutex::new(DockSnapshot {
            surfaces: persisted,
            context: DockContext::default(),
        }))
    }

    /// The full current snapshot — the `dock_state_get` return and the value
    /// broadcast on `dock:changed` (spec §3: always the full snapshot, never
    /// deltas).
    pub fn snapshot(&self) -> DockSnapshot {
        self.lock().clone()
    }

    /// The one transition path (spec §3). Mutates the registry, persists the
    /// `surfaces` map best-effort while holding the mutex (registry mutation +
    /// persist are one critical section), then ALWAYS emits `dock:changed` on
    /// an effective transition. Returns `false` (and does nothing) for a no-op.
    ///
    /// A persist failure is logged (`tracing::warn!`) and surfaced as a
    /// session-log warning, but the emit still fires — the registry is
    /// authoritative and the only consequence of a failed write is a stale
    /// layout on next launch (spec §3).
    pub fn transition(
        &self,
        app: &AppHandle,
        surface: SurfaceId,
        target: DockMode,
        context: Option<serde_json::Value>,
    ) -> bool {
        let snapshot = {
            let mut guard = self.lock();
            if !apply_with_context(&mut guard, surface, target, context) {
                return false;
            }
            // Best-effort write-through of the persisted half, INSIDE the same
            // critical section (spec §3). `DockSurfaces` is `Copy`.
            let surfaces = guard.surfaces;
            if let Err(e) = crate::config::update_config(move |cfg| {
                cfg.dock = surfaces;
                Ok(())
            }) {
                tracing::warn!(
                    target: "tuxlink::dock",
                    error = %e,
                    ?surface,
                    "dock state persist failed; registry stays authoritative (stale layout on next launch)"
                );
                if let Some(buffer) =
                    app.try_state::<std::sync::Arc<crate::session_log::SessionLogState>>()
                {
                    crate::session_log_emit::emit(
                        app,
                        &buffer,
                        crate::winlink_backend::LogLevel::Warn,
                        crate::winlink_backend::LogSource::Backend,
                        format!("Dock layout could not be saved ({e}); it will not persist to the next launch."),
                    );
                }
            }
            guard.clone()
        };
        // Emit AFTER releasing the mutex — the registry is already consistent
        // and no listener callback should re-enter under our lock.
        let _ = app.emit("dock:changed", &snapshot);
        true
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DockSnapshot> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Step 1 test (brief): the continuity token is stored on an effective
    /// transition, REPLACED on the next effective transition of the same
    /// surface, and a no-op transition changes neither state nor token nor the
    /// return (spec §3).
    #[test]
    fn context_stored_on_effective_transition_and_cleared_on_next() {
        let mut snap = DockSnapshot::default();
        assert!(apply_with_context(
            &mut snap,
            SurfaceId::Routines,
            DockMode::Popped,
            Some(serde_json::json!({"view":"designer"}))
        ));
        assert_eq!(snap.context.routines.as_ref().unwrap()["view"], "designer");
        // Next transition of the same surface REPLACES the token (spec §3).
        assert!(apply_with_context(
            &mut snap,
            SurfaceId::Routines,
            DockMode::Docked,
            None
        ));
        assert!(snap.context.routines.is_none());
        // No-op transition: state, context, and return all unchanged.
        assert!(!apply_with_context(
            &mut snap,
            SurfaceId::Routines,
            DockMode::Docked,
            Some(serde_json::json!({"x":1}))
        ));
        assert!(snap.context.routines.is_none());
    }

    /// Step 1 test (brief): restoration fires exactly once.
    #[test]
    fn shell_mounted_is_idempotent() {
        let gate = RestorationGate::default();
        assert!(gate.arm()); // first call: proceed
        assert!(!gate.arm()); // every later call: no-op
    }

    /// A no-op transition must not disturb a token stored by a PRIOR effective
    /// transition of the same surface (double dock-back safety, spec §3).
    #[test]
    fn noop_transition_preserves_prior_token() {
        let mut snap = DockSnapshot::default();
        assert!(apply_with_context(
            &mut snap,
            SurfaceId::TacMap,
            DockMode::Popped,
            Some(serde_json::json!({"z":9}))
        ));
        // Re-pop on an already-Popped surface: no-op, token untouched.
        assert!(!apply_with_context(
            &mut snap,
            SurfaceId::TacMap,
            DockMode::Popped,
            Some(serde_json::json!({"z":10}))
        ));
        assert_eq!(snap.context.tac_map.as_ref().unwrap()["z"], 9);
    }

    /// `DockRegistry::new` seeds the persisted surfaces and starts with empty
    /// continuity tokens (context is runtime-only — spec §3, §7).
    #[test]
    fn new_seeds_surfaces_and_empty_context() {
        let mut persisted = DockSurfaces::default();
        persisted.set(SurfaceId::AprsChat, DockMode::Popped);
        let reg = DockRegistry::new(persisted);
        let snap = reg.snapshot();
        assert_eq!(snap.surfaces.get(SurfaceId::AprsChat), DockMode::Popped);
        assert_eq!(snap.surfaces.get(SurfaceId::Routines), DockMode::Docked);
        assert!(snap.context.aprs_chat.is_none());
    }
}
