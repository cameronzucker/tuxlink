// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = tuxlink_lib::uninstall_cleanup::handle_cli(std::env::args_os()) {
        std::process::exit(code);
    }
    tuxlink_lib::run()
}
