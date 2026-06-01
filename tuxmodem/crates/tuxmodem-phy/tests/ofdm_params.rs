//! Task 6.1 acceptance tests: OFDM mode descriptor table sanity-checks.

use tuxmodem_phy::ofdm_main::ofdm_params::{OfdmModeName, OfdmParams};

#[test]
fn ofdm_mid_mode_params_round_trip() {
    let params = OfdmParams::for_mode(OfdmModeName::Mid);
    assert!(params.fft_size().is_power_of_two());
    assert!(params.cp_len() > 0);
    assert!(!params.subcarrier_indices().is_empty());
    // Audio-band placement: sub-carriers must sit between ~200 and
    // ~2700 Hz given 48 kHz sample rate and FT-818 SSB passband.
    let sr = 48_000.0_f32;
    let fft = params.fft_size() as f32;
    for &idx in params.subcarrier_indices() {
        let f = idx as f32 * sr / fft;
        assert!(
            (200.0..=2700.0).contains(&f),
            "sub-carrier at {f} Hz out of audio band"
        );
    }
}

#[test]
fn all_three_ofdm_modes_have_descriptors() {
    for m in [OfdmModeName::Narrow, OfdmModeName::Mid, OfdmModeName::Wide] {
        let _ = OfdmParams::for_mode(m);
    }
}
