//! Task 6.2 acceptance test: OFDM TX emits the expected sample count
//! (FFT body + CP) for a Mid-mode QPSK symbol.

use sonde_phy::ofdm_main::ofdm_params::{OfdmModeName, OfdmParams};
use sonde_phy::ofdm_main::transmitter::OfdmTransmitter;

#[test]
fn ofdm_tx_emits_expected_sample_count() {
    let params = OfdmParams::for_mode(OfdmModeName::Mid);
    let tx = OfdmTransmitter::new(&params);
    let n_data = params.data_indices().len();
    let bits = vec![0u8; n_data * 2]; // QPSK = 2 bits / sub-carrier
    let bits_per_sc = vec![2u8; params.subcarrier_indices().len()];
    let samples = tx.modulate_one_symbol(&bits, &bits_per_sc);
    let expected = params.fft_size() + params.cp_len();
    assert_eq!(samples.len(), expected);
}
