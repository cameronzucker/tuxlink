//! FT-8 Costas synchronization (M2): coarse 2-D `(fc, t0)` search over the
//! spectrogram, ranked/deduped candidates, per-candidate fine refinement, and
//! per-symbol tone-power extraction feeding the M1 [`crate::llr::soft_demap`].
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! The **search structure** follows **WB2FKO "Synchronization in FT8"**: a coarse
//! 2-D `(fc, t0)` scan of the spectrogram evaluates the Costas array at the three
//! sync blocks (symbol offsets 0, 36, 72), producing a ranked, frequency-deduped
//! candidate list (WB2FKO: "as many as 200 candidates … the weaker of a nearby
//! pair is discarded") at 3.125 Hz / 40 ms resolution, followed by per-candidate
//! fine time/frequency refinement (WB2FKO's `ft8b`/`sync8d` fine step).
//!
//! The **scoring metric is MIT `ft8_lib`'s `ft8_sync_score`, NOT WB2FKO's raw
//! `Sabc = t/tN` energy ratio.** WB2FKO normalizes on-tone energy by the off-tone
//! energy; that ratio is unstable in practice — a near-empty spectral region
//! (denominator → 0) out-scores the true signal. [`costas_metric`] instead scores
//! the mean **dB contrast** of each Costas tone against its immediate frequency
//! (±1 tone) and time (±1 symbol) neighbours: scale-invariant, and immune to
//! spectral emptiness (an empty region scores ≈ 0 dB because the tone is no
//! brighter than its own neighbours there). See [`costas_metric`]'s docstring for
//! the exact `ft8_lib` correspondence. Costas positions that fall outside the slot
//! at negative `t0` are excluded from the mean — this subsumes WB2FKO's separate
//! `Sbc` (first-block-dropped) case.
//!
//! **Fine refinement** searches sub-bin time (±40 ms) and frequency (±two FT-8
//! tones, ±12.5 Hz — widened from WB2FKO's ±2.5 Hz because this crate's coarse
//! metric can mislocate an off-grid carrier by >1 tone; see [`fine_refine`]) by
//! maximizing the Costas cross-energy. **Symbol
//! extraction** reads the eight tone powers per info symbol, mirroring `ft8_lib`
//! `decode.c` `ft8_extract_symbol`/`ft8_extract_likelihood`. The `(fc, t0)` search
//! bounds are cross-checked against `ft8_lib` `ftx_find_candidates`.

use crate::channelize::{
    compute_spectrogram, tone_power, Spectrogram, BIN_HZ, FREQ_OSR, HOP_SAMPLES, SYMBOL_SAMPLES,
    TIME_OSR,
};
use crate::consts::{COSTAS, INFO_SYMBOLS, SAMPLE_RATE_HZ};
use crate::crc::check_crc;
use crate::decode::ldpc_decode_ms_default;
use crate::llr::soft_demap;
use crate::message::{message_identity, unpack, HashTable, Payload, PAYLOAD_BYTES};
use crate::symbols::COSTAS_OFFSETS;
use std::collections::HashSet;

/// FT-8 tone spacing in Hz (QEX 2020 §4). Local alias for readability.
const TONE_HZ: f64 = 6.25;

/// Lowest tone-0 audio frequency the coarse search scans.
/// provenance: WB2FKO waterfall example passband (200–2500 Hz); widened low so
/// the fixtures' 800 Hz signal sits comfortably inside the search.
pub const FREQ_MIN_HZ: f64 = 100.0;

/// Highest tone-0 audio frequency the coarse search scans (tone 7 then sits at
/// `+43.75 Hz`, well below Nyquist).
/// provenance: WB2FKO passband example (upper ~2500 Hz); widened to admit the
/// 2400 Hz fixture.
pub const FREQ_MAX_HZ: f64 = 2600.0;

