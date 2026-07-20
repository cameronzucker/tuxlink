//! Shared secondary-window helper (tuxlink-dmwte Task 3, spec §3).
//!
//! Factors the get-or-focus + builder + `WindowLabelAlreadyExists` /
//! `WebviewLabelAlreadyExists` race-guard body — previously duplicated
//! near-verbatim across `help_window.rs`, `logging_window.rs`, and
//! `stations_window.rs` — into one implementation, parameterized by a
//! [`SecondaryWindowSpec`]. `help_window.rs`, `logging_window.rs`,
//! `stations_window.rs`, and `compose_window.rs`'s `compose_window_open` are
//! thin callers of [`open_secondary_window`]; each keeps its own label
//! constant, `#[tauri::command]` signature, and docstring — this file does
//! not change any of their labels, routes, sizes, or decoration values.
//!
//! **Authorization is NOT enforced here.** Each command body checks
//! [`caller_is_authorized`] (or its own rule — `compose_window.rs` keeps a
//! thin pure-fn wrapper that delegates to this module's shared
//! [`caller_is_authorized`] for its F7 defense-in-depth; `surface_pop_out`
//! in `dock/commands.rs` uses its own main-or-own-label variant,
//! `caller_may_drive`) BEFORE calling [`open_secondary_window`]. One policy
//! site per command — do not add a second, potentially-conflicting check
//! inside the helper.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// How a secondary window's close action is handled (spec §3, adrev R3-F7).
/// Recorded on [`SecondaryWindowSpec`] as declarative documentation of each
/// window's close semantics — `lib.rs`'s `on_window_event` dispatches
/// `CloseRequested` by window label (`SurfaceId::from_window_label` / the
/// `"main"` check), not by looking up this policy at dispatch time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosePolicy {
    /// Plain single-purpose windows (help/logging/stations): the native
    /// close (titlebar X / OS close) just closes this window.
    CloseSelf,
    /// Compose: every close path (in-app Close button + native titlebar X)
    /// routes through the `compose_close_self` command instead of the JS
    /// window-class close APIs (tuxlink-h2y) — see `compose_window.rs`.
    CommandRouted,
    /// Pop-* dockable surfaces: closing re-docks the surface back into the
    /// main window instead of destroying it (Task 4 wires the dispatch).
    DockBack,
}

/// Whether [`open_secondary_window`] created a fresh window or focused one that
/// was already live.
///
/// Callers that must run build-only side effects branch on this:
/// `compose_window.rs`'s post-build monitor-height clamp runs only on
/// [`SpawnOutcome::BuiltNew`], and Task 4's `surface_pop_out` must likewise
/// distinguish a window that already exists live (no pop-out transition) from
/// one it just spawned (play the transition). Callers that only need
/// get-or-focus (help/logging/stations) ignore the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnOutcome {
    /// A new window was constructed by this call.
    BuiltNew,
    /// A window with this label already existed and was shown + focused —
    /// either via the up-front get-or-focus path or the build-time race guard.
    FocusedExisting,
}

/// A secondary window's full construction contract: label, route, sizing,
/// chrome, centering, and its close policy.
#[derive(Debug, Clone, PartialEq)]
pub struct SecondaryWindowSpec {
    pub label: String,
    pub route: String,
    pub title: String,
    pub inner_size: (f64, f64),
    pub min_inner_size: (f64, f64),
    pub decorations: bool,
    /// Apply `WebviewWindowBuilder::center()` on a fresh build when `true`.
    /// Only `compose_window.rs` sets this (its historical placement); every
    /// other secondary window leaves placement to the WM (`false`).
    pub centered: bool,
    pub close_policy: ClosePolicy,
}

/// The window label authorized to open secondary windows via the shared
/// [`caller_is_authorized`] rule. `compose_window.rs` keeps its own
/// `MAIN_WINDOW_LABEL` copy (its docstring covers the F7 rationale
/// separately); this constant backs every other secondary-window command.
const MAIN_WINDOW_LABEL: &str = "main";

/// Pure guard: is `caller_label` authorized to invoke a secondary-window-open
/// command? Only the main window is authorized. Extracted as a pure function
/// so it is unit-testable without a Tauri runtime, mirroring the
/// per-window `caller_is_authorized` helpers this replaces.
pub fn caller_is_authorized(caller_label: &str) -> bool {
    caller_label == MAIN_WINDOW_LABEL
}

