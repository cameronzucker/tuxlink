pub mod app_backend;
pub mod bootstrap;
pub mod catalog;
pub mod compose_window;
pub mod config;
pub mod consent_gate;
pub mod forms;
pub mod grib;
pub mod native_mailbox;
pub mod position;
pub mod search;
pub mod session_log;
pub mod tray;
pub mod ui_commands;
pub mod user_folders;
pub mod winlink;
pub mod winlink_backend;
pub mod wizard;
pub mod modem_commands;
pub mod modem_status;

#[cfg(test)]
pub mod test_helpers;

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

    // Task 5 (tuxlink-686): build the PositionArbiter before the Builder so
    // the `let` binding stays alive for Task 11's gpsd clone.
    // Bootstrap from config; fall back gracefully (pre-wizard = no config file)
    // to GPS/None/FourCharGrid — the app always launches.
    let arbiter = {
        let (src, grid, prec) = crate::config::read_config()
            .map(|c| (c.privacy.position_source, c.identity.grid, c.privacy.position_precision))
            .unwrap_or((
                crate::config::PositionSource::Gps,
                None,
                crate::config::PositionPrecision::FourCharGrid,
            ));
        std::sync::Arc::new(crate::position::PositionArbiter::new(src, grid, prec))
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // tuxlink-0fyj: native Save As dialog for attachment download. Frontend
        // shows the dialog, then invokes `message_attachment_save` which writes
        // the decoded bytes to the chosen path on the Rust side (no IPC of the
        // attachment body).
        .plugin(tauri_plugin_dialog::init())
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
        // Task 5 (tuxlink-686): managed PositionArbiter — shared by config_set_grid
        // and (Task 11) the gpsd task. Built above the Builder so the binding
        // remains available for Task 11's clone. `.clone()` here increments the
        // Arc ref-count; the binding `arbiter` stays alive for Task 11 wiring.
        .manage(arbiter.clone())
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
        // The backend appends here AND broadcasts on `session_log:line`; this
        // managed state lets `session_log_snapshot` serve late-mounting UIs
        // without losing startup lines (spec §11.1 / adrev #1,#2,#3).
        // Cap 500: ≈ one extended CMS session's worth of log lines.
        //
        // Wrapped in an `Arc` (Task C, tuxlink-22l §11.2) so the backend's
        // bridge thread can hold a clone of the SAME buffer that
        // `session_log_snapshot` reads. Tauri's `State` derefs through the
        // `Arc`, so the command sees an identical surface.
        .manage(std::sync::Arc::new(crate::session_log::SessionLogState::new(500)))
        // tuxlink-4ek Phase 3: the shared modem session — current ARDOP status
        // snapshot + the RADIO-1 consent token. Stored as `Arc<ModemSession>`
        // so command handlers and the broadcaster (Task 3.4) reference the
        // same instance. Starts Stopped with no token (mint via the RADIO-1
        // modal flow).
        .manage(std::sync::Arc::new(crate::modem_status::ModemSession::new()))
        // tuxlink-dfmf: VARA session state — holds the TCP transport handle
        // + a status snapshot. Mutex-protected so concurrent UI commands
        // (start/stop/status) serialize on the transport handle. Phase 2
        // minimal surface; full session state machine arrives in Phase 3.
        .manage(std::sync::Arc::new(crate::winlink::modem::vara::VaraSession::new()))
        // tuxlink-0pnb: P2P-Telnet single-flight + abort coordination (mirrors
        // NativeBackend's connect_in_progress + aborting flags, but held in
        // managed state because P2P bypasses WinlinkBackend entirely).
        .manage(crate::ui_commands::P2pConnectState {
            in_progress: std::sync::atomic::AtomicBool::new(false),
            aborting: std::sync::atomic::AtomicBool::new(false),
        })
        .setup(|app| {
            use tauri::Manager as _;  // brings .state() into scope for the setup closure
            // Install system tray icon + menu (tuxlink-rit / Task 8).
            // Close-to-tray: window close button hides to tray; only
            // File→Quit / tray→Quit / Ctrl+Q actually exit the process.
            crate::tray::install(app.handle())?;

            // Task 10 (tuxlink-1hu): register the find-messages SearchService.
            // search.db + saved-searches.json live alongside the native mailbox
            // in <app_data>/native-mbox/. Failure is non-fatal — the search UI
            // degrades gracefully (empty results); the app always launches.
            match app.path().app_data_dir() {
                Ok(data_dir) => {
                    let search_root = data_dir.join("native-mbox");
                    // Ensure the directory exists before opening SQLite (Index::open
                    // calls Connection::open, which creates the .db file but expects
                    // the parent directory to already exist).
                    if let Err(e) = std::fs::create_dir_all(&search_root) {
                        eprintln!("search: could not create native-mbox dir: {e}");
                    } else {
                        match crate::search::build_service(&search_root) {
                            Ok(svc) => { app.manage(svc); }
                            Err(e) => eprintln!("search: build_service failed: {e}"),
                        }
                    }
                }
                Err(e) => eprintln!("search: could not resolve app_data_dir: {e}"),
            }

            // Task D (tuxlink-22l) app-start backend bootstrap (spec §3.3). Runs
            // OFF the main thread (a dedicated std::thread inside
            // `bootstrap::run`) so the webview paints immediately. The worker:
            //   - classifies `read_config()` via `bootstrap_decision` (adrev
            //     #14,#15: pre-wizard + offline → NotConfigured; malformed
            //     config → ConfigError; only `wizard_completed && connect_to_cms`
            //     installs the native backend),
            //   - installs NativeBackend in `BackendState` (→ Ready), and starts
            //     the session-log drain task (`tauri::async_runtime::spawn`,
            //     adrev #5) that emits one `session_log:line` event per `LogLine`.
            // ALL paths are non-fatal: the app always launches. A spawn/health
            // failure shows as an EXPLICIT error in the ribbon + session-log
            // pane (BackendPhase::Failed), not a silent empty state (spec §2).
            crate::bootstrap::run(app.handle().clone());

            // tuxlink-686 Task 11 / Codex P1-A defense-in-depth: only spawn the
            // gpsd reader when GPS is permitted. gps_state=Off means the operator
            // has disabled GPS entirely — we must not even open the gpsd socket.
            // LocalUiOnly + BroadcastAtPrecision both read; the broadcast gate is
            // in effective_broadcast_locator. Pre-wizard (no config file) defaults
            // to running (GPS-on-by-default convention; wizard completes the config).
            let gps_permitted = crate::config::read_config()
                .map(|c| c.privacy.gps_state != crate::config::GpsState::Off)
                .unwrap_or(true);
            if gps_permitted {
                let arbiter_for_gpsd =
                    (*app.state::<std::sync::Arc<crate::position::PositionArbiter>>()).clone();
                crate::position::gpsd::spawn_gpsd_client(arbiter_for_gpsd);
            }

            // tuxlink-4ek Task 3.4: spawn the modem status broadcaster — a
            // dedicated std::thread (named "modem-status-broadcaster") that
            // polls the shared ModemSession snapshot every 250 ms and emits
            // it as the `modem:status` Tauri event the frontend's
            // `useModemStatus` hook (Task 1.3) listens to. JoinHandle is
            // intentionally dropped: the thread runs for the lifetime of the
            // process (v1 has no shutdown signal; the broadcaster owns no
            // transport state so the clean-shutdown cost isn't worth it
            // yet). No tokio (ADR 0015 — modem subsystem uses std::sync /
            // std::thread primitives only).
            let session_for_broadcaster =
                (*app.state::<std::sync::Arc<crate::modem_status::ModemSession>>()).clone();
            let app_handle_for_broadcaster = app.handle().clone();
            let _broadcaster_handle = crate::modem_status::ModemStatusBroadcaster::spawn(
                session_for_broadcaster,
                move |s| {
                    // Bring `tauri::Emitter` into the closure scope so `emit`
                    // resolves on `AppHandle`. `Manager` (already imported at
                    // the top of this setup block) does NOT provide `emit` —
                    // that's `Emitter`'s extension trait (Tauri 2.x).
                    use tauri::Emitter as _;
                    let _ = app_handle_for_broadcaster
                        .emit(crate::modem_status::STATUS_EVENT, s);
                },
            );

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
            crate::wizard::verify_cms_connection,   // Task 5.4 (tuxlink-9phd): replaces wizard_run_test_send
            // Main-UI cluster commands. Task 12 (tuxlink-zsm) created
            // `mailbox_list`; Tasks 13/14/16 appended their command fns to
            // `ui_commands.rs` but deferred registration to this single
            // orchestrator integration commit (spec §4.3) to keep the shared
            // `invoke_handler` edit in one diff.
            crate::ui_commands::mailbox_list,          // Task 12 (tuxlink-zsm)
            crate::ui_commands::mailbox_move,          // tuxlink-ca5x (user-folders Phase 1)
            crate::ui_commands::user_folders_list,     // tuxlink-f62f (user-folders Phase 2)
            crate::ui_commands::folder_create,         // tuxlink-f62f
            crate::ui_commands::folder_delete,         // tuxlink-f62f
            crate::ui_commands::folder_rename,         // tuxlink-ejph (Phase 3)
            crate::ui_commands::message_read,          // Task 13 (tuxlink-y5c)
            crate::ui_commands::message_attachment_save, // tuxlink-0fyj (Save As attachment)
            crate::ui_commands::message_send,          // Task 14 (tuxlink-dm8)
            crate::ui_commands::send_form,             // HTML Forms v0.1 (tuxlink-v1p Task 3.1)
            crate::ui_commands::cms_connect,           // tuxlink-0ic (native connect)
            crate::ui_commands::cms_abort,             // tuxlink-9z2 (abort in-flight connect)
            crate::ui_commands::config_read,           // Task 16 (tuxlink-hvv)
            crate::ui_commands::backend_status,        // Task 16 (tuxlink-hvv)
            crate::ui_commands::session_log_snapshot,  // Task 15 (tuxlink-8zg integration round)
            crate::ui_commands::session_log_clear,     // Operator smoke 2026-05-31 — buffer drain
            crate::compose_window::compose_window_open, // Task 14 (tuxlink-dm8)
            crate::compose_window::compose_close_self,  // tuxlink-h2y (self-only close)
            crate::ui_commands::app_quit,             // tuxlink-ng3 (HTML File→Quit / Ctrl+Q)
            crate::ui_commands::packet_config_get,    // tuxlink-7fr (packet config read)
            crate::ui_commands::packet_config_set,    // tuxlink-7fr (packet config write)
            crate::ui_commands::packet_connect,       // tuxlink-7fr (packet dial)
            crate::ui_commands::packet_listen,        // tuxlink-7fr (arm Listen — answer inbound)
            crate::ui_commands::packet_set_listen,    // tuxlink-7fr (sticky listen)
            crate::ui_commands::packet_allowed_stations_get,         // tuxlink-inde (allowlist read)
            crate::ui_commands::packet_allowed_stations_add,         // tuxlink-inde (allowlist add)
            crate::ui_commands::packet_allowed_stations_remove,      // tuxlink-inde (allowlist remove)
            crate::ui_commands::packet_allowed_stations_set_allow_all, // tuxlink-inde (allow_all toggle)
            crate::ui_commands::packet_list_serial_devices, // tuxlink-7fr (USB/BT device picker)
            crate::ui_commands::packet_list_bluetooth_devices, // tuxlink-mqu3 (BT-MAC picker restoration)
            crate::ui_commands::ardop_list_audio_devices,   // tuxlink-y7x7 (ARDOP ALSA picker restoration)
            crate::ui_commands::config_set_grid,      // Task 5 (tuxlink-686)
            crate::ui_commands::position_set_source,  // Task 11 (tuxlink-686)
            crate::ui_commands::position_status,      // Task 11 (tuxlink-686)
            crate::ui_commands::config_set_privacy,    // tuxlink-39b (GPS privacy control surface)
            crate::ui_commands::config_set_connect,    // tuxlink-3o0 (CMS server endpoint control)
            // Task 10 (tuxlink-1hu): find-messages search commands
            crate::search::commands::tauri_search_run,
            crate::search::commands::tauri_search_list_saved,
            crate::search::commands::tauri_search_list_recent,
            crate::search::commands::tauri_search_save,
            crate::search::commands::tauri_search_unsave,
            crate::search::commands::tauri_search_promote_recent,
            crate::search::commands::tauri_search_rename,
            crate::search::commands::tauri_search_reorder,
            crate::search::commands::tauri_search_record_recent,
            crate::search::commands::tauri_search_clear_recent,
            crate::search::commands::tauri_search_rebuild_index,
            // tuxlink-ddiq: WLE catalog-request (Inquiry) framework. Bundled
            // catalog file + in-band INQUIRY@winlink.org composer/sender.
            crate::catalog::commands::catalog_list,
            crate::catalog::commands::catalog_send_inquiry,
            // tuxlink-vrpk: GRIB request via Saildocs (3rd-party SMTP).
            crate::grib::commands::grib_send_request,
            crate::modem_commands::config_get_ardop,   // tuxlink-4ek (ARDOP config read)
            crate::modem_commands::config_set_ardop,   // tuxlink-4ek (ARDOP config write)
            crate::modem_commands::modem_get_status,   // tuxlink-4ek Task 3.2 (session snapshot)
            crate::modem_commands::modem_ardop_disconnect, // tuxlink-4ek Task 3.2 (clear consent + reset)
            crate::modem_commands::modem_ardop_connect, // tuxlink-4ek Task 3.3 (RADIO-1-gated spawn + ARQ connect)
            crate::modem_commands::modem_mint_consent, // tuxlink-4ek Task 6.2 (RADIO-1 token mint — backend-only)
            crate::modem_commands::modem_ardop_b2f_exchange, // tuxlink-ytg (B2F over ARDOP — Winlink mail flows)
            // tuxlink-dfmf Phase 2: VARA UI wiring. Minimal TCP-transport lifecycle —
            // open/close/status — plus persisted config + the Pi-availability gating
            // probe. RF connect-to-peer (RADIO-1-gated) lives in a Phase 3 follow-up.
            crate::winlink::modem::vara::commands::config_get_vara,
            crate::winlink::modem::vara::commands::config_set_vara,
            crate::winlink::modem::vara::commands::vara_start_session,
            crate::winlink::modem::vara::commands::vara_stop_session,
            crate::winlink::modem::vara::commands::vara_status,
            crate::winlink::modem::vara::commands::platform_info,
            // tuxlink-0pnb Task 4 (refactored): P2P-Telnet connect + abort + peer-password management.
            // telnet_p2p_dial renamed to telnet_p2p_connect (StatusBar pipeline wiring);
            // telnet_p2p_abort added to mirror cms_abort (operator cancel semantics).
            crate::ui_commands::telnet_p2p_connect,
            crate::ui_commands::telnet_p2p_abort,
            crate::ui_commands::p2p_peer_password_set,
            crate::ui_commands::p2p_peer_password_clear,
            crate::ui_commands::p2p_peer_password_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
