//! Source of truth for what PHY modes exist.
//!
//! Per overview §5.A.1, the PHY is a ladder spanning two
//! architecturally-distinct families. This module enumerates the modes
//! and exposes a `ModeTable` that the rest of the crate reads from.
//!
//! Specific sub-carrier counts, FFT sizes, and symbol rates are pinned
//! later (Phase 6+ for OFDM ladder, Phase 8 for floor); this skeleton
//! locks in the family + naming structure first.

/// The two architecturally-distinct PHY mode families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModeFamily {
    /// Bit-adaptive OFDM main throughput family (overview §5.A.1).
    OfdmMain,
    /// Robustness floor family (overview §5.A.1). Houses both the
    /// wide-band low-density-constellation OFDM default and the
    /// situational narrow-FSK variant.
    RobustnessFloor,
}

/// Hint from link-adaptation (subsystem #7) or operator selection.
/// PHY MAY override based on channel measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeHint {
    /// "Pick something in the main throughput family; channel measurement
    /// chooses the specific OFDM mode-within-family."
    MainAuto,
    /// "Specific main-family mode pinned." The string is the short_name.
    MainPinned(&'static str),
    /// "Drop to the robustness floor; default wide-band low-density OFDM."
    Floor,
    /// "Drop to the robustness floor; explicitly request the
    /// narrow-FSK variant for a crowded band."
    FloorCrowdedBand,
}

/// An immutable mode descriptor. Pinned numeric parameters land here
/// in later phases; this skeleton carries names + family.
#[derive(Debug, Clone)]
pub struct ModeDescriptor {
    short_name: &'static str,
    family: ModeFamily,
}

impl ModeDescriptor {
    /// Stable kebab-case identifier (e.g. `"ofdm-mid"`, `"floor-wblo"`).
    pub fn short_name(&self) -> &'static str {
        self.short_name
    }
    /// Which mode family this descriptor belongs to.
    pub fn family(&self) -> ModeFamily {
        self.family
    }
}

/// Resolved mode after applying `ModeHint` + channel measurement.
pub type ResolvedMode = ModeDescriptor;

/// Read-only mode catalogue.
pub struct ModeTable {
    modes: Vec<ModeDescriptor>,
}

impl Default for ModeTable {
    fn default() -> Self {
        Self {
            modes: vec![
                // OFDM main family — placeholders; bandwidth-per-mode
                // pins in Phase 7. Three modes is a starting point per
                // PHY spec §3.Q1 ("ARDOP uses 4; tuxmodem may use fewer
                // or more"); empirical channel-sim sweep settles count.
                ModeDescriptor { short_name: "ofdm-narrow", family: ModeFamily::OfdmMain },
                ModeDescriptor { short_name: "ofdm-mid",    family: ModeFamily::OfdmMain },
                ModeDescriptor { short_name: "ofdm-wide",   family: ModeFamily::OfdmMain },
                // Floor family — default + situational
                ModeDescriptor { short_name: "floor-wblo",  family: ModeFamily::RobustnessFloor },
                ModeDescriptor { short_name: "floor-nfsk",  family: ModeFamily::RobustnessFloor },
            ],
        }
    }
}

impl ModeTable {
    /// Enumerate the distinct mode families represented in this table.
    pub fn distinct_families(&self) -> Vec<ModeFamily> {
        let mut out = Vec::new();
        for m in &self.modes {
            if !out.contains(&m.family) {
                out.push(m.family);
            }
        }
        out
    }

    /// Resolve a `ModeHint` to a concrete `ResolvedMode`. Channel SNR is
    /// reserved for Phase 7's bit-loader-driven mode picking; the v0.1
    /// skeleton ignores it.
    pub fn resolve(&self, hint: ModeHint, _channel_snr_db: Option<f32>) -> ResolvedMode {
        match hint {
            ModeHint::Floor => self.by_name("floor-wblo"),
            ModeHint::FloorCrowdedBand => self.by_name("floor-nfsk"),
            ModeHint::MainAuto => self.by_name("ofdm-mid"),
            ModeHint::MainPinned(name) => self.by_name(name),
        }
    }

    fn by_name(&self, name: &str) -> ResolvedMode {
        self.modes
            .iter()
            .find(|m| m.short_name == name)
            .cloned()
            .expect("mode-table short_name must exist; constructor enforces")
    }
}
