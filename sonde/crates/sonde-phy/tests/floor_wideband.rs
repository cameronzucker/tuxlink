//! Phase 8 acceptance tests: wide-band low-density-constellation OFDM
//! floor mode.

use sonde_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor;

#[test]
fn floor_uses_bpsk_on_every_subcarrier() {
    let floor = WidebandLowDensityFloor::new();
    let params = floor.params();
    let bits_per_sc = floor.bits_per_subcarrier();
    assert_eq!(bits_per_sc.len(), params.subcarrier_indices().len());
    for &b in &bits_per_sc {
        assert_eq!(b, 1, "wide-band low-density floor is BPSK per sub-carrier");
    }
}

#[test]
fn floor_bandwidth_is_max_passband_2300hz() {
    let floor = WidebandLowDensityFloor::new();
    let params = floor.params();
    let sr = 48_000.0_f32;
    let bin_w = sr / params.fft_size() as f32;
    let lowest_hz = *params.subcarrier_indices().first().unwrap() as f32 * bin_w;
    let highest_hz = *params.subcarrier_indices().last().unwrap() as f32 * bin_w;
    let bandwidth = highest_hz - lowest_hz;
    assert!(
        bandwidth >= 2000.0,
        "wideband floor should occupy >= 2 kHz, got {bandwidth} Hz"
    );
    assert!(highest_hz <= 2700.0, "must fit FT-818 SSB passband");
    assert!(lowest_hz >= 200.0);
}

#[test]
fn floor_clean_channel_round_trip() {
    let floor = WidebandLowDensityFloor::new();
    // Single-symbol capacity = 9 bytes at Wide-mode BPSK
    // (74 data sub-carriers / 8). Plan-doc text used "FLOOR-MODE-TEST"
    // (15 bytes), which trips PayloadTooLarge; the 9-byte payload here
    // is the right size for the Phase 8 single-symbol contract.
    let payload = b"FLOORMODE";
    let samples = floor.transmit(payload).expect("tx");
    let recovered = floor.receive(&samples).expect("rx");
    assert_eq!(recovered.as_slice(), payload);
}
