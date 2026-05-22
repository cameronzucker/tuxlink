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

impl PositionArbiter {
    pub fn new(source: PositionSource, manual_grid: Option<String>, precision: PositionPrecision) -> Self {
        Self { inner: Mutex::new(Inner { source, manual_grid, last_fix: None, precision }) }
    }
    pub fn source(&self) -> PositionSource { self.inner.lock().unwrap().source }

    /// Hand-set grid: store full precision, pin Manual (sticky). Caller validates first.
    pub fn set_manual(&self, grid: &str) {
        let mut i = self.inner.lock().unwrap();
        i.manual_grid = Some(grid.to_string());
        i.source = PositionSource::Manual;
    }

    /// Record the newest fix. Becomes the active position only while source == Gps.
    pub fn apply_gps_fix(&self, fix: Fix) {
        self.inner.lock().unwrap().last_fix = Some(fix);
    }

    /// Switch to GPS — only if a fresh fix exists. Err with a reason otherwise.
    pub fn use_gps(&self) -> Result<(), &'static str> {
        let mut i = self.inner.lock().unwrap();
        match &i.last_fix {
            Some(f) if f.is_fresh(FIX_STALENESS) => { i.source = PositionSource::Gps; Ok(()) }
            _ => Err("no usable GPS fix"),
        }
    }

    /// The active grid at full precision (Manual -> manual_grid; Gps -> fresh fix, else
    /// fall back to manual_grid so the ribbon never goes blank).
    pub fn active_grid(&self) -> Option<String> {
        let i = self.inner.lock().unwrap();
        match i.source {
            PositionSource::Manual => i.manual_grid.clone(),
            PositionSource::Gps => match &i.last_fix {
                Some(f) if f.is_fresh(FIX_STALENESS) => Some(f.grid.clone()),
                _ => i.manual_grid.clone(),
            },
        }
    }

    /// The active grid reduced to broadcast precision — the ONLY value that goes on air.
    pub fn broadcast_grid(&self) -> Option<String> {
        let precision = self.inner.lock().unwrap().precision;
        self.active_grid().map(|g| broadcast_grid(&g, precision))
    }

    pub fn has_fresh_fix(&self) -> bool {
        self.inner.lock().unwrap().last_fix.as_ref().is_some_and(|f| f.is_fresh(FIX_STALENESS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PositionPrecision;

    #[test]
    fn set_manual_pins_source_and_is_sticky_against_gps() {
        let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        a.set_manual("CN87ux");
        assert_eq!(a.source(), PositionSource::Manual);
        a.apply_gps_fix(Fix::test("DM33ab")); // GPS arrives
        assert_eq!(a.active_grid().as_deref(), Some("CN87ux")); // unchanged
        assert_eq!(a.source(), PositionSource::Manual);
    }

    #[test]
    fn gps_fix_updates_active_only_when_source_is_gps() {
        let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        a.apply_gps_fix(Fix::test("DM33ab"));
        assert_eq!(a.active_grid().as_deref(), Some("DM33ab"));
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
}
