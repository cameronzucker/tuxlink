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
    /// Active full-precision grid: GPS-fresh always wins. Falls back to
    /// `manual_grid` when no fresh fix exists.
    ///
    /// tuxlink-pjih (2026-06-01): the prior implementation gated this on
    /// `source` — `source == Manual` always returned `manual_grid`, even
    /// when a fresh GPS fix was available. Operator complaint: "Setting a
    /// manual grid now results in the GPS-derived grid no longer working,
    /// only displaying the manual grid." Resolution: `source` is no longer
    /// the display switch; it's a persisted preference, and `effective_source`
    /// (below) derives the live indicator from arbiter state. The displayed
    /// grid follows the available-data rule — GPS when fresh, manual otherwise.
    fn active_grid(&self) -> Option<String> {
        match &self.last_fix {
            Some(f) if f.is_fresh(FIX_STALENESS) => Some(f.grid.clone()),
            _ => self.manual_grid.clone(),
        }
    }

    /// The LIVE source used to render `active_grid` — `Gps` while a fresh
    /// fix exists, otherwise `Manual` (the fallback path). UI source chip
    /// reads this so it always matches what is/would be broadcast. Distinct
    /// from the stored `source` preference, which represents the operator's
    /// recorded intent but is no longer the display switch (tuxlink-pjih).
    fn effective_source(&self) -> PositionSource {
        match &self.last_fix {
            Some(f) if f.is_fresh(FIX_STALENESS) => PositionSource::Gps,
            _ => PositionSource::Manual,
        }
    }
}

impl PositionArbiter {
    pub fn new(source: PositionSource, manual_grid: Option<String>, precision: PositionPrecision) -> Self {
        Self { inner: Mutex::new(Inner { source, manual_grid, last_fix: None, precision }) }
    }
    /// The stored operator preference (`config.privacy.position_source`).
    /// NOT the display rule — see [`effective_source`] for the live source.
    pub fn source(&self) -> PositionSource { self.inner.lock().unwrap().source }

    /// The live source actually used to derive the active grid: `Gps` while
    /// a fresh GPS fix exists, otherwise `Manual` (the fallback path).
    /// Use this for any UI indicator that should reflect what is/would be
    /// broadcast — NOT [`source`], which is the persisted preference.
    pub fn effective_source(&self) -> PositionSource {
        self.inner.lock().unwrap().effective_source()
    }

    /// Hand-set the fallback grid (full precision). Caller validates first.
    ///
    /// tuxlink-pjih (2026-06-01): no longer pins `source = Manual`. The
    /// operator's complaint was that setting a manual grid made the
    /// GPS-derived grid disappear; the fix decouples grid-set from
    /// source-pin. Source is now operator preference only; `active_grid`
    /// follows GPS-fresh-else-manual.
    pub fn set_manual(&self, grid: &str) {
        self.inner.lock().unwrap().manual_grid = Some(grid.to_string());
    }

    /// Record the newest fix. Once stored, becomes the active position
    /// whenever it is fresh (tuxlink-pjih — no longer gated on `source`).
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

    /// The active grid at full precision. GPS-fresh always wins; falls back
    /// to manual_grid so the ribbon never blanks. tuxlink-pjih: no longer
    /// source-gated — see [`effective_source`] for the live source indicator.
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

    // tuxlink-pjih (2026-06-01): set_manual no longer pins source = Manual,
    // and active_grid is no longer source-gated. Operator complaint was that
    // setting a manual grid made the GPS-derived grid disappear; the fix
    // makes manual_grid the fallback used ONLY when GPS is unavailable.
    #[test]
    fn set_manual_updates_grid_without_changing_stored_source() {
        let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        a.set_manual("CN87ux");
        assert_eq!(a.source(), PositionSource::Gps); // stored preference unchanged
        assert_eq!(a.active_grid().as_deref(), Some("CN87ux")); // shown as fallback (no fresh fix)
    }

    #[test]
    fn fresh_gps_fix_wins_over_manual_grid_regardless_of_stored_source() {
        // Even when stored source is Manual (e.g. a legacy config from
        // before tuxlink-pjih), a fresh GPS fix takes the displayed grid.
        let a = PositionArbiter::new(PositionSource::Manual, None, PositionPrecision::FourCharGrid);
        a.set_manual("CN87ux");
        a.apply_gps_fix(Fix::test("DM33ab"));
        assert_eq!(a.active_grid().as_deref(), Some("DM33ab"));
        assert_eq!(a.effective_source(), PositionSource::Gps);
    }

    #[test]
    fn manual_grid_used_when_gps_fix_is_stale_or_absent() {
        let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        a.set_manual("CN87ux");
        assert_eq!(a.active_grid().as_deref(), Some("CN87ux"));
        assert_eq!(a.effective_source(), PositionSource::Manual);
    }

    #[test]
    fn gps_fix_updates_active_regardless_of_stored_source() {
        let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        a.apply_gps_fix(Fix::test("DM33ab"));
        assert_eq!(a.active_grid().as_deref(), Some("DM33ab"));
        assert_eq!(a.effective_source(), PositionSource::Gps);
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
