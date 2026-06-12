//! BER vs. SNR sweep for each PHY mode across an AWGN channel.
//!
//! Run: `cargo run --release --example ber_vs_snr_sweep -p sonde-phy`
//!
//! Until subsystem #1's `hf-channel-sim` crate is wired in via the
//! `tests/sim_adapter.rs` follow-up, this example runs against AWGN
//! only (a placeholder). When #1 lands the swap is one line:
//! `awgn_channel` → `hf_channel_sim::Channel::watterson(...)`.

use sonde_phy::audio_io::SAMPLE_RATE_HZ;
use sonde_phy::ofdm_main::ofdm_params::{OfdmModeName, OfdmParams};
use sonde_phy::ofdm_main::receiver::OfdmReceiver;
use sonde_phy::ofdm_main::transmitter::OfdmTransmitter;
use sonde_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor;

fn main() {
    println!("# sonde-phy BER vs SNR sweep");
    println!("# sample_rate_hz = {SAMPLE_RATE_HZ}");
    println!("mode,snr_db,ber");

    for mode in [OfdmModeName::Narrow, OfdmModeName::Mid, OfdmModeName::Wide] {
        let params = OfdmParams::for_mode(mode);
        let bits_per_sc = vec![2u8; params.subcarrier_indices().len()];
        for snr_db in (-5..=30).step_by(5) {
            let ber = sweep_ofdm(&params, &bits_per_sc, snr_db as f32);
            println!("ofdm-{mode:?},{snr_db},{ber:.4}");
        }
    }
    let floor = WidebandLowDensityFloor::new();
    for snr_db in (-15..=10).step_by(5) {
        let ber = sweep_floor(&floor, snr_db as f32);
        println!("floor-wblo,{snr_db},{ber:.4}");
    }
}

fn awgn_channel(signal: &[f32], snr_db: f32, seed: u64) -> Vec<f32> {
    let n = signal.len();
    let signal_power: f32 = signal.iter().map(|s| s * s).sum::<f32>() / n.max(1) as f32;
    let snr_lin = 10.0_f32.powf(snr_db / 10.0);
    let noise_var = signal_power / snr_lin.max(1e-9);
    let std = noise_var.sqrt();
    let mut state = seed;
    let mut next = move || -> f32 {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let r = (state >> 11) as f32 / (1u64 << 53) as f32;
        // Box-Muller would be better; this is a quick uniform for the
        // characterization stub.
        (r - 0.5) * 2.0 * std * (3.0_f32).sqrt()
    };
    signal.iter().map(|s| s + next()).collect()
}

fn sweep_ofdm(params: &OfdmParams, bits_per_sc: &[u8], snr_db: f32) -> f32 {
    let tx = OfdmTransmitter::new(params);
    let rx = OfdmReceiver::new(params);
    let n_data_bits: usize = bits_per_sc
        .iter()
        .enumerate()
        .filter(|(i, _)| !params.pilot_indices().contains(&params.subcarrier_indices()[*i]))
        .map(|(_, b)| *b as usize)
        .sum();
    let mut errors = 0usize;
    let mut total = 0usize;
    for trial in 0..20 {
        let payload_bits: Vec<u8> = (0..n_data_bits).map(|i| ((i + trial) % 2) as u8).collect();
        let samples = tx.modulate_one_symbol(&payload_bits, bits_per_sc);
        let impaired = awgn_channel(&samples, snr_db, trial as u64);
        let llrs = rx.demodulate_one_symbol(&impaired, bits_per_sc);
        let recovered: Vec<u8> = llrs
            .iter()
            .map(|l| if *l >= 0.0 { 0 } else { 1 })
            .collect();
        for (a, b) in recovered.iter().zip(payload_bits.iter()) {
            if a != b {
                errors += 1;
            }
            total += 1;
        }
    }
    errors as f32 / total.max(1) as f32
}

fn sweep_floor(floor: &WidebandLowDensityFloor, snr_db: f32) -> f32 {
    let mut errors = 0usize;
    let mut total = 0usize;
    for trial in 0..20 {
        let payload = vec![(trial as u8) ^ 0xA5; 8];
        let samples = floor.transmit(&payload).unwrap();
        let impaired = awgn_channel(&samples, snr_db, trial as u64);
        match floor.receive(&impaired) {
            Ok(recovered) => {
                let n = recovered.len().min(payload.len());
                for i in 0..n {
                    let xor = recovered[i] ^ payload[i];
                    errors += xor.count_ones() as usize;
                    total += 8;
                }
            }
            Err(_) => {
                errors += payload.len() * 8;
                total += payload.len() * 8;
            }
        }
    }
    errors as f32 / total.max(1) as f32
}
