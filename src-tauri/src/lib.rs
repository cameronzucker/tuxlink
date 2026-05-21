pub mod app_backend;
pub mod compose_window;
pub mod config;
pub mod consent_gate;
pub mod menu;
pub mod pat_client;
pub mod pat_config;
pub mod pat_process;
pub mod tray;
pub mod ui_commands;
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
        // Task 12 (tuxlink-zsm): the single Winlink-backend handle every UI
        // command consumes (spec §1.1). Starts `None`; the live bootstrap
        // (spawn Pat → construct PatBackend → store here → drain stream_log)
        // is STUBBED in v0.0.1 — see the `.setup()` note below. While `None`,
        // `mailbox_list` returns `NotConfigured`, which the UI renders as the
        // "not connected" empty state (NOT an error).
        .manage(crate::app_backend::AppBackend::new())
        .setup(|app| {
            // Build the native OS menu bar (tuxlink-6vi / Task 7) and wire
            // its events to Tauri IPC so the React frontend can listen on
            // the "menu" channel.
            let menu = crate::menu::build_menu(app.handle())?;
            app.set_menu(menu)?;
            crate::menu::wire_menu_events(app.handle());

            // Install system tray icon + menu (tuxlink-rit / Task 8).
            // Close-to-tray: window close button hides to tray; only
            // File→Quit / tray→Quit / Ctrl+Q actually exit the process.
            // This keeps the Pat child process alive mid-ARQ session.
            crate::tray::install(app.handle())?;

            // Task 12 backend bootstrap — STUBBED in v0.0.1 (DONE_WITH_CONCERNS).
            //
            // The live path (spec §1.1 / §3.3) is: if a tuxlink config exists
            // AND connect_to_cms == true, locate the Pat sidecar, spawn it via
            // PatProcess (renders Pat config + reads the keyring credential),
            // construct a PatBackend over the announced HTTP port, store
            // Arc<PatBackend> in AppBackend, then spawn a task that drains
            // `backend.stream_log()` and emits one `session_log:line` Tauri
            // event per LogLine (payload shape: spec §3.3).
            //
            // It is deliberately NOT wired here yet because:
            //   1. `PatBackend::spawn` (the full-lifecycle constructor) is not
            //      implemented — only `PatBackend::from_url` exists today.
            //   2. The spawn path reads keyring credentials and launches a
            //      process that can initiate a CMS session — a live-Pat /
            //      Part-97-adjacent surface a headless build must not exercise
            //      to "verify completion" (CLAUDE.md live-radio rule).
            //
            // Leaving AppBackend `None` is the graceful default: every command
            // degrades to `NotConfigured` → empty state. The model + trait +
            // commands + sidebar/list + AppShell (the Task-12 gate for Tasks
            // 13/14) are complete without it; the live spawn is a follow-up
            // (see PR body / handoff). The emit-per-LogLine glue is provided
            // by `crate::ui_commands` consumers once the backend exists.
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
            crate::ui_commands::config_read,           // Task 16 (tuxlink-hvv)
            crate::ui_commands::backend_status,        // Task 16 (tuxlink-hvv)
            crate::ui_commands::session_log_snapshot,  // Task 15 (tuxlink-8zg integration round)
            crate::compose_window::compose_window_open, // Task 14 (tuxlink-dm8)
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