/// Coarse start-time search span, in quarter-symbol steps, measured from sample 0.
/// Covers `DT ∈ [−2.5, +5] s` (`−2.5/0.04 = −62`, `5/0.04 = +125`), which brackets
/// the fixtures' centred frame at ~1.18 s (step ≈ 29.5).
/// provenance: WB2FKO `−2 ≤ ∆t ≤ +3 s` plus start-of-transmit slack; MIT
/// `ft8_lib` `ftx_find_candidates` `time_offset` loop admits partial frames.
pub const T0_STEP_MIN: isize = -62;
/// Upper bound of the coarse start-time search (see [`T0_STEP_MIN`]).
pub const T0_STEP_MAX: isize = 125;

/// Candidates within this many Hz are deduped (weaker discarded).
/// provenance: WB2FKO "If there are two candidates within 4 Hz of each other …
/// the weaker candidate is discarded."
pub const DEDUP_HZ: f64 = 4.0;

/// Maximum ranked candidates retained from the coarse search. This is an
/// implementation retention/performance cap, NOT a protocol constant: a larger
/// cap only lets the per-candidate decode loop try more low-ranked candidates
/// (all still guarded by the sync floor + CRC + `converged`), it does not change
/// any FT-8 quantity. Set above WB2FKO's observed "as many as 200 candidates" as
/// headroom for crowded bands (M3/M4); reducible with no correctness impact.
/// provenance: implementation choice; WB2FKO reports ~200 candidates as the
/// typical acquired count, this crate keeps headroom over that.
pub const MAX_CANDIDATES: usize = 300;

/// Sync-metric floor (T2.3 false-decode guard), in dB. A candidate whose mean
/// Costas neighbour-contrast (see [`costas_metric`]) is below this is not admitted
/// for decode. Chosen from the measured separation between the five real
/// single-signal fixtures (contrast 20.8–21.8 dB, noiseless) and pure-noise /
/// silence inputs (contrast ≤ ~5.8 dB even taking the max over the whole 2-D
/// search); 10.0 dB sits with wide margin in that gap. The noise ceiling is
/// measured on a single deterministic-LCG realization against noiseless fixtures;
/// this floor is re-tuned against real-SNR captures in M3/M4.
/// provenance: empirical separation measured by the `sync_metric_signal_vs_noise`
/// and `noise_stays_below_floor` KATs; guards a downstream `converged && CRC`
/// gate against admitting an empty slot (see `decode.rs` all-zero guard).
pub const SYNC_FLOOR: f32 = 10.0;

/// Small power floor so `log10` is finite in truly-empty spectral regions (both
/// the on-tone and its neighbour are ~0 there, so their dB difference is ~0).
/// This is a standard numerical guard against `log10(0)`, not an FT-8 protocol
/// value: `1e-12` sits far below any real tone power (a single 12 kHz symbol's
/// energy is O(10^0..10^16) on the i16-scaled fixtures), so it only regularizes
/// genuinely-empty bins and never shifts a real contrast. Chosen as a
/// conventional float epsilon well under the smallest meaningful power.
/// provenance: standard/public-domain numerical practice (two-tier rule — not
/// protocol-specific expression); empirically verified not to affect the
/// signal-vs-noise separation in the `sync_metric_signal_vs_noise` KAT.
const POWER_EPS: f32 = 1e-12;

/// A synchronization candidate: the tone-0 audio frequency, the start sample of
/// symbol 0 (the first Costas tone), and the normalized Costas sync metric.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    /// Tone-0 (lowest) audio frequency in Hz.
    pub freq_hz: f64,
    /// Sample offset of symbol 0 (first Costas tone) from the slot start.
    pub start_sample: f64,
    /// Costas sync score: the mean dB neighbour-contrast (`ft8_lib`
    /// `ft8_sync_score`); see [`costas_metric`]. Higher ⟹ stronger alignment.
    pub sync_metric: f32,
}

/// A successful decode: the unpacked message plus where it was found.
#[derive(Clone, Debug)]
pub struct Decoded {
    /// The unpacked human-readable message.
    pub message: String,
    /// Tone-0 audio frequency in Hz.
    pub freq_hz: f64,
    /// Sample offset of symbol 0.
    pub start_sample: f64,
    /// The winning candidate's sync metric.
    pub sync_metric: f32,
}

