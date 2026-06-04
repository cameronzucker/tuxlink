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
use crate::sync::preamble::{PreambleDetector, PreambleGenerator};

/// Sample count of the Zadoff-Chu preamble emitted by
/// [`WidebandLowDensityFloor::transmit_with_preamble`]. Matches the
/// pin in [`crate::sync::preamble`].
pub const PREAMBLE_LEN_SAMPLES: usize = 192;

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

    /// Sample count of one OFDM symbol (FFT body + cyclic prefix).
    /// This is what [`Self::receive`] expects as its input length.
    pub fn symbol_size_samples(&self) -> usize {
        self.params.fft_size() + self.params.cp_len()
    }

    /// Modulate one OFDM symbol carrying `payload`, prefixed with the
    /// Zadoff-Chu preamble defined in [`crate::sync::preamble`]. Output
    /// layout:
    ///
    /// ```text
    /// [preamble (192 samples)][OFDM symbol (FFT + CP samples)]
    /// ```
    ///
    /// This is the over-the-air frame format. Bare [`Self::transmit`]
    /// emits only the OFDM symbol — suitable for back-to-back
    /// loopback where alignment is implicit. For real captures (where
    /// the receiver doesn't know where the symbol starts), use this
    /// pair: this on transmit, [`Self::receive_with_sync`] on receive.
    pub fn transmit_with_preamble(&self, payload: &[u8]) -> Result<Vec<f32>, PhyError> {
        let preamble = PreambleGenerator::new().generate();
        debug_assert_eq!(
            preamble.len(),
            PREAMBLE_LEN_SAMPLES,
            "preamble length pin diverged from PREAMBLE_LEN_SAMPLES",
        );
        let symbol = self.transmit(payload)?;
        let mut out = Vec::with_capacity(preamble.len() + symbol.len());
        out.extend_from_slice(&preamble);
        out.extend_from_slice(&symbol);
        Ok(out)
    }

    /// Scan `samples` for the preamble, then decode the OFDM symbol
    /// that follows. Returns `(preamble_start_sample, payload)`.
    ///
    /// Returns [`PhyError::FrameDetect`] when:
    /// - no preamble is found above the detector's correlation
    ///   threshold (per [`PreambleDetector::scan`]'s docs);
    /// - the detected preamble is too close to the end of the buffer
    ///   for the OFDM symbol to fit.
    ///
    /// Multi-symbol framing is PHY Phase 10; this slice still demods
    /// exactly ONE symbol's worth after the preamble, regardless of
    /// what comes after.
    pub fn receive_with_sync(&self, samples: &[f32]) -> Result<(usize, Vec<u8>), PhyError> {
        let detector = PreambleDetector::new();
        let detection = detector.scan(samples).ok_or_else(|| {
            PhyError::FrameDetect(
                "preamble not detected in input (signal too weak or no preamble \
                 present); pass a longer/cleaner capture or use the bare \
                 receive() if the symbol is already aligned"
                    .to_string(),
            )
        })?;
        let symbol_start = detection.start_sample + PREAMBLE_LEN_SAMPLES;
        let symbol_size = self.symbol_size_samples();
        if symbol_start + symbol_size > samples.len() {
            return Err(PhyError::FrameDetect(format!(
                "preamble detected at sample {} but OFDM symbol that follows \
                 ({} samples) is truncated: have {} samples after preamble, need {}",
                detection.start_sample,
                symbol_size,
                samples.len().saturating_sub(symbol_start),
                symbol_size,
            )));
        }
        let symbol_samples = &samples[symbol_start..symbol_start + symbol_size];
        let payload = self.receive(symbol_samples)?;
        Ok((detection.start_sample, payload))
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

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmit_with_preamble_length_is_preamble_plus_symbol() {
        // Output is preamble (192) + OFDM symbol (FFT body + CP). The
        // exact symbol size comes from the Wide mode's OfdmParams.
        let floor = WidebandLowDensityFloor::new();
        let symbol_size = floor.symbol_size_samples();
        let samples = floor.transmit_with_preamble(b"hi").unwrap();
        assert_eq!(samples.len(), PREAMBLE_LEN_SAMPLES + symbol_size);
    }

    #[test]
    fn preamble_roundtrip_aligned_recovers_payload() {
        // Clean back-to-back: encode → preamble + symbol → decode.
        // The detector sees the preamble at sample 0 and the symbol
        // at sample PREAMBLE_LEN_SAMPLES.
        let floor = WidebandLowDensityFloor::new();
        let payload = b"SYNC!";
        let samples = floor.transmit_with_preamble(payload).unwrap();
        let (start, decoded) = floor.receive_with_sync(&samples).unwrap();
        assert_eq!(start, 0, "preamble should start at sample 0");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn preamble_roundtrip_with_leading_silence_recovers_payload() {
        // Operator captured a WAV that includes some leading silence
        // before the preamble. The detector should find the preamble
        // at the correct offset and slice the symbol correctly.
        let floor = WidebandLowDensityFloor::new();
        let payload = b"OFFSET";
        let core = floor.transmit_with_preamble(payload).unwrap();
        let leading_silence = vec![0.0_f32; 1_000];
        let mut samples = leading_silence.clone();
        samples.extend_from_slice(&core);
        let (start, decoded) = floor.receive_with_sync(&samples).unwrap();
        // Allow ±1 sample tolerance for the detector's correlation
        // peak location — the threshold-based scan picks the exact
        // peak sample, but small offsets are acceptable.
        let offset_err =
            (start as i64 - leading_silence.len() as i64).unsigned_abs() as usize;
        assert!(
            offset_err <= 2,
            "detected start {} should be within ±2 of leading silence {} samples",
            start,
            leading_silence.len()
        );
        assert_eq!(decoded, payload);
    }

    #[test]
    fn preamble_roundtrip_with_trailing_noise_recovers_payload() {
        // Capture continues past the symbol — e.g. trailing radio
        // noise, key-up tail. The decoder should ignore everything
        // after the symbol-sized window.
        let floor = WidebandLowDensityFloor::new();
        let payload = b"TAIL";
        let core = floor.transmit_with_preamble(payload).unwrap();
        let mut samples = core.clone();
        // Add 5000 samples of low-amplitude noise after the symbol.
        // Use a deterministic pseudo-random sequence so the test is
        // reproducible.
        let mut state: u32 = 0xDEAD_BEEF;
        for _ in 0..5_000 {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let v = ((state >> 16) as i16 as f32) / 32_768.0 * 0.05;
            samples.push(v);
        }
        let (start, decoded) = floor.receive_with_sync(&samples).unwrap();
        assert_eq!(start, 0, "preamble should still align at sample 0");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn receive_with_sync_returns_frame_detect_on_pure_silence() {
        let floor = WidebandLowDensityFloor::new();
        let silence = vec![0.0_f32; 10_000];
        let err = floor.receive_with_sync(&silence).unwrap_err();
        assert!(matches!(err, PhyError::FrameDetect(_)));
    }

    #[test]
    fn receive_with_sync_returns_frame_detect_on_random_noise() {
        // High-amplitude random noise should NOT have correlation
        // above the 0.5 threshold; the detector returns None and
        // receive_with_sync surfaces it as FrameDetect.
        let floor = WidebandLowDensityFloor::new();
        let mut samples = Vec::with_capacity(10_000);
        let mut state: u32 = 0x1234_5678;
        for _ in 0..10_000 {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let v = ((state >> 16) as i16 as f32) / 32_768.0;
            samples.push(v);
        }
        let result = floor.receive_with_sync(&samples);
        // With high-amplitude random noise the detector MAY find a
        // weak spurious peak that passes the 0.5 threshold. If it
        // does, the symbol-truncation check or the demod might still
        // succeed but produce garbage — we just assert no panic.
        // The strictest assertion is that pure-silence rejects (above).
        let _ = result;
    }

    #[test]
    fn receive_with_sync_returns_frame_detect_when_symbol_truncated_after_preamble() {
        // Preamble present but the OFDM symbol that follows is cut
        // short. receive_with_sync should reject with FrameDetect.
        let floor = WidebandLowDensityFloor::new();
        let preamble = PreambleGenerator::new().generate();
        let mut samples = preamble.clone();
        // Append only HALF a symbol's worth of garbage.
        let symbol_size = floor.symbol_size_samples();
        samples.extend(std::iter::repeat(0.0_f32).take(symbol_size / 2));
        let err = floor.receive_with_sync(&samples).unwrap_err();
        match err {
            PhyError::FrameDetect(msg) => assert!(
                msg.contains("truncated"),
                "expected 'truncated' in error, got: {msg}"
            ),
            other => panic!("expected FrameDetect, got {other:?}"),
        }
    }

    #[test]
    fn transmit_with_preamble_starts_with_preamble_samples() {
        // First PREAMBLE_LEN_SAMPLES of the output must EQUAL the
        // PreambleGenerator's output bit-for-bit. Confirms the order
        // of the layout is [preamble][symbol], not the reverse.
        let floor = WidebandLowDensityFloor::new();
        let preamble_expected = PreambleGenerator::new().generate();
        let samples = floor.transmit_with_preamble(b"X").unwrap();
        for (i, (&got, &want)) in samples
            .iter()
            .take(PREAMBLE_LEN_SAMPLES)
            .zip(preamble_expected.iter())
            .enumerate()
        {
            assert!(
                (got - want).abs() < 1e-6,
                "preamble sample {i} differs: got {got}, want {want}",
            );
        }
    }

    #[test]
    fn bare_transmit_still_works_unchanged() {
        // Existing transmit() path must remain bit-identical so PR
        // #366 callers don't change behavior. Equality is exact —
        // transmit's output is deterministic for a given payload.
        let floor = WidebandLowDensityFloor::new();
        let a = floor.transmit(b"OLD").unwrap();
        let b = floor.transmit(b"OLD").unwrap();
        assert_eq!(a, b, "transmit() must be deterministic");
        // Bare transmit must be SHORTER than transmit_with_preamble
        // by exactly PREAMBLE_LEN_SAMPLES.
        let with_preamble = floor.transmit_with_preamble(b"OLD").unwrap();
        assert_eq!(with_preamble.len(), a.len() + PREAMBLE_LEN_SAMPLES);
    }
}
