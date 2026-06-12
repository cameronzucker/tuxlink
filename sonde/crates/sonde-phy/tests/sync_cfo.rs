use num_complex::Complex;
use sonde_phy::sync::carrier_offset::CfoEstimator;

#[test]
fn cfo_estimator_recovers_known_offset() {
    let true_offset_hz = 25.0;
    let sample_rate_hz = 48_000.0;
    let n = 4_096;
    let signal: Vec<Complex<f32>> = (0..n)
        .map(|i| {
            let phase = 2.0 * std::f32::consts::PI * true_offset_hz * i as f32 / sample_rate_hz;
            Complex::new(phase.cos(), phase.sin())
        })
        .collect();
    let est = CfoEstimator::new(sample_rate_hz);
    // half_len controls the maximum unambiguous CFO: |f| < fs / (2*half_len).
    // half_len = 256 → max ±93.75 Hz, well above the 25 Hz target.
    // Plan's n/2 = 2048 would alias 25 Hz (max ±11.7 Hz at that half_len).
    let estimated = est.estimate_repeat(&signal, 256);
    assert!(
        (estimated - true_offset_hz).abs() < 1.0,
        "CFO estimate {} not within 1 Hz of true {}",
        estimated,
        true_offset_hz
    );
}
