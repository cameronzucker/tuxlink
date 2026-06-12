//! Operator antenna presets + gateway-antenna → VOACAP pattern-file mapping.
//!
//! The VOACAP deck names an antenna pattern file on each ANTENNA card, e.g.
//! `[default/swwhip.voa]`. The pattern file determines gain-vs-elevation, which
//! is what makes a path predict reachable or not at a given takeoff angle.
//!
//! ## The bug this fixes
//!
//! The prior deck hardwired the far/gateway (RX) antenna to `swwhip.voa` — a
//! short-wave whip, a *vertical with a zenith null* (measured −21.9 dBi at 90°).
//! Near-vertical-incidence (NVIS / short regional) paths arrive near-vertical,
//! land in that null, and predict ~0% reliability regardless of conditions. That
//! is correct physics for a whip but wrong for the dipoles / end-fed wires most
//! Winlink gateways and operators actually run.
//!
//! ## v1 model
//!
//! Each preset maps to one of three *stock* VOACAP files (no invented HFANT
//! patterns — antenna pattern numbers are exactly the amateur-radio specifics
//! this project treats as structurally unreliable from an AI, and require
//! on-air operator calibration):
//!
//! - `ccir.000` — the isotrope (0 dBi at every angle). The honest neutral model:
//!   **no zenith null**, so short/NVIS paths predict reachable. Used for
//!   horizontals, end-fed wires, loops, and anything unknown.
//! - `swwhip.voa` — the short-wave whip. Used ONLY for genuinely vertical
//!   antennas, where the zenith null is physically correct.
//! - `const17.voa` — a 17 dBi constant-gain stand-in for a directional beam.
//!
//! Higher-fidelity per-archetype HFANT patterns (e.g. a low horizontal dipole at
//! ~0.18 λ that *boosts* the high-angle lobe rather than merely not-nulling it)
//! are a documented follow-up; they need pattern-file parameters the licensed
//! operator validates on the air. Archetype evidence + the proposed HFANT
//! parameters live in `dev/scratch/winlink-antenna-archetypes.md`.

use serde::{Deserialize, Serialize};

use crate::catalog::stations::GatewayAntenna;

/// Operator-selectable antenna preset for the OWN (TX) station. Names reflect
/// real-world Winlink / HF-digital antennas (grounded in the Hamexandria corpus);
/// each maps to a stock VOACAP pattern file via [`AntennaPreset::voa_file`].
///
/// `EfhwSloper` is the default: the end-fed half-wave (strung horizontal or as a
/// sloper) is the most-recurring antenna across independent Winlink operators,
/// spans field and base use, and its isotrope model carries no zenith null.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AntennaPreset {
    /// End-fed half-wave, horizontal or sloper. Default. → isotrope (no null).
    #[default]
    EfhwSloper,
    /// Portable vertical whip (Chameleon MPAS / Wolf River / MP1 class). → whip.
    PortableVerticalWhip,
    /// Low NVIS wire dipole / OCFD, regional. → isotrope (no null).
    NvisWireDipole,
    /// Base vertical with radials (¼λ or compromise vertical). → whip.
    BaseVerticalRadials,
    /// Mobile HF whip (screwdriver / hamstick). → whip.
    MobileHfWhip,
    /// Random wire + 9:1 unun. → isotrope.
    RandomWireUnun,
    /// Resonant portable dipole (linked / fan / inverted-V). → isotrope.
    ResonantPortableDipole,
    /// Magnetic loop. → isotrope.
    MagneticLoop,
    /// Directional beam / Yagi / hex beam (base). → 17 dBi constant-gain.
    BeamYagi,
    /// Unknown / generic. → isotrope (NEVER a whip).
    Unknown,
}

