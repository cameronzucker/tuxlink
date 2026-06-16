//! Precomputed NEC Type-14 antenna-pattern library.
//!
//! Generated offline by `tools/pattern-gen/` (real `nec2c` geometry over the
//! 8-antenna catalog × the height grid × 30 HF frequencies), the `.voa` files
//! are committed under `patterns/` and `include_str!`'d here so the library
//! ships in the binary with no runtime `nec2c` dependency. See the Phase 1
//! picker spec/plan: `docs/design/2026-06-15-find-a-station-antenna-phase1-picker*`.
//!
//! Two pattern classes:
//! - **Height-variable** (horizontal wires + the Yagi): one pattern per grid
//!   stop in [`HEIGHT_GRID_M`]; the requested mast height snaps to the nearest stop.
//! - **Fixed** (ground-mounted verticals + the neutral `unknown`): a single
//!   pattern, height-independent.
//!
//! All patterns are modeled at poor/dry-desert ground (ε 3, σ 0.001) — the
//! Phase 1 single-ground limitation. Ground selection is inert here (see
//! [`super::antenna::operator_voa_content`]).

use super::antenna::AntennaPreset;

/// Apex-height grid (metres) for the height-variable antennas. The operator's
/// requested height snaps to the nearest stop via [`snap_height`].
pub const HEIGHT_GRID_M: [f64; 4] = [2.5, 4.0, 6.0, 9.0];

/// Whether this antenna's elevation pattern varies with mast height (horizontal
/// wires + the Yagi) or is fixed (ground-mounted verticals + the neutral model).
pub fn is_height_variable(preset: AntennaPreset) -> bool {
    matches!(
        preset,
        AntennaPreset::EfhwSloper
            | AntennaPreset::NvisWireDipole
            | AntennaPreset::ResonantPortableDipole
            | AntennaPreset::BeamYagi
    )
}

/// Snap a requested height (metres) to the nearest [`HEIGHT_GRID_M`] stop.
pub fn snap_height(height_m: f64) -> f64 {
    HEIGHT_GRID_M
        .iter()
        .copied()
        .min_by(|a, b| {
            (a - height_m)
                .abs()
                .partial_cmp(&(b - height_m).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(6.0)
}

/// `include_str!` a committed pattern file by its base name (no extension).
macro_rules! voa {
    ($f:literal) => {
        include_str!(concat!("patterns/", $f, ".voa"))
    };
}

/// The committed Type-14 `.voa` text for the selected antenna at the (snapped)
/// height. Height-variable presets resolve to the nearest grid stop; fixed
/// presets ignore height. Always returns a valid pattern (`unknown` is the
/// fallback for any preset without an explicit entry).
pub fn pattern_voa(preset: AntennaPreset, height_m: f64) -> &'static str {
    use AntennaPreset::*;
    if is_height_variable(preset) {
        // Grid stops as tenths-of-a-metre keys: 025 / 040 / 060 / 090.
        let stop = (snap_height(height_m) * 10.0).round() as u32;
        return match (preset, stop) {
            (EfhwSloper, 25) => voa!("efhw-sloper__025"),
            (EfhwSloper, 40) => voa!("efhw-sloper__040"),
            (EfhwSloper, 60) => voa!("efhw-sloper__060"),
            (EfhwSloper, _) => voa!("efhw-sloper__090"),
            (NvisWireDipole, 25) => voa!("nvis-wire-dipole__025"),
            (NvisWireDipole, 40) => voa!("nvis-wire-dipole__040"),
            (NvisWireDipole, 60) => voa!("nvis-wire-dipole__060"),
            (NvisWireDipole, _) => voa!("nvis-wire-dipole__090"),
            (ResonantPortableDipole, 25) => voa!("resonant-portable-dipole__025"),
            (ResonantPortableDipole, 40) => voa!("resonant-portable-dipole__040"),
            (ResonantPortableDipole, 60) => voa!("resonant-portable-dipole__060"),
            (ResonantPortableDipole, _) => voa!("resonant-portable-dipole__090"),
            (BeamYagi, 25) => voa!("beam-yagi__025"),
            (BeamYagi, 40) => voa!("beam-yagi__040"),
            (BeamYagi, 60) => voa!("beam-yagi__060"),
            (BeamYagi, _) => voa!("beam-yagi__090"),
            _ => voa!("unknown"),
        };
    }
    match preset {
        PortableVerticalWhip => voa!("portable-vertical-whip"),
        BaseVerticalRadials => voa!("base-vertical-radials"),
        MobileHfWhip => voa!("mobile-hf-whip"),
        _ => voa!("unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_height_picks_nearest_grid_stop() {
        assert_eq!(snap_height(2.5), 2.5);
        assert_eq!(snap_height(5.2), 6.0); // nearer 6 than 4
        assert_eq!(snap_height(0.0), 2.5); // below the grid → lowest stop
        assert_eq!(snap_height(100.0), 9.0); // above the grid → highest stop
    }

    #[test]
    fn height_variable_classification() {
        assert!(is_height_variable(AntennaPreset::EfhwSloper));
        assert!(is_height_variable(AntennaPreset::BeamYagi));
        assert!(!is_height_variable(AntennaPreset::BaseVerticalRadials));
        assert!(!is_height_variable(AntennaPreset::MobileHfWhip));
        assert!(!is_height_variable(AntennaPreset::Unknown));
    }

    #[test]
    fn lookup_snaps_height_for_horizontals() {
        // 5.2 m snaps to the 6.0 m stop; 5.8 m also snaps to 6.0 m → same file.
        assert_eq!(
            pattern_voa(AntennaPreset::EfhwSloper, 5.2),
            pattern_voa(AntennaPreset::EfhwSloper, 5.8)
        );
        // A different stop yields a different pattern.
        assert_ne!(
            pattern_voa(AntennaPreset::EfhwSloper, 2.5),
            pattern_voa(AntennaPreset::EfhwSloper, 9.0)
        );
    }

    #[test]
    fn lookup_is_height_independent_for_verticals() {
        let v1 = pattern_voa(AntennaPreset::BaseVerticalRadials, 2.0);
        let v2 = pattern_voa(AntennaPreset::BaseVerticalRadials, 30.0);
        assert_eq!(v1, v2);
    }

    #[test]
    fn every_pattern_is_a_nonempty_type14_body() {
        for p in [
            AntennaPreset::EfhwSloper,
            AntennaPreset::NvisWireDipole,
            AntennaPreset::ResonantPortableDipole,
            AntennaPreset::BeamYagi,
            AntennaPreset::PortableVerticalWhip,
            AntennaPreset::BaseVerticalRadials,
            AntennaPreset::MobileHfWhip,
            AntennaPreset::Unknown,
        ] {
            let voa = pattern_voa(p, 6.0);
            assert!(!voa.is_empty(), "{p:?} pattern is empty");
            // Round-trips through the Type-14 reader at an arbitrary block.
            assert!(
                super::super::type14::read_block_gains(voa, 14).is_ok(),
                "{p:?} pattern did not parse as Type-14"
            );
        }
    }
}
