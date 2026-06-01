//! Phase 10.3 acceptance gate: synthesize a 300-2700 Hz audio-band
//! brick-wall filter and apply it to OFDM-Wide TX samples; the RX side
//! must still decode at < 5 % BER. Passband-fit guard per PHY spec §3
//! forcing function 1 (FT-818-class SSB front-end).

use num_complex::Complex;
use rustfft::FftPlanner;
use tuxmodem_phy::audio_io::SAMPLE_RATE_HZ;
use tuxmodem_phy::ofdm_main::ofdm_params::{OfdmModeName, OfdmParams};
use tuxmodem_phy::ofdm_main::receiver::OfdmReceiver;
use tuxmodem_phy::ofdm_main::transmitter::OfdmTransmitter;

#[test]
fn wide_mode_survives_300_2700_hz_brickwall() {
    let params = OfdmParams::for_mode(OfdmModeName::Wide);
    let tx = OfdmTransmitter::new(&params);
    let bits_per_sc = vec![2u8; params.subcarrier_indices().len()];
    let n_data_bits: usize = bits_per_sc
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            !params
                .pilot_indices()
                .contains(&params.subcarrier_indices()[*i])
        })
        .map(|(_, b)| *b as usize)
        .sum();
    let payload_bits: Vec<u8> = (0..n_data_bits).map(|i| (i % 2) as u8).collect();
    let samples = tx.modulate_one_symbol(&payload_bits, &bits_per_sc);

    let filtered = brickwall_filter(&samples, SAMPLE_RATE_HZ as f32, 300.0, 2700.0);
    // The brick-wall FFT round-trip pads to the next power of two; trim
    // back to the original sample count for the RX symbol-length check.
    let filtered = &filtered[..samples.len()];
    let rx = OfdmReceiver::new(&params);
    let llrs = rx.demodulate_one_symbol(filtered, &bits_per_sc);
    let recovered: Vec<u8> = llrs
        .iter()
        .map(|l| if *l >= 0.0 { 0 } else { 1 })
        .collect();
    let ber = bit_error_rate(&recovered, &payload_bits);
    assert!(
        ber < 0.05,
        "BER {ber} too high under FT-818 passband proxy"
    );
}

fn brickwall_filter(samples: &[f32], sr_hz: f32, lo_hz: f32, hi_hz: f32) -> Vec<f32> {
    let n = samples.len().next_power_of_two();
    let mut buf: Vec<Complex<f32>> = samples.iter().map(|s| Complex::new(*s, 0.0)).collect();
    buf.resize(n, Complex::new(0.0, 0.0));
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    fft.process(&mut buf);
    let bin_w = sr_hz / n as f32;
    for (i, c) in buf.iter_mut().enumerate() {
        // Mirror at Nyquist
        let f = if i <= n / 2 {
            i as f32 * bin_w
        } else {
            (n - i) as f32 * bin_w
        };
        if f < lo_hz || f > hi_hz {
            *c = Complex::new(0.0, 0.0);
        }
    }
    ifft.process(&mut buf);
    let scale = 1.0 / n as f32;
    buf.iter().take(samples.len()).map(|c| c.re * scale).collect()
}

fn bit_error_rate(a: &[u8], b: &[u8]) -> f32 {
    let n = a.len().min(b.len());
    let errors: usize = a
        .iter()
        .zip(b.iter())
        .take(n)
        .filter(|(x, y)| x != y)
        .count();
    errors as f32 / n.max(1) as f32
}
