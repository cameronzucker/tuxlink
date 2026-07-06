//! AWGN-vs-SNR go/no-go harness for the clean-room FT-8 soft-demap + min-sum BP
//! core (Station Intelligence M1, task T1.2).
//!
//! # What this module is
//!
//! The project's **GO/NO-GO decision instrument**. It answers: *does the
//! clean-room soft-demapper (T1.1 [`crate::llr::soft_demap`]) + normalized
//! min-sum BP core ([`crate::decode::ldpc_decode_ms`]) decode within ~1–2 dB of
//! the published FT-8 AWGN threshold?* A mis-calibrated SNR axis would give a
//! false verdict, so the calibration is made **self-verifying** against
//! closed-form noncoherent-8-FSK theory (see [`tests::calibration_self_test`]):
//! before trusting any coded decode curve, the uncoded model's symbol-error rate
//! is checked against the exact `Pe(Es/N0)` formula. If that fails, the SNR axis
//! cannot be trusted and the coded curve is meaningless.
//!
//! This module carries **no waveform synthesis / FFT / channelization / sync**
//! (that is M2). It models the 8-FSK per-symbol tone observations directly in the
//! normalized "unit-noise-power-per-bin" form, which is exactly the input
//! [`crate::llr::soft_demap`] consumes.
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from:
//! - the QEX 2020 "The FT4 and FT8 Communication Protocols" paper
//!   (Franke/Somerville/Taylor): §4 (modulation), §6 (soft symbol on tone
//!   amplitudes `|Ci|`), §8 (AWGN channel model), Table 5 (N=1;BP AWGN threshold
//!   −19.6 dB), Table 6 (BP+OSD no-AP −20.8 dB), and the "SNR in a 2500 Hz
//!   reference bandwidth at P(decode)=0.5" convention;
//! - standard communications theory for the noncoherent orthogonal M-FSK AWGN
//!   model and its exact symbol-error probability (J.G. Proakis, *Digital
//!   Communications*, noncoherent MFSK — cited in `PROVENANCE.md`);
//! - the SplitMix64 public-domain PRNG (Steele/Lea/Vigna) + the Box–Muller
//!   transform (public-domain), for a deterministic, dependency-free RNG.
//!
//! No `wsjtr`/WSJT-X source is read; no binary inspected; no new crate added.

#![cfg(test)]

use crate::consts::INFO_SYMBOLS;

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic, dependency-free RNG (SplitMix64 + Box–Muller)
// ─────────────────────────────────────────────────────────────────────────────

