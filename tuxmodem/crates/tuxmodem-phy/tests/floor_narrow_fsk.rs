//! Phase 9 acceptance tests: 8-FSK situational floor mode.

use tuxmodem_phy::robustness_floor::narrow_fsk::NarrowFskFloor;

#[test]
fn narrow_fsk_round_trip_clean_channel() {
    let floor = NarrowFskFloor::new();
    let payload = b"FSK-OK";
    let samples = floor.transmit(payload).expect("tx");
    let recovered = floor.receive(&samples).expect("rx");
    assert_eq!(recovered.as_slice(), payload);
}

#[test]
fn narrow_fsk_bandwidth_fits_crowded_band_slot() {
    let floor = NarrowFskFloor::new();
    let bw_hz = floor.occupied_bandwidth_hz();
    assert!(
        bw_hz < 500.0,
        "narrow-FSK situational mode must fit a crowded-band slot <= 500 Hz"
    );
}
