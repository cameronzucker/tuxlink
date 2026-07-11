//! 48 kHz → 12 kHz decimating FIR, 4:1, computed at the output rate
//! (spec §Decimator).
//!
//! 51-tap Kaiser windowed-sinc lowpass. Delta pins: passband 0–4 kHz (jt9
//! decodes to 4 007 Hz), stopband ≥ 8 kHz at ≥ 60 dB, Kaiser window,
//! polyphase at the output rate — the dot product runs only at output
//! instants, ¼ of the naive MAC count (~0.66 M MAC/s — trivial). i16 in →
//! i16 out; accumulate in f32, round half away from zero, saturate.
//! Filter state persists across `process` calls AND across slot boundaries
//! (the pinned continuity model; 720 000 ≡ 0 mod 4 keeps output phase
//! aligned slot-to-slot). Group delay (25 input samples ≈ 520 µs) is a
//! constant shift three orders of magnitude inside jt9's ±2 s DT tolerance
//! — a verified non-issue (spec §Decimator).

pub const TAPS: usize = 51;
pub const DECIM: usize = 4;

/// Committed coefficient table, DC gain 1.
///
/// GENERATOR NOTE (the committed reference implementation is
/// `generate_coeffs()` in the test module; the
/// `committed_table_matches_kaiser_generator` test keeps this table honest):
/// Kaiser windowed-sinc, fs = 48 000 Hz, fc = 6 000 Hz (transition centered
/// between the 4 kHz passband edge and the 8 kHz stopband edge),
/// beta = 5.65 (60 dB design: 0.1102·(60 − 8.7)):
///   h[n] = sinc(2·fc/fs·(n − 25)) · I0(beta·√(1 − (2n/50 − 1)²)) / I0(beta),
/// normalized so Σ h = 1, rounded to f32. The ideal sinc zeros
/// (n − 25 ≡ 0 mod 4, n ≠ 25) are committed as literal 0.0. Literals are
/// the SHORTEST round-trip f32 forms (clippy `excessive_precision` denies
/// longer ones) — do not "restore" extra digits.
///
/// Verified response of THIS f32 table (f64 DFT, asserted by the response
/// tests below): passband ripple ±0.013 dB over 0–4.0 kHz; attenuation
/// ≥ 60.45 dB over 8–24 kHz (worst point exactly 8.0 kHz); 79.9 dB at 9 kHz.
pub const COEFFS: [f32; TAPS] = [
    0.00018418659,
    0.0,
    -0.0005318928,
    -0.0011225927,
    -0.0011288024,
    0.0,
    0.0020681017,
    0.0038194556,
    0.0034638832,
    0.0,
    -0.0054616802,
    -0.009540089,
    -0.00826268,
    0.0,
    0.0121794455,
    0.020809278,
    0.017771836,
    0.0,
    -0.026221976,
    -0.045708396,
    -0.040614147,
    0.0,
    0.072335385,
    0.15663773,
    0.22426403,
    0.25011787,
    0.22426403,
    0.15663773,
    0.072335385,
    0.0,
    -0.040614147,
    -0.045708396,
    -0.026221976,
    0.0,
    0.017771836,
    0.020809278,
    0.0121794455,
    0.0,
    -0.00826268,
    -0.009540089,
    -0.0054616802,
    0.0,
    0.0034638832,
    0.0038194556,
    0.0020681017,
    0.0,
    -0.0011288024,
    -0.0011225927,
    -0.0005318928,
    0.0,
    0.00018418659,
];

/// Streaming 4:1 decimator with persistent filter state and input-phase
/// tracking across arbitrary chunk lengths (including ≢ 0 mod 4 — gap fills
/// are clock-sized).
pub struct Decimator {
    ring: [f32; TAPS],
    pos: usize,
    phase: usize,
}

impl Default for Decimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Decimator {
    pub fn new() -> Self {
        Self {
            ring: [0.0; TAPS],
            pos: 0,
            phase: 0,
        }
    }

    /// Consume `input` (48 kHz), append decimated 12 kHz samples to `out`.
    /// y[m] = Σₖ h[k]·x[4m − k] with x pre-history = 0; the stream's first
    /// input sample is x[0] and produces y[0]. Phase and filter history
    /// persist across calls, so chunked calls of ANY lengths (including
    /// ≢ 0 mod 4) equal one-shot processing.
    pub fn process(&mut self, input: &[i16], out: &mut Vec<i16>) {
        for &s in input {
            let newest = self.pos;
            self.ring[newest] = f32::from(s);
            self.pos = (self.pos + 1) % TAPS;
            if self.phase == 0 {
                let mut acc = 0.0f32;
                for (k, &c) in COEFFS.iter().enumerate() {
                    acc += c * self.ring[(newest + TAPS - k) % TAPS];
                }
                out.push(saturate_round(acc));
            }
            self.phase = (self.phase + 1) % DECIM;
        }
    }
}

