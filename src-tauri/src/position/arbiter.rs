use std::sync::Mutex;
use crate::config::{broadcast_grid, PositionPrecision, PositionSource};

/// A GPS fix reduced to a grid + freshness, as handed in by the gpsd client.
#[derive(Debug, Clone)]
pub struct Fix {
    pub grid: String,
    pub received: std::time::Instant,
}
impl Fix {
    #[cfg(test)]
    pub fn test(grid: &str) -> Self { Self { grid: grid.into(), received: std::time::Instant::now() } }
    fn is_fresh(&self, window: std::time::Duration) -> bool { self.received.elapsed() < window }
}

const FIX_STALENESS: std::time::Duration = std::time::Duration::from_secs(30);

/// The single source of truth for position. Interior-mutable (one `Mutex`) so it
/// can live in Tauri managed state and be read by commands + the gpsd task.
pub struct PositionArbiter {
    inner: Mutex<Inner>,
}

/// Per spec §3.3 + R3 F1 + F7 (T6): callers needing a transactional critical
/// section (read config → write config → mutate arbiter) hold this struct via
/// [`PositionArbiter::with_inner`]. Fields are `pub(crate)` so the closure
/// passed to `with_inner` can read/write directly while the mutex is held.
pub(crate) struct Inner {
    pub(crate) source: PositionSource,
    pub(crate) manual_grid: Option<String>,  // last hand-set grid (full precision)
    pub(crate) last_fix: Option<Fix>,        // newest GPS fix, regardless of source
    pub(crate) precision: PositionPrecision,
}

impl Inner {
    /// Active full-precision grid: source-gated.
    /// `source == Manual` → `manual_grid`; `source == Gps` → fresh fix when
    /// available, else `manual_grid` fallback. The 2026-05-22 source contract
    /// makes `source` the display switch — the operator's chip selection is
    /// authoritative for which grid the UI shows and what is broadcast.
    fn active_grid(&self) -> Option<String> {
        match self.source {
            PositionSource::Manual => self.manual_grid.clone(),
            PositionSource::Gps => match &self.last_fix {
                Some(f) if f.is_fresh(FIX_STALENESS) => Some(f.grid.clone()),
                _ => self.manual_grid.clone(),
            },
        }
    }
}

impl PositionArbiter {
    pub fn new(source: PositionSource, manual_grid: Option<String>, precision: PositionPrecision) -> Self {
        Self { inner: Mutex::new(Inner { source, manual_grid, last_fix: None, precision }) }
    }
    /// The active position source — the chip selection that gates `active_grid`
    /// and `broadcast_grid`. Matches `config.privacy.position_source`; updated
    /// by `set_manual` (pins Manual) and `use_gps` (pins Gps, infallible per
    /// spec §1.1 relaxation — falls back to manual_grid if no fresh fix).
    pub fn source(&self) -> PositionSource { self.inner.lock().unwrap().source }

    /// Hand-set the manual grid (full precision) and pin `source = Manual`.
    /// Caller validates the grid first. Sticky against subsequent GPS fixes:
    /// `apply_gps_fix` records the fix in `last_fix` but does not flip
    /// `source` back to Gps — the operator must select the GPS chip via
    /// `use_gps` to switch.
    pub fn set_manual(&self, grid: &str) {
        let mut i = self.inner.lock().unwrap();
        i.manual_grid = Some(grid.to_string());
        i.source = PositionSource::Manual;
    }

    /// Record the newest fix in `last_fix`. Becomes the active position only
    /// while `source == Gps`; while `source == Manual`, the fix is recorded
    /// (so `has_fresh_fix` and chip-affordance checks see it) but
    /// `active_grid` continues to return `manual_grid`.
    pub fn apply_gps_fix(&self, fix: Fix) {
        self.inner.lock().unwrap().last_fix = Some(fix);
    }

