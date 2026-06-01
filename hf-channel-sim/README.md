# hf-channel-sim

Watterson-class HF ionospheric channel simulator. Pure-Rust, deterministic, AGPLv3-only.

## Status

Pre-1.0. API is unstable until 0.5.

## What it does

Simulates the HF ionospheric channel between two amateur radio stations, applying time-varying multipath fading + Doppler spread per the Watterson (1970) model and ITU-R F.520 parameter sets ("Good", "Moderate", "Poor", "Flutter"). Takes baseband audio-band samples, returns channel-impaired samples. Used as a validation harness for HF data modem development.

## Library usage

```rust
use hf_channel_sim::{
    estimate_subcarrier_snr, AwgnGenerator, ChannelCondition, WattersonChannel,
};
use num_complex::Complex;

let clean: Vec<Complex<f32>> = /* your I/Q reference signal */;

// 1. Apply the channel (noise-free).
let mut ch = WattersonChannel::from_condition(
    /* seed */ 1,
    ChannelCondition::Moderate,
    /* sample rate Hz */ 8000.0,
);
let mut observed = ch.process_block(&clean);

// 2. Add AWGN at a controlled SNR.
let mut awgn = AwgnGenerator::new(/* independent seed */ 2);
awgn.add_noise(&mut observed, /* target SNR dB */ 10.0);

// 3. Measure per-sub-carrier SNR for bit-loading characterization.
let report = estimate_subcarrier_snr(&clean, &observed, /* fft_size */ 1024, 8000.0);
println!("mean SNR at bin 100: {} dB", report.mean_snr_db[100]);
```

For end-to-end characterization with structured JSON output (the typical AI-agent harness pattern), use `run_characterization` directly.

## CLI usage

```bash
hf-channel-sim-cli \
    --condition moderate \
    --sample-rate 8000 \
    --channel-seed 1 \
    --noise-seed 2 \
    --target-snr-db 10 \
    --fft-size 1024 \
    < input.iq > report.json
```

`input.iq` is interleaved f32 little-endian I/Q pairs. `report.json` is a serde-serialized `CharacterizationReport` (citations + inputs + observed power + per-bin SNR estimates + per-window snapshots).

## Independent-creation provenance

This crate is implemented from the following open sources:

- Watterson, C.C., J.R. Juroshek, W.D. Bensema. "Experimental Confirmation of an HF Channel Model." IEEE Trans. Communication Technology, COM-18(6), Dec 1970, pp. 792–803.
- ITU-R Recommendation F.520-2. "Use of high-frequency radiotelegraph circuits for data transmission."
- ITU-R Recommendation F.1487. "Testing of HF modems with bandwidths up to 12 kHz using ionospheric channel simulators." 2000.
- Davies, K. *Ionospheric Radio*. IEE/Peter Peregrinus, 1990.
- Proakis & Salehi, *Digital Communications*, 5th ed., McGraw-Hill, 2008.

**No closed-source HF modem (VARA, ARDOP-binary distributions, Trimode, etc.) was consulted in any form during the design or implementation of this crate.** This statement is the contemporaneous record supporting the independent-creation defense for downstream consumers.

## Citing this crate

When publishing measurement results derived from `hf-channel-sim`, cite both the crate (`hf-channel-sim` vX.Y.Z) and the foundational papers above. The `CharacterizationReport::foundational_citations` field carries the citation chain as a load-bearing serialized artifact.

## License

AGPL-3.0-only. See LICENSE.

If you run a modified version of this crate as part of a network-accessible service, AGPL §13 requires you to offer source to the service's users.

## Cross-validation

Output statistics are cross-validated against GNU Radio's `fading_model` block under ITU-R F.520 standardized inputs. See `docs/cross-validation.md` for methodology, tolerances, and the operator-run Python fixture-generation script.