/// Power at `(ts, bin)` expressed in dB (`10·log10(power + POWER_EPS)`).
#[inline]
fn db_at(spec: &Spectrogram, ts: usize, bin: usize) -> f32 {
    10.0 * (spec.at(ts, bin) + POWER_EPS).log10()
}

/// Costas sync metric at coarse `(fc_bin, t0_step)`: the mean dB *contrast*
/// between each expected Costas tone and its immediate frequency (±1 tone) and
/// time (±1 symbol) neighbours, over the in-bounds Costas positions. `None` if no
/// Costas position falls inside the spectrogram.
///
/// Working in dB makes the contrast scale-invariant (a log ratio), so a fixed
/// floor generalizes across signal amplitudes; contrasting against *immediate*
/// neighbours (rather than a global off-tone mean) is what makes it robust — an
/// empty spectral region scores ≈ 0 dB because the expected tone is no brighter
/// than its own neighbours there, so it cannot masquerade as a strong candidate.
/// provenance: MIT `ft8_lib` `decode.c` `ft8_sync_score` (neighbour-difference
/// score over the three Costas blocks, `p8[sm]−p8[sm±1]` in freq and
/// `±block_stride` in time), adapted to this crate's 3.125-Hz-bin spectrogram
/// where one tone = `FREQ_OSR` bins and one symbol = `TIME_OSR` steps; WB2FKO
/// `Sabc`/`Sbc` per-block Costas summation.
fn costas_metric(spec: &Spectrogram, fc_bin: usize, t0_step: isize) -> Option<f32> {
    let mut score = 0.0f32;
    let mut num = 0u32;

    for &block in COSTAS_OFFSETS.iter() {
        for (k, &sm) in COSTAS.iter().enumerate() {
            let sym_pos = block + k; // frame symbol index 0..79
            let ts_i = t0_step + (sym_pos * TIME_OSR) as isize;
            if ts_i < 0 || ts_i as usize >= spec.num_time_steps {
                continue;
            }
            let ts = ts_i as usize;
            let on_bin = fc_bin + sm as usize * FREQ_OSR;
            let on_db = db_at(spec, ts, on_bin);

            // Frequency neighbours: one tone lower / higher.
            if sm > 0 {
                score += on_db - db_at(spec, ts, on_bin - FREQ_OSR);
                num += 1;
            }
            if sm < 7 {
                score += on_db - db_at(spec, ts, on_bin + FREQ_OSR);
                num += 1;
            }
            // Time neighbours: one symbol earlier / later (same tone bin).
            if k > 0 && ts_i - TIME_OSR as isize >= 0 {
                score += on_db - db_at(spec, ts - TIME_OSR, on_bin);
                num += 1;
            }
            if k + 1 < COSTAS.len() && ts + TIME_OSR < spec.num_time_steps {
                score += on_db - db_at(spec, ts + TIME_OSR, on_bin);
                num += 1;
            }
        }
    }

    if num == 0 {
        return None;
    }
    Some(score / num as f32)
}

