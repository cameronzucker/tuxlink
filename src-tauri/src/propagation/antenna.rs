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

/// The IONCAP parametric antenna a preset maps to. voacapl computes the
/// height/ground/frequency-dependent elevation pattern internally from these —
/// we supply *physical geometry*, never invented gain-vs-angle numbers (the
/// pattern math is the validated Fortran's job, sidestepping the AI-unreliable
/// pattern-number concern in the module docs).
struct IoncapAntenna {
    /// VOACAP antenna-type code: 22 vertical monopole, 23 horizontal dipole,
    /// 24 horizontal Yagi (verified against the voacapl source + shipped samples).
    type_code: u8,
    /// Element length/height in wavelengths (negative = wavelengths per the
    /// IONCAP sign convention). For type 22 this is the monopole element; for
    /// 23/24 it is the dipole/boom length.
    length_wl: f64,
    /// Forward gain over a dipole (dB) for directional types; 0 for plain wires.
    gain_over_dipole_db: f64,
}

impl AntennaPreset {
    /// The IONCAP parametric antenna for this preset, or `None` to keep the stock
    /// isotrope model (no height dependence) — used for `Unknown`.
    fn ioncap(self) -> Option<IoncapAntenna> {
        use AntennaPreset::*;
        match self {
            // Horizontal wires / loops → resonant half-wave horizontal dipole (23).
            // Mag loop / random wire have no native VOACAP type; the low horizontal
            // dipole is the documented proxy (see the recalibration design note).
            EfhwSloper | NvisWireDipole | RandomWireUnun | ResonantPortableDipole
            | MagneticLoop => Some(IoncapAntenna {
                type_code: 23,
                length_wl: -0.50,
                gain_over_dipole_db: 0.0,
            }),
            // Verticals → quarter-wave vertical monopole (22), ground-mounted.
            PortableVerticalWhip | BaseVerticalRadials | MobileHfWhip => Some(IoncapAntenna {
                type_code: 22,
                length_wl: -0.25,
                gain_over_dipole_db: 0.0,
            }),
            // Beam → horizontal Yagi (24) with forward gain over a dipole.
            BeamYagi => Some(IoncapAntenna {
                type_code: 24,
                length_wl: -0.50,
                gain_over_dipole_db: 6.0,
            }),
            // Unknown → keep stock isotrope (height does not apply).
            Unknown => None,
        }
    }

    /// A short title line for the generated `.voa` (≤70 chars; informational).
    fn voa_title(self) -> &'static str {
        use AntennaPreset::*;
        match self {
            EfhwSloper => "tuxlink EFHW / sloper (horizontal dipole model)",
            NvisWireDipole => "tuxlink low NVIS wire dipole",
            RandomWireUnun => "tuxlink random wire (horizontal dipole model)",
            ResonantPortableDipole => "tuxlink resonant portable dipole",
            MagneticLoop => "tuxlink magnetic loop (horizontal dipole model)",
            PortableVerticalWhip => "tuxlink portable vertical whip (monopole)",
            BaseVerticalRadials => "tuxlink base vertical + radials (monopole)",
            MobileHfWhip => "tuxlink mobile HF whip (monopole)",
            BeamYagi => "tuxlink beam / Yagi (horizontal yagi)",
            Unknown => "tuxlink generic",
        }
    }
}

/// The generated `.voa` filename written into the scratch `antennas/default/`
/// for an operator preset that maps to a parametric IONCAP antenna.
pub const OPERATOR_VOA_FILENAME: &str = "txgen.voa";

