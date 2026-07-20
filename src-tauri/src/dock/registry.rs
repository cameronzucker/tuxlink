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

/// Pure transition + pop-generation bookkeeping, with an optional re-pop guard
/// (no `AppHandle`, no persist, no emit — unit-tested below).
///
/// `generations` is the registry's per-surface monotonic pop counter, indexed
/// by [`SurfaceId::index`]. The counter is bumped on every EFFECTIVE transition
/// to [`DockMode::Popped`], so it uniquely identifies *which* pop-out era a
/// surface is currently in.
///
/// `expected_pop_generation` is the guard the close-intent liveness timeout
/// uses (spec §3, behavior 4; adrev Round-2 close-intent re-pop finding). When
/// `Some`, and it does NOT match the surface's current generation, this refuses
/// (returns `false`) BEFORE mutating anything — the surface was docked back and
/// re-popped since the caller sampled the generation, so a stale timer must not
/// destroy the fresh window. When `None`, no guard is applied (the ordinary
/// pop-out / dock-back path). The compare and the mutation are one indivisible
/// step: the caller runs this whole function inside the registry critical
/// section, so no re-pop can slip between the check and the transition.
///
/// Returns `true` iff the transition is EFFECTIVE.
pub fn apply_with_generation(
    snap: &mut DockSnapshot,
    generations: &mut [u64; 4],
    surface: SurfaceId,
    target: DockMode,
    context: Option<serde_json::Value>,
    expected_pop_generation: Option<u64>,
) -> bool {
    if let Some(expected) = expected_pop_generation {
        if generations[surface.index()] != expected {
            // Re-pop guard tripped: the surface's pop era advanced since the
            // caller sampled the generation. Change nothing (spec §3 no-op).
            return false;
        }
    }
    if !apply_with_context(snap, surface, target, context) {
        return false;
    }
    if target == DockMode::Popped {
        // Effective pop-out: advance the pop era. A no-op pop returned above via
        // `apply_with_context`, and a dock-back leaves the counter untouched, so
        // this bumps ONLY on a genuine new pop window (adrev Round-2).
        generations[surface.index()] += 1;
    }
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

/// The mutex-protected interior of [`DockRegistry`]: the authoritative snapshot
/// plus the per-surface pop-generation counters. Both live under the SAME mutex
/// so a generation read/write is atomic with the snapshot transition it guards
/// (spec §3; adrev Round-2 close-intent re-pop finding) — no separate lock, no
/// new lock-order edge. The generations are registry-internal and NEVER cross
/// the wire (they are not part of [`DockSnapshot`], so the frontend contract is
/// unchanged).
struct RegistryState {
    snapshot: DockSnapshot,
    /// Monotonic pop counter per surface, indexed by [`SurfaceId::index`].
    /// Bumped on every effective transition to [`DockMode::Popped`].
    pop_generations: [u64; 4],
}

/// The managed-state registry (spec §3). Owns the authoritative snapshot
/// behind a mutex; the persisted half seeds it at launch, the runtime
/// continuity tokens start empty (context is never persisted — spec §3, §7).
pub struct DockRegistry(Mutex<RegistryState>);

impl DockRegistry {
    /// Seed the registry from the persisted `surfaces` map (config `dock`
    /// section, config v8). Continuity tokens start empty; pop generations
    /// start at zero.
    pub fn new(persisted: DockSurfaces) -> Self {
        DockRegistry(Mutex::new(RegistryState {
            snapshot: DockSnapshot {
                surfaces: persisted,
                context: DockContext::default(),
            },
            pop_generations: [0; 4],
        }))
    }

    /// The full current snapshot — the `dock_state_get` return and the value
    /// broadcast on `dock:changed` (spec §3: always the full snapshot, never
    /// deltas).
    pub fn snapshot(&self) -> DockSnapshot {
        self.lock().snapshot.clone()
    }

    /// The surface's current pop generation (spec §3; adrev Round-2). Sampled by
    /// the close-intent liveness timeout at arm time and passed back to
    /// [`DockRegistry::transition_if_pop_generation`] so a stale timer that
    /// fires after a dock-back-and-re-pop is suppressed. Lock, read, release.
    pub fn pop_generation(&self, surface: SurfaceId) -> u64 {
        self.lock().pop_generations[surface.index()]
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
        self.transition_inner(app, surface, target, context, None)
    }

    /// Generation-guarded transition (spec §3, behavior 4; adrev Round-2). Like
    /// [`DockRegistry::transition`], but the whole thing runs IFF the surface's
    /// pop generation still equals `expected_pop_generation` — the compare and
    /// the mutation share the one critical section, so no re-pop can slip
    /// between them. The close-intent liveness timeout uses this to dock back
    /// ONLY when the surface has not been docked-back-and-re-popped since the
    /// timer was armed. Returns `false` (does nothing) on a generation mismatch,
    /// exactly like a no-op transition.
    pub fn transition_if_pop_generation(
        &self,
        app: &AppHandle,
        surface: SurfaceId,
        target: DockMode,
        context: Option<serde_json::Value>,
        expected_pop_generation: u64,
    ) -> bool {
        self.transition_inner(app, surface, target, context, Some(expected_pop_generation))
    }

    /// Shared body of [`transition`](Self::transition) and
    /// [`transition_if_pop_generation`](Self::transition_if_pop_generation).
    /// `expected_pop_generation` threads the optional re-pop guard into the pure
    /// [`apply_with_generation`] core; everything else — the write-through
    /// persist inside the critical section, the post-unlock emit — is unchanged
    /// from the original single transition path.
    fn transition_inner(
        &self,
        app: &AppHandle,
        surface: SurfaceId,
        target: DockMode,
        context: Option<serde_json::Value>,
        expected_pop_generation: Option<u64>,
    ) -> bool {
        let snapshot = {
            let mut guard = self.lock();
            let state = &mut *guard;
            if !apply_with_generation(
                &mut state.snapshot,
                &mut state.pop_generations,
                surface,
                target,
                context,
                expected_pop_generation,
            ) {
                return false;
            }
            // Best-effort write-through of the persisted half, INSIDE the same
            // critical section (spec §3). `DockSurfaces` is `Copy`.
            let surfaces = state.snapshot.surfaces;
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
            state.snapshot.clone()
        };
        // Emit AFTER releasing the mutex — the registry is already consistent
        // and no listener callback should re-enter under our lock.
        let _ = app.emit("dock:changed", &snapshot);
        true
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryState> {
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

    /// Pop generation bumps on an EFFECTIVE pop-out, and ONLY then: a no-op
    /// re-pop (already `Popped`) and a dock-back both leave it untouched
    /// (adrev Round-2 close-intent re-pop guard).
    #[test]
    fn pop_generation_bumps_only_on_effective_pop() {
        let mut snap = DockSnapshot::default();
        let mut gens = [0u64; 4];
        let s = SurfaceId::Routines;
        let i = s.index();

        // Effective pop-out: bump.
        assert!(apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Popped, None, None
        ));
        assert_eq!(gens[i], 1);

        // No-op re-pop (already Popped): no transition, no bump.
        assert!(!apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Popped, None, None
        ));
        assert_eq!(gens[i], 1);

        // Effective dock-back: no bump (only Popped bumps).
        assert!(apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Docked, None, None
        ));
        assert_eq!(gens[i], 1);

        // No-op dock-back (already Docked): no bump.
        assert!(!apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Docked, None, None
        ));
        assert_eq!(gens[i], 1);

        // Fresh pop era: bump again.
        assert!(apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Popped, None, None
        ));
        assert_eq!(gens[i], 2);

        // Other surfaces' generations are independent.
        assert_eq!(gens[SurfaceId::TacMap.index()], 0);
        assert_eq!(gens[SurfaceId::AprsChat.index()], 0);
    }

    /// The generation-guarded dock-back proceeds when the expected generation
    /// matches and REFUSES (mutating nothing) when it differs — the re-pop
    /// timeline the guard closes (adrev Round-2).
    #[test]
    fn generation_guard_refuses_on_mismatch_proceeds_on_match() {
        let mut snap = DockSnapshot::default();
        let mut gens = [0u64; 4];
        let s = SurfaceId::TacMap;

        // Pop out: generation is now 1, surface Popped.
        assert!(apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Popped, None, None
        ));
        assert_eq!(gens[s.index()], 1);
        assert_eq!(snap.surfaces.get(s), DockMode::Popped);

        // A timer armed at generation 0 (a prior pop era) must NOT dock back:
        // the guard refuses before mutating, so the surface stays Popped.
        assert!(!apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Docked, None, Some(0)
        ));
        assert_eq!(snap.surfaces.get(s), DockMode::Popped);
        assert_eq!(gens[s.index()], 1);

        // A timer armed at the current generation docks back: effective.
        assert!(apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Docked, None, Some(1)
        ));
        assert_eq!(snap.surfaces.get(s), DockMode::Docked);
    }

    /// The guard is the re-pop timeline end to end (pure): pop (gen 1) → dock
    /// back → re-pop (gen 2) → a stale timer holding gen 1 is refused, leaving
    /// the fresh pop window's `Popped` state and token intact (adrev Round-2).
    #[test]
    fn generation_guard_survives_dock_back_then_repop() {
        let mut snap = DockSnapshot::default();
        let mut gens = [0u64; 4];
        let s = SurfaceId::AprsChat;

        // Pop: gen 1. This is the generation the close-intent timer samples.
        assert!(apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Popped, None, None
        ));
        let armed = gens[s.index()];
        assert_eq!(armed, 1);

        // Webview's own dock-back lands.
        assert!(apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Docked, None, None
        ));

        // User re-pops: gen 2, fresh continuity token.
        assert!(apply_with_generation(
            &mut snap,
            &mut gens,
            s,
            DockMode::Popped,
            Some(serde_json::json!({ "view": "fresh" })),
            None
        ));
        assert_eq!(gens[s.index()], 2);

        // The stale timer fires holding gen 1: refused. Fresh window survives.
        assert!(!apply_with_generation(
            &mut snap, &mut gens, s, DockMode::Docked, None, Some(armed)
        ));
        assert_eq!(snap.surfaces.get(s), DockMode::Popped);
        assert_eq!(snap.context.aprs_chat.as_ref().unwrap()["view"], "fresh");
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
