# hf-channel-sim

Watterson-class HF ionospheric channel simulator. Pure-Rust, deterministic, AGPLv3-only.

## Status

Pre-1.0. API is unstable until 0.5.

## What it does

Simulates the HF ionospheric channel between two amateur radio stations, applying time-varying multipath fading + Doppler spread per the Watterson (1970) model and ITU-R F.520 parameter sets ("Good", "Moderate", "Poor", "Flutter"). Takes baseband audio-band samples, returns channel-impaired samples. Used as a validation harness for HF data modem development.

## Independent-creation provenance

This crate is implemented from the following open sources:

- Watterson, C.C., J.R. Juroshek, W.D. Bensema. "Experimental Confirmation of an HF Channel Model." IEEE Trans. Communication Technology, COM-18(6), Dec 1970, pp. 792–803.
- ITU-R Recommendation F.520-2. "Use of high-frequency radiotelegraph circuits for data transmission."
- ITU-R Recommendation F.1487. "Testing of HF modems with bandwidths up to 12 kHz using ionospheric channel simulators." 2000.
- Davies, K. *Ionospheric Radio*. IEE/Peter Peregrinus, 1990.
- Proakis & Salehi, *Digital Communications*, 5th ed., McGraw-Hill, 2008.

**No closed-source HF modem (VARA, ARDOP-binary distributions, Trimode, etc.) was consulted in any form during the design or implementation of this crate.** This statement is the contemporaneous record supporting the independent-creation defense for downstream consumers.

## License

AGPL-3.0-only. See LICENSE.

If you run a modified version of this crate as part of a network-accessible service, AGPL §13 requires you to offer source to the service's users.

## Cross-validation

Output statistics are cross-validated against [ITS HF channel simulator releases] and/or [GNU Radio HF channel OOT modules] under ITU-R F.520 standardized inputs. See `docs/cross-validation.md`.