/// A tiny deterministic PRNG (SplitMix64). Public-domain algorithm by
/// Steele/Lea/Vigna (the same generator Java's `SplittableRandom` uses and the
/// one Vigna recommends for seeding xoshiro). Chosen so the AWGN sweep is fully
/// reproducible and CI-stable with **no external crate** (the brief forbids
/// adding `rand`).
///
/// Each call to [`Rng::next_u64`] advances the 64-bit state by the fixed odd
/// increment `0x9E3779B97F4A7C15` (the golden-ratio constant) and finalizes it
/// with the SplitMix64 mixing function.
/// provenance: SplitMix64, public domain (Steele, Lea, Vigna, "Fast Splittable
/// Pseudorandom Number Generators", OOPSLA 2014); constants as published.
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    /// Seed the generator. Distinct seeds give independent streams; the same
    /// seed always reproduces the same sequence (the determinism the harness
    /// relies on).
    pub(crate) fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    /// SplitMix64 core: advance state, mix, return the next 64-bit value.
    /// provenance: SplitMix64 reference implementation (public domain).
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform `f64` in the half-open interval `[0, 1)`, using the top 53 bits
    /// (the `f64` mantissa width) so every representable value is equiprobable.
    /// provenance: standard "top-53-bits / 2^53" uniform construction.
    fn next_f64(&mut self) -> f64 {
        // 2^-53 * (top 53 bits) ∈ [0, 1).
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Two independent standard-normal `N(0, 1)` samples via the Box–Muller
    /// transform. `u1` is drawn from `(0, 1]` (shifted off 0) so `ln(u1)` is
    /// finite.
    /// provenance: Box–Muller transform (G.E.P. Box & M.E. Muller, 1958; public
    /// domain).
    fn next_gaussian_pair(&mut self) -> (f64, f64) {
        // Shift u1 into (0, 1] to keep ln() finite (next_f64 can return 0.0).
        let u1 = 1.0 - self.next_f64();
        let u2 = self.next_f64();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        (r * theta.cos(), r * theta.sin())
    }

    /// One complex AWGN sample `(x, y)` with `x, y ~ N(0, 0.5)` independently, so
    /// `E[|n|²] = E[x²] + E[y²] = 0.5 + 0.5 = 1` — noise power per FSK bin = 1,
    /// the normalized "unit-noise-power-per-bin" convention the SNR axis assumes.
    /// provenance: standard complex-AWGN model with per-component variance
    /// `N0/2` normalized to `0.5` (QEX 2020 §8 AWGN channel; any comms text).
    fn complex_noise(&mut self) -> (f64, f64) {
        let (g0, g1) = self.next_gaussian_pair();
        let s = 0.5f64.sqrt(); // scale N(0,1) → N(0, 0.5): std = sqrt(0.5)
        (g0 * s, g1 * s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SNR-in-2500-Hz reference-bandwidth conversion (QEX convention)
// ─────────────────────────────────────────────────────────────────────────────

/// FT-8 symbol duration in seconds (`T = 0.16 s`). Reuses the crate constant so
/// a future edit to the symbol rate propagates here automatically.
/// provenance: QEX 2020 §4 / Table 4 (`crate::consts::SYMBOL_SECS`).
const T_SYMBOL_SECS: f64 = crate::consts::SYMBOL_SECS;

/// QEX's reference bandwidth for quoted SNR thresholds, in Hz.
/// provenance: QEX 2020 §"Table 5" text — "signal-to-noise ratios in 2500 Hz
/// bandwidth at which decoding probability is 0.5".
const REF_BANDWIDTH_HZ: f64 = 2500.0;

/// The additive dB offset between symbol SNR (`Es/N0`, linear `γ`) and
/// SNR-in-2500-Hz. QEX quotes thresholds as `SNR_2500 = Ps/(N0·2500)` with
/// `Ps = Es/T`, so `SNR_2500 = (Es/N0)/(T·2500) = γ / (0.16·2500) = γ / 400`.
/// In dB: `SNR_2500_dB = 10·log10(γ) − 10·log10(0.16·2500) = 10·log10(γ) − 26.02`.
///
/// `10·log10(0.16·2500) = 10·log10(400) ≈ 26.0206 dB`.
/// provenance: QEX 2020 §4 (`Ps = Es/T`, `T = 0.16 s`) + Table-5 text (2500 Hz
/// reference bandwidth); arithmetic per the T1.2 brief.
fn snr_offset_db() -> f64 {
    10.0 * (T_SYMBOL_SECS * REF_BANDWIDTH_HZ).log10()
}

/// Convert a linear symbol SNR `γ = Es/N0` to SNR in a 2500 Hz reference
/// bandwidth, in dB: `SNR_2500_dB = 10·log10(γ) − 26.02 dB`.
/// provenance: see [`snr_offset_db`] (QEX 2020 §4 + Table-5 text).
pub(crate) fn gamma_to_snr2500_db(gamma: f64) -> f64 {
    10.0 * gamma.log10() - snr_offset_db()
}

/// Inverse of [`gamma_to_snr2500_db`]: given a target SNR-in-2500-Hz in dB,
/// return the linear symbol SNR `γ = Es/N0 = 10^((SNR_2500_dB + 26.02)/10)`.
/// provenance: see [`snr_offset_db`] (QEX 2020 §4 + Table-5 text).
pub(crate) fn snr2500_db_to_gamma(snr2500_db: f64) -> f64 {
    10.0f64.powf((snr2500_db + snr_offset_db()) / 10.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// The noncoherent 8-FSK AWGN tone model (per info symbol)
// ─────────────────────────────────────────────────────────────────────────────

/// FSK order: 8 tones per FT-8 channel symbol (3 bits/symbol).
/// provenance: QEX 2020 §4 (8-FSK).
const M_FSK: usize = 8;

/// Synthesize the 8 per-tone **magnitudes** `|value_k|` for one info symbol under
/// noncoherent 8-FSK over AWGN, given the true tone `true_tone ∈ 0..=7` and a
/// linear symbol SNR `gamma = Es/N0`.
///
/// Model (normalized unit-noise-power-per-bin form):
/// - each tone `k` gets complex AWGN `n_k = (x + iy)`, `x, y ~ N(0, 0.5)`
///   (so `E[|n_k|²] = 1`);
/// - the **true** tone additionally carries a signal phasor of amplitude
///   `a = sqrt(γ)` at a uniformly-random phase θ (noncoherent):
///   `value_t = a·(cosθ + i·sinθ) + n_t`; every other tone is noise only;
/// - the returned observation is the 8 **magnitudes** `|value_k|` (amplitudes,
///   i.e. sqrt of power), matching QEX §6's soft symbol
///   `Lj = K·(max|Ci|₁ − max|Ci|₀)`, which is defined on tone **amplitudes**
///   `|Ci|`. Feed magnitudes to [`crate::llr::soft_demap`], **not** powers.
///
/// provenance: QEX 2020 §4 (modulation), §6 (soft symbol on `|Ci|`), §8 (AWGN
/// channel model); standard noncoherent orthogonal M-FSK AWGN model (Proakis,
/// *Digital Communications*).
pub(crate) fn symbol_magnitudes(true_tone: u8, gamma: f64, rng: &mut Rng) -> [f32; M_FSK] {
    debug_assert!((true_tone as usize) < M_FSK, "tone out of range");
    let a = gamma.sqrt(); // signal amplitude on the true tone
    // Random uniform phase θ ∈ [0, 2π) (noncoherent detection).
    let theta = 2.0 * std::f64::consts::PI * rng.next_f64();
    let (sig_re, sig_im) = (a * theta.cos(), a * theta.sin());

    let mut mags = [0.0f32; M_FSK];
    for (k, m) in mags.iter_mut().enumerate() {
        let (nx, ny) = rng.complex_noise();
        let (re, im) = if k == true_tone as usize {
            (sig_re + nx, sig_im + ny)
        } else {
            (nx, ny)
        };
        *m = (re * re + im * im).sqrt() as f32;
    }
    mags
}

/// Synthesize the full `58 × 8` magnitude observation for a whole frame's 58 true
/// tones at symbol SNR `gamma`. The direct input to [`crate::llr::soft_demap`].
/// provenance: repeated application of [`symbol_magnitudes`] (QEX 2020 §4/§6/§8).
pub(crate) fn frame_magnitudes(
    true_tones: &[u8; INFO_SYMBOLS],
    gamma: f64,
    rng: &mut Rng,
) -> [[f32; M_FSK]; INFO_SYMBOLS] {
    let mut out = [[0.0f32; M_FSK]; INFO_SYMBOLS];
    for (o, &tone) in out.iter_mut().zip(true_tones.iter()) {
        *o = symbol_magnitudes(tone, gamma, rng);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Closed-form noncoherent orthogonal M-FSK symbol-error probability
// ─────────────────────────────────────────────────────────────────────────────

/// Exact symbol-error probability for noncoherent orthogonal M-FSK over AWGN at
/// linear symbol SNR `gamma = Es/N0`, `M = 8`:
///
/// ```text
/// Pe(γ) = Σ_{n=1}^{M-1} (−1)^{n+1} · C(M−1, n)/(n+1) · exp( −(n/(n+1))·γ )
/// ```
///
/// This is the closed-form union-free result (an exact alternating sum, not a
/// bound) for equal-energy orthogonal signaling with square-law / envelope
/// detection. Used ONLY to calibrate the model's SNR axis (the uncoded ML
/// detector picks `argmax_k |value_k|`); it never touches the coded decode path.
/// provenance: J.G. Proakis, *Digital Communications*, noncoherent orthogonal
/// M-ary FSK symbol-error probability (cited in `PROVENANCE.md`).
pub(crate) fn noncoherent_mfsk_pe(gamma: f64) -> f64 {
    let m = M_FSK as u64;
    let mut pe = 0.0f64;
    for n in 1..m {
        // Binomial coefficient C(M-1, n) as an exact integer (M-1 = 7, small).
        let binom = binomial(m - 1, n) as f64;
        let sign = if n % 2 == 1 { 1.0 } else { -1.0 };
        let nf = n as f64;
        pe += sign * binom / (nf + 1.0) * (-(nf / (nf + 1.0)) * gamma).exp();
    }
    pe
}

/// Exact binomial coefficient `C(n, k)` for the small values used here
/// (`n ≤ 7`), computed iteratively to avoid factorial overflow.
/// provenance: standard combinatorial identity.
fn binomial(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut result = 1u64;
    for i in 0..k {
        result = result * (n - i) / (i + 1);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::{CODEWORD_BITS, PAYLOAD_BITS};
    use crate::crc::{add_crc, check_crc};
    use crate::decode::ldpc_decode_ms;
    use crate::ldpc::ldpc_encode;
    use crate::llr::soft_demap;
    use crate::symbols::bits_to_symbols;

    /// Max-iteration cap for the coded sweep. The landed default is 30; we pass
    /// it explicitly so the harness pins the value it measured against.
    /// provenance: `crate::decode::DEFAULT_MAX_ITERS` (30); brief §2 ("30").
    const SWEEP_MAX_ITERS: usize = 30;

    // ── 1. CALIBRATION SELF-TEST — the linchpin ──────────────────────────────
    //
    // Before trusting ANY coded curve, verify the model's SNR axis reproduces
    // closed-form noncoherent-8-FSK theory. The uncoded ML detector is
    // `argmax_k |value_k|`. For a few Es/N0 values, generate many symbols with
    // random true tones, measure the empirical symbol-error rate, and assert it
    // matches `noncoherent_mfsk_pe(γ)` within tolerance. If this FAILS the model
    // calibration is wrong and the SNR axis cannot be trusted — fix the model
    // before looking at decode curves.
    #[test]
    fn calibration_self_test() {
        assert_model_calibrated();
    }

    /// The calibration linchpin, factored out so it also runs INSIDE
    /// [`awgn_snr_curve`] — otherwise `cargo test -- --ignored awgn_snr_curve`
    /// (the documented go/no-go command) would run only the ignored sweep and
    /// SKIP the non-ignored `calibration_self_test`, leaving the coded curve
    /// ungated by the Pe-vs-model check. Panics (failing the caller) if the
    /// model's uncoded symbol-error rate diverges from closed-form theory.
    fn assert_model_calibrated() {
        // Es/N0 test points in dB (symbol SNR, NOT SNR-in-2500-Hz).
        let esn0_db = [3.0f64, 6.0, 9.0];
        let n_symbols = 40_000usize;
        // Fixed seed for reproducibility; stream varies by symbol index inside.
        let mut rng = Rng::new(0x00C0_FFEE_1234_5678);

        // Absolute + relative tolerance band. The estimator's 1σ on a Bernoulli
        // rate p over N samples is sqrt(p(1-p)/N); at N=40k and p~0.01..0.3 this
        // is well under the band. Use a generous ±15% relative OR ±0.01 absolute
        // (whichever is larger) so a low-Pe point isn't tripped by float noise.
        let rel_tol = 0.15;
        let abs_tol = 0.01;

        eprintln!("\n=== T1.2 calibration self-test: model SER vs closed-form Pe (noncoherent 8-FSK) ===");
        eprintln!(
            "{:>10}  {:>8}  {:>12}  {:>12}  {:>10}",
            "Es/N0 dB", "gamma", "theory Pe", "model SER", "rel err"
        );
        for &db in &esn0_db {
            let gamma = 10.0f64.powf(db / 10.0);
            let theory = noncoherent_mfsk_pe(gamma);

            let mut errors = 0usize;
            for _ in 0..n_symbols {
                // True tone from the RNG (uniform 0..8); independent noise per
                // symbol via the shared advancing stream.
                let true_tone = (rng.next_u64() % M_FSK as u64) as u8;
                let mags = symbol_magnitudes(true_tone, gamma, &mut rng);
                // Uncoded ML detection = argmax magnitude.
                let mut best_k = 0usize;
                let mut best_v = f32::NEG_INFINITY;
                for (k, &v) in mags.iter().enumerate() {
                    if v > best_v {
                        best_v = v;
                        best_k = k;
                    }
                }
                if best_k != true_tone as usize {
                    errors += 1;
                }
            }
            let model_ser = errors as f64 / n_symbols as f64;
            let rel_err = if theory > 0.0 {
                (model_ser - theory).abs() / theory
            } else {
                (model_ser - theory).abs()
            };
            eprintln!(
                "{db:>10.1}  {gamma:>8.3}  {theory:>12.5}  {model_ser:>12.5}  {rel_err:>10.4}"
            );

            let within = (model_ser - theory).abs() <= abs_tol || rel_err <= rel_tol;
            assert!(
                within,
                "calibration FAILED at Es/N0={db} dB: model SER {model_ser:.5} vs theory {theory:.5} \
                 (rel err {rel_err:.4} > {rel_tol}, abs err {:.5} > {abs_tol}). \
                 The SNR axis is mis-calibrated — do NOT trust the coded curve.",
                (model_ser - theory).abs()
            );
        }
        eprintln!("=== calibration self-test PASSED — SNR axis trustworthy ===\n");
    }

    /// SNR conversion is exact and round-trips. `−26.02 dB` offset and the two
    /// anchor conversions are pinned here so a future edit to the conversion
    /// can't silently shift the whole SNR axis.
    #[test]
    fn snr_conversion_is_exact() {
        // 10·log10(0.16·2500) = 10·log10(400) ≈ 26.0206 dB.
        assert!(
            (snr_offset_db() - 26.0206).abs() < 1e-3,
            "SNR offset {} != 26.02 dB",
            snr_offset_db()
        );
        // Round-trip γ → dB → γ.
        for &g in &[1.0f64, 10.0, 100.0, 400.0, 2512.0] {
            let db = gamma_to_snr2500_db(g);
            let g2 = snr2500_db_to_gamma(db);
            assert!((g - g2).abs() / g < 1e-9, "round-trip failed for γ={g}");
        }
        // γ = 400 ⇒ SNR_2500 = 0 dB (since 400/400 = 1).
        assert!(gamma_to_snr2500_db(400.0).abs() < 1e-6);
    }

    /// Build `n` DISTINCT deterministic 174-bit codewords: seed the PRNG, draw
    /// distinct 77-bit payloads → `add_crc` → `ldpc_encode`. The AWGN decode
    /// probability is codeword-independent for this linear code over the
    /// symmetric channel (geometric uniformity); distinct codewords just buy more
    /// independent trials and guard against a min-sum codeword-specific quirk.
    fn distinct_codewords(n: usize, seed: u64) -> Vec<[bool; CODEWORD_BITS]> {
        let mut rng = Rng::new(seed);
        let mut seen: std::collections::HashSet<[bool; PAYLOAD_BITS]> = std::collections::HashSet::new();
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let mut payload = [false; PAYLOAD_BITS];
            // 77 bits from ceil(77/64)=2 u64 draws.
            let w0 = rng.next_u64();
            let w1 = rng.next_u64();
            for (i, bit) in payload.iter_mut().enumerate() {
                let word = if i < 64 { w0 } else { w1 };
                let shift = if i < 64 { i } else { i - 64 };
                *bit = (word >> shift) & 1 == 1;
            }
            if seen.insert(payload) {
                out.push(ldpc_encode(&add_crc(&payload)));
            }
        }
        out
    }

    /// Run the AWGN decode sweep once. Returns, per SNR point, the decode
    /// probability, plus the total false-decode count across the whole sweep.
    /// A **success** requires all three guards: (a) `converged` (syndrome == 0),
    /// (b) the recovered 91-bit prefix passes `check_crc`, and (c) the recovered
    /// codeword equals the true codeword (a genuine decode). A **false decode**
    /// is converged + CRC-OK but codeword ≠ true.
    fn run_sweep(
        snr_points_db: &[f64],
        codewords: &[[bool; CODEWORD_BITS]],
        n_trials: usize,
        seed: u64,
    ) -> (Vec<f64>, usize) {
        // Precompute true tones per codeword.
        let true_tones: Vec<[u8; INFO_SYMBOLS]> =
            codewords.iter().map(bits_to_symbols).collect();

        let mut probs = Vec::with_capacity(snr_points_db.len());
        let mut false_decodes = 0usize;

        for (si, &snr_db) in snr_points_db.iter().enumerate() {
            let gamma = snr2500_db_to_gamma(snr_db);
            let mut successes = 0usize;
            let total = codewords.len() * n_trials;
            for (ci, tones) in true_tones.iter().enumerate() {
                let cw = &codewords[ci];
                for t in 0..n_trials {
                    // Distinct, reproducible stream per (SNR point, codeword,
                    // trial): mix indices into the seed so every realization is
                    // independent yet deterministic.
                    let stream_seed = seed
                        ^ (si as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        ^ (ci as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
                        ^ (t as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
                    let mut rng = Rng::new(stream_seed);
                    let mags = frame_magnitudes(tones, gamma, &mut rng);
                    let llr = soft_demap(&mags);
                    let res = ldpc_decode_ms(&llr, SWEEP_MAX_ITERS);
                    if res.converged && check_crc(&res.message_bits()) {
                        if &res.codeword == cw {
                            successes += 1;
                        } else {
                            false_decodes += 1;
                        }
                    }
                }
            }
            probs.push(successes as f64 / total as f64);
        }
        (probs, false_decodes)
    }

    /// Linear interpolation of the SNR at which P(decode) crosses 0.5. Scans from
    /// the strongest (least negative) SNR toward the weakest and finds the first
    /// bracket where P goes from ≥ 0.5 to < 0.5; interpolates within it. Returns
    /// `None` if 0.5 is never crossed in range.
    fn interp_50pct(snr_db: &[f64], probs: &[f64]) -> Option<f64> {
        for i in 0..probs.len().saturating_sub(1) {
            let (p_hi, p_lo) = (probs[i], probs[i + 1]);
            // Bracket: high-SNR point ≥ 0.5, next (lower-SNR) point < 0.5.
            if p_hi >= 0.5 && p_lo < 0.5 {
                let (s_hi, s_lo) = (snr_db[i], snr_db[i + 1]);
                // Linear interp on P vs SNR between the two bracketing points.
                let frac = (p_hi - 0.5) / (p_hi - p_lo);
                return Some(s_hi + frac * (s_lo - s_hi));
            }
        }
        None
    }

    // ── 2. THE SNR CURVE — the gate itself ───────────────────────────────────
    //
    // Sweep SNR-in-2500-Hz over a range around the published FT-8 AWGN
    // threshold, measure decode-probability per point over many independent
    // noise realizations, interpolate the 50% crossing, print a readable table
    // with BOTH reference anchors, and assert a LOOSE sanity bound (crossing
    // better than −16.0 dB) AND zero false decodes. The precise GO/NO-GO is
    // adjudicated by a human from the printed number, NOT by a tight CI assert.
    //
    // `#[ignore]`: this is the full-resolution CHARACTERIZATION run — 50
    // codewords × 200 trials × 19 SNR points = 190 000 min-sum decodes. That is
    // ~40 s in `--release` but exceeds a sane wall-clock in an unoptimized debug
    // build (CI's default `cargo test` profile), so it is not run by default.
    // Run it explicitly for the go/no-go record:
    //   cargo test -p tuxlink-ft8 --manifest-path src-tauri/Cargo.toml \
    //     --release -- --ignored awgn_snr_curve --nocapture
    // The fast `awgn_curve_smoke` below guards the same decode-vs-noise path in
    // CI's default (debug) profile.
    #[test]
    #[ignore = "full-resolution go/no-go characterization (190k decodes); run with --release --ignored. See awgn_curve_smoke for the CI-default guard."]
    fn awgn_snr_curve() {
        // Reference anchors (QEX 2020).
        // provenance: QEX 2020 Table 5 (FT8, AWGN, "N=1; BP" = −19.6 dB) and
        // Table 6 (FT8, AWGN, BP+OSD, "No AP" = −20.8 dB).
        const ANCHOR_FAIR_DB: f64 = -19.6; // FAIR anchor for THIS config (N=1 demap + BP)
        const ANCHOR_HEADLINE_DB: f64 = -20.8; // full-decoder headline (BP+OSD, no AP)

        // Gate the coded curve on the calibration self-check FIRST, so this test
        // is self-protecting even when invoked alone via `--ignored`. If the SNR
        // axis is mis-calibrated the coded numbers below are meaningless.
        assert_model_calibrated();

        // ≥50 distinct codewords for independent trials.
        let codewords = distinct_codewords(50, 0x00F7_8000_0000_0001);
        assert_eq!(codewords.len(), 50);

        // Sweep −15 … −24 dB at 0.5 dB resolution (finer near −18…−21).
        let mut snr_points: Vec<f64> = Vec::new();
        let mut s = -15.0f64;
        while s >= -24.0 - 1e-9 {
            snr_points.push(s);
            s -= 0.5;
        }

        // ≥100 trials/point (200 for a stable 50% estimate near threshold).
        let n_trials = 200usize;
        let (probs, false_decodes) =
            run_sweep(&snr_points, &codewords, n_trials, 0xA1B2_C3D4_E5F6_0789);

        // Interpolated 50% crossing.
        let crossing = interp_50pct(&snr_points, &probs);

        // ── Readable table ───────────────────────────────────────────────────
        eprintln!("\n=== T1.2 AWGN decode-probability sweep (soft_demap + normalized min-sum BP, {SWEEP_MAX_ITERS} iters) ===");
        eprintln!(
            "  codewords={}  trials/point={}  total/point={}",
            codewords.len(),
            n_trials,
            codewords.len() * n_trials
        );
        eprintln!("{:>14}  {:>12}", "SNR_2500 (dB)", "P(decode)");
        for (snr_db, p) in snr_points.iter().zip(probs.iter()) {
            eprintln!("{snr_db:>14.1}  {p:>12.3}");
        }
        eprintln!("{:-<30}", "");
        match crossing {
            Some(c) => eprintln!("  interpolated P(decode)=0.5 crossing: {c:>7.2} dB"),
            None => eprintln!("  interpolated P(decode)=0.5 crossing: NONE in [{}, {}] dB",
                snr_points.first().unwrap(), snr_points.last().unwrap()),
        }
        eprintln!("  reference anchors:");
        eprintln!(
            "    FAIR (this config: N=1 demap + BP, no OSD/block-det) = {ANCHOR_FAIR_DB:>6.1} dB  [QEX Table 5]"
        );
        eprintln!(
            "    HEADLINE (full decoder: BP+OSD, no AP)               = {ANCHOR_HEADLINE_DB:>6.1} dB  [QEX Table 6]"
        );
        if let Some(c) = crossing {
            eprintln!(
                "  measured vs FAIR: {:+.2} dB   measured vs HEADLINE: {:+.2} dB",
                c - ANCHOR_FAIR_DB,
                c - ANCHOR_HEADLINE_DB
            );
        }
        eprintln!("  false decodes across whole sweep: {false_decodes}");
        eprintln!("=== end AWGN sweep ===\n");

        // ── Assertions (loose sanity gate; human adjudicates GO/NO-GO) ───────
        // Zero false decodes with the converged+CRC+codeword guard stack.
        assert_eq!(
            false_decodes, 0,
            "AWGN sweep produced {false_decodes} false decodes (converged+CRC-OK but wrong codeword)"
        );
        // The crossing must exist and be better (more negative) than the plan's
        // NO-GO / STOP line. The plan sets STOP at ≥4 dB worse than the −20.8
        // headline = −16.8 dB: a crossing at or above that means the LLR+BP core
        // is broken and this test SHOULD hard-fail. Values in the −16.8…−18.8
        // window (the edge of the "within 1–2 dB of −20.8" GO band) are the
        // "marginal, surface to the operator" zone — the printed anchors above
        // drive that human GO-vs-marginal call; the hard assertion only enforces
        // the unambiguous NO-GO. Our measured −19.69 dB clears this comfortably.
        const STOP_LINE_DB: f64 = -16.8; // 4 dB worse than the −20.8 headline
        let c = crossing.expect("P(decode) never crossed 0.5 in the swept range — core is broken");
        assert!(
            c < STOP_LINE_DB,
            "50% crossing {c:.2} dB is at/above the {STOP_LINE_DB} dB STOP line \
             (≥4 dB worse than the −20.8 headline) — NO-GO, core is broken"
        );
    }

    /// Fast CI-default (debug-profile) guard for the coded decode-vs-noise path.
    /// A small reduced sweep — enough to prove the core decodes near-perfectly
    /// well above threshold, fails well below it, and produces zero false decodes
    /// — without the full characterization's 190k-decode cost. This runs in the
    /// default `cargo test` profile; the precise 50% crossing lives in the
    /// `#[ignore]`d `awgn_snr_curve`. Bounds are deliberately loose (large margin
    /// vs the full curve's 1.000 @ −17 dB and 0.000 @ −23 dB) so it is not flaky.
    #[test]
    fn awgn_curve_smoke() {
        let codewords = distinct_codewords(20, 0x00F7_8000_0000_0001);
        // Three points: a strong rail (≈all decode), a NEAR-THRESHOLD point, and
        // a weak rail (≈none decode). The mid point is what actually guards the
        // GO/NO-GO threshold in default CI — a decoder whose threshold regressed
        // by more than ~1 dB would push P(−20 dB) out of the asserted band even
        // though both rails still look fine.
        let snr_points = [-17.0f64, -20.0, -23.0];
        let n_trials = 40usize; // 20 × 40 × 3 = 2400 decodes — a few seconds in debug.
        let (probs, false_decodes) =
            run_sweep(&snr_points, &codewords, n_trials, 0xA1B2_C3D4_E5F6_0789);

        assert_eq!(false_decodes, 0, "smoke sweep produced {false_decodes} false decodes");
        assert!(
            probs[0] >= 0.90,
            "P(decode) at −17 dB = {:.3}, expected ≥0.90 (core should decode near-perfectly well above threshold)",
            probs[0]
        );
        // Near-threshold guard. The full curve puts P(−20 dB) ≈ 0.32; with 800
        // samples the 1σ is ≈0.017, so [0.12, 0.62] is non-flaky yet a ≳1 dB
        // threshold regression (which drives P toward 0 or 1 here) trips it.
        assert!(
            (0.12..=0.62).contains(&probs[1]),
            "P(decode) at −20 dB = {:.3}, expected in [0.12, 0.62] (near-threshold guard: \
             a decoder whose 50% point regressed >~1 dB from −19.7 dB would fall outside this)",
            probs[1]
        );
        assert!(
            probs[2] <= 0.10,
            "P(decode) at −23 dB = {:.3}, expected ≤0.10 (core should almost never decode well below threshold)",
            probs[2]
        );
    }

    // ── 3. Guard / edge tests ────────────────────────────────────────────────

    /// Pure-noise slots (no signal at all: the "true tone" carries only noise, so
    /// every tone is i.i.d. noise) must produce ZERO converged + CRC-OK decodes
    /// over many trials. Guards against false decodes manufactured from noise.
    #[test]
    fn pure_noise_yields_zero_false_decodes() {
        let n_trials = 3_000usize;
        let mut false_or_valid = 0usize;
        // Arbitrary "intended" tones (their identity is irrelevant — no signal is
        // added, so the observation is pure noise regardless).
        let tones = [0u8; INFO_SYMBOLS];
        for t in 0..n_trials {
            let mut rng = Rng::new(0xDEAD_BEEF_0000_0000 ^ t as u64);
            // gamma = 0 ⇒ signal amplitude a = 0 ⇒ every tone is noise only.
            let mags = frame_magnitudes(&tones, 0.0, &mut rng);
            let llr = soft_demap(&mags);
            let res = ldpc_decode_ms(&llr, SWEEP_MAX_ITERS);
            // A converged + CRC-passing decode from pure noise is a false decode.
            if res.converged && check_crc(&res.message_bits()) {
                false_or_valid += 1;
            }
        }
        assert_eq!(
            false_or_valid, 0,
            "pure noise produced {false_or_valid} converged+CRC-OK decodes (false decodes)"
        );
    }

    /// Determinism: running the sweep twice with the same seed yields identical
    /// P(decode) values and identical false-decode counts. Pins reproducibility.
    #[test]
    fn sweep_is_deterministic() {
        let codewords = distinct_codewords(8, 0x0102_0304_0506_0708);
        let snr_points = [-17.0f64, -19.0, -21.0];
        let n_trials = 40usize;
        let seed = 0x5555_AAAA_5555_AAAA;

        let (p1, f1) = run_sweep(&snr_points, &codewords, n_trials, seed);
        let (p2, f2) = run_sweep(&snr_points, &codewords, n_trials, seed);
        assert_eq!(p1, p2, "sweep P(decode) not reproducible across identical-seed runs");
        assert_eq!(f1, f2, "sweep false-decode count not reproducible");

        // And the codeword corpus itself is deterministic given its seed.
        let cw2 = distinct_codewords(8, 0x0102_0304_0506_0708);
        assert_eq!(codewords, cw2, "distinct_codewords not deterministic for a fixed seed");
    }

    /// The noncoherent-MFSK Pe formula is sane: monotonically decreasing in γ,
    /// bounded in `[0, 1)`, and at γ → 0 approaches `(M-1)/M` (a uniform random
    /// guess among M tones errs with probability `(M-1)/M`). Pins the closed
    /// form independent of the model, so the calibration test compares two
    /// independently-trusted quantities.
    #[test]
    fn pe_formula_is_sane() {
        // Bounded and decreasing.
        let mut prev = 1.0f64;
        for &db in &[-3.0f64, 0.0, 3.0, 6.0, 9.0, 12.0] {
            let g = 10.0f64.powf(db / 10.0);
            let pe = noncoherent_mfsk_pe(g);
            assert!((0.0..1.0).contains(&pe), "Pe {pe} out of [0,1) at {db} dB");
            assert!(pe <= prev + 1e-9, "Pe not decreasing at {db} dB ({pe} > {prev})");
            prev = pe;
        }
        // γ → 0: Pe → (M-1)/M = 7/8 (uniform-guess error rate). Use a tiny γ.
        let pe0 = noncoherent_mfsk_pe(1e-6);
        assert!(
            (pe0 - 7.0 / 8.0).abs() < 1e-3,
            "Pe(γ→0) {pe0} != 7/8 (uniform-guess floor)"
        );
        // Binomial coefficient sanity.
        assert_eq!(binomial(7, 0), 1);
        assert_eq!(binomial(7, 1), 7);
        assert_eq!(binomial(7, 3), 35);
        assert_eq!(binomial(7, 7), 1);
    }
}
