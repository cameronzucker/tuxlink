use tuxmodem_phy::audio_io::{AudioBuffer, SAMPLE_RATE_HZ};

#[test]
fn sample_rate_is_pinned_at_48khz() {
    assert_eq!(SAMPLE_RATE_HZ, 48_000);
}

#[test]
fn audio_buffer_round_trips_to_wav_and_back() {
    let tmp = std::env::temp_dir().join("tuxmodem-phy-test-audio.wav");
    let original: Vec<f32> = (0..480).map(|i| (i as f32 * 0.01).sin()).collect();
    let buf = AudioBuffer::from_samples(original.clone());
    buf.write_wav(&tmp).expect("write");
    let loaded = AudioBuffer::read_wav(&tmp).expect("read");
    assert_eq!(loaded.samples().len(), original.len());
    for (a, b) in loaded.samples().iter().zip(original.iter()) {
        assert!((a - b).abs() < 1e-4, "wav round-trip diverges: {a} vs {b}");
    }
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn buffer_duration_at_48k_is_correct() {
    let buf = AudioBuffer::from_samples(vec![0.0; 48_000]);
    assert!((buf.duration_seconds() - 1.0).abs() < 1e-6);
}
