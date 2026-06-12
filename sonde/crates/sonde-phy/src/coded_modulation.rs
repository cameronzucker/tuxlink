//! Coded-modulation contracts.
//!
//! The FEC layer is a separate crate (`sonde-fec`, subsystem #4).
//! PHY composes a `Box<dyn FecCodec>` per mode. Phase 10 lands the
//! trait + an identity stub; the real FEC plugs in once #4's sibling
//! plan lands. The soft-LLR-in / decoded-bytes-out contract is the
//! inter-crate boundary.

/// Fractional code rate `num/den`. `value()` returns the rate as
/// f32 (e.g. `CodeRate { num: 1, den: 2 }` → 0.5).
#[derive(Debug, Clone, Copy)]
pub struct CodeRate {
    /// Numerator (information-bit count per code block).
    pub num: u32,
    /// Denominator (coded-bit count per code block).
    pub den: u32,
}

impl CodeRate {
    /// Rate as `num / den`, f32-coerced.
    pub fn value(&self) -> f32 {
        self.num as f32 / self.den as f32
    }
}

/// FEC layer errors surfaced across the soft-LLR / decoded-bytes
/// boundary.
#[derive(Debug, thiserror::Error)]
pub enum FecError {
    /// Decode failed (uncorrectable error pattern, bad block, etc.).
    #[error("decode failure: {0}")]
    DecodeFailure(String),
}

/// Bus-contract trait for the FEC layer. Implementors live in the
/// `sonde-fec` crate (subsystem #4); PHY composes one per mode.
pub trait FecCodec {
    /// Encode information bits to coded bits (length grows by
    /// `1/rate()`).
    fn encode(&self, info_bits: &[u8]) -> Vec<u8>;
    /// Soft-decode LLRs to information bits. LLR sign convention
    /// matches PHY spec R2: positive ⇒ bit=0, negative ⇒ bit=1.
    fn decode_soft(&self, llr: &[f32]) -> Result<Vec<u8>, FecError>;
    /// Coding rate as `num/den`.
    fn rate(&self) -> CodeRate;
    /// Information-bit block length the codec operates on.
    fn block_info_bits(&self) -> usize;
    /// Coded-bit block length the codec produces.
    fn block_coded_bits(&self) -> usize;
}

/// Pass-through FEC codec. Useful as the Phase 10 placeholder and for
/// channel-sim cases where the BER characterization should isolate
/// PHY effects from coding gain.
pub struct IdentityFec {
    block: usize,
}

impl IdentityFec {
    /// Construct an identity FEC at the given block length (bits).
    pub fn new(block: usize) -> Self {
        Self { block }
    }
}

impl FecCodec for IdentityFec {
    fn encode(&self, info_bits: &[u8]) -> Vec<u8> {
        info_bits.to_vec()
    }
    fn decode_soft(&self, llr: &[f32]) -> Result<Vec<u8>, FecError> {
        Ok(llr
            .iter()
            .map(|l| if *l >= 0.0 { 0u8 } else { 1u8 })
            .collect())
    }
    fn rate(&self) -> CodeRate {
        CodeRate { num: 1, den: 1 }
    }
    fn block_info_bits(&self) -> usize {
        self.block
    }
    fn block_coded_bits(&self) -> usize {
        self.block
    }
}
