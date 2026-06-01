// SPDX-License-Identifier: AGPL-3.0-only
//
// hf-channel-sim — Watterson-class HF ionospheric channel simulator.
// Copyright (C) 2026 tuxmodem contributors.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License v3 as
// published by the Free Software Foundation. See LICENSE.
//
// Independent-creation provenance: implemented from Watterson, Juroshek,
// Bensema (1970); ITU-R F.520; ITU-R F.1487. No prior-art modem internals
// (VARA, ARDOP, FLDigi, Trimode) consulted. See ADR 0014.

//! Watterson-class HF ionospheric channel simulator.
//!
//! Implements a 2-tap time-varying complex-Gaussian channel model per
//! Watterson 1970 + ITU-R F.520 + ITU-R F.1487, applied to baseband audio-
//! band samples. Deterministic, reproducible, AI-agent-friendly.

#![deny(missing_docs)]

pub mod analysis;
pub mod channel;
pub mod fading;
pub mod noise;
pub mod params;
pub mod report;
pub mod rng;

pub use analysis::{estimate_subcarrier_snr, SubcarrierSnrEstimate};
pub use channel::WattersonChannel;
pub use noise::AwgnGenerator;
pub use params::{ChannelCondition, WattersonParams};
pub use report::{run_characterization, CharacterizationInputs, CharacterizationReport};
