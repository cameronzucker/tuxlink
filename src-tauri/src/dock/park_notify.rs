//! Park notification — the Tauri-side attention channel for a routine that
//! parks awaiting transmit consent (spec §6, behavior 7).
//!
//! This lives on the dock side (not in the pure routines engine) deliberately:
//! resolving *which window* hosts the consent modal is a dock concern
//! ([`crate::dock::consent_host_window`]), and firing a desktop notification +
//! GTK urgency hint is a Tauri concern. The routines engine
//! ([`crate::routines::consent`]) stays clean — it only emits
//! `AwaitingConsent` into its sink, and the production sink
//! ([`crate::routines::events::TauriRoutinesEventSink`]) delegates here when
//! that event lands. The `RoutinesEventSink` trait gains nothing.
//!
//! Two-channel attention (spec §6, adrev R3-F2 / Codex-6): the desktop
//! notification is the guaranteed cross-backend channel; `request_user_attention`
//! is free X11 polish (a no-op on labwc/Wayland). Focus is evaluated at park
//! time (accepted, spec §6).

use std::sync::Arc;

use tauri::{AppHandle, Manager, UserAttentionType};

use crate::dock::consent_host_window;
use crate::dock::registry::DockRegistry;

/// Fire the park-notification for the run that just parked (spec §6). Resolves
/// the consent host window from the CURRENT dock state; if that window exists
/// and is NOT focused, fires a desktop notification and requests critical user
/// attention. Best-effort throughout — a missing daemon, an absent window, or
/// an unresolvable routine name never fails the park (the modal + badge remain
/// the primary consent surfaces).
pub fn notify_awaiting_consent(app: &AppHandle, run_id: &str) {
    // The registry is the canonical host resolver (spec §6). If it is not
    // managed yet, there is no window to point at — nothing to do.
    let Some(registry) = app.try_state::<DockRegistry>() else {
        return;
    };
    let host_label = consent_host_window(registry.snapshot().surfaces.routines);

    let Some(window) = app.get_webview_window(host_label) else {
        return;
    };
    // Only surface attention when the host window is NOT already focused —
    // a focused window is already showing the modal (spec §6).
    if window.is_focused().unwrap_or(false) {
        return;
    }

    let routine = resolve_routine_name(app, run_id);

    // Guaranteed cross-backend channel: desktop notification.
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title("Routine awaiting transmit consent")
            .body(format!("Routine awaiting transmit consent — {routine}"))
            .show();
    }

    // Free X11 polish (Wayland/labwc no-op): urgency hint on the host window.
    let _ = window.request_user_attention(Some(UserAttentionType::Critical));
}

/// Best-effort routine-name lookup for the notification body. Falls back to a
/// generic label if the routines state is unavailable or the run is unknown —
/// the notification is an attention cue, not a Part 97 record.
fn resolve_routine_name(app: &AppHandle, run_id: &str) -> String {
    app.try_state::<Arc<crate::routines::session::RoutinesState>>()
        .and_then(|state| state.run_status(run_id))
        .map(|status| status.routine)
        .unwrap_or_else(|| "a routine".to_string())
}
