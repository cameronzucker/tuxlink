pub mod app_backend;
pub mod bootstrap;
pub mod catalog;
pub mod contacts;
pub mod compose_window;
pub mod config;
pub mod consent_gate;
pub mod favorites;
pub mod forms;
pub mod grib;
pub mod help_window;
pub mod logging;
pub mod logging_window;
pub mod theme_state;
pub mod native_mailbox;
pub mod position;
pub mod search;
pub mod session_log;
pub mod session_log_emit;
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
        // tuxlink-0fyj/tuxlink-ewtb: native Save As dialog for attachment
        // download plus explicit image preview. Save writes decoded bytes to
        // the chosen path; preview returns a bounded image payload on demand.
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
        // tuxlink-0gsy (spec §8.2): managed theme-state singleton — the help
        // window calls theme_get_scheme to bootstrap and listens for
        // color_scheme_changed events emitted by theme_broadcast_scheme.
        .manage(crate::theme_state::ThemeState::default())
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
        // Task A (tuxlink-22l): durable session-log history. The bridge
        // appends here AND broadcasts on `session_log:line`; this managed
        // state lets `session_log_snapshot` and logging export retain the
        // complete operator-visible session history. The radio panel applies
        // the visible-row cap; retention stays complete until Clear.
        //
        // Wrapped in an `Arc` (Task C, tuxlink-22l §11.2) so the backend's
        // bridge thread can hold a clone of the SAME buffer that
        // `session_log_snapshot` reads. Tauri's `State` derefs through the
        // `Arc`, so the command sees an identical surface.
        .manage(std::sync::Arc::new(crate::session_log::SessionLogState::unbounded()))
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
        // tuxlink-xehu: Telnet-P2P listener shared state — the in-flight
        // listener's shutdown flag + bound socket addr. None when no listener
        // is armed; Some(...) when one is running.
        .manage(std::sync::Arc::new(crate::ui_commands::TelnetListenState::default()))
        // tuxlink-61yg: ARDOP listener shared state — the in-flight consumer
        // task's shutdown flag.
        .manage(std::sync::Arc::new(crate::ui_commands::ArdopListenState::default()))
        // HTML Forms P1 Task 8 (tuxlink-tzr5; original plan tuxlink-ytya): the
        // shared registry of open `forms::http_server::FormSession`s. Owned
        // here (process-lifetime) so the open + close Tauri commands and
        // their forwarder tasks all reference the same map. Each open mints
        // a fresh ephemeral port + a 16-hex-char token; close drops the
        // session (its Drop impl aborts the serve task and releases the
        // port).
        .manage(std::sync::Arc::new(
            crate::forms::http_server::FormSessionRegistry::new(),
        ))
        // tuxlink-9ls2: VARA listener shared state — the in-flight consumer
        // task's shutdown flag. Mirrors the ARDOP listener; VARA differs only
        // in that the transport is externally-managed (operator must
        // vara_open_session before vara_listen can arm).
        .manage(std::sync::Arc::new(crate::ui_commands::VaraListenState::default()))
        // tuxlink-a2gd: polite station-list cache (TTL 30 min; per-key coalescing +
        // stale-on-error). Needs no app_data_dir, so it lives in the top-level chain
        // (NOT the .setup() app_data_dir arm where per-feature stores register).
        .manage(std::sync::Arc::new(
            crate::catalog::stations_cache::StationsCache::new(
                30 * 60 * 1000, // TTL: 30 min
                15 * 60 * 1000, // min-refetch floor: 15 min (bounds retries during an outage)
                std::sync::Arc::new(crate::catalog::stations_cache::SystemClock),
            ),
        ))
        .setup(|app| {
            use tauri::Manager as _;  // brings .state() into scope for the setup closure

            // alpha-logging (tuxlink-qjgx Task 6): initialize the tracing pipeline.
            // Pull the already-managed SessionLogState Arc, then init the full
            // subscriber composition (Filter + Fanout layers), disk consumer,
            // free-disk guard. Amendment D: fails soft — Degraded means the app
            // continues without disk logging; Full installs the WorkerGuard.
            {
                let session_log = (*app.state::<std::sync::Arc<crate::session_log::SessionLogState>>()).clone();
                match crate::logging::init(session_log) {
                    crate::logging::InitOutcome::Full(handle_arc) => {
                        // handle_arc is Arc<LoggingHandle> — init() wraps it so the
                        // bounded_timer clone doesn't cause a try_unwrap panic.
                        app.manage(handle_arc.clone());
                        // Amendment E.5.8: spawn probe runner that fires on first_paint_complete.
                        crate::logging::env_probes::spawn_runner(
                            app.handle().clone(),
                            handle_arc,
                        );
                    }
                    crate::logging::InitOutcome::Degraded { reason } => {
                        app.manage(crate::logging::DegradedHandle { reason: reason.clone() });
                        eprintln!("tuxlink: logging degraded — {reason}");
                        // No probe runner in degraded mode (no LoggingHandle to pass).
                    }
                }
            }

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
                    // contacts (tuxlink-raez, Task A2): the contacts.json address
                    // book store. `ContactsStore::open` is INFALLIBLE (degrades to
                    // an empty store on a read/parse error, preserving the corrupt
                    // bytes) so it is UNCONDITIONALLY managed — no guard branch,
                    // never blocks startup. Reuses the already-resolved `data_dir`
                    // (C2: app_data_dir resolved ONCE here, not per-command).
                    app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                        crate::contacts::store::ContactsStore::open(
                            data_dir.join("contacts.json"),
                        ),
                    )));

                    // favorites (tuxlink-egmp, Task B2): the stations.json
                    // per-radio-mode Favorites/Recents store. `FavoritesStore::open`
                    // is INFALLIBLE (same degrade-and-preserve contract as
                    // ContactsStore) so it is UNCONDITIONALLY managed — no guard
                    // branch, no startup block. Reuses the already-resolved
                    // `data_dir` (C2: app_data_dir resolved ONCE here).
                    app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                        crate::favorites::store::FavoritesStore::open(
                            data_dir.join("stations.json"),
                        ),
                    )));

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

                        // tuxlink-hnkn P2 Task 4: FormDraftLibrary — named slot
                        // store for save/reuse of form field-value sets. Lives as a
                        // sibling SQLite file to search.db (Option B schema-home
                        // decision: independent lifecycle, survives search-index
                        // rebuild). Open failure is non-fatal at launch — the app
                        // still starts. But subsequent IPC calls to
                        // form_draft_library_* will error at State<Arc<DraftLibrary>>
                        // extraction because .manage() never ran. This matches the
                        // SearchService precedent above.
                        match crate::forms::draft_library::DraftLibrary::open(
                            search_root.join("form_draft_library.db"),
                        ) {
                            Ok(lib) => {
                                app.manage(std::sync::Arc::new(lib));
                            }
                            Err(e) => eprintln!("form-draft-library: open failed: {e}"),
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
            crate::ui_commands::message_attachment_preview, // tuxlink-ewtb (image attachment preview)
            crate::ui_commands::message_attachment_save, // tuxlink-0fyj (Save As attachment)
            crate::ui_commands::message_send,          // Task 14 (tuxlink-dm8)
            crate::ui_commands::send_form,             // HTML Forms v0.1 (tuxlink-v1p Task 3.1)
            // HTML Forms P1 Task 8 (tuxlink-tzr5; original plan tuxlink-ytya):
            // webview-form command surface — catalog list + per-open
            // http_server lifecycle.
            crate::ui_commands::forms_list_catalog,
            crate::ui_commands::open_webview_form,
            crate::ui_commands::close_webview_form_server,
            // HTML Forms Phase 3 (tuxlink-xipa): runtime-updateable WLE
            // Standard Forms snapshot — operator-triggered refresh via
            // CatalogBrowser "Refresh forms…" affordance. Check is read-only;
            // refresh performs the download + atomic swap (forms::updater).
            crate::ui_commands::forms_check_for_update,
            crate::ui_commands::forms_refresh,
            // HTML Forms P1 Task 10 critical-fix (tuxlink-tzr5): catalog-form
            // submit pathway — `send_form` only knows the 5 BUNDLED_FORMS
            // FormDefs; the webview path needs a parallel command that
            // synthesizes the XML envelope from field_values + WLE conventions.
            crate::ui_commands::send_webview_form,
            // HTML Forms P1 Task 11 (tuxlink-tzr5): receive-side Viewer fallback.
            // MessageView calls this for messages whose form_id has no native
            // React View component; the http_server serves the WLE
            // *_Viewer.html with the parsed FormPayload bound into its
            // {var X} placeholders + hidden inputs.
            crate::ui_commands::open_webview_viewer,
            crate::ui_commands::cms_connect,           // tuxlink-0ic (native connect)
            crate::ui_commands::cms_abort,             // tuxlink-9z2 (abort in-flight connect)
            crate::ui_commands::config_read,           // Task 16 (tuxlink-hvv)
            crate::ui_commands::backend_status,        // Task 16 (tuxlink-hvv)
            crate::ui_commands::session_log_snapshot,  // Task 15 (tuxlink-8zg integration round)
            crate::ui_commands::session_log_clear,     // Operator smoke 2026-05-31 — buffer drain
            crate::compose_window::compose_window_open, // Task 14 (tuxlink-dm8)
            crate::compose_window::compose_close_self,  // tuxlink-h2y (self-only close)
            crate::help_window::help_window_open,       // tuxlink-0gsy (spec §3)
            crate::theme_state::theme_get_scheme,       // tuxlink-0gsy (spec §8.2)
            crate::theme_state::theme_broadcast_scheme, // tuxlink-0gsy (spec §8.2)
            crate::search::commands::docs_search,       // tuxlink-0gsy (spec §9.3)
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
            // tuxlink-dhbl: ARDOP P2P listener — allowed-stations + arms + listen toggle.
            crate::ui_commands::ardop_listen,
            crate::ui_commands::ardop_set_listen,
            crate::ui_commands::ardop_allowed_stations_get,
            crate::ui_commands::ardop_allowed_stations_add,
            crate::ui_commands::ardop_allowed_stations_remove,
            crate::ui_commands::ardop_allowed_stations_set_allow_all,
            // tuxlink-9ls2: VARA P2P listener — same shape as ARDOP but the
            // operator-managed transport requires vara_open_session first.
            crate::ui_commands::vara_listen,
            crate::ui_commands::vara_set_listen,
            crate::ui_commands::vara_allowed_stations_get,
            crate::ui_commands::vara_allowed_stations_add,
            crate::ui_commands::vara_allowed_stations_remove,
            crate::ui_commands::vara_allowed_stations_set_allow_all,
            crate::ui_commands::config_set_grid,      // Task 5 (tuxlink-686)
            crate::ui_commands::position_set_source,  // Task 11 (tuxlink-686)
            crate::ui_commands::position_status,      // Task 11 (tuxlink-686)
            crate::ui_commands::position_current_fix, // tuxlink-hnkn P2 (PositionFormV2 pre-fill)
            crate::ui_commands::messages_meta_query_for_log, // tuxlink-hnkn P2 Task 2 (ICS-309 log query)
            crate::ui_commands::render_ics309_pdf,            // tuxlink-hnkn P2 Task 2 (ICS-309 PDF export)
            crate::ui_commands::config_set_privacy,    // tuxlink-39b (GPS privacy control surface)
            crate::ui_commands::config_set_connect,    // tuxlink-3o0 (CMS server endpoint control)
            // tuxlink-6c9y (Task A7): Network PO relay favorites — persist in config.
            crate::ui_commands::network_po_favorites_get,
            crate::ui_commands::network_po_favorites_add,
            crate::ui_commands::network_po_favorites_remove,
            crate::ui_commands::network_po_favorites_set,
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
            // tuxlink-a2gd: location-aware station-list direct poll + reply parse-with-fallback.
            crate::catalog::commands::catalog_fetch_stations,
            crate::catalog::commands::catalog_parse_reply,
            // tuxlink-vrpk: GRIB request via Saildocs (3rd-party SMTP).
            crate::grib::commands::grib_send_request,
            crate::modem_commands::config_get_ardop,   // tuxlink-4ek (ARDOP config read)
            crate::modem_commands::config_set_ardop,   // tuxlink-4ek (ARDOP config write)
            crate::modem_commands::modem_get_status,   // tuxlink-4ek Task 3.2 (session snapshot)
            crate::modem_commands::modem_ardop_disconnect, // tuxlink-4ek Task 3.2 (clear consent + reset)
            crate::modem_commands::modem_ardop_connect, // tuxlink-4ek Task 3.3 (RADIO-1-gated spawn + ARQ connect)
            crate::modem_commands::modem_ardop_b2f_exchange, // tuxlink-ytg (B2F over ARDOP — Winlink mail flows)
            // tuxlink-0ye6 Task 3.5: ARDOP session lifecycle commands —
            // ardop_open_session spawns ardopcf + records (intent, transport_kind)
            // + auto-arms the listener iff intent calls for it; ardop_close_session
            // disarms + aborts + clears active mode + tears down the transport.
            // modem_ardop_connect / modem_ardop_disconnect stay registered for the
            // Connect button's path until Task 3.6 widens b2f_exchange.
            crate::modem_commands::ardop_open_session,
            crate::modem_commands::ardop_close_session,
            // tuxlink-dfmf Phase 2: VARA UI wiring. Minimal TCP-transport lifecycle —
            // open/close/status — plus persisted config + the Pi-availability gating
            // probe. RF connect-to-peer (RADIO-1-gated) lives in a Phase 3 follow-up.
            crate::winlink::modem::vara::commands::config_get_vara,
            crate::winlink::modem::vara::commands::config_set_vara,
            crate::winlink::modem::vara::commands::vara_open_session,
            crate::winlink::modem::vara::commands::vara_close_session,
            crate::winlink::modem::vara::commands::vara_status,
            crate::winlink::modem::vara::commands::platform_info,
            // tuxlink-0ye6 Task 3.4: VARA dial-path B2F exchange — CONNECT to peer
            // + B2F handshake + intent-filtered mailbox drain + DISCONNECT, all
            // in one Tauri call. Mirror of `modem_ardop_b2f_exchange`'s shape.
            crate::winlink::modem::vara::commands::modem_vara_b2f_exchange,
            // tuxlink-0pnb Task 4 (refactored): P2P-Telnet connect + abort + peer-password management.
            // telnet_p2p_dial renamed to telnet_p2p_connect (StatusBar pipeline wiring);
            // telnet_p2p_abort added to mirror cms_abort (operator cancel semantics).
            crate::ui_commands::telnet_p2p_connect,
            crate::ui_commands::telnet_p2p_abort,
            crate::ui_commands::p2p_peer_password_set,
            crate::ui_commands::p2p_peer_password_clear,
            crate::ui_commands::p2p_peer_password_status,
            // tuxlink-xehu: Telnet-P2P listener — allowlist + keyring station password +
            // arm/disarm with TTL. Wire spec: dev/scratch/winlink-re/findings/telnet-p2p.md.
            crate::ui_commands::telnet_listen,
            crate::ui_commands::telnet_set_listen,
            crate::ui_commands::telnet_allowed_stations_get,
            crate::ui_commands::telnet_allowed_stations_add_callsign,
            crate::ui_commands::telnet_allowed_stations_remove_callsign,
            crate::ui_commands::telnet_allowed_stations_add_ip,
            crate::ui_commands::telnet_allowed_stations_remove_ip,
            crate::ui_commands::telnet_allowed_stations_set_allow_all,
            crate::ui_commands::telnet_station_password_set,
            crate::ui_commands::telnet_station_password_clear,
            crate::ui_commands::telnet_station_password_is_set,
            crate::ui_commands::telnet_listen_config_get,
            crate::ui_commands::telnet_listen_config_set,
            // alpha-logging (tuxlink-qjgx Task 6): Logging window + commands.
            crate::logging_window::logging_window_open,
            crate::logging::commands::logging_status,
            crate::logging::commands::logging_set_detailed_mode,
            crate::logging::commands::logging_set_retention,
            crate::logging::commands::logging_export,
            crate::logging::commands::logging_open_directory,
            crate::logging::commands::logging_clear_history,
            crate::logging::commands::logging_env_probes_snapshot,
            crate::logging::commands::logging_env_probes_rerun,
            crate::logging::commands::emit_first_paint_complete,   // Amendment E.7.7
            crate::logging::commands::report_issue_flow,           // Task 8 — Report Issue
            // tuxlink-hnkn P2 Task 4: FormDraftLibrary — save/reuse named form slots.
            crate::ui_commands::form_draft_library_list,
            crate::ui_commands::form_draft_library_upsert,
            crate::ui_commands::form_draft_library_delete,
            // tuxlink-7do4 Task 13: smart-auth-diagnostics banner recovery commands.
            crate::ui_commands::credentials_write_password, // spec §4.3 (i) — Mode 3 re-enter password
            crate::ui_commands::wizard_reopen,              // spec §4.3 (ii) — Mode 4 try different callsign
            crate::ui_commands::auth_diagnostic_clear,      // spec §4.3 (v) — banner Dismiss
            // tuxlink-7do4 Task 14: auth-only credential test command.
            crate::ui_commands::cms_connect_test,           // spec §4.3 (iii) — "Check this password works"
            // contacts (tuxlink-raez, Task A2): address-book CRUD over the
            // managed `Arc<Mutex<ContactsStore>>`. Mutations stamp id/timestamps
            // in the command layer and emit the `contacts:changed` cross-window
            // event (H9). KEEP THIS BLOCK CONTIGUOUS + LABELED — the favorites
            // (tuxlink-egmp) block appends adjacent; the merge is a clean
            // concatenation.
            crate::contacts::commands::contacts_read,
            crate::contacts::commands::contact_upsert,
            crate::contacts::commands::contact_delete,
            crate::contacts::commands::group_upsert,
            crate::contacts::commands::group_delete,
            crate::contacts::commands::contacts_suggestions, // Task A3: suggest-from-history
            // favorites (tuxlink-egmp, Task B2): per-radio-mode Favorites/Recents
            // CRUD + the honest connection record, over the managed
            // `Arc<Mutex<FavoritesStore>>`. `favorite_upsert` MERGES only
            // operator-editable fields (M12) so a stale whole-object upsert can't
            // clobber a concurrent star; `favorite_star` /
            // `favorite_record_attempt` are the only writers of starred /
            // last_attempt_at / the log. No cross-window event (single-window
            // radio-dock surface). KEEP THIS BLOCK CONTIGUOUS + LABELED.
            crate::favorites::commands::favorites_read,
            crate::favorites::commands::favorite_upsert,
            crate::favorites::commands::favorite_delete,
            crate::favorites::commands::favorite_star,
            crate::favorites::commands::favorite_record_attempt,
            crate::favorites::commands::favorites_recents,
            crate::favorites::commands::favorite_tod_hint,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
