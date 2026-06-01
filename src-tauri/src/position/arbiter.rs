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
struct Inner {
    source: PositionSource,
    manual_grid: Option<String>,  // last hand-set grid (full precision)
    last_fix: Option<Fix>,        // newest GPS fix, regardless of source
    precision: PositionPrecision,
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
    /// by `set_manual` (pins Manual) and `use_gps` (pins Gps when a fresh fix
    /// exists).
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

    /// Switch to GPS — only if a fresh fix exists. Err with a reason otherwise.
    pub fn use_gps(&self) -> Result<(), &'static str> {
        let mut i = self.inner.lock().unwrap();
        match &i.last_fix {
            Some(f) if f.is_fresh(FIX_STALENESS) => { i.source = PositionSource::Gps; Ok(()) }
            _ => Err("no usable GPS fix"),
        }
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
        assert_eq!(arbiter.active_grid().as_deref(), Some("EM75"));

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

    #[test]
    fn use_gps_requires_a_usable_fix() {
        let a = PositionArbiter::new(PositionSource::Manual, Some("CN87".into()), PositionPrecision::FourCharGrid);
        assert!(a.use_gps().is_err());            // no fix yet
        a.apply_gps_fix(Fix::test("DM33ab"));     // stored as last_fix even while Manual
        assert!(a.use_gps().is_ok());
        assert_eq!(a.source(), PositionSource::Gps);
        assert_eq!(a.active_grid().as_deref(), Some("DM33ab"));
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
}
