//! Per-family parity-check matrix factories and the [`CodeFamily`] selector.
//!
//! Implementations land progressively: floor rate-1/4 in Phase 3 Task 3.3,
//! OFDM WiFi-family in Phase 3 Task 3.4.

/// Named LDPC code family. Concrete codecs in this crate dispatch on this
/// to pick a parity-check matrix + decoder iteration cap.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CodeFamily {
    /// Rate-1/4 floor code (n=2048, k=512). Used by the wide-band low-density
    /// OFDM PHY mode for noise-floor operation.
    FloorRate14,
    /// Rate-adaptive WiFi-style family. n is 648 or 1296; rate is one of
    /// the [`WifiLdpcRate`] variants.
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
    /// n = 648 codeword bits.
    N648,
    /// n = 1296 codeword bits.
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
