//! Phase 7 acceptance tests: water-filling bit-loader behavior.

use sonde_phy::ofdm_main::bit_loader::WaterfillingBitLoader;

#[test]
fn high_snr_subcarriers_get_more_bits_than_low_snr() {
    // 16 sub-carriers; first 8 high-SNR, last 8 low-SNR.
    let snr_db: Vec<f32> = (0..16)
        .map(|i| if i < 8 { 30.0 } else { 5.0 })
        .collect();
    let loader = WaterfillingBitLoader::new();
    let bits = loader.allocate(&snr_db, 6); // cap at 64-QAM
    assert!(bits.iter().take(8).sum::<u8>() > bits.iter().skip(8).sum::<u8>());
}

#[test]
fn below_threshold_subcarriers_get_zero_bits() {
    let snr_db: Vec<f32> = vec![-10.0, -5.0, 0.0, 10.0, 20.0, 30.0];
    let loader = WaterfillingBitLoader::new();
    let bits = loader.allocate(&snr_db, 6);
    // Below ~3 dB no constellation yields useful BER even with FEC.
    assert_eq!(bits[0], 0);
    assert_eq!(bits[1], 0);
    assert!(bits[5] > 0);
}

#[test]
fn allocation_caps_at_max_bits_per_subcarrier() {
    let snr_db: Vec<f32> = vec![60.0; 4];
    let loader = WaterfillingBitLoader::new();
    let bits = loader.allocate(&snr_db, 4);
    for &b in &bits {
        assert!(b <= 4);
    }
}
