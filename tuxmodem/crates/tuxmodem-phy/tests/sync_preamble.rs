use tuxmodem_phy::audio_io::SAMPLE_RATE_HZ;
use tuxmodem_phy::sync::preamble::{PreambleDetector, PreambleGenerator};

#[test]
fn preamble_self_correlation_peaks_at_known_offset() {
    let gen = PreambleGenerator::new();
    let preamble = gen.generate();
    let mut signal = vec![0.0_f32; 4_800]; // 100 ms of silence at 48 kHz
    let insertion = 1_200; // 25 ms in
    for (i, s) in preamble.iter().enumerate() {
        signal[insertion + i] += *s;
    }
    let detector = PreambleDetector::new();
    let detection = detector.scan(&signal).expect("should detect");
    assert!(
        (detection.start_sample as i64 - insertion as i64).abs() < 32,
        "detection {} not within 32 samples of insertion {}",
        detection.start_sample,
        insertion,
    );
    assert!(detection.snr_estimate_db > 10.0);
    let _ = SAMPLE_RATE_HZ; // assert compile dependency on sample rate
}

#[test]
fn preamble_is_not_falsely_detected_in_noise() {
    use rand::prelude::*;
    let mut rng = StdRng::seed_from_u64(0xC0DE);
    let signal: Vec<f32> = (0..48_000).map(|_| rng.gen_range(-0.1..0.1)).collect();
    let detector = PreambleDetector::new();
    let detection = detector.scan(&signal);
    assert!(detection.is_none(), "false detection in pure noise");
}
