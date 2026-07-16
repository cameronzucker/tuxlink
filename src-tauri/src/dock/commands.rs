//! Dock Tauri commands and the shared dock-back path (spec §3).
//!
//! Five `#[tauri::command]`s plus the `AppHandle`-touching helpers the
//! `on_window_event` close-intent timeout and the WebKitGTK crash signal also
//! call ([`dock_back`], [`connect_crash_signal`]). The pure transition core
//! lives in [`crate::dock::registry`]; everything here is the thin
//! spawn/destroy/focus/emit shell around it.

use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::dock::registry::{DockRegistry, RestorationGate};
use crate::dock::{DockMode, SurfaceId};
use crate::secondary_window::{open_secondary_window, pop_window_spec, SpawnOutcome};

/// Is `caller_label` allowed to drive `surface`'s dock commands? The main
/// window always may; a surface's own pop window may drive its OWN surface
/// (re-pop is focus; ✕/Ctrl+W flush their token via `surface_dock_back`).
/// This is the Rust-side per-surface ACL (spec §3 — capabilities do not gate
/// custom commands in Tauri 2; the `caller_is_authorized` pattern does).
fn caller_may_drive(caller_label: &str, surface: SurfaceId) -> bool {
    caller_label == "main" || caller_label == surface.window_label()
}

/// Pop a surface out to its own OS window, or focus it if already live
/// (spec §3, behavior 1). A live window is a no-op transition: it is shown +
/// focused (by [`open_secondary_window`]) and NO dock transition runs. A fresh
/// spawn sets `Popped` (storing `context`), persists, emits `dock:changed`,
/// and connects the crash signal. A spawn failure changes nothing and returns
/// the error verbatim (spec §3 "Pop-out mutates the registry only after the
/// window spawn succeeds").
#[tauri::command]
pub fn surface_pop_out(
    app: AppHandle,
    caller: WebviewWindow,
    registry: State<DockRegistry>,
    surface: SurfaceId,
    context: Option<serde_json::Value>,
) -> Result<(), String> {
    if !caller_may_drive(caller.label(), surface) {
        return Err(format!(
            "surface_pop_out({surface:?}) may only be invoked from the main window or {} (caller: {})",
            surface.window_label(),
            caller.label()
        ));
    }
    let spec = pop_window_spec(surface);
    match open_secondary_window(&app, caller.label(), &spec)? {
        // Already live: show+focus already happened in the helper; pop-out on a
        // live window is a no-op (spec §3 behavior 1) — no transition.
        SpawnOutcome::FocusedExisting => Ok(()),
        // Freshly spawned: the transition is now effective.
        SpawnOutcome::BuiltNew => {
            registry.transition(&app, surface, DockMode::Popped, context);
            connect_crash_signal(&app, surface);
            Ok(())
        }
    }
}

/// Dock a surface back inline (spec §3, behavior 2). Runs the dock-back
/// transition (set `Docked`, store `context`, persist, emit); if effective,
/// destroys the pop window. Already-`Docked` is a no-op — no emit, no destroy.
#[tauri::command]
pub fn surface_dock_back(
    app: AppHandle,
    caller: WebviewWindow,
    registry: State<DockRegistry>,
    surface: SurfaceId,
    context: Option<serde_json::Value>,
) -> Result<(), String> {
    if !caller_may_drive(caller.label(), surface) {
        return Err(format!(
            "surface_dock_back({surface:?}) may only be invoked from the main window or {} (caller: {})",
            surface.window_label(),
            caller.label()
        ));
    }
    dock_back(&app, &registry, surface, context);
    Ok(())
}

/// The shared dock-back path (spec §3): the one transition, then — only if it
/// was EFFECTIVE — destroy the window. `destroy()`, NEVER `close()`, which
/// would re-fire `CloseRequested` into the same route and loop. Both
/// `surface_dock_back`, the close-intent liveness timeout, and the crash
/// signal converge here; the no-op suppression makes concurrent dock-backs
/// (webview flush + timeout, or ✕ + crash) idempotent by construction.
pub fn dock_back(
    app: &AppHandle,
    registry: &DockRegistry,
    surface: SurfaceId,
    context: Option<serde_json::Value>,
) {
    if registry.transition(app, surface, DockMode::Docked, context) {
        if let Some(window) = app.get_webview_window(surface.window_label()) {
            let _ = window.destroy();
        }
    }
}

/// Generation-guarded dock-back, used ONLY by the `on_window_event`
/// close-intent liveness timeout (spec §3, behavior 4; adrev Round-2). The
/// timer samples the surface's pop generation when the WM-close arms it, then
/// passes it here as `expected_pop_generation`. The transition runs IFF that
/// generation still holds — the compare and the mutation are one indivisible
/// step inside the registry's single mutex ([`DockRegistry::transition_if_pop_generation`]),
/// so a re-pop cannot slip between them.
///
/// This closes the re-pop race the plain [`dock_back`] left open: WM-close arms
/// the 1.5 s timer → the webview's own `surface_dock_back` lands (Docked, window
/// destroyed) → the user re-pops within 1.5 s (new window, generation bumped) →
/// the stale timer fires. With the plain path, that timer would find `Popped`,
/// run an EFFECTIVE dock-back, and destroy the freshly re-popped window while
/// clearing its new continuity token. The generation guard makes the stale
/// timer a no-op instead.
///
/// The residual "re-pop between the transition and `destroy()`" window is not
/// exploitable: the transition is EFFECTIVE (and we reach `destroy()`) only when
/// the surface was still `Popped` at generation `expected_pop_generation` — i.e.
/// the webview never docked back (the hung-webview case this timer exists for).
/// A surface that is still `Popped` with a live window cannot be re-popped
/// (`surface_pop_out` on a live window only focuses; it runs no transition and
/// bumps no generation), so `destroy()` always targets the original hung window,
/// never a fresh one.
pub fn dock_back_if_generation(
    app: &AppHandle,
    registry: &DockRegistry,
    surface: SurfaceId,
    context: Option<serde_json::Value>,
    expected_pop_generation: u64,
) {
    if registry.transition_if_pop_generation(
        app,
        surface,
        DockMode::Docked,
        context,
        expected_pop_generation,
    ) {
        if let Some(window) = app.get_webview_window(surface.window_label()) {
            let _ = window.destroy();
        }
    }
}