    /// Update the broadcast precision (operator changed it in Settings, tuxlink-39b).
    /// Keeps the arbiter's GPS-broadcast path in sync with `config.privacy.position_precision`.
    pub fn set_precision(&self, precision: PositionPrecision) {
        self.inner.lock().unwrap().precision = precision;
    }

    /// Switch to GPS (now infallible per spec §1.1 the relaxation).
    /// Always sets source = Gps. If no fresh fix exists, display falls back
    /// to manual_grid per State 4 / State 5 (spec row 3).
    pub fn use_gps(&self) {
        let mut i = self.inner.lock().unwrap();
        i.source = PositionSource::Gps;
    }

    /// The active grid at full precision, gated on `source`. Manual →
    /// `manual_grid`; Gps → fresh fix, else `manual_grid` fallback so the
    /// ribbon never blanks while a manual grid is set. Reads under one lock
    /// alongside `broadcast_grid` to close the TOCTOU window on the privacy
    /// boundary.
    pub fn active_grid(&self) -> Option<String> {
        self.inner.lock().unwrap().active_grid()
    }

    /// The active grid reduced to broadcast precision — the ONLY value that goes on air.
    /// Reads both the active grid and the precision under a single lock to close the
    /// TOCTOU window on the privacy boundary.
    pub fn broadcast_grid(&self) -> Option<String> {
        let i = self.inner.lock().unwrap();
        let precision = i.precision;
        i.active_grid().map(|g| broadcast_grid(&g, precision))
    }

    pub fn has_fresh_fix(&self) -> bool {
        self.inner.lock().unwrap().last_fix.as_ref().is_some_and(|f| f.is_fresh(FIX_STALENESS))
    }