/// Coarse 2-D `(fc, t0)` search over the spectrogram. Returns candidates ranked
/// by sync metric (descending), deduped so no two are within [`DEDUP_HZ`], capped
/// at [`MAX_CANDIDATES`].
pub fn coarse_candidates(spec: &Spectrogram) -> Vec<Candidate> {
    let fc_bin_min = (FREQ_MIN_HZ / BIN_HZ).ceil() as usize;
    let fc_bin_max = (FREQ_MAX_HZ / BIN_HZ).floor() as usize;
    // Highest tone (7) must stay inside the spectrogram.
    let fc_bin_hi = fc_bin_max.min(spec.num_bins.saturating_sub(1 + 7 * FREQ_OSR));

    let mut all: Vec<Candidate> = Vec::new();
    for fc_bin in fc_bin_min..=fc_bin_hi {
        for t0_step in T0_STEP_MIN..=T0_STEP_MAX {
            if let Some(metric) = costas_metric(spec, fc_bin, t0_step) {
                // A non-finite metric (NaN/inf from non-finite input samples) must
                // never rank as a candidate: `total_cmp` would order it and
                // `metric < SYNC_FLOOR` is false for NaN, so it would slip past the
                // floor guard. Reject it here so a corrupt slot yields no decode.
                if !metric.is_finite() {
                    continue;
                }
                all.push(Candidate {
                    freq_hz: fc_bin as f64 * BIN_HZ,
                    start_sample: (t0_step * HOP_SAMPLES as isize) as f64,
                    sync_metric: metric,
                });
            }
        }
    }

    // Rank strongest-first. `total_cmp` avoids a latent panic if a future change
    // ever lets a NaN metric through (today `db_at`'s `+POWER_EPS` keeps it finite).
    all.sort_by(|a, b| b.sync_metric.total_cmp(&a.sync_metric));

    // Greedy dedup: keep the strongest, drop any later candidate within DEDUP_HZ
    // of one already kept (WB2FKO's weaker-of-nearby-pair discard).
    let mut kept: Vec<Candidate> = Vec::new();
    for c in all {
        if kept.len() >= MAX_CANDIDATES {
            break;
        }
        if kept
            .iter()
            .any(|k| (k.freq_hz - c.freq_hz).abs() < DEDUP_HZ)
        {
            continue;
        }
        kept.push(c);
    }
    kept
}

/// Sum of Costas on-tone energies for a precise `(start_sample, freq_hz)` via the
/// single-bin DFT — the objective the fine refinement maximizes.
fn costas_cross_energy(samples: &[f32], start_sample: f64, freq_hz: f64) -> f32 {
    let mut sum = 0.0f32;
    for &block in COSTAS_OFFSETS.iter() {
        for (k, &costas_tone) in COSTAS.iter().enumerate() {
            let sym_pos = block + k;
            let sym_start = start_sample + (sym_pos * SYMBOL_SAMPLES) as f64;
            let f = freq_hz + costas_tone as f64 * TONE_HZ;
            sum += tone_power(
                samples,
                sym_start.round() as isize,
                SYMBOL_SAMPLES,
                f,
                SAMPLE_RATE_HZ,
            );
        }
    }
    sum
}

/// Fine-refine a coarse candidate: search sub-step time (±[`HOP_SAMPLES`], the
/// ±40 ms coarse cell) and sub-bin frequency (±two tones) for the alignment that
/// maximizes the Costas cross-energy. A distinct step from the coarse search.
///
/// The frequency span is ±two FT-8 tones (±12.5 Hz), not WB2FKO's ±2.5 Hz,
/// because the coarse dB neighbour-contrast metric ([`costas_metric`]) can
/// mislocate an OFF-GRID carrier by more than one tone: on `gen_ft8` carriers off
/// the 3.125 Hz coarse grid the coarse top candidate has been measured up to
/// ~6.6 Hz from the true tone-0 (a ±6.25 Hz window would then never evaluate the
/// real carrier). Two tones contains it with margin, and the Costas cross-energy
/// objective is sharply unimodal at the true carrier (orders of magnitude above
/// its neighbours), so the wider span cannot latch onto a wrong tone for a single
/// signal. The 0.25 Hz step divides the 6.25 Hz tone exactly, so a bin-multiple
/// carrier is still reachable exactly. WB2FKO's ±2.5 Hz assumes a 3-Hz-accurate
/// coarse stage; this crate's coarse metric is coarser in frequency.
/// (M3/M4 caveat: in a multi-signal slot a ±2-tone fine window can reach a
/// neighbour only ~2 tones away; multi-signal deconfliction is M3/M4 scope, and
/// dedup + per-signal energy peaks bound the risk — re-evaluate when crowded-band
/// decoding lands.)
/// provenance: WB2FKO fine sync (`∆t = ±40 ms`, sub-Hz `∆f` in `ft8b`/`sync8d`),
/// frequency span widened to ±2 tones for this crate's coarse metric — see note.
pub fn fine_refine(samples: &[f32], coarse: &Candidate) -> Candidate {
    // Time grid: ±40 ms in 2 ms steps (24 samples) — divides the 240-sample
    // half-cell so an on-boundary true offset is reachable exactly.
    const DT_STEP: isize = 24;
    const DT_SPAN: isize = HOP_SAMPLES as isize; // ±480 samples
                                                 // Frequency grid: ±two tones (±12.5 Hz) in 0.25 Hz steps (50 per side).
    const DF_STEPS: i32 = 50;
    const DF_HZ: f64 = TONE_HZ / 25.0; // 0.25 Hz; ±50 steps = ±12.5 Hz (±2 tones)

    let base = coarse.start_sample;
    let mut best = *coarse;
    let mut best_energy = f32::NEG_INFINITY;

    let mut dt = -DT_SPAN;
    while dt <= DT_SPAN {
        for i in -DF_STEPS..=DF_STEPS {
            let start = base + dt as f64;
            let freq = coarse.freq_hz + i as f64 * DF_HZ;
            let e = costas_cross_energy(samples, start, freq);
            if e > best_energy {
                best_energy = e;
                best = Candidate { freq_hz: freq, start_sample: start, sync_metric: coarse.sync_metric };
            }
        }
        dt += DT_STEP;
    }
    best
}

