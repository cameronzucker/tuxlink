//! Default robustness floor mode: wide-band low-density-constellation
//! OFDM. BPSK per sub-carrier across ~2.3 kHz with FEC composition.
//!
//! FEC composition is via the `FecCodec` trait from subsystem #4;
//! Phase 10 wires the real FEC in. Phase 8 uses a pass-through
//! "identity FEC" so the PHY can land without a hard dependency on the
//! FEC crate.
//!
//! Single-symbol scope in this phase: payload must fit in one OFDM
//! symbol's data capacity (~9 bytes at BPSK over the Wide-mode 74 data
//! sub-carriers). Multi-symbol framing arrives in Phase 10.

use crate::error::PhyError;
use crate::ofdm_main::ofdm_params::{OfdmModeName, OfdmParams};
use crate::ofdm_main::receiver::OfdmReceiver;
use crate::ofdm_main::transmitter::OfdmTransmitter;

/// Default robustness floor: wide-band OFDM, BPSK on every occupied
/// sub-carrier. Strategic posture is "go wider, not denser" — see
/// overview §5.A.1.
pub struct WidebandLowDensityFloor {
    params: OfdmParams,
}

impl WidebandLowDensityFloor {
    /// Construct the floor with its pinned Wide-mode OFDM parameters
    /// (full 2300 Hz passband).
    pub fn new() -> Self {
        Self {
            params: OfdmParams::for_mode(OfdmModeName::Wide),
        }
    }

    /// Borrowed access to the underlying OFDM parameter set.
    pub fn params(&self) -> &OfdmParams {
        &self.params
    }

    /// BPSK on every occupied sub-carrier — entries at pilot positions
    /// are ignored by the transmitter / receiver but follow the same
    /// index convention as [`OfdmParams::subcarrier_indices`].
    pub fn bits_per_subcarrier(&self) -> Vec<u8> {
        vec![1; self.params.subcarrier_indices().len()]
    }

    /// Modulate one OFDM symbol carrying `payload` (MSB-first byte →
    /// bit expansion). Errors with [`PhyError::PayloadTooLarge`] when
    /// the payload exceeds the single-symbol data capacity.
    pub fn transmit(&self, payload: &[u8]) -> Result<Vec<f32>, PhyError> {
        let mut payload_bits: Vec<u8> = Vec::with_capacity(payload.len() * 8);
        for byte in payload {
            for bit_idx in (0..8).rev() {
                payload_bits.push((byte >> bit_idx) & 1);
            }
        }
        let bits_per_sc = self.bits_per_subcarrier();
        let data_per_symbol = self.params.data_indices().len();
        if payload_bits.len() > data_per_symbol {
            return Err(PhyError::PayloadTooLarge {
                actual: payload.len(),
                capacity: data_per_symbol / 8,
            });
        }
        payload_bits.resize(data_per_symbol, 0);
        let tx = OfdmTransmitter::new(&self.params);
        Ok(tx.modulate_one_symbol(&payload_bits, &bits_per_sc))
    }

    /// Demodulate one OFDM symbol back to a byte payload. Trailing
    /// zero bytes from the bit-padding are trimmed; multi-symbol
    /// framing (which would carry the exact byte count explicitly)
    /// lands in Phase 10.
    pub fn receive(&self, samples: &[f32]) -> Result<Vec<u8>, PhyError> {
        let bits_per_sc = self.bits_per_subcarrier();
        let rx = OfdmReceiver::new(&self.params);
        let llrs = rx.demodulate_one_symbol(samples, &bits_per_sc);
        let bits: Vec<u8> = llrs
            .iter()
            .map(|l| if *l >= 0.0 { 0 } else { 1 })
            .collect();
        let mut bytes = Vec::with_capacity(bits.len() / 8);
        for chunk in bits.chunks(8) {
            if chunk.len() < 8 {
                break;
            }
            let mut b = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                b |= bit << (7 - i);
            }
            bytes.push(b);
        }
        let last_nonzero = bytes
            .iter()
            .rposition(|&b| b != 0)
            .map(|i| i + 1)
            .unwrap_or(0);
        bytes.truncate(last_nonzero);
        Ok(bytes)
    }
}

impl Default for WidebandLowDensityFloor {
    fn default() -> Self {
        Self::new()
    }
}
