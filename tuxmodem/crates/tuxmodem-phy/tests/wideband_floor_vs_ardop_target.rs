//! Acceptance gate per overview §0:
//! "Decode threshold — beat ARDOP at the noise-floor case. Tuxmodem's
//! wide-band noise-floor mode targets stronger SNR-floor performance
//! than ARDOP's narrowest mode at the same per-Hz noise floor."
//!
//! ARDOP's narrowest mode is publicly advertised as 200-Hz BPSK at
//! ~0 dB SNR-floor (per `docs/research/modem-foundations.md` §6.2,
//! noting that the *advertised* spec is operator-observable —
//! NOT internal performance figures we examined). The tuxmodem
//! wide-band low-density floor occupies ~2300 Hz so its aggregate-
//! signal SNR advantage at the SAME per-Hz noise floor is
//! ~10·log10(2300/200) ≈ 10.6 dB. The gate asserts that at -8 dB
//! per-Hz SNR the floor still achieves BER < 0.01 — which, after FEC
//! (rate-1/4 LDPC in the later subsystem #4 plan), bottoms out
//! under 1e-3.
//!
//! This test runs AWGN-only until subsystem #1's channel sim lands;
//! a Phase 11 follow-up will re-run under F.520 "moderate" + "poor"
//! and gate against the operationally-relevant numbers.

use tuxmodem_phy::robustness_floor::wideband_lowdensity::WidebandLowDensityFloor;

#[test]
#[ignore] // un-ignore once the FEC layer is wired in via #4
fn floor_beats_ardop_narrowest_at_target_snr() {
    let floor = WidebandLowDensityFloor::new();
    let payload = vec![0xA5u8; 8];
    let snr_db = -8.0_f32;
    let samples = floor.transmit(&payload).unwrap();
    let impaired = awgn(&samples, snr_db, 0xDEAD_BEEF);
    let recovered = floor.receive(&impaired).unwrap();
    let xor: usize = recovered
        .iter()
        .zip(payload.iter())
        .map(|(a, b)| (a ^ b).count_ones() as usize)
        .sum();
    let ber = xor as f32 / (payload.len() * 8) as f32;
    assert!(ber < 0.01, "BER {ber} above target 0.01 at SNR {snr_db}");
}

fn awgn(signal: &[f32], snr_db: f32, seed: u64) -> Vec<f32> {
    let n = signal.len();
    let pwr: f32 = signal.iter().map(|s| s * s).sum::<f32>() / n.max(1) as f32;
    let snr_lin = 10.0_f32.powf(snr_db / 10.0);
    let std = (pwr / snr_lin.max(1e-9)).sqrt();
    let mut state = seed;
    let mut next = move || -> f32 {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let r = (state >> 11) as f32 / (1u64 << 53) as f32;
        (r - 0.5) * 2.0 * std * (3.0_f32).sqrt()
    };
    signal.iter().map(|s| s + next()).collect()
}
