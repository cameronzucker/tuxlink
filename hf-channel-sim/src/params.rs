// SPDX-License-Identifier: AGPL-3.0-only

//! ITU-R F.520 / F.1487 standardized HF channel-condition parameter sets.

use serde::{Deserialize, Serialize};

/// Watterson-model channel parameters: delay spread + Doppler spread.
///
/// Per Watterson (1970) and ITU-R F.520-2. The two paths are independently
/// faded with complex-Gaussian taps; the delay spread is the time between
/// their arrivals; the Doppler spread is the bi-sided fading bandwidth of
/// each tap.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WattersonParams {
    /// Multipath delay spread (Δτ) between the two paths, in seconds.
    pub delay_spread_s: f64,
    /// Doppler frequency spread (2σ) of each tap's fading process, in Hz.
    pub doppler_spread_hz: f64,
}

/// ITU-R F.520-2 + F.1487 standardized channel conditions.
///
/// Cite this enum variant by name in any BER/throughput claim — per F.1487,
/// performance results are only comparable when measured against the same
/// standardized condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelCondition {
    /// Good: Δτ = 0.5 ms, 2σ = 0.1 Hz. Stable low-latitude daylight.
    Good,
    /// Moderate: Δτ = 1.0 ms, 2σ = 0.5 Hz. Typical mid-latitude.
    Moderate,
    /// Poor: Δτ = 2.0 ms, 2σ = 1.0 Hz. Disturbed / high-latitude.
    Poor,
    /// Flutter: Δτ = 0.5 ms, 2σ = 10.0 Hz. Auroral / equatorial flutter.
    Flutter,
}

impl ChannelCondition {
    /// Return the F.520 / F.1487 numeric parameters for this condition.
    pub fn params(self) -> WattersonParams {
        match self {
            Self::Good => WattersonParams {
                delay_spread_s: 0.5e-3,
                doppler_spread_hz: 0.1,
            },
            Self::Moderate => WattersonParams {
                delay_spread_s: 1.0e-3,
                doppler_spread_hz: 0.5,
            },
            Self::Poor => WattersonParams {
                delay_spread_s: 2.0e-3,
                doppler_spread_hz: 1.0,
            },
            Self::Flutter => WattersonParams {
                delay_spread_s: 0.5e-3,
                doppler_spread_hz: 10.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_matches_f520() {
        let p = ChannelCondition::Good.params();
        assert_eq!(p.delay_spread_s, 0.5e-3);
        assert_eq!(p.doppler_spread_hz, 0.1);
    }

    #[test]
    fn moderate_matches_f520() {
        let p = ChannelCondition::Moderate.params();
        assert_eq!(p.delay_spread_s, 1.0e-3);
        assert_eq!(p.doppler_spread_hz, 0.5);
    }

    #[test]
    fn poor_matches_f520() {
        let p = ChannelCondition::Poor.params();
        assert_eq!(p.delay_spread_s, 2.0e-3);
        assert_eq!(p.doppler_spread_hz, 1.0);
    }

    #[test]
    fn flutter_matches_f1487() {
        let p = ChannelCondition::Flutter.params();
        assert_eq!(p.delay_spread_s, 0.5e-3);
        assert_eq!(p.doppler_spread_hz, 10.0);
    }

    #[test]
    fn serde_roundtrip_condition() {
        let c = ChannelCondition::Moderate;
        let json = serde_json::to_string(&c).unwrap();
        let back: ChannelCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
