pub mod config;
pub mod menu;
pub mod pat_client;
pub mod pat_config;
pub mod pat_process;
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
    tauri::Builder::default()
        .manage(crate::wizard::WizardMutex::new())
        .setup(|app| {
            // Build the native OS menu bar (tuxlink-6vi / Task 7) and wire
            // its events to Tauri IPC so the React frontend can listen on
            // the "menu" channel.
            let menu = crate::menu::build_menu(app.handle())?;
            app.set_menu(menu)?;
            crate::menu::wire_menu_events(app.handle());
            Ok(())
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
