//! Task 6.3 acceptance test: clean-channel TX→RX OFDM round-trip
//! must recover transmitted bits exactly (zero BER on a noiseless,
//! unitary channel).

use sonde_phy::ofdm_main::ofdm_params::{OfdmModeName, OfdmParams};
use sonde_phy::ofdm_main::receiver::OfdmReceiver;
use sonde_phy::ofdm_main::transmitter::OfdmTransmitter;

#[test]
fn ofdm_round_trip_clean_channel_zero_ber() {
    let params = OfdmParams::for_mode(OfdmModeName::Mid);
    let tx = OfdmTransmitter::new(&params);
    let bits_per_sc = vec![2u8; params.subcarrier_indices().len()];

    // Total payload bits = sum of bpc over non-pilot sub-carriers.
    let payload_bit_len: usize = bits_per_sc
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            !params
                .pilot_indices()
                .contains(&params.subcarrier_indices()[*i])
        })
        .map(|(_, bpc)| *bpc as usize)
        .sum();
    let payload_bits: Vec<u8> = (0..payload_bit_len).map(|i| (i % 2) as u8).collect();

    let samples = tx.modulate_one_symbol(&payload_bits, &bits_per_sc);
    let rx = OfdmReceiver::new(&params);
    let recovered_llr = rx.demodulate_one_symbol(&samples, &bits_per_sc);
    // LLR convention (PHY spec R2): positive ⇒ bit=0, negative ⇒ bit=1.
    let recovered_bits: Vec<u8> = recovered_llr
        .iter()
        .map(|l| if *l >= 0.0 { 0 } else { 1 })
        .collect();
    assert_eq!(
        recovered_bits, payload_bits,
        "clean-channel round-trip must be lossless"
    );
}