/// Build the [`SecondaryWindowSpec`] for a dockable surface's pop-out window
/// (spec §3 sizes: TacMap 1100×750, Routines 960×680, AprsChat 440×640,
/// Elmer 520×720 (tuxlink-mfssz); all
/// three share the 420×360 floor, custom chrome (`decorations: false`), and
/// [`ClosePolicy::DockBack`] since popping a surface out is a dock-state
/// transition, not window destruction). Labels/routes/titles come from
/// [`crate::dock::SurfaceId`]'s own methods — never re-typed here.
pub fn pop_window_spec(surface: crate::dock::SurfaceId) -> SecondaryWindowSpec {
    let inner_size = match surface {
        crate::dock::SurfaceId::TacMap => (1100.0, 750.0),
        crate::dock::SurfaceId::Routines => (960.0, 680.0),
        crate::dock::SurfaceId::AprsChat => (440.0, 640.0),
        // tuxlink-mfssz: chat-column proportions, a shade wider than APRS
        // Chat for the tool chips + model form.
        crate::dock::SurfaceId::Elmer => (520.0, 720.0),
    };
    SecondaryWindowSpec {
        label: surface.window_label().to_string(),
        route: surface.route().to_string(),
        title: surface.title().to_string(),
        inner_size,
        min_inner_size: (420.0, 360.0),
        decorations: false,
        centered: false,
        close_policy: ClosePolicy::DockBack,
    }
}

/// Open (or focus, if already open) a secondary Tauri webview window per
/// `spec`. Transplanted verbatim (parameterized) from the get-or-focus +
/// builder + race-guard body shared by `help_window.rs`, `logging_window.rs`,
/// and `stations_window.rs` prior to this refactor.
///
/// **Does not check authorization** — see this module's docstring and
/// [`caller_is_authorized`]. `caller_label` is accepted (rather than
/// dropped) purely so a call site's authorization decision is traceable in
/// logs alongside the window it produced.
pub fn open_secondary_window(
    app: &AppHandle,
    caller_label: &str,
    spec: &SecondaryWindowSpec,
) -> Result<SpawnOutcome, String> {
    tracing::debug!(
        caller = caller_label,
        label = %spec.label,
        "open_secondary_window: opening or focusing"
    );

    // Idempotent: focus an already-open window with this label.
    if let Some(existing) = app.get_webview_window(&spec.label) {
        existing.show().map_err(|e| format!("show failed: {e}"))?;
        existing
            .set_focus()
            .map_err(|e| format!("set_focus failed: {e}"))?;
        return Ok(SpawnOutcome::FocusedExisting);
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        spec.label.as_str(),
        WebviewUrl::App(spec.route.clone().into()),
    )
    .title(spec.title.as_str())
    .inner_size(spec.inner_size.0, spec.inner_size.1)
    .min_inner_size(spec.min_inner_size.0, spec.min_inner_size.1)
    .resizable(true)
    .decorations(spec.decorations);
    if spec.centered {
        builder = builder.center();
    }
    let build_result = builder.build();

    match build_result {
        Ok(_) => Ok(SpawnOutcome::BuiltNew),
        // Match the compose/help/logging/stations race-guard: a concurrent
        // call may race past the get_webview_window check above and hit
        // AlreadyExists from build().
        Err(tauri::Error::WindowLabelAlreadyExists(_))
        | Err(tauri::Error::WebviewLabelAlreadyExists(_)) => {
            if let Some(existing) = app.get_webview_window(&spec.label) {
                let _ = existing.show();
                let _ = existing.set_focus();
            }
            Ok(SpawnOutcome::FocusedExisting)
        }
        Err(e) => Err(format!(
            "secondary window build failed (label {}): {e}",
            spec.label
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_main_is_authorized() {
        assert!(caller_is_authorized("main"));
        for bad in ["help", "stations", "compose-x", "pop-routines", ""] {
            assert!(!caller_is_authorized(bad), "{bad} must not spawn windows");
        }
    }

    /// The pop windows' spec constants (spec §3: sizes; §3 wire table: labels/
    /// routes/titles; decorations always false = custom chrome).
    #[test]
    fn pop_specs_match_wire_contract() {
        use crate::dock::SurfaceId;
        let map = pop_window_spec(SurfaceId::TacMap);
        assert_eq!(map.label, "pop-tacmap");
        assert_eq!(map.route, "/pop/tacmap");
        assert_eq!(map.inner_size, (1100.0, 750.0));
        assert!(matches!(map.close_policy, ClosePolicy::DockBack));
        assert!(!map.decorations);
        assert!(!map.centered); // pop windows leave placement to the WM
        let routines = pop_window_spec(SurfaceId::Routines);
        assert_eq!(routines.inner_size, (960.0, 680.0));
        let chat = pop_window_spec(SurfaceId::AprsChat);
        assert_eq!(chat.inner_size, (440.0, 640.0));
    }
}
