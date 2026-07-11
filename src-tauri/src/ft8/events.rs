//! The event seam (spec §Events). The trait is defined here; the production
//! `TauriEventSink` + the event-name constants are wired in the commands
//! task, keeping this file test-consumable without a Tauri dependency in
//! unit tests.

use crate::ft8::records::{Ft8ListeningChange, SlotRecord};

/// Side-effect sink the service emits into (AprsState `EventSink`
/// precedent). Production: Tauri `AppHandle::emit`, fire-and-forget; tests:
/// a recording sink.
pub trait EventSink: Send + Sync {
    /// `ft8-listening:change` — axis/flags/phase/band/sweep summary, emitted
    /// on every change to any of them.
    fn emit_listening_change(&self, change: &Ft8ListeningChange);
    /// `ft8-decodes:slot` — one per slot boundary (including drops/discards).
    fn emit_slot(&self, record: &SlotRecord);
}