/// Extract the 8 tone powers for each of the 58 info symbols at a refined
/// candidate → the `[[f32; 8]; 58]` array [`crate::llr::soft_demap`] consumes.
/// Info symbol `i` sits at frame position `i + (i < 29 ? 7 : 14)` (the two
/// 29-symbol groups between the Costas blocks); tone index `t` is the raw FSK
/// tone at `freq_hz + t·6.25 Hz` (NOT Gray-decoded — the demapper applies Gray).
/// provenance: MIT `ft8_lib` `decode.c` `ft8_extract_likelihood` skip-schedule
/// (`sym_idx = k + (k<29?7:14)`) + `ft8_extract_symbol` (reads the 8 tone bins);
/// crate frame layout `symbols::assemble_frame`.
pub fn extract_info_powers(samples: &[f32], cand: &Candidate) -> [[f32; 8]; INFO_SYMBOLS] {
    let mut powers = [[0.0f32; 8]; INFO_SYMBOLS];
    for (i, sym) in powers.iter_mut().enumerate() {
        let frame_pos = if i < 29 { i + 7 } else { i + 14 };
        let sym_start = cand.start_sample + (frame_pos * SYMBOL_SAMPLES) as f64;
        for (tone, slot) in sym.iter_mut().enumerate() {
            let f = cand.freq_hz + tone as f64 * TONE_HZ;
            *slot = tone_power(
                samples,
                sym_start.round() as isize,
                SYMBOL_SAMPLES,
                f,
                SAMPLE_RATE_HZ,
            );
        }
    }
    powers
}

/// Build a [`Payload`] from the 77 decoded payload bits (MSB-first into 10 bytes).
fn payload_from_bits(bits: &[bool; crate::consts::PAYLOAD_BITS]) -> Payload {
    let mut bytes = [0u8; PAYLOAD_BYTES];
    for (n, &b) in bits.iter().enumerate() {
        if b {
            bytes[n / 8] |= 0x80 >> (n % 8);
        }
    }
    Payload { bytes }
}

/// Full M2 decode pipeline over a 12 kHz real audio slot: channelize → coarse
/// sync → (per candidate above [`SYNC_FLOOR`]) fine-refine → extract → soft-demap
/// → LDPC decode → guard (`converged` AND CRC AND sync floor) → unpack. Returns
/// one [`Decoded`] per distinct signal found (deduped by frequency).
///
/// The sync-metric floor is the M2 false-decode guard: candidates rank
/// descending, so once one falls below the floor the rest do too and the loop
/// stops. Combined with the decoder's `converged` (all-zero rejection) and the
/// CRC-14 check, an empty/noise slot yields no decode.
pub fn decode_samples(samples: &[f32], sample_rate: u32) -> Vec<Decoded> {
    decode_samples_with_floor(samples, sample_rate, SYNC_FLOOR)
}

