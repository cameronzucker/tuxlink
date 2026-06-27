pub mod app_backend;
pub mod basemap;
pub mod bootstrap;
pub mod tiles;
pub mod catalog;
pub mod contacts;
pub mod compose_window;
pub mod config;
pub mod consent_gate;
pub mod favorites;
pub mod forms;
pub mod grib;
pub mod identity;
pub mod help_window;
pub mod logging;
pub mod logging_window;
pub mod media;
pub mod stations_window;
pub mod theme_state;
pub mod native_mailbox;
pub mod position;
pub mod search;
pub mod session_log;
pub mod session_log_emit;
pub mod tray;
pub mod ui_commands;
pub mod ui_core;
pub mod uninstall_cleanup;
pub mod user_folders;
pub mod winlink;
pub mod winlink_backend;
pub mod wizard;
pub mod mcp_ports;
pub mod modem_commands;
pub mod modem_status;
pub mod propagation;
pub mod mesh;

#[cfg(test)]
pub mod test_helpers;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn uninstall_cleanup_preview(
    mode: crate::uninstall_cleanup::CleanupMode,
) -> Result<crate::uninstall_cleanup::CleanupReport, String> {
    crate::uninstall_cleanup::preview_current_user_cleanup(mode)
}

#[tauri::command]
fn uninstall_cleanup_execute(
    mode: crate::uninstall_cleanup::CleanupMode,
) -> Result<crate::uninstall_cleanup::CleanupReport, String> {
    crate::uninstall_cleanup::execute_current_user_cleanup(mode)
}

/// GL rendering mode for the Linux WebKitGTK webview (tuxlink-4pdu).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlMode {
    /// Real GPU. On Raspberry Pi V3D this additionally needs
    /// `MESA_GLES_VERSION_OVERRIDE=3.2`: WebKitGTK/ANGLE's WebGL2 init wants GLES
    /// 3.2, V3D's Mesa advertises 3.1, and without the override ANGLE aborts with
    /// `GL_INVALID_OPERATION` → magenta canvas. With it, WebGL renders on the real
    /// V3D GPU (proven on a Pi 5: loader uses vc4/v3d, never llvmpipe). The "Apple
    /// GPU" renderer string the prior tuxlink-spo2 fix trusted is an ANGLE spoof
    /// (it shows even when GL works) and was a misdiagnosis.
    Hardware,
    /// llvmpipe CPU rasterizer — the safe fallback when hardware WebGL can't init.
    Software,
}

/// True when the device-tree model names a Raspberry Pi (the V3D/ANGLE WebGL2
/// version gate applies). Pure for testing; [`detect_raspberry_pi`] reads the model.
#[cfg(target_os = "linux")]
fn model_is_raspberry_pi(dt_model: Option<&str>) -> bool {
    dt_model.map(|m| m.contains("Raspberry Pi")).unwrap_or(false)
}

/// Read `/proc/device-tree/model` (NUL-terminated) and classify as Pi or not.
#[cfg(target_os = "linux")]
fn detect_raspberry_pi() -> bool {
    let bytes = std::fs::read("/proc/device-tree/model").ok();
    let model = bytes.as_ref().map(|b| String::from_utf8_lossy(b));
    model_is_raspberry_pi(model.as_deref())
}

/// Decide the GL mode from the explicit override, the safe-mode marker, and
/// whether this is a Pi. Pure — unit-tested.
///
/// - `TUXLINK_GL=software|hardware` (escape hatch) forces the mode.
/// - else (auto): a Pi whose previous hardware attempt never confirmed a render
///   (`marker_present`) drops to Software (safe mode); otherwise Hardware. Off-Pi
///   defaults to Hardware (native GPU GL; the marker only governs the Pi override).
#[cfg(target_os = "linux")]
fn decide_gl_mode(tuxlink_gl: Option<&str>, marker_present: bool, is_pi: bool) -> GlMode {
    match tuxlink_gl.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("software") | Some("sw") | Some("llvmpipe") => GlMode::Software,
        Some("hardware") | Some("hw") | Some("gpu") => GlMode::Hardware,
        _ if is_pi && marker_present => GlMode::Software,
        _ => GlMode::Hardware,
    }
}

/// Env vars to set for `mode` on this machine. Pure — unit-tested.
///
/// `WEBKIT_DISABLE_DMABUF_RENDERER` is always set (tuxlink-wfw first-frame static).
/// Software adds the llvmpipe pair (tuxlink-spo2 fallback). Hardware on a Pi adds
/// the GLES version override that passes ANGLE's WebGL2 gate (tuxlink-4pdu);
/// Hardware off-Pi adds nothing GL-specific (use the native driver — and notably
/// does NOT force llvmpipe, which the prior all-Linux software default wrongly did).
#[cfg(target_os = "linux")]
fn gl_env_vars(mode: GlMode, is_pi: bool) -> Vec<(&'static str, &'static str)> {
    let mut vars = vec![("WEBKIT_DISABLE_DMABUF_RENDERER", "1")];
    match mode {
        GlMode::Software => {
            vars.push(("LIBGL_ALWAYS_SOFTWARE", "1"));
            vars.push(("GALLIUM_DRIVER", "llvmpipe"));
        }
        GlMode::Hardware => {
            if is_pi {
                vars.push(("MESA_GLES_VERSION_OVERRIDE", "3.2"));
            }
        }
    }
    vars
}

/// Path of the safe-mode marker (XDG_STATE_HOME aware, default
/// `~/.local/state/tuxlink/gl-hardware-pending`). Armed when a hardware attempt
/// begins; cleared by [`gl_render_confirmed`] when the map renders a frame. Its
/// presence at startup means the previous hardware attempt never confirmed → the
/// next auto launch falls back to software (tuxlink-4pdu).
#[cfg(target_os = "linux")]
fn gl_safe_mode_marker_path() -> Option<std::path::PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| std::path::PathBuf::from(h).join(".local").join("state"))
        })?;
    Some(state.join("tuxlink").join("gl-hardware-pending"))
}

/// Decide + apply the WebKitGTK GL env BEFORE any webview/GL init, and manage the
/// safe-mode marker. tuxlink-4pdu (hardware recovery, guarded + self-healing) +
/// tuxlink-wfw (dmabuf) + tuxlink-spo2 (software fallback). Edition 2021 →
/// `set_var` is safe here (single-threaded startup, before the webview exists).
#[cfg(target_os = "linux")]
fn apply_linux_webview_gl_env() {
    let is_pi = detect_raspberry_pi();
    let marker = gl_safe_mode_marker_path();
    let marker_present = marker.as_ref().map(|p| p.exists()).unwrap_or(false);
    let tuxlink_gl = std::env::var("TUXLINK_GL").ok();
    let mode = decide_gl_mode(tuxlink_gl.as_deref(), marker_present, is_pi);

    for (key, value) in gl_env_vars(mode, is_pi) {
        std::env::set_var(key, value);
    }

    // Marker lifecycle: arm before a Pi hardware attempt (a launch that never
    // confirms a render disarms nothing → next auto launch is safe-mode software);
    // disarm whenever we run software (forced or fallen-back).
    if let Some(path) = marker {
        if mode == GlMode::Hardware && is_pi {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&path, b"hardware GL attempt; removed on confirmed map render\n");
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }

    tracing::info!(
        target: "tuxlink::gl",
        is_pi,
        ?mode,
        marker_present,
        tuxlink_gl = tuxlink_gl.as_deref().unwrap_or("(unset)"),
        "applied WebKitGTK GL env"
    );
}

