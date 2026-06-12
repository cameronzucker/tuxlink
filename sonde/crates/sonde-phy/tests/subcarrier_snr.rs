use num_complex::Complex;
use sonde_phy::subcarrier_snr::SubcarrierSnrEstimator;

#[test]
fn pilot_aided_estimator_returns_per_bin_snr_vector() {
    // 64-bin FFT; fill with unit pilot symbols + per-bin additive Gaussian noise.
    let n_bins = 64;
    let mut rng_state = 0xC0FFEEu32;
    let mut noise = || -> f32 {
        rng_state = rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((rng_state >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.2
    };
    let pilots: Vec<Complex<f32>> = (0..n_bins).map(|_| Complex::new(1.0, 0.0)).collect();
    let received: Vec<Complex<f32>> = pilots
        .iter()
        .map(|p| Complex::new(p.re + noise(), p.im + noise()))
        .collect();

    let estimator = SubcarrierSnrEstimator::new(n_bins);
    let per_bin_snr_db = estimator.estimate_from_pilots(&received, &pilots);
    assert_eq!(per_bin_snr_db.len(), n_bins);
    for snr in &per_bin_snr_db {
        assert!(
            *snr > 10.0 && *snr < 50.0,
            "SNR {} out of plausible range",
            snr
        );
    }
}