/// Build the VOACAP pattern-file content for the operator's (TX) antenna, with
/// the operator's `height_m` (metres, above ground) and `ground` plugged in.
///
/// Returns `None` when the preset has no parametric model (`Unknown`) — the
/// caller then falls back to the stock isotrope file from [`AntennaPreset::voa_file`].
///
/// Height enters as a **positive metres** value (param [7] for the horizontal
/// types), so voacapl recomputes the height-in-wavelengths pattern per band
/// within a run — a 9 m dipole is ~0.1 λ on 80 m but ~0.4 λ on 20 m, with the
/// correct per-band high-angle (NVIS) behaviour. Length stays −0.50 λ (resonant
/// half-wave, assuming an ATU-matched amateur wire on each band). For verticals
/// (type 22, ground-mounted) the height field does not apply; the −0.25 λ element
/// drives the pattern. voacapl reads each line's leading number (list-directed),
/// so exact column alignment is not required.
pub fn operator_voa_content(
    preset: AntennaPreset,
    height_m: f64,
    ground: GroundType,
) -> Option<String> {
    let ant = preset.ioncap()?;
    let (eps, sig) = ground.constants();
    // Clamp to a physically sane mast range so a junk height can't produce a
    // pathological deck (voacapl would otherwise accept absurd values).
    let h = if height_m.is_finite() {
        height_m.clamp(0.5, 100.0)
    } else {
        9.0
    };
    let title = preset.voa_title();
    let content = if ant.type_code == 22 {
        // Vertical monopole: 7 params, element height/length at [6].
        format!(
            "{title}\n 7     7 parameters\n  0.00  [ 1] Max Gain dBi..:\n  22    [ 2] Antenna Type..:\n  {eps:.0}    [ 3] Dielectric....:\n {sig:.5} [ 4] Conductivity..:\n 14.000  [ 5] Operating Freq:\n {len:.2}  [ 6] Antenna Height:\n  0.0   [ 7] Gain ab dipole:\n",
            len = ant.length_wl,
        )
    } else {
        // Horizontal dipole (23) / Yagi (24): 8 params, height (metres) at [7].
        format!(
            "{title}\n 8     8 parameters\n  0.00  [ 1] Max Gain dBi..:\n  {tc}    [ 2] Antenna Type..:\n  {eps:.0}    [ 3] Dielectric....:\n {sig:.5} [ 4] Conductivity..:\n 14.000  [ 5] Operating Freq:\n {len:.2}  [ 6] Antenna Length:\n {h:.2}  [ 7] Antenna Height:\n  {gd:.1}   [ 8] Gain ab dipole:\n",
            tc = ant.type_code,
            len = ant.length_wl,
            gd = ant.gain_over_dipole_db,
        )
    };
    Some(content)
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

    #[test]
    fn ground_type_defaults_to_average_and_serializes_kebab() {
        assert_eq!(GroundType::default(), GroundType::Average);
        assert_eq!(serde_json::to_string(&GroundType::SeaWater).unwrap(), "\"sea-water\"");
        let back: GroundType = serde_json::from_str("\"poor-soil\"").unwrap();
        assert_eq!(back, GroundType::PoorSoil);
    }

    #[test]
    fn horizontal_preset_generates_type23_voa_with_height_and_ground() {
        let voa = operator_voa_content(AntennaPreset::NvisWireDipole, 9.0, GroundType::Average)
            .expect("horizontal preset must produce a parametric pattern");
        // Type 23 (horizontal dipole), 8 params, half-wave length, metres height,
        // average-ground constants.
        assert!(voa.contains("8     8 parameters"), "want 8-param file:\n{voa}");
        assert!(voa.contains("23    [ 2] Antenna Type"), "want type 23:\n{voa}");
        assert!(voa.contains("-0.50  [ 6] Antenna Length"), "want half-wave length:\n{voa}");
        assert!(voa.contains("9.00  [ 7] Antenna Height"), "want 9 m height:\n{voa}");
        assert!(voa.contains("13    [ 3] Dielectric"), "want average ε_r=13:\n{voa}");
        assert!(voa.contains("0.00500 [ 4] Conductivity"), "want average σ=0.005:\n{voa}");
    }

    #[test]
    fn vertical_preset_generates_type22_monopole() {
        let voa = operator_voa_content(AntennaPreset::BaseVerticalRadials, 9.0, GroundType::Average)
            .expect("vertical preset must produce a parametric pattern");
        // Type 22 (vertical monopole), 7 params, quarter-wave element; the height
        // field does not apply (ground-mounted).
        assert!(voa.contains("7     7 parameters"), "want 7-param file:\n{voa}");
        assert!(voa.contains("22    [ 2] Antenna Type"), "want type 22:\n{voa}");
        assert!(voa.contains("-0.25  [ 6] Antenna Height"), "want quarter-wave element:\n{voa}");
        assert!(!voa.contains("[ 8]"), "type 22 has no 8th param:\n{voa}");
    }

    #[test]
    fn beam_generates_type24_yagi_with_gain_over_dipole() {
        let voa = operator_voa_content(AntennaPreset::BeamYagi, 15.0, GroundType::Average)
            .expect("beam must produce a parametric pattern");
        assert!(voa.contains("24    [ 2] Antenna Type"), "want type 24:\n{voa}");
        assert!(voa.contains("15.00  [ 7] Antenna Height"), "want 15 m boom height:\n{voa}");
        assert!(voa.contains("6.0   [ 8] Gain ab dipole"), "want forward gain over dipole:\n{voa}");
    }

    #[test]
    fn sea_water_ground_constants_appear_in_voa() {
        let voa = operator_voa_content(AntennaPreset::EfhwSloper, 9.0, GroundType::SeaWater).unwrap();
        assert!(voa.contains("80    [ 3] Dielectric"), "want sea ε_r=80:\n{voa}");
        assert!(voa.contains("5.00000 [ 4] Conductivity"), "want sea σ=5.0:\n{voa}");
    }

    #[test]
    fn unknown_preset_has_no_generated_pattern() {
        assert!(
            operator_voa_content(AntennaPreset::Unknown, 9.0, GroundType::Average).is_none(),
            "Unknown keeps the stock isotrope file, not a generated pattern"
        );
    }

    #[test]
    fn nonfinite_or_extreme_height_is_clamped_not_propagated() {
        let nan = operator_voa_content(AntennaPreset::NvisWireDipole, f64::NAN, GroundType::Average).unwrap();
        assert!(nan.contains("9.00  [ 7] Antenna Height"), "NaN height → 9 m fallback:\n{nan}");
        let huge = operator_voa_content(AntennaPreset::NvisWireDipole, 9999.0, GroundType::Average).unwrap();
        assert!(huge.contains("100.00  [ 7] Antenna Height"), "height clamped to 100 m:\n{huge}");
    }
}
