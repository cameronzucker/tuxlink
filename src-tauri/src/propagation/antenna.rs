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
//! ## Phase 1 model (TX): precomputed NEC Type-14 patterns
//!
//! The operator's own (TX) antenna resolves to a **precomputed NEC Type-14
//! pattern** via [`operator_voa_content`] → [`crate::propagation::patterns`].
//! Those patterns are generated offline by `tools/pattern-gen/` (real `nec2c`
//! geometry, never fabricated gain-vs-angle) and committed under
//! `propagation/patterns/`. The catalog is curated to eight defensible models
//! over a four-stop height grid; every preset, including `Unknown`, has an entry.
//!
//! ## RX / gateway model: stock VOACAP files
//!
//! The far/gateway (RX) end still maps to one of three *stock* VOACAP files via
//! [`gateway_voa_file`] / [`AntennaPreset::voa_file`] (no invented HFANT patterns
//! — antenna pattern numbers are exactly the amateur-radio specifics this project
//! treats as structurally unreliable from an AI, and require on-air operator
//! calibration):
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
    /// Resonant portable dipole (linked / fan / inverted-V). → isotrope.
    ResonantPortableDipole,
    /// Directional beam / Yagi / hex beam (base). → 17 dBi constant-gain.
    BeamYagi,
    /// Unknown / generic. → neutral flat pattern (NEVER a whip). Also the
    /// `#[serde(other)]` catch-all: any persisted value the build no longer
    /// recognizes (e.g. the retired `random-wire-unun` / `magnetic-loop`)
    /// deserializes to `Unknown` instead of failing the whole prefs load.
    #[serde(other)]
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
            | AntennaPreset::ResonantPortableDipole
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

/// Ground electrical properties for the VOACAP antenna model. The complex ground
/// reflection coefficient (a function of permittivity ε_r and conductivity σ)
/// shapes the elevation pattern — especially the low-angle lobes of horizontals
/// and the gain of verticals. Operator-selectable; `Average` is the safe default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GroundType {
    /// Average soil (ε_r 13, σ 0.005 S/m) — the EZNEC/VOACAP generic default.
    #[default]
    Average,
    /// Salt water (ε_r 80, σ 5.0) — best low-angle ground.
    SeaWater,
    /// Good/moist soil, marsh, fresh-water-rich (ε_r 40, σ 0.02).
    GoodSoil,
    /// Poor/rocky/sandy/desert soil (ε_r 3, σ 0.001).
    PoorSoil,
}

impl GroundType {
    /// `(dielectric ε_r, conductivity σ S/m)` — standard VOACAP/EZNEC values.
    fn constants(self) -> (f64, f64) {
        match self {
            GroundType::Average => (13.0, 0.005),
            GroundType::SeaWater => (80.0, 5.0),
            GroundType::GoodSoil => (40.0, 0.02),
            GroundType::PoorSoil => (3.0, 0.001),
        }
    }
}

/// The generated `.voa` filename written into the scratch `antennas/default/`
/// for the operator's selected preset. Every preset now resolves to a
/// precomputed Type-14 pattern (see [`crate::propagation::patterns`]), so this
/// file is always written.
pub const OPERATOR_VOA_FILENAME: &str = "txgen.voa";

/// Build the VOACAP pattern-file content for the operator's (TX) antenna.
///
/// Phase 1: returns the **precomputed NEC Type-14 pattern** for the selected
/// preset and (snapped) height, generated offline by `tools/pattern-gen/` and
/// committed under `propagation/patterns/`. Every preset has a library entry —
/// including `Unknown` (a neutral flat pattern) — so this always returns `Some`.
///
/// `_ground` is accepted for forward-compatibility (a future ground × pattern
/// matrix) but is **inert** in Phase 1: every pattern is modeled at poor/dry
/// desert ground (ε 3, σ 0.001). The operator-facing UI labels this limitation;
/// see the spec's "Single-ground limitation" section. The path's ground card,
/// where ground still matters, is emitted separately in `deck.rs`.
pub fn operator_voa_content(
    preset: AntennaPreset,
    height_m: f64,
    _ground: GroundType,
) -> Option<String> {
    Some(crate::propagation::patterns::pattern_voa(preset, height_m).to_string())
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
            AntennaPreset::ResonantPortableDipole,
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

    #[test]
    fn ground_type_defaults_to_average_and_serializes_kebab() {
        assert_eq!(GroundType::default(), GroundType::Average);
        assert_eq!(serde_json::to_string(&GroundType::SeaWater).unwrap(), "\"sea-water\"");
        let back: GroundType = serde_json::from_str("\"poor-soil\"").unwrap();
        assert_eq!(back, GroundType::PoorSoil);
    }

    #[test]
    fn removed_presets_migrate_to_unknown() {
        // A prefs file persisted before the catalog was curated still carries the
        // retired `random-wire-unun` value. `#[serde(other)]` maps it to `Unknown`
        // rather than failing the whole prefs load, and the OTHER fields survive.
        let json = r#"{"antenna_preset":"random-wire-unun","req_snr_db":42.0,
            "tx_power_w":50.0,"antenna_height_m":4.0,"ground_type":"poor-soil",
            "noise_environment":"rural"}"#;
        let p: crate::propagation::prefs::PropagationPrefs = serde_json::from_str(json).unwrap();
        assert_eq!(p.antenna_preset, AntennaPreset::Unknown);
        assert_eq!(p.req_snr_db, 42.0); // not nuked to default
        // `magnetic-loop` (the other retired value) migrates the same way.
        let back: AntennaPreset = serde_json::from_str("\"magnetic-loop\"").unwrap();
        assert_eq!(back, AntennaPreset::Unknown);
    }

    #[test]
    fn operator_voa_uses_precomputed_library() {
        // Phase 1: every preset returns the precomputed Type-14 pattern, and a
        // horizontal's pattern changes with height (the library is height-indexed).
        let low = operator_voa_content(AntennaPreset::NvisWireDipole, 2.5, GroundType::PoorSoil).unwrap();
        let high = operator_voa_content(AntennaPreset::NvisWireDipole, 9.0, GroundType::PoorSoil).unwrap();
        assert_ne!(low, high, "height must change the emitted pattern for a horizontal");
        // Unknown now has a (neutral) library entry, so it returns Some — not None.
        let unk = operator_voa_content(AntennaPreset::Unknown, 9.0, GroundType::PoorSoil);
        assert!(unk.is_some(), "Unknown resolves to the neutral library pattern");
    }

    #[test]
    fn ground_is_inert_for_the_precomputed_library() {
        // Phase 1 models poor-desert ground regardless of the selector; the ground
        // argument must not change the returned pattern. (The path ground card,
        // where ground still matters, is emitted separately in deck.rs.)
        let a = operator_voa_content(AntennaPreset::EfhwSloper, 9.0, GroundType::SeaWater).unwrap();
        let b = operator_voa_content(AntennaPreset::EfhwSloper, 9.0, GroundType::Average).unwrap();
        assert_eq!(a, b, "Phase 1 ground selector is inert under precomputed patterns");
    }

    #[test]
    fn vertical_pattern_is_height_independent() {
        // Ground-mounted verticals carry a single pattern; height does not apply.
        let v1 = operator_voa_content(AntennaPreset::BaseVerticalRadials, 2.0, GroundType::PoorSoil).unwrap();
        let v2 = operator_voa_content(AntennaPreset::BaseVerticalRadials, 30.0, GroundType::PoorSoil).unwrap();
        assert_eq!(v1, v2, "a vertical's pattern is fixed across heights");
    }
}
