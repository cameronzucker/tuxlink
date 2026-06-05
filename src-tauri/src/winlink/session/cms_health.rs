//! Runtime state tracking CMS connection health (spec §9.7).
//!
//! Updated by winlink::session and winlink::telnet on connection
//! success/failure events. Read by the network probe at probe time.
//!
//! CMS_HEALTH is a process-lifetime singleton exposed via re-export at
//! crate::winlink::cms_health so the network probe can import it without
//! referencing crate::winlink::session::* (RADIO-1 probe isolation contract).

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use std::sync::RwLock;

#[derive(Debug, Clone, serde::Serialize)]
pub enum CmsAttemptOutcome {
    Success,
    TimeoutMs(u32),
    Refused,
    DnsFailed,
    Other(String),
}

#[derive(Default)]
pub struct CmsHealthState {
    last_successful: RwLock<Option<DateTime<Utc>>>,
    last_attempt: RwLock<Option<DateTime<Utc>>>,
    last_outcome: RwLock<Option<CmsAttemptOutcome>>,
}

impl CmsHealthState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_success(&self) {
        let now = Utc::now();
        if let Ok(mut w) = self.last_successful.write() {
            *w = Some(now);
        }
        if let Ok(mut w) = self.last_attempt.write() {
            *w = Some(now);
        }
        if let Ok(mut w) = self.last_outcome.write() {
            *w = Some(CmsAttemptOutcome::Success);
        }
    }

    pub fn record_failure(&self, outcome: CmsAttemptOutcome) {
        let now = Utc::now();
        if let Ok(mut w) = self.last_attempt.write() {
            *w = Some(now);
        }
        if let Ok(mut w) = self.last_outcome.write() {
            *w = Some(outcome);
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "last_successful_at": self.last_successful.read().ok().and_then(|r| r.as_ref().map(|d| d.to_rfc3339())),
            "last_attempt_at": self.last_attempt.read().ok().and_then(|r| r.as_ref().map(|d| d.to_rfc3339())),
            "last_outcome": self.last_outcome.read().ok().and_then(|r| r.clone()),
        })
    }
}

/// Process-lifetime singleton for CMS health tracking.
///
/// Re-exported at crate::winlink::cms_health::CMS_HEALTH for probe access.
pub static CMS_HEALTH: Lazy<CmsHealthState> = Lazy::new(CmsHealthState::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_success_and_snapshot() {
        let state = CmsHealthState::new();
        state.record_success();
        let snap = state.snapshot();
        assert!(snap.get("last_successful_at").unwrap().is_string());
    }

    #[test]
    fn records_failure_outcome() {
        let state = CmsHealthState::new();
        state.record_failure(CmsAttemptOutcome::TimeoutMs(5000));
        let snap = state.snapshot();
        assert!(snap.get("last_outcome").is_some());
    }
}
