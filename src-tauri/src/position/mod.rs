pub mod arbiter;
pub mod gpsd;
pub mod maidenhead;
pub use arbiter::{Fix, PositionArbiter};
pub use crate::config::PositionSource;
pub use maidenhead::{grid_to_lat_lon, lat_lon_to_grid};

/// The effective on-air locator: the precision-reduced grid that WILL be broadcast,
/// honoring BOTH precision and the `gps_state` privacy control. GPS-derived positions
/// go on air ONLY when `gps_state == BroadcastAtPrecision`; under `Off` or `LocalUiOnly`
/// the on-air locator falls back to the stored (manually-configured) config grid —
/// GPS is never broadcast. A hand-set Manual grid is operator-entered (not GPS) and
/// broadcasts regardless of `gps_state`.
///
/// This is the SINGLE source of truth for the on-air locator: `native_connect` (the
/// actual transmission) and `position_status` (the ribbon's display) both call it,
/// so the ribbon always shows exactly what is/would be transmitted.
pub fn effective_broadcast_locator(
    config: &crate::config::Config,
    arbiter: Option<&PositionArbiter>,
) -> String {
    use crate::config::{broadcast_grid, GpsState, PositionSource};
    let config_grid = || {
        config
            .identity
            .grid
            .as_deref()
            .map(|g| broadcast_grid(g, config.privacy.position_precision))
            .unwrap_or_default()
    };
    match arbiter {
        None => config_grid(),
        Some(a)
            if a.source() == PositionSource::Gps
                && config.privacy.gps_state != GpsState::BroadcastAtPrecision =>
        {
            config_grid()
        }
        Some(a) => a.broadcast_grid().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        broadcast_grid, Config, ConnectConfig, CmsTransport, GpsState, IdentityConfig,
        PositionPrecision, PositionSource, PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };

    fn base_config(gps_state: GpsState, grid: Option<&str>) -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig { connect_to_cms: false, transport: CmsTransport::Telnet, host: crate::config::default_cms_host() },
            identity: IdentityConfig {
                callsign: None,
                identifier: None,
                grid: grid.map(|s| s.to_string()),
            },
            privacy: PrivacyConfig {
                gps_state,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: crate::config::PacketConfig::default(),
            modem_ardop: None,
        }
    }

    // No-arbiter path: falls back to config grid, precision-reduced.
    #[test]
    fn effective_no_arbiter_falls_back_to_config_grid() {
        let cfg = base_config(GpsState::BroadcastAtPrecision, Some("CN87ux"));
        assert_eq!(effective_broadcast_locator(&cfg, None), "CN87");
    }

    #[test]
    fn effective_no_arbiter_empty_when_no_config_grid() {
        let cfg = base_config(GpsState::BroadcastAtPrecision, None);
        assert_eq!(effective_broadcast_locator(&cfg, None), "");
    }

    // Arbiter, source == Gps, gps_state == Off → config grid (GPS never on air).
    #[test]
    fn effective_gps_source_off_falls_back_to_config_grid() {
        let cfg = base_config(GpsState::Off, Some("DM33"));
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("CN87ux"));
        // GPS is Off: the GPS fix must NOT go on air → config grid "DM33".
        assert_eq!(effective_broadcast_locator(&cfg, Some(&arbiter)), "DM33");
    }

    // Arbiter, source == Gps, gps_state == LocalUiOnly → config grid (no GPS on air).
    #[test]
    fn effective_gps_source_local_ui_only_falls_back_to_config_grid() {
        let cfg = base_config(GpsState::LocalUiOnly, Some("DM33"));
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("CN87ux"));
        assert_eq!(effective_broadcast_locator(&cfg, Some(&arbiter)), "DM33");
    }

    // Arbiter, source == Gps, gps_state == BroadcastAtPrecision → live GPS grid.
    #[test]
    fn effective_gps_source_broadcast_returns_gps_grid() {
        let cfg = base_config(GpsState::BroadcastAtPrecision, Some("DM33"));
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("CN87ux"));
        assert_eq!(effective_broadcast_locator(&cfg, Some(&arbiter)), "CN87");
    }

    // Arbiter, source == Manual, any gps_state → arbiter's manual grid (broadcasts regardless).
    #[test]
    fn effective_manual_source_broadcasts_regardless_of_gps_state() {
        for gps_state in [GpsState::Off, GpsState::LocalUiOnly, GpsState::BroadcastAtPrecision] {
            let cfg = base_config(gps_state, Some("DM33"));
            let arbiter = PositionArbiter::new(
                PositionSource::Manual,
                Some("CN87ux".into()),
                PositionPrecision::FourCharGrid,
            );
            assert_eq!(
                effective_broadcast_locator(&cfg, Some(&arbiter)),
                "CN87",
                "Manual source must broadcast regardless of gps_state={gps_state:?}"
            );
        }
    }

    // tuxlink-882 regression: no-arbiter path with precision reduction still works.
    #[test]
    fn effective_no_arbiter_precision_reduction_still_applied() {
        let cfg = base_config(GpsState::BroadcastAtPrecision, Some("CN87ux"));
        assert_eq!(
            effective_broadcast_locator(&cfg, None),
            broadcast_grid("CN87ux", PositionPrecision::FourCharGrid),
        );
    }
}