/// Clear the safe-mode marker once the map confirms a successful GPU render
/// (tuxlink-4pdu). Invoked from the frontend on the map's first `load`. Best-effort
/// (a missing marker — e.g. software mode — is a harmless no-op).
#[cfg(target_os = "linux")]
#[tauri::command]
fn gl_render_confirmed() {
    if let Some(path) = gl_safe_mode_marker_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// No GL-env/marker management off Linux; the command exists so the frontend can
/// call it unconditionally.
#[cfg(not(target_os = "linux"))]
#[tauri::command]
fn gl_render_confirmed() {}

/// Extract a human-readable message from a panic payload (tuxlink-ebyt). Panics
/// carry `&str` (the common `panic!("msg")` / `unwrap`/`expect` case) or `String`;
/// anything else is reported generically rather than lost. Used by the panic hook
/// that forwards panics into the structured log.
fn panic_payload_string(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic payload>".to_string())
}

#[cfg(test)]
mod panic_payload_tests {
    use super::panic_payload_string;

    #[test]
    fn extracts_str_and_string_payloads_else_generic() {
        // &str payload (panic!("…"), unwrap/expect).
        assert_eq!(panic_payload_string(&"boom"), "boom");
        // String payload (panic!("{}", x)).
        assert_eq!(panic_payload_string(&String::from("dynamic boom")), "dynamic boom");
        // Non-string payload → generic, not lost.
        assert_eq!(panic_payload_string(&42_i32), "<non-string panic payload>");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_gl_env_tests {
    use super::{decide_gl_mode, gl_env_vars, model_is_raspberry_pi, GlMode};

    fn has(vars: &[(&'static str, &'static str)], key: &str) -> Option<&'static str> {
        vars.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    #[test]
    fn detects_raspberry_pi_from_model_string() {
        assert!(model_is_raspberry_pi(Some("Raspberry Pi 5 Model B Rev 1.0")));
        assert!(model_is_raspberry_pi(Some("Raspberry Pi 4 Model B")));
        assert!(!model_is_raspberry_pi(Some("Generic x86_64 Desktop")));
        assert!(!model_is_raspberry_pi(None));
    }

    #[test]
    fn explicit_override_forces_mode_regardless_of_pi_or_marker() {
        for &is_pi in &[true, false] {
            for &marker in &[true, false] {
                assert_eq!(decide_gl_mode(Some("software"), marker, is_pi), GlMode::Software);
                assert_eq!(decide_gl_mode(Some("hardware"), marker, is_pi), GlMode::Hardware);
                // aliases + case-insensitivity + whitespace
                assert_eq!(decide_gl_mode(Some("  SW "), marker, is_pi), GlMode::Software);
                assert_eq!(decide_gl_mode(Some("GPU"), marker, is_pi), GlMode::Hardware);
            }
        }
    }

    #[test]
    fn auto_mode_defaults_to_hardware_but_safe_modes_on_pi_after_failed_attempt() {
        // Pi, no prior failure → hardware (the recovered path, tuxlink-4pdu).
        assert_eq!(decide_gl_mode(None, false, true), GlMode::Hardware);
        // Pi, prior hardware attempt never confirmed → safe-mode software.
        assert_eq!(decide_gl_mode(None, true, true), GlMode::Software);
        // Off-Pi: native GPU GL; the Pi-only marker does not force software.
        assert_eq!(decide_gl_mode(None, true, false), GlMode::Hardware);
        assert_eq!(decide_gl_mode(None, false, false), GlMode::Hardware);
    }

    #[test]
    fn software_mode_sets_llvmpipe_and_dmabuf_disable() {
        let v = gl_env_vars(GlMode::Software, true);
        assert_eq!(has(&v, "WEBKIT_DISABLE_DMABUF_RENDERER"), Some("1"));
        assert_eq!(has(&v, "LIBGL_ALWAYS_SOFTWARE"), Some("1"));
        assert_eq!(has(&v, "GALLIUM_DRIVER"), Some("llvmpipe"));
        assert_eq!(has(&v, "MESA_GLES_VERSION_OVERRIDE"), None);
    }

    #[test]
    fn hardware_on_pi_sets_gles_override_and_no_software_force() {
        let v = gl_env_vars(GlMode::Hardware, true);
        // The fix: pass ANGLE's WebGL2 gate on V3D (tuxlink-4pdu).
        assert_eq!(has(&v, "MESA_GLES_VERSION_OVERRIDE"), Some("3.2"));
        // Must NOT force software — that was the misdiagnosis being reversed.
        assert_eq!(has(&v, "LIBGL_ALWAYS_SOFTWARE"), None);
        assert_eq!(has(&v, "GALLIUM_DRIVER"), None);
        // dmabuf-disable retained (tuxlink-wfw, separate first-frame-static bug).
        assert_eq!(has(&v, "WEBKIT_DISABLE_DMABUF_RENDERER"), Some("1"));
    }

    #[test]
    fn hardware_off_pi_uses_native_gl_no_override_no_software() {
        let v = gl_env_vars(GlMode::Hardware, false);
        assert_eq!(has(&v, "WEBKIT_DISABLE_DMABUF_RENDERER"), Some("1"));
        assert_eq!(has(&v, "MESA_GLES_VERSION_OVERRIDE"), None);
        assert_eq!(has(&v, "LIBGL_ALWAYS_SOFTWARE"), None);
        assert_eq!(has(&v, "GALLIUM_DRIVER"), None);
    }
}

/// Verify `dir` is a private directory we can trust to hold the MCP socket:
/// a real directory (not a symlink), owned by `uid`, with NO group/other
/// permission bits set (mode `& 0o077 == 0`). Uses `symlink_metadata` so a
/// planted symlink is rejected rather than followed. Returns `false` (caller
/// skips the MCP server) on any failed check or stat error.
///
/// tuxlink-cvx84 FIX 1: hardens the runtime-dir ancestor chain so another local
/// user cannot pre-create / rename / replace an ancestor under the process umask
/// and plant a fake `mcp.sock`.
#[cfg(target_os = "linux")]
fn mcp_dir_is_safe(dir: &std::path::Path, uid: u32) -> bool {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    match std::fs::symlink_metadata(dir) {
        Ok(meta) => {
            let is_dir = meta.file_type().is_dir();
            let not_symlink = !meta.file_type().is_symlink();
            let uid_owned = meta.uid() == uid;
            let private = meta.permissions().mode() & 0o077 == 0;
            is_dir && not_symlink && uid_owned && private
        }
        Err(_) => false,
    }
}

/// Create `dir` (best-effort; tolerates "already exists"), chmod it to `0700`,
/// then verify it via [`mcp_dir_is_safe`]. Returns `Some(dir)` only when the
/// final on-disk state is a private (0700, uid-owned, non-symlink) directory;
/// otherwise LOGs a warning and returns `None` so the caller skips the MCP
/// server. Used to harden EACH level of the runtime-dir chain explicitly
/// (tuxlink-cvx84 FIX 1).
#[cfg(target_os = "linux")]
fn harden_and_verify_mcp_dir(dir: &std::path::Path, uid: u32) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    // Create this single level. Do NOT use create_dir_all here — each level is
    // created + hardened + verified in turn by the caller, so an ancestor that
    // already exists must independently pass the safety check below.
    match std::fs::create_dir(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => {
            tracing::warn!(
                target: "mcp",
                dir = %dir.display(),
                error = %e,
                "could not create MCP runtime dir; skipping MCP server"
            );
            return None;
        }
    }
    // chmod 0700 (idempotent; fixes a too-permissive dir we just adopted).
    if let Err(e) =
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    {
        tracing::warn!(
            target: "mcp",
            dir = %dir.display(),
            error = %e,
            "could not set MCP runtime dir to 0700; skipping MCP server"
        );
        return None;
    }
    // Re-stat (via symlink_metadata) and verify owner + mode + non-symlink.
    if mcp_dir_is_safe(dir, uid) {
        Some(dir.to_path_buf())
    } else {
        tracing::warn!(
            target: "mcp",
            dir = %dir.display(),
            "MCP runtime dir is not a private (0700, uid-owned, non-symlink) directory after hardening; skipping MCP server"
        );
        None
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Force the working WebKitGTK GL environment before the webview inits.
    // tuxlink-spo2 (software GL) + tuxlink-wfw (DMA-BUF disable). See the const.
    #[cfg(target_os = "linux")]
    apply_linux_webview_gl_env();

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
        // tuxlink-dyop Phase 6 (map-picker v2 §8.2/§8.3): the bespoke `tile`
        // URI scheme — the ONLY webview→backend path for LAN map tiles. The
        // Leaflet TileLayer's `tile://localhost/{z}/{x}/{y}` requests land here.
        // The handler extracts the URL path, retrieves the managed
        // `TileGatekeeper` (set up in the app_data_dir arm below), and runs the
        // SSRF-guarded serve pipeline on the async runtime, responding when the
        // fetch settles. Production passes `allow_loopback = false`. NEVER the
        // general asset protocol; only this bespoke scheme. Phase-0 spike proved
        // the `tile:` img-src token renders in a packaged WebKitGTK build.
        .register_asynchronous_uri_scheme_protocol("tile", |ctx, request, responder| {
            use tauri::Manager as _;
            // The path is `/{z}/{x}/{y}` (or `…/{y}.png`); serve_tile tolerates
            // the leading `/`. Own it before moving into the async task.
            let path = request.uri().path().to_string();

            // tuxlink-ndi4 (plan A1): the vector-basemap PMTiles branch. PMTiles
            // archives are served as RAW bytes over HTTP-206 `Range` on
            // `tile://pmtiles/<archive>`, consumed by the `pmtiles` JS lib's native
            // `FetchSource` — a distinct path-prefix branch from the LAN-raster
            // `serve_tile` pipeline below (which is image-magic / SSRF shaped and
            // parked for imagery). Zero content decoding here: PMTiles internal
            // compression is decoded by the JS client.
            if let Some(archive_id) =
                crate::basemap::parse_pmtiles_uri(request.uri().host(), &path)
            {
                let range = request
                    .headers()
                    .get(tauri::http::header::RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(crate::basemap::parse_range_header);
                let registry = match ctx
                    .app_handle()
                    .try_state::<std::sync::Arc<crate::basemap::PmtilesRegistry>>()
                {
                    Some(state) => (*state).clone(),
                    None => {
                        let _ = tauri::http::Response::builder()
                            .status(503)
                            .header(tauri::http::header::CONTENT_TYPE, "text/plain")
                            .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .body(b"basemap registry unavailable".to_vec())
                            .map(|resp| responder.respond(resp));
                        return;
                    }
                };
                let archive = match registry.get(&archive_id) {
                    Some(a) => a,
                    None => {
                        // Archive not registered (e.g. bundle resource absent, or an
                        // unknown region pack). 404 → MapLibre treats the source as
                        // empty; the bundled overview beneath stays visible.
                        let _ = tauri::http::Response::builder()
                            .status(404)
                            .header(tauri::http::header::CONTENT_TYPE, "text/plain")
                            .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .body(b"pmtiles archive not found".to_vec())
                            .map(|resp| responder.respond(resp));
                        return;
                    }
                };
                // Only the bundled, truly-immutable overview archive may be served
                // with `Cache-Control: immutable` (B4, tuxlink-vnk7 — Codex P1).
                // Region packs share a stable `tile://pmtiles/<id>` URL but are
                // MUTABLE (delete + re-download under the same id), so an immutable
                // directive would serve stale ranges; they keep the original
                // no-cache-directive behavior until their URLs are content-addressed.
                let cacheable = archive_id == crate::basemap::BUNDLED_OVERVIEW_ARCHIVE_ID;
                // Positioned file reads are blocking I/O — run them off the async
                // worker pool (matches the project's spawn_blocking idiom). The
                // responder is `Send`, so it can resolve from the blocking thread.
                tauri::async_runtime::spawn_blocking(move || {
                    match crate::basemap::read_range(&archive, range) {
                        Ok(rr) => {
                            let mut builder = tauri::http::Response::builder()
                                .status(rr.status)
                                .header(
                                    tauri::http::header::CONTENT_TYPE,
                                    crate::basemap::PMTILES_CONTENT_TYPE,
                                )
                                .header(tauri::http::header::ACCEPT_RANGES, "bytes")
                                // tuxlink-56ki: the webview consumes this via
                                // maplibre/pmtiles `fetch()` from a different origin
                                // (tauri://localhost packaged, http://localhost:1420
                                // in dev), so the response MUST carry CORS or the
                                // browser blocks it ("not allowed by
                                // Access-Control-Allow-Origin", even on a 206) →
                                // blank map. Local bundled bytes, no credentials, so
                                // `*` is safe. Expose Content-Range/-Length so the
                                // pmtiles client can read the range it requested.
                                .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                                .header(
                                    tauri::http::header::ACCESS_CONTROL_EXPOSE_HEADERS,
                                    "Content-Range, Content-Length, ETag, Accept-Ranges",
                                )
                                .header(
                                    tauri::http::header::CONTENT_LENGTH,
                                    rr.body.len().to_string(),
                                )
                                // Stable per-archive ETag (length-derived) so the
                                // pmtiles client never sees a mid-read ETag change
                                // between its header read and subsequent tile reads.
                                .header(
                                    tauri::http::header::ETAG,
                                    format!("\"{}\"", rr.total_len),
                                );
                            // Immutable cache directive ONLY for the bundled
                            // overview (never changes) — lets the webview cache its
                            // directory/leaf ranges instead of refetching per tile
                            // during pan/zoom (B4, tuxlink-vnk7). Mutable packs are
                            // excluded (Codex P1).
                            if cacheable {
                                builder = builder.header(
                                    tauri::http::header::CACHE_CONTROL,
                                    crate::basemap::PMTILES_CACHE_CONTROL,
                                );
                            }
                            if let Some(cr) = &rr.content_range {
                                builder =
                                    builder.header(tauri::http::header::CONTENT_RANGE, cr);
                            }
                            if let Ok(resp) = builder.body(rr.body) {
                                responder.respond(resp);
                            }
                        }
                        Err(_) => {
                            let _ = tauri::http::Response::builder()
                                .status(500)
                                .header(tauri::http::header::CONTENT_TYPE, "text/plain")
                                .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                                .body(b"pmtiles read error".to_vec())
                                .map(|resp| responder.respond(resp));
                        }
                    }
                });
                return;
            }

            // Retrieve the managed gatekeeper Arc and clone it into the task.
            // If it is not yet managed (setup failed to resolve app_data_dir),
            // respond 503 rather than panic.
            let gk = match ctx
                .app_handle()
                .try_state::<std::sync::Arc<crate::tiles::TileGatekeeper>>()
            {
                Some(state) => (*state).clone(),
                None => {
                    let _ = tauri::http::Response::builder()
                        .status(503)
                        .header(tauri::http::header::CONTENT_TYPE, "text/plain")
                        // tuxlink-1tai: the webview consumes tile:// via fetch() from
                        // a different origin, so every response (incl. errors) MUST
                        // carry CORS or the browser blocks it → blank. Same fix class
                        // as the pmtiles branch above; applied to the legacy raster
                        // path so a future maplibre imagery source can't re-blank.
                        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(b"tile gatekeeper unavailable".to_vec())
                        .map(|resp| responder.respond(resp));
                    return;
                }
            };
            tauri::async_runtime::spawn(async move {
                // Production = NO loopback (allow_loopback = false).
                let result = crate::tiles::serve::serve_tile(&gk, &path, false).await;
                let response = match result {
                    Ok((bytes, mime)) => tauri::http::Response::builder()
                        .status(200)
                        .header(tauri::http::header::CONTENT_TYPE, mime)
                        // tuxlink-1tai: CORS so the webview can read its own tile
                        // proxy (local/LAN bytes, no credentials → `*` is safe).
                        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(bytes),
                    Err(e) => {
                        use crate::tiles::serve::ServeError;
                        let status = match e {
                            ServeError::NoSource | ServeError::NotFound => 404,
                            ServeError::BadPath(_) => 400,
                            // §8.5 breaker open: the source is degraded + cooling.
                            // 503 signals "transiently unavailable; serve bundled"
                            // — the webview falls back to the bundled raster for
                            // these tiles without learning source-internal detail.
                            ServeError::SourceDegraded => 503,
                            ServeError::Upstream(_) => 502,
                        };
                        tauri::http::Response::builder()
                            .status(status)
                            .header(tauri::http::header::CONTENT_TYPE, "text/plain")
                            // tuxlink-1tai: CORS on the error path too (see above).
                            .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .body(e.to_string().into_bytes())
                    }
                };
                if let Ok(response) = response {
                    responder.respond(response);
                }
            });
        })
        .manage(crate::wizard::WizardMutex::new())
        // tuxlink-bsiy: the single inbound-selection rendezvous. cms_connect threads
        // a clone of this Arc into the selecting decider; the resolve command (Task
        // 5) and cms_abort read the SAME managed Arc, so an operator answer/abort
        // reaches the decider parked in the blocking exchange thread (Codex #1).
        .manage(crate::winlink::inbound_selection::SelectionRegistry::default())
        // tuxlink-z0le: in-app form-import staging registry (token → staged dir).
        .manage(std::sync::Arc::new(
            crate::forms::import::ImportStagingRegistry::default(),
        ))
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
        // tuxlink-nx95: native UV-Pro Benshi control session (APRS-chat Phase 2).
        // Holds the RFCOMM/GAIA driver + cached radio state + the single-
        // Bluetooth-host owner-lock. Shared by the uvpro_* commands and the
        // uvpro:status broadcaster.
        .manage(std::sync::Arc::new(
            crate::winlink::ax25::uvpro::session::UvproSession::new(),
        ))
        // tuxlink-0pnb: P2P-Telnet single-flight + abort coordination (mirrors
        // NativeBackend's connect_in_progress + aborting flags, but held in
        // managed state because P2P bypasses WinlinkBackend entirely).
        .manage(crate::ui_commands::P2pConnectState {
            in_progress: std::sync::atomic::AtomicBool::new(false),
            aborting: std::sync::atomic::AtomicBool::new(false),
        })
        // tuxlink-6c9y: single-flight + abort coordination for the Telnet
        // "Post Office" connect path (RMS Relay over plaintext TCP).
        .manage(crate::ui_commands::PostOfficeConnectState::default())
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
        // Phase 2 (tuxlink-7iy2): identity CRUD command state (keyring-backed; no store path held).
        .manage(crate::identity::IdentityService::new())
        // APRS tactical-chat engine lifecycle (tuxlink-2f2n, Task 10). Task 11's
        // Tauri commands consume this via `State<'_, AprsState>`.
        .manage(crate::winlink::aprs::engine::AprsState::default())
        // tuxlink-ndi4 (plan A1/A3): the vector-basemap PMTiles registry the
        // `tile://pmtiles/<archive>` branch reads. Managed unconditionally (the
        // handler is on the Builder above); the bundled `world` archive is
        // registered in `.setup()` once its resource path resolves.
        .manage(std::sync::Arc::new(crate::basemap::PmtilesRegistry::new()))
        // tuxlink-7dwqa (Plan 2): armed-grant + taint egress-authorization gate
        // for the MCP server's agent caller. `EgressGuard` is the single
        // authoritative gate; arm/disarm/status commands are registered below.
        .manage(std::sync::Arc::new(crate::ui_core::security::EgressGuard::new()))
        .setup(|app| {
            use tauri::Manager as _;  // brings .state() into scope for the setup closure

            // tuxlink-z0le: reap orphaned form-import staging dirs left by a
            // crashed prior run (best-effort, before any import can run).
            crate::forms::import::sweep_stale_staging();

            // tuxlink-ndi4 (plan A1/A8): register the bundled world z0–6 vector
            // basemap archive under id "world" so `tile://pmtiles/world` resolves.
            // Resolves a packaged resource; absent in a build that has not yet
            // bundled the archive (the out-of-band `scripts/build-basemap-bundle.sh`
            // output) — that is non-fatal: the registry stays empty, the handler
            // returns 404, and the map renders nothing for the source rather than
            // crashing. The actual render is verified at the WebKitGTK smoke.
            match app
                .path()
                .resolve("resources/basemap/world-z0-6.pmtiles", tauri::path::BaseDirectory::Resource)
            {
                Ok(world_path) if world_path.exists() => {
                    let registry = app
                        .state::<std::sync::Arc<crate::basemap::PmtilesRegistry>>();
                    match registry.register_path("world", &world_path) {
                        Ok(len) => {
                            tracing::info!(target: "basemap", bytes = len, "registered bundled world PMTiles");
                        }
                        Err(e) => {
                            tracing::warn!(target: "basemap", error = %e, "failed to open bundled world PMTiles");
                        }
                    }
                }
                Ok(world_path) => {
                    tracing::warn!(target: "basemap", path = %world_path.display(), "bundled world PMTiles not present; basemap source will be empty");
                }
                Err(e) => {
                    tracing::warn!(target: "basemap", error = %e, "could not resolve bundled world PMTiles resource path");
                }
            }

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

                // tuxlink-ebyt: route panics (command / thread / async-task) into
                // the structured log. Without this a backend panic crashes with
                // NOTHING in the robust logs — the worst + most invisible failure
                // class. Installed AFTER logging::init so `tracing::error!` reaches
                // the FanoutLayer; chains the previous hook so the default stderr
                // backtrace is preserved.
                let previous_hook = std::panic::take_hook();
                std::panic::set_hook(Box::new(move |info| {
                    let location = info
                        .location()
                        .map(|l| format!("{}:{}", l.file(), l.line()))
                        .unwrap_or_else(|| "<unknown>".to_string());
                    let payload = panic_payload_string(info.payload());
                    let thread = std::thread::current()
                        .name()
                        .unwrap_or("<unnamed>")
                        .to_string();
                    tracing::error!(
                        target: "tuxlink::panic",
                        location = %location,
                        thread = %thread,
                        "panic: {payload}",
                    );
                    previous_hook(info);
                }));
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
                    // tuxlink-dyop Phase 6: the TileGatekeeper managed state the
                    // `tile`-scheme handler (registered on the Builder above)
                    // consumes. Cache root is `<app_data>/tile-cache`; `new` does
                    // NO I/O (the cache layer creates the tree lazily on first
                    // write). Managed as `Arc<TileGatekeeper>` so the handler can
                    // clone it out of managed state per request. The active source
                    // starts `None`; tuxlink-9rek seeds it from the persisted
                    // config below so a configured LAN tile source survives a
                    // restart. An unconfigured serve returns 404 (NoSource).
                    let tile_gatekeeper = std::sync::Arc::new(
                        crate::tiles::TileGatekeeper::new(data_dir.join("tile-cache")),
                    );
                    // tuxlink-9rek: rehydrate the active source from config at
                    // boot (mirrors the StationsCache disk-seed below). Without
                    // this the gatekeeper starts empty, `tile_source_status`
                    // reports `Bundled`, `useTileSource` returns null, and the
                    // map silently falls back to the bundled raster (maxZoom 3)
                    // even though `config.map_tile_source` is set — the persist
                    // half shipped without the load half.
                    if let Ok(cfg) = crate::config::read_config() {
                        if let Some(src) = cfg.map_tile_source {
                            tile_gatekeeper.set_source(Some(src));
                        }
                    }
                    app.manage(tile_gatekeeper);

                    // tuxlink-ndi4 (phase 4): region-pack subsystem. Resolve the
                    // packs dir under app-data, sweep interrupted/orphaned pack
                    // files, and re-register every installed pack into the already-
                    // managed PmtilesRegistry so `tile://pmtiles/<id>` resolves
                    // after a restart. `init_packs` is best-effort (sweep/register
                    // failures log, never block startup). The returned BasemapState
                    // (cached manifest + packs dir) backs the basemap_* commands.
                    {
                        let registry =
                            app.state::<std::sync::Arc<crate::basemap::PmtilesRegistry>>();
                        let basemap_state = crate::basemap::commands::init_packs(
                            data_dir.join("basemap-packs"),
                            &registry,
                        );
                        app.manage(std::sync::Arc::new(basemap_state));
                    }

                    // tuxlink-dx57 U2: persistent station-list cache. Seeds from
                    // disk on launch so a cold offline start shows last-known-good
                    // results. TTL and min-refetch are identical to the former
                    // in-memory-only registration (30 min / 15 min).
                    app.manage(std::sync::Arc::new(
                        crate::catalog::stations_cache::StationsCache::new_persistent(
                            30 * 60 * 1000, // TTL: 30 min
                            15 * 60 * 1000, // min-refetch floor: 15 min
                            std::sync::Arc::new(crate::catalog::stations_cache::SystemClock),
                            data_dir.join("station-listings-cache.json"),
                        ),
                    ));

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

                    // forms sequence counters (tuxlink-2tom / G12-C): per-form
                    // serial numbers for SeqInc forms. `SeqCounterStore::open` is
                    // INFALLIBLE (degrade-to-empty on read error) like the stores
                    // above, so it is unconditionally managed. Reuses the
                    // already-resolved `data_dir`.
                    app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                        crate::forms::sequence::SeqCounterStore::open(
                            data_dir.join("forms-sequence-counters.json"),
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
                Err(e) => {
                    eprintln!("search: could not resolve app_data_dir: {e}");
                    // tuxlink-dx57 U2: app_data_dir unavailable — fall back to an
                    // in-memory cache so catalog_fetch_stations always resolves its
                    // State<Arc<StationsCache>> extractor. No persistence in this path.
                    app.manage(std::sync::Arc::new(
                        crate::catalog::stations_cache::StationsCache::new(
                            30 * 60 * 1000, // TTL: 30 min
                            15 * 60 * 1000, // min-refetch floor: 15 min
                            std::sync::Arc::new(crate::catalog::stations_cache::SystemClock),
                        ),
                    ));
                    // tuxlink-2tom: `send_webview_form` requires the seq-counter
                    // State to resolve, so manage a fallback here too (temp path —
                    // degraded persistence) rather than break ALL webview form
                    // sends when app_data_dir is unavailable.
                    app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                        crate::forms::sequence::SeqCounterStore::open(
                            std::env::temp_dir().join("tuxlink-forms-sequence-counters.json"),
                        ),
                    )));
                }
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

            // tuxlink-wl7n Task 9: scheduled Trash auto-purge. When the operator
            // has `trash_auto_purge` enabled (the default), permanently remove
            // Trash items older than `trash_retention_days` — once startup has
            // installed the backend, then every 6h. Best-effort: any failure
            // logs a warning and the loop continues — never panics, never blocks
            // launch. The config is RE-READ each tick so toggling the setting at
            // runtime takes effect without a restart (disabled → the tick is a
            // no-op).
            //
            // Codex P2 fix: the sweep routes through the MANAGED `BackendState`
            // backend (`purge_expired_trash`), NOT a bare `Mailbox`. The managed
            // backend carries the attached search index and the `mailbox:changed`
            // sink, so an auto-purge drops the `search.db` rows and refreshes the
            // UI — a bare `Mailbox` unlinked the files but left stale index rows
            // and never invalidated the folder queries. Until bootstrap installs
            // the backend there is no indexed Trash to sweep, so a tick with no
            // current backend simply waits for the next one.
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Six hours between sweeps; the first tick fires immediately
                    // (tokio's interval yields at t=0), giving the startup sweep
                    // (once the backend is present).
                    let mut ticker =
                        tokio::time::interval(std::time::Duration::from_secs(6 * 60 * 60));
                    loop {
                        ticker.tick().await;
                        // Re-read config each tick so a runtime toggle of the
                        // setting (or a changed retention window) is honored.
                        let (enabled, retention_days) = match crate::config::read_config() {
                            Ok(cfg) => (cfg.trash_auto_purge, cfg.trash_retention_days),
                            // Unreadable/absent config: fall back to the defaults
                            // (auto-purge on, 30 days) so a pre-wizard install
                            // still self-tidies rather than hoarding deleted mail.
                            Err(_) => (true, 30),
                        };
                        if !enabled {
                            continue;
                        }
                        // No installed backend yet (pre-wizard / mid-bootstrap):
                        // nothing indexed to sweep — retry on the next tick.
                        let backend = match handle
                            .state::<crate::app_backend::BackendState>()
                            .current()
                        {
                            Some(b) => b,
                            None => continue,
                        };
                        match backend.purge_expired_trash(i64::from(retention_days)).await {
                            Ok(n) if n > 0 => {
                                tracing::info!(
                                    target: "tuxlink::trash",
                                    purged = n,
                                    retention_days,
                                    "trash auto-purge removed expired items"
                                );
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(
                                    target: "tuxlink::trash",
                                    "trash auto-purge sweep failed: {e}"
                                );
                            }
                        }
                    }
                });
            }

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

            // tuxlink-nx95: spawn the UV-Pro control status broadcaster — a
            // std::thread that, while a native control session is connected,
            // polls live status every 2 s and emits it as the `uvpro:status`
            // event (mirrors the modem broadcaster above). Idle (disconnected)
            // ticks are cheap no-ops; no tokio (ADR 0015).
            let uvpro_for_broadcaster = (*app
                .state::<std::sync::Arc<crate::winlink::ax25::uvpro::session::UvproSession>>())
            .clone();
            let app_handle_for_uvpro = app.handle().clone();
            let _uvpro_broadcaster = crate::winlink::ax25::uvpro::commands::spawn_status_broadcaster(
                uvpro_for_broadcaster,
                move |s| {
                    use tauri::Emitter as _;
                    let _ = app_handle_for_uvpro
                        .emit(crate::winlink::ax25::uvpro::commands::STATUS_EVENT, s);
                },
            );

            // tuxlink-ipjt Task 6: offline HF path-prediction state.
            // PropagationState is ALWAYS managed (exactly once) so the Tauri
            // extractor never fails before the command body runs.
            //   - Ready(...) when all engine assets resolve.
            //   - Unavailable("<reason>") on any soft-disable path.
            // Failures are soft (eprintln + Unavailable — never abort launch — F17/F10).
            // F10: no /tmp fallback; missing app_cache_dir → Unavailable.
            // F2: voacapl binary is a Tauri externalBin sidecar placed ADJACENT
            // to the main exe (not under Resource). The packaged-.deb path must
            // be confirmed by the Task 7 gated test / operator smoke.
            {
                use crate::propagation::commands::{PropagationState, ReadyPropagation};
                use crate::propagation::{engine::EnginePaths, ssn};

                let prop_state = match (app.path().app_cache_dir(), std::env::current_exe()) {
                    (Ok(cache), Ok(exe)) => {
                        match exe.parent() {
                            None => {
                                let reason = "current_exe has no parent dir".to_string();
                                eprintln!("propagation: prediction disabled ({reason})");
                                PropagationState::Unavailable(reason)
                            }
                            Some(bindir) => {
                                if let Err(e) = std::fs::create_dir_all(&cache) {
                                    let reason = format!("could not create cache dir: {e}");
                                    eprintln!("propagation: prediction disabled ({reason})");
                                    PropagationState::Unavailable(reason)
                                } else {
                                    match (
                                        app.path().resolve(
                                            "resources/itshfbc",
                                            tauri::path::BaseDirectory::Resource,
                                        ),
                                        ssn::SsnForecast::from_json(ssn::BUNDLED_SSN_FORECAST),
                                    ) {
                                        (Ok(itshfbc), Ok(forecast)) => {
                                            PropagationState::Ready(ReadyPropagation {
                                                paths: EnginePaths {
                                                    binary: bindir.join("voacapl"),
                                                    itshfbc_root: itshfbc,
                                                },
                                                scratch_parent: cache,
                                                clock: std::sync::Arc::new(
                                                    crate::catalog::stations_cache::SystemClock,
                                                ),
                                                forecast,
                                            })
                                        }
                                        (it, fc) => {
                                            let reason = format!(
                                                "resource resolution failed (itshfbc={:?}, forecast_ok={})",
                                                it.err(),
                                                fc.is_ok()
                                            );
                                            eprintln!("propagation: prediction disabled ({reason})");
                                            PropagationState::Unavailable(reason)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    (cache, exe) => {
                        let reason = format!(
                            "app_cache_dir unavailable ({:?}) or current_exe failed ({:?})",
                            cache.err(),
                            exe.err()
                        );
                        eprintln!("propagation: prediction disabled ({reason})");
                        PropagationState::Unavailable(reason)
                    }
                };
                app.manage(prop_state);
            }

            // tuxlink-cvx84 (MCP phase 3.1, Task 4): bind the MCP server's
            // Unix-domain-socket endpoint and serve the real router. This is the
            // monolith embedder of `tuxlink-mcp-core`: it injects the app's own
            // identity (`env!` here resolves to the MONOLITH's CARGO_PKG_*, not
            // the core crate's) and shares the already-managed `Arc<EgressGuard>`
            // so `server_info` reaches the live, operator-facing send-authority
            // state. Tier-2 (the standalone testserver) already exercises the
            // protocol round-trip; this is what makes tier-3 (the real app) work.
            //
            // Linux-only: the runtime-dir hardening helpers and the UDS transport
            // use Unix/Linux filesystem APIs (uid ownership, mode bits). The app
            // ships on Linux (WebKitGTK); the bound `#[cfg]` keeps a non-Linux
            // build of the crate compiling.
            #[cfg(target_os = "linux")]
            {
                // Resolve + harden a per-user runtime dir for the MCP socket.
                // The threat: another local user races us to create/rename/
                // replace an ancestor dir created under the process umask (often
                // 0o775, group/world-writable) and plants a hostile dir at our
                // path so they can later swap in a fake `mcp.sock`. Defense:
                // create each dir EXPLICITLY, chmod 0700, then VERIFY (via
                // symlink_metadata so we never follow a planted symlink) that the
                // result is a real, uid-owned, 0700, non-symlink directory. Any
                // failed check → LOG and SKIP starting the MCP server (never
                // crash, never block setup over an optional agent endpoint).
                //
                // SAFETY: `getuid(2)` takes no arguments and cannot fail (POSIX).
                let my_uid = unsafe { libc::getuid() };

                // Build a hardened, verified-private /tmp/tuxlink-<uid>/tuxlink
                // dir we fully own: create + chmod 0700 + verify (uid-owned,
                // 0700, non-symlink) at EACH level so an attacker cannot pre-plant
                // a group/world-writable ancestor under the process umask. /tmp's
                // sticky bit prevents other users from renaming/replacing our
                // subdir. Used both when XDG_RUNTIME_DIR is unset AND when it is
                // set-but-not-private.
                let temp_fallback = |uid: u32| -> Option<std::path::PathBuf> {
                    let base = std::env::temp_dir().join(format!("tuxlink-{uid}"));
                    harden_and_verify_mcp_dir(&base, uid)
                        .and_then(|base| harden_and_verify_mcp_dir(&base.join("tuxlink"), uid))
                };

                // `mcp_dir` is set when (and only when) we have a verified-private
                // socket dir; `None` means "skip the MCP server".
                let mcp_dir: Option<std::path::PathBuf> = match std::env::var(
                    "XDG_RUNTIME_DIR",
                ) {
                    Ok(dir) if !dir.is_empty() => {
                        // XDG_RUNTIME_DIR is usually 0700/uid-owned, but some
                        // systems make /run/user/<uid> group-writable (0770).
                        // Verify the supplied base; if private, create + chmod +
                        // verify the `tuxlink` child there.
                        let base = std::path::PathBuf::from(dir);
                        if mcp_dir_is_safe(&base, my_uid) {
                            harden_and_verify_mcp_dir(&base.join("tuxlink"), my_uid)
                        } else {
                            // Set-but-not-private: do NOT skip the MCP server —
                            // fall back to a private temp runtime dir we create
                            // and harden ourselves (security unchanged; the socket
                            // dir is still a verified-private 0700 dir we own).
                            tracing::warn!(
                                target: "mcp",
                                dir = %base.display(),
                                "XDG_RUNTIME_DIR is not a private (0700, uid-owned, non-symlink) directory; falling back to a private temp runtime dir for the MCP socket"
                            );
                            temp_fallback(my_uid)
                        }
                    }
                    // XDG_RUNTIME_DIR unset/empty.
                    _ => temp_fallback(my_uid),
                };
                if let Some(mcp_dir) = mcp_dir {
                    let sock_path = mcp_dir.join("mcp.sock");

                    // Share the already-managed egress authority (line ~655's
                    // `.manage(Arc::new(EgressGuard::new()))`). Same TypeId: the
                    // managed `crate::ui_core::security::EgressGuard` is a glob
                    // re-export of `tuxlink_security::EgressGuard`.
                    let guard = app
                        .state::<std::sync::Arc<tuxlink_security::EgressGuard>>()
                        .inner()
                        .clone();

                    // env! HERE resolves to the MONOLITH (tuxlink / its version) —
                    // Task-4-injects-identity, so `server_info` reports the app's
                    // identity, not the core crate's 0.0.0.
                    // Inject the REAL monolith port adapters (phase 3.2 Chunk
                    // 2). Each holds a cloned AppHandle and reads live managed
                    // state on demand; redaction (grid 4-char, wire-line creds,
                    // BT-MAC minimization) happens inside these impls so RAW
                    // values never cross into mcp-core. One struct per domain.
                    let h = app.handle();
                    let mcp_state = std::sync::Arc::new(tuxlink_mcp_core::McpState {
                        // Clone so the bare `guard` binding survives for the
                        // egress port's `guard.clone()` below (same shared Arc).
                        guard: guard.clone(),
                        name: env!("CARGO_PKG_NAME").to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        status: std::sync::Arc::new(crate::mcp_ports::MonolithStatusPort::new(
                            h.clone(),
                        )),
                        mailbox: std::sync::Arc::new(crate::mcp_ports::MonolithMailboxPort::new(
                            h.clone(),
                        )),
                        search: std::sync::Arc::new(crate::mcp_ports::MonolithSearchPort::new(
                            h.clone(),
                        )),
                        config: std::sync::Arc::new(crate::mcp_ports::MonolithConfigPort::new(
                            h.clone(),
                        )),
                        devices: std::sync::Arc::new(crate::mcp_ports::MonolithDevicePort::new(
                            h.clone(),
                        )),
                        logs: std::sync::Arc::new(crate::mcp_ports::MonolithLogPort::new(
                            h.clone(),
                        )),
                        // Phase 3.3 Chunk 2: GATED Agent egress + UNGATED abort.
                        // The egress port shares the SAME `Arc<EgressGuard>` the
                        // operator's egress_arm/egress_disarm mutate, so the gate
                        // sees the live arm/taint state at call time.
                        egress: std::sync::Arc::new(crate::mcp_ports::MonolithEgressPort::new(
                            h.clone(),
                            guard.clone(),
                        )),
                        abort: std::sync::Arc::new(crate::mcp_ports::MonolithAbortPort::new(
                            h.clone(),
                        )),
                        // Phase 3.4 Chunk 2: GATED config/state writes + UNGATED
                        // compose/staging. The write port shares the SAME
                        // `Arc<EgressGuard>` the operator's egress_arm/disarm
                        // mutate (validate-before-gate per impl); the compose port
                        // is ungated (stages local outbox drafts only).
                        write: std::sync::Arc::new(crate::mcp_ports::MonolithWritePort::new(
                            h.clone(),
                            guard.clone(),
                        )),
                        compose: std::sync::Arc::new(crate::mcp_ports::MonolithComposePort::new(
                            h.clone(),
                        )),
                        // Phase 3.2 Chunk 2: station-intelligence READS. Both are
                        // ungated, non-tainting reads — the station finder routes
                        // through the polite offline cache; prediction/solar are
                        // offline compute. The prediction port injects the
                        // operator's OWN tx_grid from config (never agent-supplied).
                        stations: std::sync::Arc::new(crate::mcp_ports::MonolithStationPort::new(
                            h.clone(),
                        )),
                        prediction: std::sync::Arc::new(
                            crate::mcp_ports::MonolithPredictionPort::new(h.clone()),
                        ),
                    });
                    let router = tuxlink_mcp_core::router::TuxlinkMcp::new(mcp_state);

                    tracing::info!(
                        target: "mcp",
                        socket = %sock_path.display(),
                        "starting MCP server on Unix socket"
                    );

                    // Serve forever on the runtime; do NOT block setup. `serve`
                    // binds a 0600 single-caller UDS and runs an accept loop until
                    // an accept fails or the future is dropped.
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) =
                            tuxlink_mcp_core::transport_uds::serve(router, &sock_path).await
                        {
                            tracing::error!(
                                target: "mcp",
                                socket = %sock_path.display(),
                                error = %e,
                                "MCP server stopped with error"
                            );
                        }
                    });
                }
            }

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
                    // tuxlink-5rvp / #882: the close path is config-aware.
                    // Read config synchronously (read_config is sync).
                    match crate::config::read_config().ok() {
                        // First-ever close (config readable, prompt not yet
                        // answered): show the one-time explainer modal. Keep the
                        // window VISIBLE (do NOT minimize) so the user sees it;
                        // the frontend re-issues the close via resolve_close_prompt
                        // once they answer.
                        Some(c) if !c.close_prompt_seen => {
                            api.prevent_close();
                            use tauri::Emitter as _;
                            let _ = window.emit("show-close-prompt", ());
                        }
                        // The operator opted out (close = quit). Exit explicitly
                        // via the canonical app.exit(0) path (the SAME path as
                        // File→Quit / tray Quit / resolve_close_prompt) rather
                        // than relying on implicit last-window-close: with a tray
                        // icon present and no RunEvent::ExitRequested handler,
                        // implicit exit is fragile and a missed exit would strand
                        // a GUI-less process. exit(0) is unambiguous. (Reaching
                        // this arm means close_prompt_seen is already true, since
                        // the Settings toggle marks the prompt seen — see
                        // set_close_to_tray.)
                        Some(c) if !c.close_to_tray => {
                            use tauri::Manager as _;
                            window.app_handle().exit(0);
                        }
                        // Default minimize-to-tray. Covers BOTH the operator who
                        // kept the default (close_to_tray=true, prompt seen) AND a
                        // None config (fresh install / unreadable): the None case
                        // MUST NOT emit the prompt — during the wizard the prompt
                        // UI isn't mounted and resolve_close_prompt would fail the
                        // same read, so the close would silently no-op (window
                        // un-closeable). Minimize is the safe fallback that never
                        // kills the process mid-transfer.
                        // tuxlink-9zd: on Linux the SNI tray often does not register
                        // (e.g. Wayland + wf-panel-pi has no SNI host), so hide()
                        // would strand the window — process alive, no GUI path back.
                        // minimize() keeps it in the compositor's window list
                        // (recoverable via the panel/window-switcher). macOS/Windows
                        // have a reliable tray, so hide() there.
                        _ => {
                            api.prevent_close();
                            #[cfg(target_os = "linux")]
                            let _ = window.minimize();
                            #[cfg(not(target_os = "linux"))]
                            let _ = window.hide();
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            uninstall_cleanup_preview,
            uninstall_cleanup_execute,
            crate::wizard::get_wizard_completed,
            crate::wizard::wizard_persist_cms,
            crate::wizard::wizard_persist_offline,
            crate::wizard::verify_cms_connection,   // Task 5.4 (tuxlink-9phd): replaces wizard_run_test_send
            // tuxlink-vfb3: CMS account lifecycle (in-app credential management).
            crate::winlink::cms_account::cms_password_change,
            crate::winlink::cms_account::cms_password_change_available,
            crate::winlink::cms_account::cms_account_create,
            crate::winlink::cms_account::cms_account_exists,
            crate::winlink::cms_account::cms_account_validate_password,
            crate::winlink::cms_account::cms_account_set_recovery_email,
            crate::winlink::cms_account::cms_account_send_recovery,
            crate::winlink::cms_account::cms_account_remove,
            // tuxlink-ndi4 (phase 4): offline region-pack manager (Tools→Offline maps).
            crate::basemap::commands::basemap_get_manifest,
            crate::basemap::commands::basemap_refresh_manifest,
            crate::basemap::commands::basemap_list_packs,
            crate::basemap::commands::basemap_download_pack,
            crate::basemap::commands::basemap_cancel_download,
            crate::basemap::commands::basemap_delete_pack,
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
            crate::ui_commands::folder_move,           // tuxlink-ka3z (nesting)
            crate::ui_commands::message_read,          // Task 13 (tuxlink-y5c)
            crate::ui_commands::message_set_read_state, // tuxlink-etxt (read/unread)
            crate::ui_commands::message_set_read_state_bulk, // tuxlink-etxt (bulk read/unread)
            crate::ui_commands::message_move_bulk,     // tuxlink-l80q (bulk move/archive)
            crate::ui_commands::message_delete,        // tuxlink-wl7n (delete to Trash)
            crate::ui_commands::message_delete_bulk,   // tuxlink-wl7n (bulk delete to Trash)
            crate::ui_commands::message_restore,       // tuxlink-wl7n (restore from Trash)
            crate::ui_commands::message_restore_bulk,  // tuxlink-wl7n (bulk restore from Trash)
            crate::ui_commands::trash_empty,           // tuxlink-wl7n (empty Trash)
            crate::ui_commands::trash_purge_one,       // tuxlink-wl7n (purge one from Trash)
            crate::ui_commands::message_attachment_preview, // tuxlink-ewtb (image attachment preview)
            crate::ui_commands::message_attachment_save, // tuxlink-0fyj (Save As attachment)
            crate::ui_commands::message_send,          // Task 14 (tuxlink-dm8)
            crate::media::commands::prepare_attachment, // tuxlink-mg4s — attach-time image resize
            crate::ui_commands::send_form,             // HTML Forms v0.1 (tuxlink-v1p Task 3.1)
            // HTML Forms P1 Task 8 (tuxlink-tzr5; original plan tuxlink-ytya):
            // webview-form command surface — catalog list + per-open
            // http_server lifecycle.
            crate::ui_commands::forms_list_catalog,
            crate::ui_commands::open_webview_form,
            crate::ui_commands::close_webview_form_server,
            // tuxlink-cumx / G8: on-demand faithful PDF export of a rendered form.
            crate::ui_commands::forms_export_pdf,
            // tuxlink-954o / G8b: direct print of a rendered form (system dialog).
            crate::ui_commands::forms_print,
            // tuxlink-z0le/fwob: in-app form import (G5+G6) — preview→commit,
            // cancel, custom-folder reveal, and per-form uninstall.
            crate::ui_commands::forms_import_preview,
            crate::ui_commands::forms_import_commit,
            crate::ui_commands::forms_import_cancel,
            crate::ui_commands::open_forms_folder,
            crate::ui_commands::forms_custom_delete,
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
            crate::ui_commands::open_webview_reply,     // tuxlink-hhfx / G10 (editable pre-bound reply session)
            crate::ui_commands::forms_sequence_status,  // tuxlink-2tom / G12-C (SeqInc serial counters)
            crate::ui_commands::forms_sequence_reset,   // tuxlink-2tom / G12-C (reset a form's next serial)
            crate::ui_commands::cms_connect,           // tuxlink-0ic (native connect)
            crate::ui_commands::cms_abort,             // tuxlink-9z2 (abort in-flight connect)
            crate::ui_commands::cms_disconnect,        // tuxlink-avu9 (graceful packet DISC)
            crate::ui_commands::cms_resolve_inbound_selection,    // tuxlink-bsiy (inbound message selection resolve)
            crate::ui_commands::config_read,           // Task 16 (tuxlink-hvv)
            crate::ui_commands::backend_status,        // Task 16 (tuxlink-hvv)
            crate::ui_commands::session_log_snapshot,  // Task 15 (tuxlink-8zg integration round)
            crate::ui_commands::session_log_clear,     // Operator smoke 2026-05-31 — buffer drain
            crate::ui_commands::session_log_append,    // tuxlink-nnjz — frontend-originated operator lines (MissingTargetError)
            crate::compose_window::compose_window_open, // Task 14 (tuxlink-dm8)
            crate::compose_window::compose_close_self,  // tuxlink-h2y (self-only close)
            crate::help_window::help_window_open,       // tuxlink-0gsy (spec §3)
            crate::stations_window::stations_window_open, // tuxlink-2phz (env panel pop-out)
            crate::theme_state::theme_get_scheme,       // tuxlink-0gsy (spec §8.2)
            crate::theme_state::theme_broadcast_scheme, // tuxlink-0gsy (spec §8.2)
            crate::search::commands::docs_search,       // tuxlink-0gsy (spec §9.3)
            crate::ui_commands::app_quit,             // tuxlink-ng3 (HTML File→Quit / Ctrl+Q)
            crate::ui_commands::resolve_close_prompt, // tuxlink-5rvp / #882 (one-time close prompt)
            crate::ui_commands::set_close_to_tray,    // tuxlink-5rvp / #882 (Settings toggle)
            crate::ui_commands::packet_config_get,    // tuxlink-7fr (packet config read)
            crate::ui_commands::packet_config_set,    // tuxlink-7fr (packet config write)
            crate::ui_commands::aprs_config_get,      // tuxlink-2f2n (APRS config read)
            crate::ui_commands::aprs_config_set,      // tuxlink-2f2n (APRS config write)
            crate::ui_commands::aprs_listen_start,    // tuxlink-2f2n (start APRS engine)
            crate::ui_commands::aprs_listen_stop,     // tuxlink-2f2n (stop APRS engine)
            crate::ui_commands::aprs_listen_status,   // tuxlink-9grg (query listen state on mount)
            crate::ui_commands::aprs_send,            // tuxlink-2f2n (queue APRS message)
            crate::ui_commands::aprs_abort,           // tuxlink-2f2n (abort in-flight APRS TX)
            crate::identity::commands::identity_list,        // tuxlink-7iy2 (Phase 2 identity CRUD)
            crate::identity::commands::identity_add_full,    // tuxlink-7iy2
            crate::identity::commands::identity_add_tactical,// tuxlink-7iy2
            crate::identity::commands::identity_remove,      // tuxlink-7iy2
            crate::identity::commands::identity_authenticate, // tuxlink-5ekg (Phase 6 re-auth)
            crate::identity::commands::identity_lock,         // tuxlink-5ekg
            crate::identity::commands::identity_active,       // tuxlink-5ekg
            crate::ui_commands::packet_connect,       // tuxlink-7fr (packet dial)
            crate::ui_commands::packet_listen,        // tuxlink-7fr (arm Listen — answer inbound)
            crate::ui_commands::packet_set_listen,    // tuxlink-7fr (sticky listen)
            crate::ui_commands::packet_allowed_stations_get,         // tuxlink-inde (allowlist read)
            crate::ui_commands::packet_allowed_stations_add,         // tuxlink-inde (allowlist add)
            crate::ui_commands::packet_allowed_stations_remove,      // tuxlink-inde (allowlist remove)
            crate::ui_commands::packet_allowed_stations_set_allow_all, // tuxlink-inde (allow_all toggle)
            crate::ui_commands::packet_list_serial_devices, // tuxlink-7fr (USB/BT device picker)
            crate::ui_commands::packet_list_bluetooth_devices, // tuxlink-mqu3 (BT-MAC picker restoration)
            crate::ui_commands::packet_list_audio_devices, // tuxlink-yq3l P7.1 (managed sound-card + PTT picker)
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
            crate::ui_commands::config_set_aredn_master_node_host, // tuxlink-1w7t (AREDN mesh discovery host)
            crate::mesh::mesh_discover_post_offices,    // tuxlink-1w7t (AREDN Post Office discovery)
            // tuxlink-6c9y (Task A7): Network PO relay favorites — persist in config.
            crate::ui_commands::network_po_favorites_get,
            crate::ui_commands::network_po_favorites_add,
            crate::ui_commands::network_po_favorites_remove,
            crate::ui_commands::network_po_favorites_set,
            crate::ui_commands::config_set_review_inbound, // tuxlink-bsiy (review-pending-messages preference)
            crate::ui_commands::config_set_trash_auto_purge, // tuxlink-wl7n (trash auto-purge + retention)
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
            // tuxlink-6j14: operator-configurable service codes (MARS/SHARES/EMCOMM).
            crate::catalog::commands::catalog_get_service_codes,
            crate::catalog::commands::catalog_set_service_codes,
            // tuxlink-xrbw: ingest a radio-delivered PUB_* station-list reply into the cache.
            crate::catalog::commands::catalog_ingest_listing_reply,
            // tuxlink-ipjt Task 6: offline HF path prediction (voacapl sidecar).
            crate::propagation::commands::propagation_predict_path,
            // tuxlink-s0r1: operator antenna preset + REQ.SNR + power prefs.
            crate::propagation::commands::propagation_prefs_read,
            crate::propagation::commands::propagation_prefs_write,
            // tuxlink-9xy1 slice 1: GPS source detection probes (unprivileged).
            crate::position::probe::gps_probe_gpsd,
            crate::position::probe::gps_probe_serial_devices,
            crate::position::probe::gps_probe_dialout,
            crate::position::probe::gps_probe_modemmanager,
            // tuxlink-m9ej: one-click "Fix it for me" via the pkexec helper.
            crate::position::gps_fix::gps_run_fix,
            crate::position::gps_fix::gps_pkexec_available,
            // tuxlink-n399: one-click full gpsd setup + package-manager probe.
            crate::position::gps_fix::gps_setup_gpsd,
            crate::position::gps_fix::gps_pkg_manager,
            // tuxlink-vrpk: GRIB request via Saildocs (3rd-party SMTP).
            crate::grib::commands::grib_send_request,
            crate::modem_commands::config_get_ardop,   // tuxlink-4ek (ARDOP config read)
            crate::modem_commands::config_set_ardop,   // tuxlink-4ek (ARDOP config write)
            crate::modem_commands::config_get_rig,      // tuxlink-8fkkk (radio-level rig config read; shared ARDOP+VARA)
            crate::modem_commands::config_set_rig,      // tuxlink-8fkkk (radio-level rig config write)
            crate::modem_commands::modem_get_status,   // tuxlink-4ek Task 3.2 (session snapshot)
            crate::modem_commands::modem_ardop_disconnect, // tuxlink-4ek Task 3.2 (clear consent + reset)
            crate::modem_commands::modem_ardop_connect, // tuxlink-4ek Task 3.3 (RADIO-1-gated spawn + ARQ connect)
            crate::modem_commands::ardop_tune_rig,     // tuxlink-8fkkk Task 7 (Tune-only: set freq+mode, drop serial)
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
            // tuxlink-nx95: native UV-Pro Benshi device-control commands (APRS-chat
            // Phase 2). Connect/disconnect over the radio's native Bluetooth link +
            // read status (channel/freq/mode/battery/RSSI) + set channel/freq/mode.
            // Non-transmitting; single-Bluetooth-host arbitrated via the owner-lock.
            crate::winlink::ax25::uvpro::commands::uvpro_connect,
            crate::winlink::ax25::uvpro::commands::uvpro_disconnect,
            crate::winlink::ax25::uvpro::commands::uvpro_get_status,
            crate::winlink::ax25::uvpro::commands::uvpro_get_channels,
            crate::winlink::ax25::uvpro::commands::uvpro_set_channel,
            crate::winlink::ax25::uvpro::commands::uvpro_set_frequency,
            crate::winlink::ax25::uvpro::commands::uvpro_set_mode,
            // tuxlink-0pnb Task 4 (refactored): P2P-Telnet connect + abort + peer-password management.
            // telnet_p2p_dial renamed to telnet_p2p_connect (StatusBar pipeline wiring);
            // telnet_p2p_abort added to mirror cms_abort (operator cancel semantics).
            crate::ui_commands::telnet_p2p_connect,
            crate::ui_commands::telnet_p2p_abort,
            // tuxlink-6c9y Task C1: Telnet "Post Office" connect + abort (RMS
            // Relay over plaintext TCP; reuses cms_resolve_inbound_selection for
            // the inbound-selection resolve seam — it is registry-generic).
            crate::ui_commands::telnet_post_office_connect,
            crate::ui_commands::telnet_post_office_abort,
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
            crate::logging::commands::log_frontend_error,          // tuxlink-4b96 — webview errors → logs
            gl_render_confirmed,                                    // tuxlink-4pdu — map render confirmed → clear GL safe-mode marker
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
            // tuxlink-je5d: read-only connection record by callsign, aggregating
            // attempts across every favorite whose gateway == callsign (reuses
            // the favorites store + tod_hint gate; no new storage).
            crate::contacts::commands::contacts_connection_record,
            // tuxlink-s1o1: recent gateways for Winlink map layer pin rendering.
            crate::contacts::commands::contacts_recent_gateways,
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
            // tuxlink-dyop Phase 8.1: LAN map-tile command surface. configure
            // (validate→activate→persist), test (dry-run validate), clear-cache,
            // and a no-network status reflection of the gatekeeper. All take the
            // managed `Arc<TileGatekeeper>` set up in the app_data_dir arm above.
            crate::tiles::commands::configure_tile_source,
            crate::tiles::commands::test_tile_source,
            crate::tiles::commands::clear_tile_cache,
            crate::tiles::commands::tile_source_status,
            // tuxlink-7dwqa (Plan 2): egress arm/disarm/status — operator
            // delegates send authority to the MCP agent caller for a timed window.
            crate::ui_core::security_commands::egress_arm,
            crate::ui_core::security_commands::egress_disarm,
            crate::ui_core::security_commands::egress_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