    /// Hold the arbiter mutex for a full transactional critical section. Used
    /// by commands that need to read config → write config → mutate arbiter
    /// atomically (spec §3.3, R3 F1 + F7 from the 2026-06-01 position-subsystem
    /// restoration adrev).
    ///
    /// Without this wrapper, `config_set_grid` and `position_set_source` had a
    /// TOCTOU window: one task could persist `position_source = X` to disk
    /// while another task's later `arbiter.set_manual` / `use_gps` overwrote
    /// the arbiter source with `Y`, leaving disk and arbiter disagreeing on
    /// the final source.
    pub(crate) fn with_inner<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        let mut i = self.inner.lock().unwrap();
        f(&mut i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PositionPrecision;

    // R4 P0 #1: temporal sticky sequence.
    // set_manual → apply_gps_fix → still Manual && active_grid == manual_grid && last_fix recorded.
    #[test]
    fn set_manual_pins_source_and_is_sticky_against_gps() {
        let arbiter = PositionArbiter::new(
            crate::config::PositionSource::Gps,
            None,
            crate::config::PositionPrecision::FourCharGrid,
        );
        arbiter.set_manual("EM75");
        assert_eq!(arbiter.source(), crate::config::PositionSource::Manual,
            "set_manual must pin source = Manual");
        assert_eq!(arbiter.active_grid().as_deref(), Some("EM75"),
            "active_grid must follow manual_grid immediately after set_manual");

        // GPS fix arrives WHILE source = Manual; arbiter must record last_fix
        // but active_grid must stay manual_grid (sticky).
        arbiter.apply_gps_fix(Fix::test("DM33ab"));
        assert_eq!(arbiter.source(), crate::config::PositionSource::Manual,
            "GPS fix must not flip source = Manual");
        assert_eq!(arbiter.active_grid().as_deref(), Some("EM75"),
            "active_grid must stay manual_grid while source = Manual");
        assert!(arbiter.has_fresh_fix(),
            "apply_gps_fix must record last_fix even while source = Manual");
    }

    #[test]
    fn broadcast_grid_reduces_to_precision() {
        let a = PositionArbiter::new(PositionSource::Manual, Some("CN87ux".into()), PositionPrecision::FourCharGrid);
        assert_eq!(a.broadcast_grid().as_deref(), Some("CN87"));
    }

    // R4 P0 #2 + Codex P0 #1: use_gps is infallible; falls back to manual_grid
    // when source flips to Gps without a fresh fix.
    #[test]
    fn use_gps_succeeds_without_fresh_fix_and_yields_manual_fallback() {
        let arbiter = PositionArbiter::new(
            crate::config::PositionSource::Manual,
            Some("EM75".to_string()),
            crate::config::PositionPrecision::FourCharGrid,
        );
        assert_eq!(arbiter.source(), crate::config::PositionSource::Manual);
        assert!(!arbiter.has_fresh_fix(), "fixture has no fix");

        // use_gps() is now infallible — no Result, no panic, no error.
        arbiter.use_gps();
        assert_eq!(arbiter.source(), crate::config::PositionSource::Gps,
            "use_gps must flip source = Gps regardless of fresh fix");
        // active_grid falls back to manual_grid per spec State 4.
        assert_eq!(arbiter.active_grid().as_deref(), Some("EM75"),
            "active_grid must fall back to manual_grid in State 4");
    }

    // tuxlink-39b: changing precision at runtime (settings UI) must change the
    // broadcast reduction. The arbiter holds its own `precision` (used on the
    // GPS-broadcast path), so config_set_privacy must keep it in sync.
    #[test]
    fn set_precision_changes_broadcast_reduction() {
        let a = PositionArbiter::new(PositionSource::Manual, Some("CN87ux".into()), PositionPrecision::FourCharGrid);
        assert_eq!(a.broadcast_grid().as_deref(), Some("CN87")); // 4-char default
        a.set_precision(PositionPrecision::SixCharGrid);
        assert_eq!(a.broadcast_grid().as_deref(), Some("CN87ux")); // full 6-char after change
    }

    // R3 F4 (tuxlink-c79g T7): proptest over the State 1-5 matrix from spec §3.4.
    // The five `active_grid` invariants I1-I5 cover all reachable cells of
    // (source × manual_grid × apply_fix). I6 (synchronization between
    // config.privacy.position_source and arbiter.source after config_set_grid)
    // is covered by config_set_grid_pins_manual_source_in_config_and_arbiter
    // from Task 4 — not re-encoded here.
    use proptest::prelude::*;

    fn arb_source() -> impl Strategy<Value = PositionSource> {
        prop_oneof![Just(PositionSource::Manual), Just(PositionSource::Gps)]
    }

    fn arb_manual_grid() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),
            Just(Some("EM75".to_string())),
            Just(Some("CN87xx".to_string())),
        ]
    }

    proptest! {
        // I1: source = Manual && manual_grid = None → active_grid = None.
        // I2: source = Manual && manual_grid set → active_grid = manual_grid.
        // I3: source = Gps && fresh fix → active_grid = fix.grid.
        // I4: source = Gps && no fix && manual_grid set → active_grid = manual_grid.
        // I5: source = Gps && no fix && manual_grid = None → active_grid = None.
        #[test]
        fn state_space_active_grid_matches_i1_through_i5(
            source in arb_source(),
            manual_grid in arb_manual_grid(),
            apply_fix in proptest::bool::ANY,
        ) {
            let arbiter = PositionArbiter::new(
                source,
                manual_grid.clone(),
                PositionPrecision::FourCharGrid,
            );
            if apply_fix {
                arbiter.apply_gps_fix(Fix::test("DM33ab"));
            }
            let active = arbiter.active_grid();
            match (source, apply_fix, manual_grid.as_deref()) {
                // I1
                (PositionSource::Manual, _, None) => prop_assert_eq!(active, None),
                // I2
                (PositionSource::Manual, _, Some(g)) => prop_assert_eq!(active.as_deref(), Some(g)),
                // I3
                (PositionSource::Gps, true, _) => prop_assert_eq!(active.as_deref(), Some("DM33ab")),
                // I4
                (PositionSource::Gps, false, Some(g)) => prop_assert_eq!(active.as_deref(), Some(g)),
                // I5
                (PositionSource::Gps, false, None) => prop_assert_eq!(active, None),
            }
        }
    }
}
