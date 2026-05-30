use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ModemState {
    Stopped,
    Spawning,
    Initializing,
    Idle,
    Connecting,
    ConnectedIrs,
    ConnectedIss,
    Disconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArqFlags {
    pub busy: bool,
    pub rx: bool,
    pub tx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModemStatus {
    pub state: ModemState,
    pub peer: Option<String>,
    pub mode: Option<String>,
    pub width_hz: Option<u32>,
    pub ptt_backend: Option<String>, // "rts" | "cat" | "vox"
    pub sn_db: Option<f32>,
    pub vu_dbfs: Option<f32>,
    pub throughput_bps: Option<u32>,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
    pub uptime_sec: u64,
    pub arq_flags: ArqFlags,
    pub last_error: Option<String>,
}

impl ModemStatus {
    pub fn stopped() -> Self {
        Self {
            state: ModemState::Stopped,
            peer: None,
            mode: None,
            width_hz: None,
            ptt_backend: None,
            sn_db: None,
            vu_dbfs: None,
            throughput_bps: None,
            bytes_rx: 0,
            bytes_tx: 0,
            uptime_sec: 0,
            arq_flags: ArqFlags { busy: false, rx: false, tx: false },
            last_error: None,
        }
    }
}

/// Shared per-app modem session state.
///
/// Wraps the current `ModemStatus` snapshot + the in-process RADIO-1 consent
/// token. `Arc<ModemSession>` is stored in Tauri state and shared between
/// command handlers and the broadcaster.
#[derive(Debug)]
pub struct ModemSession {
    inner: Mutex<ModemSessionInner>,
}

#[derive(Debug)]
struct ModemSessionInner {
    status: ModemStatus,
    consent_token: Option<String>,
    // The actual ArdopTransport handle is added in Task 3.2 once we have a
    // sane Option<...> + Send story.
}

impl ModemSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ModemSessionInner {
                status: ModemStatus::stopped(),
                consent_token: None,
            }),
        }
    }

    pub fn status_snapshot(&self) -> ModemStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn set_status(&self, s: ModemStatus) {
        self.inner.lock().unwrap().status = s;
    }

    /// Generate + remember a new consent token. Returns the token so the
    /// frontend can pass it to `modem_ardop_connect`.
    pub fn mint_consent_token(&self) -> String {
        // 16 random hex chars — enough for in-process uniqueness; not a secret.
        let token: String = (0..16)
            .map(|_| {
                let n: u8 = rand::random::<u8>() & 0xF;
                std::char::from_digit(n as u32, 16).unwrap()
            })
            .collect();
        self.inner.lock().unwrap().consent_token = Some(token.clone());
        token
    }

    pub fn has_valid_token(&self, candidate: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.consent_token.as_deref() == Some(candidate)
    }

    pub fn clear_consent_token(&self) {
        self.inner.lock().unwrap().consent_token = None;
    }
}

impl Default for ModemSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_serializes_to_documented_shape() {
        let s = ModemStatus::stopped();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["state"], "stopped");
        assert_eq!(json["bytesRx"], 0);
        assert!(json["peer"].is_null());
        assert_eq!(json["arqFlags"]["busy"], false);
    }

    #[test]
    fn connected_irs_roundtrips() {
        let s = ModemStatus {
            state: ModemState::ConnectedIrs,
            peer: Some("W7RMS-10".into()),
            mode: Some("4FSK 500".into()),
            width_hz: Some(500),
            ptt_backend: Some("rts".into()),
            sn_db: Some(8.4),
            vu_dbfs: Some(-18.0),
            throughput_bps: Some(540),
            bytes_rx: 4128,
            bytes_tx: 982,
            uptime_sec: 222,
            arq_flags: ArqFlags { busy: true, rx: true, tx: false },
            last_error: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: ModemStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        // confirm the wire form has camelCase + kebab-case for state
        assert!(json.contains("\"state\":\"connected-irs\""));
        assert!(json.contains("\"bytesRx\":4128"));
    }

    #[test]
    fn modem_session_starts_stopped_with_no_token() {
        let s = ModemSession::new();
        assert_eq!(s.status_snapshot().state, ModemState::Stopped);
        assert!(!s.has_valid_token("any-token"));
    }

    #[test]
    fn modem_session_accepts_minted_token_and_invalidates_on_clear() {
        let s = ModemSession::new();
        let t = s.mint_consent_token();
        assert!(s.has_valid_token(&t));
        s.clear_consent_token();
        assert!(!s.has_valid_token(&t));
    }
}
