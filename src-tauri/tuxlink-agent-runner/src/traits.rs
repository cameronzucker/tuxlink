//! The two async ports the loop runs over (T2).
//!
//! Both are object-safe and `Send + Sync` so the loop can hold them as
//! `&dyn Provider` / `&dyn ToolInvoker`. We use `#[async_trait]` to match
//! `tuxlink-mcp-core`'s port traits (object-safe async fns, MSRV-1.75-friendly).
//!
//! **SEC-4 (capability containment).** Note what these traits do NOT expose: a
//! `ToolInvoker` offers `tools()` (read-only schema list) and `invoke()` (relay
//! a call). It has no method to arm send authority, clear taint, or otherwise
//! mutate the security guard. The loop is therefore *incapable by construction*
//! of arming or clearing taint — it only ever holds a `&dyn ToolInvoker` plus a
//! read-only [`EgressStatus`] snapshot. The real `EgressGuard` (with `arm()` /
//! `clear_taint()`) lives in `tuxlink-security` and is never reachable from this
//! crate; see the crate-level docs and the `no_security_mutating_dep` test.

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::conversation::Conversation;
use crate::types::{CallAuthority, ModelTurn, ToolCall, ToolOutcome, ToolSpec};

/// A read-only snapshot of egress status the loop may *observe* but never
/// mutate (SEC-4). It carries no handle that could arm or clear taint — it is a
/// plain value copied out of the security layer by the frontend before `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EgressStatus {
    /// Whether send authority is currently armed (informational only).
    pub armed: bool,
    /// Whether the session is tainted by untrusted content (informational only).
    pub tainted: bool,
}

/// The model side of the loop. One call = one model turn.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Produce the next [`ModelTurn`] given the running transcript and the tools
    /// the model may call. Implementations must be cancellation-friendly via the
    /// timeout the loop wraps around this call.
    async fn turn(
        &self,
        conversation: &Conversation,
        tools: &[ToolSpec],
    ) -> Result<ModelTurn, ProviderError>;
}

/// A Provider failure. Transport/parse errors surface here; the loop maps them
/// onto a `NeedsOperator` outcome (it cannot make progress without the model).
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// A transport or upstream error contacting the model.
    #[error("provider transport error: {0}")]
    Transport(String),
    /// The model response could not be parsed into a [`ModelTurn`].
    #[error("provider returned an unparseable turn: {0}")]
    Unparseable(String),
}

/// The tool side of the loop — the single canonical tool path (ARCH-1 / SEC-2).
/// It never reaches below the MCP tool boundary, so taint / redaction / schema
/// enforcement below it are never bypassed.
#[async_trait]
pub trait ToolInvoker: Send + Sync {
    /// The tools the model may call, with their JSON schemas (used by the loop
    /// for COR-3 validation). Read-only — this is the only schema source.
    fn tools(&self) -> &[ToolSpec];

    /// Invoke a tool. `authority` is supplied by the loop and is ALWAYS
    /// [`CallAuthority::Agent`] (SEC-3) — the runner has no way to construct any
    /// other value. `cancel` is propagated so an in-flight tool can abort
    /// cooperatively (COR-2).
    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome;
}