/// Focus a popped surface (spec §5, behavior 8) — the single most load-bearing
/// call in the feature (every visual pathway ends here). `unminimize` → `show`
/// → `set_focus`, in that order. A stale pathway (window absent) is a no-op
/// `Ok(())`: the `dock:changed` reconcile heals the UI.
#[tauri::command]
pub fn surface_focus(app: AppHandle, surface: SurfaceId) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(surface.window_label()) {
        window.unminimize().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The full snapshot for mounting webviews (spec §3). Listen for `dock:changed`
/// FIRST, then call this, then reconcile (spec §5 subscription order).
#[tauri::command]
pub fn dock_state_get(registry: State<DockRegistry>) -> crate::dock::DockSnapshot {
    registry.snapshot()
}

/// Idempotent launch-restoration trigger (spec §3, behavior 6). The FIRST call
/// (guarded by [`RestorationGate`]) walks every surface and spawns a window for
/// each persisted `Popped`; later calls no-op. Fired from AppShell's mount
/// effect — never during the wizard (the frontend gates it, Task 8).
#[tauri::command]
pub fn shell_mounted(
    app: AppHandle,
    registry: State<DockRegistry>,
    gate: State<RestorationGate>,
) -> Result<(), String> {
    if !gate.arm() {
        return Ok(());
    }
    let snap = registry.snapshot();
    for surface in SurfaceId::ALL {
        if snap.surfaces.get(surface) != DockMode::Popped {
            continue;
        }
        let spec = pop_window_spec(surface);
        // Get-or-focus: a pop window that raced ahead of the shell is focused,
        // not double-built. Restoration does NOT re-run the transition — the
        // surface is already `Popped` in the (persisted, authoritative)
        // registry; it only needs its window.
        match open_secondary_window(&app, "main", &spec) {
            Ok(SpawnOutcome::BuiltNew) => connect_crash_signal(&app, surface),
            Ok(SpawnOutcome::FocusedExisting) => {}
            Err(e) => tracing::warn!(
                target: "tuxlink::dock",
                error = %e,
                ?surface,
                "launch restoration could not spawn popped surface window"
            ),
        }
    }
    Ok(())
}

/// Connect the WebKitGTK `web-process-terminated` signal on a freshly spawned
/// pop window and route it into the shared dock-back path (spec §3, behavior 5;
/// adrev R2-F2 / R3-F1). A WebProcess crash kills the content but not the OS
/// window and fires no Tauri window event — this is the ONLY crash safety net.
///
/// Reaches the underlying `webkit2gtk::WebView` via `WebviewWindow::with_webview`
/// exactly as `forms/pdf_export.rs` does. `connect_web_process_terminated` is
/// available because `tauri-runtime-wry` activates `webkit2gtk`'s `v2_40`
/// feature (which chains to `v2_20`, where the signal is gated) — no Cargo
/// change (spec §3; verified against the vendored source).
#[cfg(target_os = "linux")]
fn connect_crash_signal(app: &AppHandle, surface: SurfaceId) {
    let Some(window) = app.get_webview_window(surface.window_label()) else {
        return;
    };
    let app_for_cb = app.clone();
    let _ = window.with_webview(move |platform| {
        use webkit2gtk::WebViewExt;
        let webview = platform.inner(); // webkit2gtk::WebView
        webview.connect_web_process_terminated(move |_webview, _reason| {
            // Crash path: context is lost (edits since last save lost — accepted
            // and stated, spec §3). Route into the same dock-back transition.
            let registry = app_for_cb.state::<DockRegistry>();
            dock_back(&app_for_cb, &registry, surface, None);
        });
    });
}

/// Non-Linux stub: the crash signal is a WebKitGTK (Linux) concern. Tuxlink
/// ships Linux-only, but the cfg keeps the crate compiling elsewhere.
#[cfg(not(target_os = "linux"))]
fn connect_crash_signal(_app: &AppHandle, _surface: SurfaceId) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_may_drive_every_surface() {
        for s in SurfaceId::ALL {
            assert!(caller_may_drive("main", s));
        }
    }

    #[test]
    fn a_pop_window_may_drive_only_its_own_surface() {
        assert!(caller_may_drive("pop-routines", SurfaceId::Routines));
        assert!(!caller_may_drive("pop-routines", SurfaceId::TacMap));
        assert!(!caller_may_drive("pop-tacmap", SurfaceId::AprsChat));
    }

    #[test]
    fn unknown_callers_may_drive_nothing() {
        for bad in ["", "help", "compose-draft-x", "stations"] {
            for s in SurfaceId::ALL {
                assert!(!caller_may_drive(bad, s), "{bad} must not drive {s:?}");
            }
        }
    }
}
