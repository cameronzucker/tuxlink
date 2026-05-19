pub mod config;
pub mod menu;
pub mod pat_client;
pub mod pat_config;
pub mod pat_process;
pub mod tray;
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

            Ok(())
        })
        .on_window_event(|window, event| {
            // Task 8 — close-to-tray: intercept the window X / Alt-F4 path
            // and hide instead of closing. The process + Pat child stay alive.
            // Only the Quit menu item (menu:file:quit / tray:quit) calls
            // app.exit(0), which bypasses this handler entirely.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            crate::wizard::get_wizard_completed,
            crate::wizard::wizard_persist_cms,
            crate::wizard::wizard_persist_offline,
            crate::wizard::wizard_run_test_send,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
