//! Per-block decode statistics surfaced from FEC up to ARQ (subsystem #6)
//! via MAC (subsystem #5).
//!
//! ARQ uses these as input to the retransmission decision: a CRC-fail
//! block triggers NACK / selective-repeat retransmit. ARQ does NOT
//! inspect individual LLRs; FEC does NOT know about ARQ's window or
//! sequence numbers.

/// Per-block residual-error report. Producer-side type that ARQ
/// (subsystem #6) consumes via MAC (subsystem #5).
///
/// The R1 reconciliation item in PR #183's body asked whether
/// `ResidualErrorStats` (this name) or `FecOutcome` (a candidate from
/// the #6 plan) was the canonical name; the plan recommendation was
/// to adopt #6's enum shape at the interop boundary while keeping
/// the producer-side struct here. The final reconciliation lands
/// when #6's crate is implemented.
#[derive(Clone, Debug)]
pub struct ResidualErrorStats {
    /// Did this block decode cleanly (CRC passed)?
    pub block_ok: bool,
    /// SPA iteration count used.
    pub iterations: u32,
    /// LLR-magnitude-derived confidence in `[0, 1]`. `None` if the
    /// decoder doesn't compute it.
    pub confidence_score: Option<f32>,
}
