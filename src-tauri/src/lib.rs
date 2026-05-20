pub mod app_backend;
pub mod compose_window;
pub mod config;
pub mod menu;
pub mod pat_client;
pub mod pat_config;
pub mod pat_process;
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
        .invoke_handler(tauri::generate_handler![
            greet,
            crate::wizard::get_wizard_completed,
            crate::wizard::wizard_persist_cms,
            crate::wizard::wizard_persist_offline,
            crate::wizard::wizard_run_test_send,
            // Task 12 (tuxlink-zsm). Tasks 13/14/16's commands are registered
            // in the orchestrator integration commit (spec §4.3), not here.
            crate::ui_commands::mailbox_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