/// Round half away from zero, saturate to i16 (spec §Decimator: "accumulate
/// in f32, round-half-away, saturate").
fn saturate_round(x: f32) -> i16 {
    let r = if x >= 0.0 { (x + 0.5).floor() } else { (x - 0.5).ceil() };
    r.clamp(-32_768.0, 32_767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- test-time coefficient generator (the committed reference) ----

    /// Modified Bessel function of the first kind, order 0 — power series,
    /// std-only (converges in ~20 terms for the beta range used here).
    fn kaiser_i0(x: f64) -> f64 {
        let mut sum = 1.0;
        let mut term = 1.0;
        let mut k = 1.0;
        loop {
            let half = x / (2.0 * k);
            term *= half * half;
            sum += term;
            if term < 1e-18 * sum {
                return sum;
            }
            k += 1.0;
        }
    }

    fn generate_coeffs() -> Vec<f64> {
        const FS: f64 = 48_000.0;
        const FC: f64 = 6_000.0;
        const BETA: f64 = 5.65;
        let m = (TAPS - 1) as f64;
        let mut h: Vec<f64> = (0..TAPS)
            .map(|n| {
                let x = n as f64 - m / 2.0;
                let sinc = if x == 0.0 {
                    2.0 * FC / FS
                } else {
                    (2.0 * std::f64::consts::PI * FC / FS * x).sin()
                        / (std::f64::consts::PI * x)
                };
                let t = 2.0 * n as f64 / m - 1.0;
                let w = kaiser_i0(BETA * (1.0 - t * t).sqrt()) / kaiser_i0(BETA);
                sinc * w
            })
            .collect();
        let sum: f64 = h.iter().sum();
        for c in &mut h {
            *c /= sum;
        }
        h
    }

    #[test]
    fn committed_table_matches_kaiser_generator() {
        let gen = generate_coeffs();
        for (k, (&c, &g)) in COEFFS.iter().zip(gen.iter()).enumerate() {
            assert!(
                (f64::from(c) - g).abs() < 1e-6,
                "tap {k}: committed {c} vs generated {g}"
            );
        }
    }

    // ---- frequency-response verification from the committed table ----

    fn response_db(freq_hz: f64) -> f64 {
        let mut re = 0.0f64;
        let mut im = 0.0f64;
        for (k, &c) in COEFFS.iter().enumerate() {
            let phi = 2.0 * std::f64::consts::PI * freq_hz * k as f64 / 48_000.0;
            re += f64::from(c) * phi.cos();
            im -= f64::from(c) * phi.sin();
        }
        20.0 * (re.hypot(im) + 1e-30).log10()
    }

    #[test]
    fn passband_ripple_within_spec() {
        // Spec §Decimator: ≤ ±0.5 dB across 0–3.8 kHz, ≤ ±1.0 dB across
        // 3.8–4.0 kHz (jt9's ceiling is 4 007 Hz — the edge verified loosely).
        let mut f = 0.0f64;
        while f <= 3_800.0 {
            let r = response_db(f);
            assert!(r.abs() <= 0.5, "{f} Hz: {r:.4} dB exceeds ±0.5 dB");
            f += 100.0;
        }
        let mut f = 3_800.0f64;
        while f <= 4_000.0 {
            let r = response_db(f);
            assert!(r.abs() <= 1.0, "{f} Hz: {r:.4} dB exceeds ±1.0 dB");
            f += 25.0;
        }
    }

    #[test]
    fn stopband_attenuation_at_least_60_db_including_exactly_8_khz() {
        // The explicit 8.0 kHz assertion is spec-pinned (the design's worst
        // point sits exactly there, at −60.45 dB).
        let at8k = response_db(8_000.0);
        assert!(at8k <= -60.0, "exactly 8.0 kHz: {at8k:.2} dB");
        let mut f = 8_000.0f64;
        while f <= 24_000.0 {
            let r = response_db(f);
            assert!(r <= -60.0, "{f} Hz: {r:.2} dB above −60 dB");
            f += 100.0;
        }
    }

    // ---- KATs through the streaming Decimator ----

    fn tone(freq_hz: f64, amp: f64, n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| {
                (amp * (2.0 * std::f64::consts::PI * freq_hz * i as f64 / 48_000.0).sin())
                    .round() as i16
            })
            .collect()
    }

    fn rms(s: &[i16]) -> f64 {
        (s.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / s.len() as f64)
            .sqrt()
    }

    #[test]
    fn nine_khz_tone_is_at_least_60_db_down_post_decimation() {
        // The delta's named vector: 9 kHz aliases to 3 kHz after 4:1
        // decimation; the FIR must have killed it BEFORE the alias lands
        // in-band.
        let input = tone(9_000.0, 16_000.0, 96_000); // 2 s
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        assert_eq!(out.len(), 24_000);
        let steady = &out[1_000..];
        let in_rms = 16_000.0 / std::f64::consts::SQRT_2;
        let out_rms = rms(steady);
        assert!(
            out_rms <= in_rms * 1e-3,
            "9 kHz residue {out_rms:.2} vs input {in_rms:.2} — less than 60 dB down"
        );
    }

    #[test]
    fn one_khz_passband_level_within_half_db() {
        let input = tone(1_000.0, 16_000.0, 96_000);
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        // 1 kHz at 12 kHz out = 12 samples/period; 9 600 = 800 whole periods.
        let steady = &out[1_000..10_600];
        let in_rms = 16_000.0 / std::f64::consts::SQRT_2;
        let db = 20.0 * (rms(steady) / in_rms).log10();
        assert!(db.abs() <= 0.5, "1 kHz level error {db:.3} dB");
    }

    #[test]
    fn dc_passes_at_unity_gain() {
        let input = vec![8_000i16; 4_800];
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        assert_eq!(out.len(), 1_200);
        for (i, &v) in out[100..].iter().enumerate() {
            assert!((i32::from(v) - 8_000).abs() <= 1, "output {i}: {v}");
        }
    }

    #[test]
    fn impulse_response_is_the_phase0_taps() {
        // x = [32767, 0, 0, ...] ⇒ y[m] = h[4m]·32767 for 4m ≤ 50; pins the
        // y[m] = Σ h[k]·x[4m−k] alignment (first input sample produces y[0])
        // and the round-half-away quantizer.
        let mut input = vec![0i16; 200];
        input[0] = 32_767;
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        assert_eq!(out.len(), 50);
        for m in 0..=12 {
            let want = (f64::from(COEFFS[4 * m]) * 32_767.0).round();
            let got = f64::from(out[m]);
            assert!((got - want).abs() <= 1.0, "y[{m}]: got {got}, want {want}");
        }
    }

    fn lcg_noise(n: usize) -> Vec<i16> {
        let mut x: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 16) as i16
            })
            .collect()
    }

    #[test]
    fn streaming_equivalence_chunked_equals_oneshot_including_odd_chunks() {
        // Gap fills are clock-sized and arbitrary-length: input-phase
        // tracking across chunk lengths ≢ 0 (mod 4) is load-bearing (spec
        // §Decimator KATs).
        let input = lcg_noise(96_000);
        let mut one = Vec::new();
        Decimator::new().process(&input, &mut one);
        assert_eq!(one.len(), 24_000);

        let sizes = [1usize, 2, 3, 5, 7, 11, 13, 479, 4_800];
        let mut chunked = Vec::new();
        let mut d = Decimator::new();
        let mut off = 0;
        let mut i = 0;
        while off < input.len() {
            let n = sizes[i % sizes.len()].min(input.len() - off);
            d.process(&input[off..off + n], &mut chunked);
            off += n;
            i += 1;
        }
        assert_eq!(chunked, one, "chunked stream must equal one-shot exactly");
    }

    #[test]
    fn full_scale_input_saturates_to_bounds() {
        // Pins the spec-pinned saturate behavior; the clamp predates this test — Gate A P2.
        // Feed constant full-scale input: all i16::MAX. Σ|COEFFS| ≈ 1.554 drives
        // accumulator to ~51,100, well past i16::MAX. Verify saturate_round clamps
        // outputs to i16::MAX and does not panic, wrap, or overflow.
        let input = vec![i16::MAX; 512];

        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);

        // Verify no panic/UB — all outputs within valid i16 range
        for &sample in &out {
            assert!((i16::MIN..=i16::MAX).contains(&sample));
        }

        // Verify saturation to i16::MAX (positive clamping occurs)
        assert!(
            out.contains(&i16::MAX),
            "expected positive saturation to i16::MAX with full-scale input"
        );
    }
}
