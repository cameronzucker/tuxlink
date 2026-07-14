//! The consent port (spec §4, §8): the executor's seam to pause an attended
//! transmit step for per-transmission operator consent BEFORE it runs.
//!
//! Attended mode (§97.109 / §97.221 control vocabulary) requires a control
//! operator's affirmative go-ahead at each transmission. The executor
//! ([`crate::executor::run_action_step_shared`]) enforces this by *parking* a
//! `transmits: true` step in an attended run on this port — a WAITING state
//! (`RunState::AwaitingConsent`) entered **before** the per-step timeout, so an
//! operator who steps away does not blow the transmit step's timeout. The
//! monolith supplies the real implementation (`routines::consent::ConsentRegistry`,
//! which emits the `AwaitingConsent` UI event and holds the grant channel);
//! leaf tests substitute [`crate::fakes::FakeConsent`].
//!
//! The port is Tauri-free: it names only run/step ids and a `StepError`
//! outcome, so the leaf crate never depends on the monolith's event surface.

use async_trait::async_trait;

use crate::error::StepError;

/// The attended-consent parking desk (spec §4). The executor awaits [`park`]
/// for a transmit step whose run is attended, racing it against the run's
/// cancellation token.
///
/// [`park`]: ConsentPort::park
#[async_trait]
pub trait ConsentPort: Send + Sync {
    /// Park `(run_id, step_id)` awaiting the operator's per-transmission
    /// consent. Resolves `Ok(())` when consent is granted — the executor then
    /// proceeds into the timed `execute`. Resolves `Err(StepError::Cancelled)`
    /// if the grant channel is torn down without a grant.
    ///
    /// **Drop contract (no stale-sender leak).** The returned future MUST
    /// release its parked entry if it is dropped before resolving — the
    /// executor drops it when the run is cancelled while parked (it races this
    /// future against the cancel token and takes the cancel branch). An
    /// implementation backed by a registry map therefore uses an RAII guard so
    /// a cancelled park leaves no orphaned grant sender behind.
    async fn park(&self, run_id: &str, step_id: &str) -> Result<(), StepError>;
}