/// [`decode_samples`] with an explicit sync-metric floor. The default entry point
/// passes [`SYNC_FLOOR`]; this variant exists so the oracle-parity harness and
/// floor-calibration diagnostics can sweep the floor against real-SNR captures
/// (M3 carry-forward #4) without editing a compile-time constant. Lowering the
/// floor trades recall for false-decode risk; the `converged && CRC` guard is the
/// backstop that keeps a lower floor from admitting garbage.
pub fn decode_samples_with_floor(samples: &[f32], sample_rate: u32, floor: f32) -> Vec<Decoded> {
    let spec = compute_spectrogram(samples, sample_rate);
    let cands = coarse_candidates(&spec);
    let mut hash = HashTable::new();
    let mut out: Vec<Decoded> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for cand in cands {
        if cand.sync_metric < floor {
            break; // ranked descending — nothing below here qualifies
        }
        if let Some(d) = try_decode_candidate(samples, &cand, &mut hash) {
            // Within-slot dedup on normalized message identity (WSJT-X multiset
            // semantics; plan T3.1). This replaces M2's frequency-only guard:
            // `coarse_candidates` already discards candidates within DEDUP_HZ, so
            // a second frequency filter here would wrongly drop two genuinely
            // distinct signals that fine-refine close together in a crowded slot
            // (hurting the M4 ≥85% gate). The message string is the correct key —
            // the same signal acquired at two candidates decodes to the same
            // message and collapses; two different stations never do. The key is
            // `message_identity` (hashed callsigns collapsed to `<*>`), so the
            // SAME signal that renders `<...> A B` before its call is learned and
            // `<CALL> A B` after still dedups to one decode (Codex adrev P2).
            if seen.insert(message_identity(&d.message)) {
                out.push(d); // first sighting of this message in the slot
            }
        }
    }
    out
}

