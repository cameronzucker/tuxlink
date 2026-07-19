//! Durable transcript observation of the agent loop (tuxlink-gzbpo).
//!
//! The in-memory [`Conversation`](crate::Conversation) the loop mutates is the
//! ONLY place a tool call's arguments and a tool result's content ever exist:
//! the webview progress-event stream drops both (a tool chip carries the tool
//! *name* only, and tool results are never emitted at all), and the
//! conversation itself is trimmed to the last N turns at the session layer and
//! is never persisted to disk. So today there is no durable, greppable record
//! of what the agent actually did — which args it sent, what each tool returned
//! — the exact evidence needed to debug why a model mis-authored a call.
//!
//! [`TranscriptSink`] is the seam that closes that gap. The runner calls it once
//! per message it appends, **incrementally, as the message is appended**, so a
//! complete transcript survives the session-layer trim and a crashed or
//! budget-exhausted run still leaves a complete-up-to-that-point record on disk
//! (a whole-conversation write at run end would lose exactly the long, flailing
//! runs that most need capturing, and would face an unsolvable dedup problem
//! across the trim since [`Message`] has no id).
//!
//! Like the loop's `on_event` progress sink, a `TranscriptSink` is
//! **fire-and-forget**: [`TranscriptSink::record`] MUST NOT block, panic, or
//! influence any [`RunOutcome`](crate::RunOutcome), cancellation, or timeout.
//! The runner records only the messages IT appends (assistant turns, tool
//! calls, tool results, the fed-back validation error); the caller is
//! responsible for recording the operator's own input turns, so a multi-turn
//! session that carries a (trimmed) conversation forward never double-records
//! history it already wrote.

use crate::conversation::Message;

/// A fire-and-forget observer of each [`Message`] the agent loop appends to the
/// running conversation. See the module docs for the durability rationale and
/// the fire-and-forget contract a `record` implementation must honor.
pub trait TranscriptSink: Send + Sync {
    /// Called once per message the loop appends, in append order. The message
    /// is borrowed; an implementation that persists it must do so synchronously
    /// and cheaply (clone/serialize and return) — it must never block the loop,
    /// panic, or affect the run's outcome.
    fn record(&self, message: &Message);
}

/// A [`TranscriptSink`] that discards every message. The default for callers
/// (e.g. [`crate::run`]) that do not want a durable transcript.
pub struct NullTranscript;

impl TranscriptSink for NullTranscript {
    fn record(&self, _message: &Message) {}
}
