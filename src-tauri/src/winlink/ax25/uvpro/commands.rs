//! Tauri command + event surface for the native UV-Pro control profile
//! (tuxlink-nx95). Thin wrappers over [`UvproSession`]; the real logic + tests
//! live in `session.rs`. Each command delegates to an `*_inner` helper that takes
//! `&Arc<UvproSession>` so it is unit-testable without a Tauri `State`.
//!
//! Errors serialize as `{ kind, message }` so the frontend can switch on `kind`
//! (`LinkBusy`, `NotConnected`, `Timeout`, …) per the API contract.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::config;
use crate::winlink::ax25::link::KissLinkConfig;

use super::model::{UvproChannel, UvproStatus};
use super::rf_ch::{Bandwidth, Modulation};
use super::session::UvproSession;
use super::settings::Vfo;
use super::UvproError;

/// Tauri event name the status broadcaster emits on (mirrors `modem:status`).
pub const STATUS_EVENT: &str = "uvpro:status";

/// How often the broadcaster polls live status while connected.
const BROADCAST_INTERVAL: Duration = Duration::from_secs(2);

/// Frontend-facing error: a stable `kind` + human `message`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UvproCommandError {
    pub kind: String,
    pub message: String,
}

impl From<UvproError> for UvproCommandError {
    fn from(e: UvproError) -> Self {
        UvproCommandError {
            kind: e.kind().to_string(),
            message: e.to_string(),
        }
    }
}

/// The configured UV-Pro MAC, if the packet transport is a Bluetooth link.
fn configured_mac() -> Option<String> {
    match config::read_config().ok()?.packet.link {
        Some(KissLinkConfig::Bluetooth { mac }) => Some(mac),
        _ => None,
    }
}

fn parse_vfo(vfo: Option<String>) -> Vfo {
    match vfo.as_deref() {
        Some("b") | Some("B") => Vfo::B,
        _ => Vfo::A,
    }
}

// ---- inner helpers (testable without Tauri State) ----

pub fn uvpro_connect_inner(
    session: &Arc<UvproSession>,
    mac: Option<String>,
) -> Result<UvproStatus, UvproError> {
    let mac = mac.or_else(configured_mac).ok_or(UvproError::BadMac)?;
    session.connect(&mac)
}

pub fn uvpro_get_status_inner(session: &Arc<UvproSession>) -> UvproStatus {
    session.status_snapshot()
}

// ---- Tauri commands ----

#[tauri::command]
pub fn uvpro_connect(
    mac: Option<String>,
    session: State<'_, Arc<UvproSession>>,
) -> Result<UvproStatus, UvproCommandError> {
    uvpro_connect_inner(&session, mac).map_err(Into::into)
}

#[tauri::command]
pub fn uvpro_disconnect(session: State<'_, Arc<UvproSession>>) -> UvproStatus {
    session.disconnect();
    session.status_snapshot()
}

#[tauri::command]
pub fn uvpro_get_status(session: State<'_, Arc<UvproSession>>) -> UvproStatus {
    uvpro_get_status_inner(&session)
}

#[tauri::command]
pub fn uvpro_get_channels(
    session: State<'_, Arc<UvproSession>>,
) -> Result<Vec<UvproChannel>, UvproCommandError> {
    session.channels().map_err(Into::into)
}

#[tauri::command]
pub fn uvpro_set_channel(
    channel_id: u8,
    vfo: Option<String>,
    session: State<'_, Arc<UvproSession>>,
) -> Result<UvproStatus, UvproCommandError> {
    session.set_channel(channel_id, parse_vfo(vfo)).map_err(Into::into)
}

#[tauri::command]
pub fn uvpro_set_frequency(
    channel_id: u8,
    rx_mhz: f64,
    tx_mhz: Option<f64>,
    session: State<'_, Arc<UvproSession>>,
) -> Result<UvproStatus, UvproCommandError> {
    session
        .set_frequency(channel_id, rx_mhz, tx_mhz)
        .map_err(Into::into)
}

#[tauri::command]
pub fn uvpro_set_mode(
    channel_id: u8,
    mode: Modulation,
    bandwidth: Option<Bandwidth>,
    session: State<'_, Arc<UvproSession>>,
) -> Result<UvproStatus, UvproCommandError> {
    session
        .set_mode(channel_id, mode, bandwidth)
        .map_err(Into::into)
}

/// Spawn the background status broadcaster: while connected, poll live status
/// every [`BROADCAST_INTERVAL`] and hand each snapshot to `emit`. If the link
/// dies under us, disconnect (releasing the owner-lock) and emit the disconnected
/// snapshot once. NO auto-reconnect.
pub fn spawn_status_broadcaster<F>(session: Arc<UvproSession>, emit: F) -> std::thread::JoinHandle<()>
where
    F: Fn(UvproStatus) + Send + 'static,
{
    // Battery has no push event, so refresh it every Nth tick (~30 s at a 2 s
    // interval) — bounded per the spec, never every tick.
    const BATTERY_EVERY_N_TICKS: u32 = 15;
    std::thread::spawn(move || {
        let mut tick: u32 = 0;
        loop {
            std::thread::sleep(BROADCAST_INTERVAL);
            if !session.is_connected() {
                continue;
            }
            tick = tick.wrapping_add(1);
            let result = if tick.is_multiple_of(BATTERY_EVERY_N_TICKS) {
                session.poll_battery()
            } else {
                session.poll_tick()
            };
            match result {
                Ok(snap) => emit(snap),
                Err(_) => {
                    // The link is wedged/closed; tear down and announce once.
                    session.disconnect();
                    emit(session.status_snapshot());
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::ax25::uvpro::model::ConnState;

    #[test]
    fn get_status_inner_reports_disconnected_before_connect() {
        let sess = Arc::new(UvproSession::new());
        assert_eq!(uvpro_get_status_inner(&sess).state, ConnState::Disconnected);
    }

    #[test]
    fn connect_inner_surfaces_link_busy() {
        let sess = Arc::new(UvproSession::new());
        let _g = sess.link_lock().acquire("kiss-packet").unwrap();
        let err = uvpro_connect_inner(&sess, Some("38:D2:00:01:55:5C".into())).unwrap_err();
        assert!(matches!(err, UvproError::LinkBusy { .. }));
    }

    #[test]
    fn connect_inner_without_mac_or_config_is_bad_mac() {
        // No mac passed; config may or may not have one in the test env, so only
        // assert the error type when no mac resolves.
        let sess = Arc::new(UvproSession::new());
        if configured_mac().is_none() {
            let err = uvpro_connect_inner(&sess, None).unwrap_err();
            assert!(matches!(err, UvproError::BadMac));
        }
    }

    #[test]
    fn command_error_carries_kind() {
        let e: UvproCommandError = UvproError::LinkBusy { holder: "x".into() }.into();
        assert_eq!(e.kind, "LinkBusy");
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("\"kind\":\"LinkBusy\""), "{j}");
    }

    #[test]
    fn parse_vfo_defaults_to_a() {
        assert!(matches!(parse_vfo(None), Vfo::A));
        assert!(matches!(parse_vfo(Some("b".into())), Vfo::B));
        assert!(matches!(parse_vfo(Some("a".into())), Vfo::A));
    }
}