/// Attempt to decode ONE candidate: fine-refine → extract → soft-demap → LDPC →
/// guard (`converged` AND CRC-14) → unpack. Returns the [`Decoded`] message or
/// `None` if any guard rejects it. The `hash` table is threaded so a decoded
/// callsign is available to resolve later hashed references in the same slot
/// (M3 carry-forward #1); pass one `&mut HashTable` across a slot's candidates.
///
/// Note this applies NO sync-metric floor — the caller gates on
/// [`Candidate::sync_metric`]. The `converged && CRC` pair is the intrinsic
/// zero-false guard here.
pub fn try_decode_candidate(
    samples: &[f32],
    cand: &Candidate,
    hash: &mut HashTable,
) -> Option<Decoded> {
    let refined = fine_refine(samples, cand);
    let powers = extract_info_powers(samples, &refined);
    let llr = soft_demap(&powers);
    let res = ldpc_decode_ms_default(&llr);
    if !res.converged || !check_crc(&res.message_bits()) {
        return None;
    }
    let payload = payload_from_bits(&res.payload_bits());
    let message = unpack(&payload, hash).ok()?;
    Some(Decoded {
        message,
        freq_hz: refined.freq_hz,
        start_sample: refined.start_sample,
        sync_metric: cand.sync_metric,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc::add_crc;
    use crate::ldpc::ldpc_encode;
    use crate::message::pack;
    use crate::symbols::bits_to_symbols;
    use std::path::PathBuf;

    /// Load a committed gen fixture WAV as f32 samples.
    fn load_fixture(name: &str) -> Vec<f32> {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/fixtures/gen");
        path.push(name);
        let mut reader = hound::WavReader::open(&path)
            .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 12_000, "fixture must be 12 kHz");
        assert_eq!(spec.channels, 1, "fixture must be mono");
        reader.samples::<i16>().map(|s| s.unwrap() as f32).collect()
    }

    /// The 58 transmitted info tones for a message, via the crate's own encoder
    /// (independent of the ft8_lib-generated WAV) — the ground truth the
    /// extracted tones' argmax must match.
    fn transmitted_info_tones(message: &str) -> [u8; INFO_SYMBOLS] {
        let mut hash = HashTable::new();
        let payload = pack(message, &mut hash).expect("pack fixture message");
        let mut bits = [false; crate::consts::PAYLOAD_BITS];
        for (n, b) in bits.iter_mut().enumerate() {
            *b = payload.bytes[n / 8] & (0x80 >> (n % 8)) != 0;
        }
        let codeword = ldpc_encode(&add_crc(&bits));
        bits_to_symbols(&codeword)
    }

    /// The tone with the greatest extracted power for each info symbol.
    fn argmax(sym: &[f32; 8]) -> u8 {
        let mut bi = 0usize;
        for i in 1..8 {
            if sym[i] > sym[bi] {
                bi = i;
            }
        }
        bi as u8
    }

    /// Coarse search top candidate on `std_cq_1500.wav`: fc ≈ 1500 Hz and the
    /// start sample near the fixture's centred frame (14160) within one coarse
    /// cell (±480 samples, before fine refine).
    #[test]
    fn coarse_top_candidate_locates_1500() {
        let samples = load_fixture("std_cq_1500.wav");
        let spec = compute_spectrogram(&samples, 12_000);
        let cands = coarse_candidates(&spec);
        assert!(!cands.is_empty());
        let top = cands[0];
        // Coarse dB-contrast localizes fc to within one tone (fine refine nails it).
        assert!((top.freq_hz - 1500.0).abs() <= TONE_HZ, "fc {} not within one tone of 1500", top.freq_hz);
        assert!(
            (top.start_sample - 14160.0).abs() <= HOP_SAMPLES as f64,
            "start {} not within one cell of 14160",
            top.start_sample
        );
    }

    /// The coarse frequency search is not overfit to 1500 Hz: 800 and 2400 Hz
    /// fixtures put their top candidate at the right carrier.
    #[test]
    fn coarse_locates_other_frequencies() {
        for (name, freq) in [("std_cq_0800.wav", 800.0), ("std_cq_2400.wav", 2400.0)] {
            let samples = load_fixture(name);
            let spec = compute_spectrogram(&samples, 12_000);
            let cands = coarse_candidates(&spec);
            assert!(!cands.is_empty(), "{name}: no candidates");
            let top = cands[0];
            assert!(
                (top.freq_hz - freq).abs() <= TONE_HZ,
                "{name}: fc {} not within one tone of {freq}",
                top.freq_hz
            );
        }
    }

    /// Fine refinement tightens time and frequency: after refine the start sample
    /// is within a few ms of 14160 and the carrier within 0.5 Hz of 1500.
    #[test]
    fn fine_refine_tightens_alignment() {
        let samples = load_fixture("std_cq_1500.wav");
        let spec = compute_spectrogram(&samples, 12_000);
        let top = coarse_candidates(&spec)[0];
        let refined = fine_refine(&samples, &top);
        assert!(
            (refined.start_sample - 14160.0).abs() <= 60.0,
            "refined start {} not within 5 ms of 14160",
            refined.start_sample
        );
        assert!(
            (refined.freq_hz - 1500.0).abs() <= 0.5,
            "refined fc {} not within 0.5 Hz of 1500",
            refined.freq_hz
        );
    }

    /// Extracted info-symbol tones (argmax of each `[f32;8]`) match the
    /// independently-encoded transmitted tone sequence for all 58 info symbols —
    /// the strong intermediate assertion that sync landed correctly.
    #[test]
    fn extracted_symbols_match_transmitted() {
        let samples = load_fixture("std_cq_1500.wav");
        let spec = compute_spectrogram(&samples, 12_000);
        let refined = fine_refine(&samples, &coarse_candidates(&spec)[0]);
        let powers = extract_info_powers(&samples, &refined);
        let truth = transmitted_info_tones("CQ K1ABC FN42");
        let mut mismatches = 0;
        for (i, sym) in powers.iter().enumerate() {
            if argmax(sym) != truth[i] {
                mismatches += 1;
            }
        }
        assert_eq!(mismatches, 0, "{mismatches}/58 extracted info tones wrong");
    }

    /// The sync metric at the true alignment is materially higher than at a
    /// random (fc, t0) — the off-tone normalization works. Also records the
    /// signal-side floor calibration number.
    #[test]
    fn sync_metric_signal_vs_noise() {
        let samples = load_fixture("std_cq_1500.wav");
        let spec = compute_spectrogram(&samples, 12_000);
        let top = coarse_candidates(&spec)[0];
        // A deliberately-wrong alignment: 500 Hz off, 1 s early.
        let wrong_bin = ((1000.0) / BIN_HZ).round() as usize;
        let wrong = costas_metric(&spec, wrong_bin, 5).unwrap();
        // The separation must straddle the floor: signal clears it, off-alignment
        // does not. (Asserting `wrong < FLOOR` is meaningful even when `wrong` is
        // negative dB, unlike a bare `top > wrong*5` ratio.)
        assert!(
            top.sync_metric > SYNC_FLOOR,
            "signal metric {} below floor {SYNC_FLOOR}",
            top.sync_metric
        );
        assert!(
            wrong < SYNC_FLOOR,
            "off-alignment metric {wrong} not below floor {SYNC_FLOOR}"
        );
    }

    /// Pure noise and silence stay below [`SYNC_FLOOR`] across the entire 2-D
    /// search, so the false-decode guard admits nothing. Records the noise-side
    /// floor calibration number.
    #[test]
    fn noise_stays_below_floor() {
        // Silence.
        let silence = vec![0.0f32; 180_000];
        let spec = compute_spectrogram(&silence, 12_000);
        let cands = coarse_candidates(&spec);
        let top_silence = cands.first().map(|c| c.sync_metric).unwrap_or(0.0);
        assert!(top_silence < SYNC_FLOOR, "silence top metric {top_silence} ≥ floor");

        // Deterministic white noise (test-only LCG).
        let mut state = 0x1234_5678u32;
        let noise: Vec<f32> = (0..180_000)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect();
        let spec = compute_spectrogram(&noise, 12_000);
        let cands = coarse_candidates(&spec);
        let top_noise = cands.first().map(|c| c.sync_metric).unwrap_or(0.0);
        assert!(
            top_noise < SYNC_FLOOR,
            "noise top metric {top_noise} ≥ floor {SYNC_FLOOR}"
        );
        // No decode from either.
        assert!(decode_samples(&silence, 12_000).is_empty(), "silence decoded");
        assert!(decode_samples(&noise, 12_000).is_empty(), "noise decoded");
    }

    /// Non-finite input (all-NaN slot) yields NO candidates and NO decode. Guards
    /// the regression where a NaN metric slips past the sync floor (`NaN < FLOOR`
    /// is false) and `total_cmp` ranks it, making the decoder grind every
    /// candidate. (Codex adrev 2026-07-07.)
    #[test]
    fn nonfinite_input_yields_no_decode() {
        let nan = vec![f32::NAN; 180_000];
        let spec = compute_spectrogram(&nan, 12_000);
        assert!(
            coarse_candidates(&spec).is_empty(),
            "NaN slot produced coarse candidates"
        );
        assert!(
            decode_samples(&nan, 12_000).is_empty(),
            "NaN slot produced a decode"
        );
    }

    /// A strong unmodulated carrier (CW tone, no Costas structure) yields NO
    /// decode: a single tone matches at most one of the seven distinct Costas
    /// tones per block, so its sync metric stays below the floor. This is the
    /// realistic adversarial false-decode input for a passive HF decoder — more
    /// so than white noise. (Final whole-branch review 2026-07-07.)
    #[test]
    fn cw_carrier_yields_no_decode() {
        use std::f64::consts::PI;
        let f = 1500.0_f64;
        // Full-scale i16 amplitude — far stronger than the fixtures' signal.
        let carrier: Vec<f32> = (0..180_000)
            .map(|n| (2.0 * PI * f * n as f64 / 12_000.0).sin() as f32 * 20_000.0)
            .collect();
        let spec = compute_spectrogram(&carrier, 12_000);
        let top = coarse_candidates(&spec)
            .first()
            .map(|c| c.sync_metric)
            .unwrap_or(0.0);
        assert!(
            top < SYNC_FLOOR,
            "CW carrier top metric {top} ≥ floor {SYNC_FLOOR}"
        );
        assert!(
            decode_samples(&carrier, 12_000).is_empty(),
            "CW carrier produced a decode"
        );
    }
}
