//! LDPC code families. Each submodule constructs its parity-check
//! matrix; [`build`] is the per-[`CodeFamily`] dispatch.
//!
//! Implementations:
//! - [`floor_rate14`] — rate-1/4 (n=2048, k=512) regular (3,4) MacKay
//!   construction for the noise-floor mode.
//! - [`ofdm_wifi_family`] — quasi-cyclic rate-compatible family for
//!   the bit-adaptive OFDM main family (n ∈ {648, 1296}, rate ∈
//!   {1/2, 2/3, 3/4, 5/6}).

pub mod floor_rate14;
pub mod ofdm_wifi_family;

use crate::parity_matrix::ParityCheckMatrix;

/// Named LDPC code family. Concrete codecs in this crate dispatch on
/// this to pick a parity-check matrix + decoder iteration cap.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CodeFamily {
    /// Rate-1/4 floor code (n=2048, k=512). Used by the wide-band
    /// low-density OFDM PHY mode for noise-floor operation.
    FloorRate14,
    /// Rate-adaptive WiFi-style family. n is 648 or 1296; rate is
    /// one of the [`WifiLdpcRate`] variants.
    OfdmAdaptive {
        /// Block length n.
        block_n: BlockN,
        /// Code rate.
        rate: WifiLdpcRate,
    },
}

/// WiFi-style LDPC family block length.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BlockN {
    /// n = 648 codeword bits (24 column-blocks of Z=27).
    N648,
    /// n = 1296 codeword bits (24 column-blocks of Z=54).
    N1296,
}

/// WiFi-style LDPC family code rate. Distinct from
/// [`tuxmodem_phy::coded_modulation::CodeRate`] (a `num/den` struct);
/// this enum is a named-rate convenience for dispatch.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WifiLdpcRate {
    /// 1/2 — strongest correction in the WiFi family.
    R1_2,
    /// 2/3.
    R2_3,
    /// 3/4.
    R3_4,
    /// 5/6 — highest throughput in the WiFi family.
    R5_6,
}

impl WifiLdpcRate {
    /// Numerator and denominator of the code rate.
    pub fn ratio(self) -> (usize, usize) {
        match self {
            Self::R1_2 => (1, 2),
            Self::R2_3 => (2, 3),
            Self::R3_4 => (3, 4),
            Self::R5_6 => (5, 6),
        }
    }
}

/// Construct the parity-check matrix for the requested code family.
pub fn build(family: CodeFamily) -> ParityCheckMatrix {
    match family {
        CodeFamily::FloorRate14 => floor_rate14::build(),
        CodeFamily::OfdmAdaptive { block_n, rate } => ofdm_wifi_family::build(block_n, rate),
    }
}
