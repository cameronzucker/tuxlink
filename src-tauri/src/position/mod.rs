pub mod arbiter;
pub mod geo;
pub mod gps_fix;
pub mod gpsd;
pub mod maidenhead;
pub mod probe;
pub use arbiter::{Fix, PositionArbiter};
pub use crate::config::PositionSource;
pub use maidenhead::{grid_to_lat_lon, lat_lon_to_grid};

/// The effective ON-AIR locator: the precision-reduced grid that WILL be
/// broadcast over RF/CMS, honoring BOTH precision and the `gps_state` privacy
/// control. GPS-derived positions go on air ONLY when
/// `gps_state == BroadcastAtPrecision`; under `Off` or `LocalUiOnly` the on-air
/// locator falls back to the stored (manually-configured) config grid —
/// GPS is never broadcast under those states. A hand-set Manual grid is
/// operator-entered (not GPS) and broadcasts regardless of `gps_state`.
///
/// This is the canonical on-air locator: `native_connect` (transmission) calls
/// it. For RIBBON DISPLAY, use [`effective_ui_locator`] instead — that function
/// is NOT privacy-gated for `LocalUiOnly` because the operator's literal
/// intent under that state is "show GPS locally, don't broadcast it."
///
/// Spec: docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md §2.5
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
    // Position-subsystem restoration: the privacy gate keys on
    // `a.source()` — the chip selection that IS the operator's authoritative
    // intent under the 2026-05-22 source contract. "GPS mode + privacy
    // forbids live GPS broadcast" falls back to the operator-stored config
    // grid; `source` reflects that intent directly (no separate
    // "effective" derivation).
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