impl AntennaPreset {
    /// The stock VOACAP pattern file (under `itshfbc/antennas/default/`) this
    /// preset is modeled with. See the module docs for why only these three.
    pub fn voa_file(self) -> &'static str {
        match self {
            // Horizontals / wires / loops / unknown: isotrope — no zenith null,
            // so NVIS and short regional paths are not artificially killed.
            AntennaPreset::EfhwSloper
            | AntennaPreset::NvisWireDipole
            | AntennaPreset::RandomWireUnun
            | AntennaPreset::ResonantPortableDipole
            | AntennaPreset::MagneticLoop
            | AntennaPreset::Unknown => "ccir.000",
            // Genuine verticals: the whip's zenith null is the right physics.
            AntennaPreset::PortableVerticalWhip
            | AntennaPreset::BaseVerticalRadials
            | AntennaPreset::MobileHfWhip => "swwhip.voa",
            // Directional beam: constant-gain stand-in (stock dir. pattern TBD).
            AntennaPreset::BeamYagi => "const17.voa",
        }
    }
}

/// Map a gateway's self-reported antenna code (the `B`/`D`/`V` "Antenna being
/// used" letter parsed from the listing) to a VOACAP pattern file for the RX end.
///
/// `None` (the gateway listed no code) falls back to the **isotrope**, never a
/// whip — assuming a whip for every unknown gateway is exactly the bug that made
/// NVIS paths read 0%.
pub fn gateway_voa_file(antenna: Option<GatewayAntenna>) -> &'static str {
    match antenna {
        Some(GatewayAntenna::Vertical) => "swwhip.voa",
        Some(GatewayAntenna::Beam) => "const17.voa",
        // Dipole → isotrope (a horizontal dipole has no zenith null); unknown → isotrope.
        Some(GatewayAntenna::Dipole) | None => "ccir.000",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_is_efhw_sloper() {
        assert_eq!(AntennaPreset::default(), AntennaPreset::EfhwSloper);
    }

    #[test]
    fn horizontals_and_unknown_model_as_isotrope_no_null() {
        for p in [
            AntennaPreset::EfhwSloper,
            AntennaPreset::NvisWireDipole,
            AntennaPreset::RandomWireUnun,
            AntennaPreset::ResonantPortableDipole,
            AntennaPreset::MagneticLoop,
            AntennaPreset::Unknown,
        ] {
            assert_eq!(p.voa_file(), "ccir.000", "{p:?} must model as isotrope");
        }
    }

    #[test]
    fn verticals_model_as_whip() {
        for p in [
            AntennaPreset::PortableVerticalWhip,
            AntennaPreset::BaseVerticalRadials,
            AntennaPreset::MobileHfWhip,
        ] {
            assert_eq!(p.voa_file(), "swwhip.voa", "{p:?} must model as whip");
        }
    }

    #[test]
    fn beam_models_as_constant_gain() {
        assert_eq!(AntennaPreset::BeamYagi.voa_file(), "const17.voa");
    }

    #[test]
    fn gateway_unknown_falls_back_to_isotrope_never_whip() {
        assert_eq!(gateway_voa_file(None), "ccir.000");
        assert_ne!(gateway_voa_file(None), "swwhip.voa");
    }

    #[test]
    fn gateway_codes_map_to_expected_files() {
        assert_eq!(gateway_voa_file(Some(GatewayAntenna::Vertical)), "swwhip.voa");
        assert_eq!(gateway_voa_file(Some(GatewayAntenna::Beam)), "const17.voa");
        assert_eq!(gateway_voa_file(Some(GatewayAntenna::Dipole)), "ccir.000");
    }

    #[test]
    fn preset_round_trips_through_serde_kebab_case() {
        let json = serde_json::to_string(&AntennaPreset::EfhwSloper).unwrap();
        assert_eq!(json, "\"efhw-sloper\"");
        let back: AntennaPreset = serde_json::from_str("\"portable-vertical-whip\"").unwrap();
        assert_eq!(back, AntennaPreset::PortableVerticalWhip);
    }
}
