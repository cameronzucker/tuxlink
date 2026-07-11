//! Test-only helpers shared between integration tests and #[cfg(test)] code
//! in lib modules.
//!
//! Visibility: `pub` (not just `pub(crate)`) so integration tests in
//! `tests/` — which are separate crates — can import via
//! `tuxlink_lib::test_helpers::native_test_config`.

use crate::config::{
    CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
    PositionPrecision, PrivacyConfig, CONFIG_SCHEMA_VERSION,
};

/// Returns a test-only `Config` with a real callsign (N7CPZ) and blank identity
/// fields that make connect/send fail predictably without reaching a real CMS.
/// Used by `NativeBackend::test_fixture()` and integration tests in
/// `tests/winlink_backend_test.rs`.
#[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
pub fn native_test_config() -> Config {
    Config {
        elmer: crate::config::ElmerConfig::default(),
        schema_version: CONFIG_SCHEMA_VERSION,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
            host: crate::config::default_cms_host(),
        },
        identity: IdentityConfig {
            active_full: Some("N7CPZ".to_string()),
            identifier: None,
            grid: Some("DM33".to_string()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::BroadcastAtPrecision,
            position_precision: PositionPrecision::FourCharGrid,
            position_source: crate::config::PositionSource::Gps,
        },
        pat_mbo_address: None,
        packet: PacketConfig::default(),
        modem_ardop: None,
        modem_vara: None,
        rig: crate::config::RigUiConfig::default(),
        telnet_listen: crate::config::TelnetListenUiConfig::default(),
        network_po_favorites: Vec::new(),
        review_inbound_before_download: false,
        map_tile_source: None,
        aredn_master_node_host: None,
        aprs: crate::config::AprsConfig::default(),
        ft8: crate::config::Ft8Config::default(),
        wwv_offair: None,
        trash_auto_purge: true,
        trash_retention_days: 30,
        close_to_tray: true,
        close_prompt_seen: false,
        active_connection: None,
        p2p_limits: crate::contacts::limiter::P2pLimitsConfig::default(),
    }
}