/// The effective LOCAL UI locator: the precision-reduced grid that the ribbon
/// displays to the operator. NOT privacy-gated by `gps_state` — the local UI
/// shows the operator's position so long as the operator hasn't disabled GPS
/// entirely. Distinct from [`effective_broadcast_locator`] (the on-air locator
/// which IS privacy-gated): they intentionally diverge under
/// `source=Gps + LocalUiOnly + fresh fix`, where the operator wants to see
/// their position locally but not broadcast it.
///
/// Derivation per spec §2.5 + §3.4 I7:
/// - `source=Gps + fresh fix + gps_state ∈ {LocalUiOnly, BroadcastAtPrecision}`
///   → `arbiter.broadcast_grid()` (live precision-reduced fix)
/// - `source=Gps + (no fresh fix OR gps_state=Off)` → `config_grid` (fallback)
/// - `source=Manual` → `config_grid` (manually-stored grid)
///
/// Spec: docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md §2.5
pub fn effective_ui_locator(
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
        Some(a) if a.source() == PositionSource::Manual => config_grid(),
        // source=Gps from here on
        Some(_) if config.privacy.gps_state == GpsState::Off => config_grid(),
        // source=Gps + LocalUiOnly/BroadcastAtPrecision: live fix if present,
        // else fall back to config_grid (operator hasn't disabled GPS, but no
        // fresh fix is available to display). Spec §3.4 I7.
        Some(a) if !a.has_fresh_fix() => config_grid(),
        Some(a) => a.broadcast_grid().unwrap_or_else(config_grid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        broadcast_grid, Config, ConnectConfig, CmsTransport, GpsState, IdentityConfig,
        PositionPrecision, PositionSource, PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };

    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn base_config(gps_state: GpsState, grid: Option<&str>) -> Config {
        Config {
            elmer: crate::config::ElmerConfig::default(),
            p2p_limits: crate::contacts::limiter::P2pLimitsConfig::default(),
            ft8: crate::config::Ft8Config::default(),
            wwv_offair: None,
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig { connect_to_cms: false, transport: CmsTransport::Telnet, host: crate::config::default_cms_host() },
            identity: IdentityConfig {
                active_full: None,
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
            modem_vara: None,
            rig: crate::config::RigUiConfig::default(),
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
            aprs: crate::config::AprsConfig::default(),
            trash_auto_purge: true,
            trash_retention_days: 30,
            close_to_tray: true,
            close_prompt_seen: false,
            active_connection: None,
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

    // ========================================================================
    // tuxlink-va1i: effective_ui_locator — local UI must show live GPS
    // under LocalUiOnly while broadcast stays at config_grid.
    // ========================================================================

    // The operator-reported regression test (spec §2.5 + §3.4 I7):
    // source=Gps + LocalUiOnly + fresh fix → ui_locator shows live precision-reduced
    // fix, broadcast_locator stays at config_grid. The two intentionally diverge.
    #[test]
    fn ui_locator_diverges_from_broadcast_locator_under_localui_only_with_fresh_fix() {
        let mut cfg = base_config(GpsState::LocalUiOnly, Some("DM33"));
        cfg.privacy.position_precision = PositionPrecision::SixCharGrid;
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::SixCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("DM33ww"));
        // UI sees the live fix.
        assert_eq!(
            effective_ui_locator(&cfg, Some(&arbiter)),
            "DM33ww",
            "LocalUiOnly: ribbon must show the live GPS fix"
        );
        // On-air locator stays at the config grid (precision-reduced).
        assert_eq!(
            effective_broadcast_locator(&cfg, Some(&arbiter)),
            broadcast_grid("DM33", PositionPrecision::SixCharGrid),
            "LocalUiOnly: on-air locator stays at config_grid"
        );
    }

    // Pin the Off semantic: even with a fresh GPS fix in the arbiter, the UI
    // falls back to config_grid because the operator chose to disable GPS.
    #[test]
    fn ui_locator_under_off_falls_back_even_with_fresh_fix() {
        let cfg = base_config(GpsState::Off, Some("DM33"));
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("DM33ww"));
        assert_eq!(
            effective_ui_locator(&cfg, Some(&arbiter)),
            "DM33",
            "operator chose Off → no live fix in UI (precision-reduced config grid)"
        );
    }

    // No-arbiter path mirrors broadcast: ui_locator falls back to config_grid.
    #[test]
    fn ui_locator_no_arbiter_falls_back_to_config_grid() {
        let cfg = base_config(GpsState::LocalUiOnly, Some("CN87ux"));
        assert_eq!(effective_ui_locator(&cfg, None), "CN87");
    }

    // Manual source always shows config_grid in the UI (the manually-stored grid
    // IS what the operator wants displayed, irrespective of GPS state).
    #[test]
    fn ui_locator_manual_source_shows_config_grid_regardless_of_gps_state() {
        for gps_state in [GpsState::Off, GpsState::LocalUiOnly, GpsState::BroadcastAtPrecision] {
            let cfg = base_config(gps_state, Some("DM33"));
            let arbiter = PositionArbiter::new(
                PositionSource::Manual,
                Some("CN87ux".into()),
                PositionPrecision::FourCharGrid,
            );
            // Even with a fresh GPS fix lurking in the arbiter, source=Manual
            // routes the UI to config_grid.
            arbiter.apply_gps_fix(Fix::test("EM75ab"));
            assert_eq!(
                effective_ui_locator(&cfg, Some(&arbiter)),
                "DM33",
                "Manual source: UI shows config_grid for gps_state={gps_state:?}"
            );
        }
    }

    // ui_locator matrix proptest — spans source × fix_state × gps_state × grid_set.
    // Mirrors the T7 active_grid matrix pattern (arbiter.rs::tests::state_space_...).
    use proptest::prelude::*;

    fn arb_source_ui() -> impl Strategy<Value = PositionSource> {
        prop_oneof![Just(PositionSource::Manual), Just(PositionSource::Gps)]
    }

    fn arb_gps_state_ui() -> impl Strategy<Value = GpsState> {
        prop_oneof![
            Just(GpsState::Off),
            Just(GpsState::LocalUiOnly),
            Just(GpsState::BroadcastAtPrecision),
        ]
    }

    fn arb_config_grid_ui() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),
            Just(Some("EM75".to_string())),
            Just(Some("CN87xx".to_string())),
        ]
    }

    proptest! {
        // Spec §3.4 I7 invariant:
        // - Source=Manual → expected = config_grid (precision-reduced)
        // - Source=Gps + gps_state=Off → expected = config_grid
        // - Source=Gps + apply_fix + gps_state ∈ {LocalUiOnly, BroadcastAtPrecision}
        //     → expected = fix.broadcast_grid() (precision-reduced live fix)
        // - Source=Gps + no apply_fix + gps_state ∈ {LocalUiOnly, BroadcastAtPrecision}
        //     → expected = config_grid (no fix to display)
        #[test]
        fn ui_locator_matrix(
            source in arb_source_ui(),
            gps_state in arb_gps_state_ui(),
            config_grid_opt in arb_config_grid_ui(),
            apply_fix in proptest::bool::ANY,
        ) {
            let cfg = base_config(gps_state, config_grid_opt.as_deref());
            let arbiter = PositionArbiter::new(
                source,
                None,
                PositionPrecision::FourCharGrid,
            );
            if apply_fix {
                arbiter.apply_gps_fix(Fix::test("DM33ab"));
            }
            let actual = effective_ui_locator(&cfg, Some(&arbiter));

            // Expected: config_grid (precision-reduced, "" when no grid set).
            let expected_config = config_grid_opt
                .as_deref()
                .map(|g| broadcast_grid(g, cfg.privacy.position_precision))
                .unwrap_or_default();

            match (source, gps_state, apply_fix) {
                // Manual → config_grid regardless.
                (PositionSource::Manual, _, _) => {
                    prop_assert_eq!(&actual, &expected_config);
                }
                // Gps + Off → config_grid regardless of fix.
                (PositionSource::Gps, GpsState::Off, _) => {
                    prop_assert_eq!(&actual, &expected_config);
                }
                // Gps + LocalUiOnly/BroadcastAtPrecision + fresh fix → live fix grid.
                (PositionSource::Gps, GpsState::LocalUiOnly, true)
                | (PositionSource::Gps, GpsState::BroadcastAtPrecision, true) => {
                    // Fix is "DM33ab"; precision-reduced via arbiter.broadcast_grid()
                    // at FourCharGrid → "DM33".
                    prop_assert_eq!(&actual, "DM33");
                }
                // Gps + LocalUiOnly/BroadcastAtPrecision + no fix → config_grid.
                (PositionSource::Gps, GpsState::LocalUiOnly, false)
                | (PositionSource::Gps, GpsState::BroadcastAtPrecision, false) => {
                    prop_assert_eq!(&actual, &expected_config);
                }
            }
        }
    }
}
