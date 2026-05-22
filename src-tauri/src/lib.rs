pub mod app_backend;
pub mod bootstrap;
pub mod compose_window;
pub mod config;
pub mod consent_gate;
pub mod native_mailbox;
pub mod pat_client;
pub mod pat_config;
pub mod pat_process;
pub mod position;
pub mod session_log;
pub mod tray;
pub mod ui_commands;
pub mod winlink;
pub mod winlink_backend;
pub mod wizard;

#[cfg(test)]
mod build_support;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // tuxlink-wfw: on Linux/GTK the webkit2gtk DMA-BUF renderer (Mesa V3D on
    // Pi-class hardware) paints uninitialized GPU memory on first frame —
    // the window shows "TV static" until the first repaint. Disabling the
    // DMA-BUF renderer path fixes it with no discernible regression. Set
    // before the webview initializes (webkit reads this env var at web-context
    // creation, during window setup). Edition 2021 → set_var is safe.
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // Task 14 (tuxlink-dm8): per-compose-window geometry persistence.
        // `tauri-plugin-window-state` hooks the WebviewWindow lifecycle to
        // save/restore size+position keyed by window label. Registered here
        // (the integration commit, spec §4.3) — `compose_window.rs` only
        // builds the window; the plugin's Builder hook does the persistence.
        //
        // tuxlink-9zd: exclude StateFlags::VISIBLE. The main window's
        // close-to-tray path leaves it hidden/minimized; persisting visibility
        // would save `visible=false` on exit and the NEXT launch could start
        // invisible with no GUI path back (compounding the Wayland tray-absent
        // strand). Excluding VISIBLE guarantees every launch is visible while
        // still persisting size/position for the main + compose windows.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .manage(crate::wizard::WizardMutex::new())
        // Task D (tuxlink-22l): the single Winlink-backend managed state every
        // UI command + the `backend_status` ribbon consume (spec §3.4, adrev
        // #9). `BackendState` holds `(phase, Option<Arc<backend>>)` behind ONE
        // lock — replacing Task 12's `AppBackend(RwLock<Option<…>>)`, which
        // could not distinguish "offline / not connected" from "configured but
        // Pat failed". Starts `(NotConfigured, None)`; the `.setup()` bootstrap
        // below drives the phase (Spawning → Ready / Failed / ConfigError) and
        // installs the live backend once Pat is up. While `NotConfigured`,
        // `mailbox_list` returns `NotConfigured` (the UI's "not connected"
        // empty state) and `backend_status` returns `None`.
        .manage(crate::app_backend::BackendState::new())
        // Task A (tuxlink-22l): durable session-log ring buffer. The bridge
        // (Task C) appends here AND broadcasts on `session_log:line`; this
        // managed state lets `session_log_snapshot` serve late-mounting UIs
        // without losing Pat's startup lines (spec §11.1 / adrev #1,#2,#3).
        // Cap 500: ≈ one extended Pat CMS session's worth of log lines.
        //
        // Wrapped in an `Arc` (Task C, tuxlink-22l §11.2) so `PatBackend::spawn`
        // can hold a clone of the SAME buffer its bridge thread appends to —
        // the buffer `session_log_snapshot` reads. Tauri's `State` derefs
        // through the `Arc`, so the command sees an identical surface.
        .manage(std::sync::Arc::new(crate::session_log::SessionLogState::new(500)))
        .setup(|app| {
            // Install system tray icon + menu (tuxlink-rit / Task 8).
            // Close-to-tray: window close button hides to tray; only
            // File→Quit / tray→Quit / Ctrl+Q actually exit the process.
            // This keeps the Pat child process alive mid-ARQ session.
            crate::tray::install(app.handle())?;

            // Task D (tuxlink-22l) app-start Pat bootstrap (spec §3.3). Runs
            // OFF the main thread (a dedicated std::thread inside
            // `bootstrap::run`) so the webview paints immediately rather than
            // blocking on Pat's up-to-10s announce. The worker:
            //   - classifies `read_config()` via `bootstrap_decision` (adrev
            //     #14,#15: pre-wizard + offline → NotConfigured; malformed
            //     config → ConfigError; only `wizard_completed && connect_to_cms`
            //     spawns Pat),
            //   - resolves the Pat sidecar (adrev #12: 0-byte dev stub / missing
            //     → Failed with an actionable message; `PAT_BINARY` dev override),
            //   - calls the BLOCKING `PatBackend::spawn`, installs the backend in
            //     `BackendState` (→ Ready), and starts the session-log drain task
            //     (`tauri::async_runtime::spawn`, adrev #5) that emits one
            //     `session_log:line` event per `LogLine`.
            // ALL paths are non-fatal: the app always launches. A spawn/health
            // failure shows as an EXPLICIT error in the ribbon + session-log
            // pane (BackendPhase::Failed), not a silent empty state (spec §2).
            //
            // Part 97 (spec §6): the spawned argv is `pat … http --addr
            // 127.0.0.1:<port>` (HTTP mode, loopback only) and never calls Pat's
            // connect/send APIs — this code is WRITTEN + COMMITTED here; the
            // licensee RUNS the spawn path. No agent/CI executes it to "verify."
            crate::bootstrap::run(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Task 8 — close-to-tray: intercept the window X / Alt-F4 path on
            // the MAIN window and hide instead of closing. The process + Pat
            // child stay alive.
            // Only the Quit menu item (menu:file:quit / tray:quit) calls
            // app.exit(0), which bypasses this handler entirely.
            //
            // Guard on "main" so Task 14's compose windows close normally (they
            // need real close + unsaved-draft handling, not hide-to-tray).
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    // tuxlink-9zd: on Linux the SNI tray often does not register
                    // (e.g. Wayland + wf-panel-pi has no SNI host), so hide()
                    // would strand the window — process alive, no GUI path back.
                    // minimize() keeps the window in the compositor's window list
                    // (always recoverable via the panel/window-switcher) while
                    // still keeping the process + Pat child alive mid-session.
                    // macOS/Windows have a reliable tray, so hide() there.
                    #[cfg(target_os = "linux")]
                    let _ = window.minimize();
                    #[cfg(not(target_os = "linux"))]
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            crate::wizard::get_wizard_completed,
            crate::wizard::wizard_persist_cms,
            crate::wizard::wizard_persist_offline,
            crate::wizard::wizard_run_test_send,
            crate::wizard::wizard_test_send_is_mocked,
            // Main-UI cluster commands. Task 12 (tuxlink-zsm) created
            // `mailbox_list`; Tasks 13/14/16 appended their command fns to
            // `ui_commands.rs` but deferred registration to this single
            // orchestrator integration commit (spec §4.3) to keep the shared
            // `invoke_handler` edit in one diff.
            crate::ui_commands::mailbox_list,          // Task 12 (tuxlink-zsm)
            crate::ui_commands::message_read,          // Task 13 (tuxlink-y5c)
            crate::ui_commands::message_send,          // Task 14 (tuxlink-dm8)
            crate::ui_commands::cms_connect,           // tuxlink-0ic (native connect)
            crate::ui_commands::cms_abort,             // tuxlink-9z2 (abort in-flight connect)
            crate::ui_commands::config_read,           // Task 16 (tuxlink-hvv)
            crate::ui_commands::backend_status,        // Task 16 (tuxlink-hvv)
            crate::ui_commands::session_log_snapshot,  // Task 15 (tuxlink-8zg integration round)
            crate::compose_window::compose_window_open, // Task 14 (tuxlink-dm8)
            crate::compose_window::compose_close_self,  // tuxlink-h2y (self-only close)
            crate::ui_commands::app_quit,             // tuxlink-ng3 (HTML File→Quit / Ctrl+Q)
            crate::ui_commands::packet_config_get,    // tuxlink-7fr (packet config read)
            crate::ui_commands::packet_config_set,    // tuxlink-7fr (packet config write)
            crate::ui_commands::packet_connect,       // tuxlink-7fr (packet dial)
            crate::ui_commands::packet_set_listen,    // tuxlink-7fr (sticky listen)
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
