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
    /// `ft8-waterfall:columns` — one per 4-column FFT batch (L3, Task A6).
    /// Defaulted no-op so non-waterfall sinks (and the many test
    /// constructions of production sinks) need no change; the Tauri sink and
    /// the recording test sink override it.
    fn emit_waterfall(&self, _batch: &crate::ft8::waterfall::WaterfallBatch) {}
}

/// Delta-named events (spec §Events).
pub const FT8_SLOT_EVENT: &str = "ft8-decodes:slot";
pub const FT8_LISTENING_EVENT: &str = "ft8-listening:change";
pub const FT8_WATERFALL_EVENT: &str = "ft8-waterfall:columns";

/// Production sink: Tauri `AppHandle::emit`, fire-and-forget (modem:status
/// precedent — a failed emit is a UI-absent condition, never a service
/// error).
pub struct TauriEventSink {
    pub app: tauri::AppHandle,
}

impl EventSink for TauriEventSink {
    fn emit_listening_change(&self, change: &Ft8ListeningChange) {
        use tauri::Emitter as _;
        let _ = self.app.emit(FT8_LISTENING_EVENT, change);
    }
    fn emit_slot(&self, record: &SlotRecord) {
        use tauri::Emitter as _;
        let _ = self.app.emit(FT8_SLOT_EVENT, record);
    }
    fn emit_waterfall(&self, batch: &crate::ft8::waterfall::WaterfallBatch) {
        use tauri::Emitter as _;
        let _ = self.app.emit(FT8_WATERFALL_EVENT, batch);
    }
}
