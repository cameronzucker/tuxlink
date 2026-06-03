//! Test-only helpers shared between integration tests and #[cfg(test)] code
//! in lib modules.
//!
//! Visibility: `pub` (not just `pub(crate)`) so integration tests in
//! `tests/` — which are separate crates — can import via
//! `tuxlink_lib::test_helpers::native_test_config`.

use crate::config::{
    CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
    PositionPrecision, PrivacyConfig,
};

/// Returns a test-only `Config` with a real callsign (N7CPZ) and blank identity
/// fields that make connect/send fail predictably without reaching a real CMS.
/// Used by `NativeBackend::test_fixture()` and integration tests in
/// `tests/winlink_backend_test.rs`.
#[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
pub fn native_test_config() -> Config {
    Config {
        schema_version: 1,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
            host: crate::config::default_cms_host(),
        },
        identity: IdentityConfig {
            callsign: Some("N7CPZ".to_string()),
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
        telnet_listen: crate::config::TelnetListenUiConfig::default(),
    }
}
