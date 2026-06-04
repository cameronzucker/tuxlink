//! Lazy loopback HTTP server for HTML Forms webview path.
//!
//! Lifecycle is per-form-open, NOT application-lifetime: each
//! [`FormSession::open`] call binds a fresh `127.0.0.1:0` listener,
//! spawns a serve task, and serves ONE form at root `/`. When the caller
//! drops the session (or calls [`FormSession::close`]) the serve task is
//! aborted and the port is released.
//!
//! ## Why per-session, not multi-session
//!
//! Per the 2026-06-01 WLE snapshot recon
//! (`dev/scratch/2026-06-01-wle-snapshot-recon.md`), WLE Standard Forms
//! POST to `http://{FormServer}:{FormPort}` with **no path component**
//! (e.g. `action="http://127.0.0.1:34567"` after substitution). That
//! ecosystem contract forces a per-session-per-port design: the port
//! demarcates the session, not a URL path or token. This matches the
//! Winlink Express reference implementation.
//!
//! ## Security model (per design §10, with recon-time revision)
//!
//! - **Loopback only**: bind 127.0.0.1, never 0.0.0.0
//! - **Ephemeral port**: kernel-assigned (`:0`); no fixed port discoverable
//!   in config files
//! - **Per-form-open lifecycle**: listener is up only while the operator
//!   has the form open
//! - **Scoped Tauri capability** (`forms-webview.json`): the child webview
//!   gets HTTP access to `http://127.0.0.1:*` and nothing else; no IPC, no
//!   fs, no shell, no window control
//! - **No URL-token defense**: per the recon Option A recommendation, the
//!   token is dropped because the WLE form's submit action carries no path.
//!   The defense is loopback + ephemeral port + capability scope, matching
//!   the WLE Express reference contract
//!
//! ## Routes
//!
//! - `GET /` → serve the form HTML with WLE substitutions + skin link
//! - `POST /` → accept the form submission; parse body; emit ParsedBody
//!   on the in-process channel; respond with a brief "Submitted ✓" page
//! - `GET /skin.css` → serve the static tuxlink skin (`forms::skin`)
//! - `GET /folder/<path>/<file>` → serve adjacent assets from the
//!   template's folder (P1 minimal support for `{FormFolder}` references;
//!   path-traversal rejected via canonicalize)
//! - anything else → 404
//!
//! Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md
//!       Task 6.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    extract::{Path as AxumPath, State},
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinHandle};

use super::multipart::{parse_multipart, parse_urlencoded, ParsedBody};
use super::skin;
use super::wle_templates::Template;

/// Body-size cap for /submit. WLE form submissions are well under 100 KB
/// in practice; 1 MB is generous headroom and prevents trivial DOS.
const MAX_SUBMIT_BODY_BYTES: usize = 1_048_576;

/// Content-Security-Policy header value sent with HTML responses.
///
/// Rationale (Codex 2026-06-01 P1 #2): without an explicit CSP, a malicious
/// custom-form HTML file on a loopback origin can still load external
/// scripts / images / submit to external URLs (Tauri capabilities only
/// constrain Tauri IPC, not ordinary browser loads). This locks the
/// origin to its own assets.
///
/// - `default-src 'self'` — no external resources by default
/// - `script-src 'self' 'unsafe-inline'` — WLE templates have inline
///   `<script>` blocks; same-origin scripts (e.g. /bridge.js if we ship
///   one) are allowed
/// - `style-src 'self' 'unsafe-inline'` — WLE templates have inline styles
///   (`<style>` blocks + style="" attributes)
/// - `img-src 'self' data:` — allow inline data URIs for embedded
///   images / icons
/// - `connect-src 'self'` — prevent `fetch()` to external endpoints
/// - `form-action 'self'` — prevent form submission to external URLs (the
///   exfiltration attack Codex called out)
/// - `frame-src 'none'` + `object-src 'none'` — no iframes/objects
const FORM_CSP: &str =
    "default-src 'self'; \
     script-src 'self' 'unsafe-inline'; \
     style-src 'self' 'unsafe-inline'; \
     img-src 'self' data:; \
     connect-src 'self'; \
     form-action 'self'; \
     frame-src 'none'; \
     object-src 'none'";

/// State shared with the axum router. Cheap to clone (Arc-wrapped channel).
#[derive(Clone)]
struct SessionState {
    /// Pre-substituted form HTML, ready to serve at GET /.
    form_html: String,
    /// Absolute path to the template's parent folder, for /folder/* asset serving.
    template_folder_path: PathBuf,
    /// Channel for emitting parsed submissions back to the caller.
    submit_tx: mpsc::UnboundedSender<ParsedBody>,
    /// Port the listener is bound to. Used to validate the Origin header
    /// on POST / per Codex 2026-06-01 P1 #3.
    port: u16,
}

/// One open form session. Drop the value (or call [`close`]) to tear
/// down the serve task and free the port.
pub struct FormSession {
    pub port: u16,
    /// Receive submissions from the form. The receiver is held in an
    /// `Option` so the command layer can `take_submit_rx()` and move it
    /// into a forwarder task without removing the session from the
    /// registry (the registry retains the `FormSession` for its
    /// AbortHandle on `close`). Direct callers of `FormSession::open`
    /// (e.g., the http_server integration test) still see the channel
    /// here.
    submit_rx: Option<mpsc::UnboundedReceiver<ParsedBody>>,
    /// AbortHandle for the spawned serve task; calling `abort()` shuts
    /// down the listener (the inner JoinHandle is intentionally not kept
    /// — Drop relies solely on the AbortHandle for sync teardown).
    abort: AbortHandle,
}

impl FormSession {
    /// Open a new form session: read the template, substitute placeholders,
    /// bind a fresh listener, start serving.
    ///
    /// `template` is the form to serve; its HTML is loaded from
    /// `template.path` immediately so a later rename / delete doesn't break
    /// the open session.
    pub async fn open(template: Template) -> Result<Self, String> {
        let raw = std::fs::read_to_string(&template.path)
            .map_err(|e| format!("read template: {e}"))?;
        let folder = template
            .path
            .parent()
            .ok_or_else(|| "template has no parent folder".to_string())?
            .to_path_buf();

        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .map_err(|e| format!("bind 127.0.0.1:0: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {e}"))?
            .port();

        let form_html = substitute_template(&raw, port, &template.folder);

        let (submit_tx, submit_rx) = mpsc::unbounded_channel();
        let state = Arc::new(SessionState {
            form_html,
            template_folder_path: folder,
            submit_tx,
            port,
        });

        let router = build_router(state);
        let serve_handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let abort = serve_handle.abort_handle();
        // The JoinHandle drops here; the spawned task continues running
        // (tokio keeps it alive until completion or abort). The
        // AbortHandle is what we hold for shutdown.
        Ok(Self {
            port,
            submit_rx: Some(submit_rx),
            abort,
        })
    }

    /// The URL the child webview should navigate to. Form-fetch + submit
    /// share the same origin; submit lands at `/` per the WLE contract.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    /// Move the submit receiver out of the session. Returns `None` if it
    /// has already been taken. Used by [`FormSessionRegistry::open`] to
    /// hand the receiver to a forwarder task while the registry retains
    /// the [`FormSession`] for its `AbortHandle`.
    pub fn take_submit_rx(&mut self) -> Option<mpsc::UnboundedReceiver<ParsedBody>> {
        self.submit_rx.take()
    }

    /// Explicit shutdown. Aborts the serve task; the listener is dropped
    /// + port released. Idempotent.
    pub fn close(&mut self) {
        self.abort.abort();
    }
}

impl Drop for FormSession {
    fn drop(&mut self) {
        self.abort.abort();
        // serve_handle awaits cancellation in the tokio runtime; we don't
        // block here (Drop must be sync).
    }
}

/// Substitute the WLE placeholders. Stable, order-independent string ops.
///
/// {FormServer} → 127.0.0.1
/// {FormPort}   → <port>
/// {FormFolder} → /folder/<url-encoded-folder>
fn substitute_template(raw: &str, port: u16, folder: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    let folder_path = format!(
        "/folder/{}",
        utf8_percent_encode(folder, NON_ALPHANUMERIC)
    );
    let with_subs = raw
        .replace("{FormServer}", "127.0.0.1")
        .replace("{FormPort}", &port.to_string())
        .replace("{FormFolder}", &folder_path);
    inject_skin_link(&with_subs)
}

/// Insert `<link rel="stylesheet" href="/skin.css">` into <head>. If there's
/// no `<head>`, prepend it before the doctype/html (browsers tolerate that).
fn inject_skin_link(html: &str) -> String {
    let link = r#"<link rel="stylesheet" href="/skin.css">"#;
    if let Some(pos) = html.to_lowercase().find("<head>") {
        let mut out = html.to_string();
        let insert_at = pos + "<head>".len();
        out.insert_str(insert_at, link);
        out
    } else if let Some(pos) = html.to_lowercase().find("<html") {
        // No <head>; prepend before <html>. (Some WLE templates open with
        // `<!DOCTYPE …>` then `<html …>`.)
        let mut out = html.to_string();
        out.insert_str(pos, link);
        out
    } else {
        // Malformed: just prepend.
        format!("{link}{html}")
    }
}

fn build_router(state: Arc<SessionState>) -> Router {
    Router::new()
        .route("/", any(root_handler))
        .route("/skin.css", get(skin_handler))
        .route("/folder/{*path}", get(folder_handler))
        .with_state(state)
}

/// GET / serves the form; POST / accepts the submit.
async fn root_handler(
    State(state): State<Arc<SessionState>>,
    req: Request<Body>,
) -> Response {
    match *req.method() {
        Method::GET => html_with_csp(&state.form_html),
        Method::POST => submit_handler(state, req).await,
        _ => (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response(),
    }
}

/// Wrap an HTML body with the Content-Security-Policy header that locks
/// the form-server origin to its own assets (Codex 2026-06-01 P1 #2).
fn html_with_csp(body: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/html; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        FORM_CSP.parse().unwrap(),
    );
    (headers, body.to_string()).into_response()
}

/// Validate the request's Origin header against the local server's origin.
///
/// Per Codex 2026-06-01 P1 #3, the embedded WebKitGTK webview sends an
/// `Origin: http://127.0.0.1:<port>` header on POST submits from the
/// loaded form. Any other origin (or absent header) indicates the request
/// is NOT from the legitimate form session — most likely a same-host
/// process that discovered the ephemeral port. Reject with 403.
fn origin_matches_local(headers: &HeaderMap, port: u16) -> bool {
    let expected = format!("http://127.0.0.1:{}", port);
    headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|o| o == expected)
        .unwrap_or(false)
}

async fn submit_handler(state: Arc<SessionState>, req: Request<Body>) -> Response {
    if !origin_matches_local(req.headers(), state.port) {
        return (StatusCode::FORBIDDEN, "origin mismatch").into_response();
    }
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = match to_bytes(req.into_body(), MAX_SUBMIT_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "body too large").into_response()
        }
    };
    let parsed = if let Some(boundary) = content_type
        .split(';')
        .map(|s| s.trim())
        .find_map(|s| s.strip_prefix("boundary="))
    {
        // multer is strict about quoted boundaries; trim quotes if present.
        let boundary = boundary.trim_matches('"');
        match parse_multipart(boundary, body).await {
            Ok(p) => p,
            Err(e) => return (StatusCode::BAD_REQUEST, format!("multipart parse: {e}")).into_response(),
        }
    } else {
        let body_str = match std::str::from_utf8(&body) {
            Ok(s) => s,
            Err(_) => return (StatusCode::BAD_REQUEST, "non-UTF-8 urlencoded body").into_response(),
        };
        match parse_urlencoded(body_str) {
            Ok(p) => p,
            Err(e) => return (StatusCode::BAD_REQUEST, format!("urlencoded parse: {e}")).into_response(),
        }
    };
    if state.submit_tx.send(parsed).is_err() {
        // Receiver dropped; the session is closing. Return success anyway
        // so the form's onsubmit doesn't show a confusing error.
    }
    html_with_csp(SUBMITTED_HTML)
}

const SUBMITTED_HTML: &str = r#"<!doctype html>
<html><head><title>Submitted</title>
<link rel="stylesheet" href="/skin.css">
</head><body><h2>Submitted ✓</h2>
<p>Returning to tuxlink…</p>
</body></html>"#;

async fn skin_handler() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/css; charset=utf-8".parse().unwrap(),
    );
    (headers, skin::generate().to_string()).into_response()
}

/// Serve an asset inside the template's folder. Path is URL-decoded by
/// axum; we canonicalize against the folder and reject anything that
/// escapes it.
async fn folder_handler(
    State(state): State<Arc<SessionState>>,
    AxumPath(rest): AxumPath<String>,
) -> Response {
    // Split off the encoded folder prefix; rest = "<encoded-folder>/<file...>"
    let mut parts = rest.splitn(2, '/');
    let _folder_segment = parts.next().unwrap_or("");
    let file_path = parts.next().unwrap_or("");
    // Note: the encoded folder is informational; we always serve from the
    // session's template folder (which is the only folder the operator
    // intended to be readable). This also blunts any directory-name
    // forgery attempt by a malicious template.
    if file_path.is_empty() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let candidate = state.template_folder_path.join(file_path);
    let canonical = match candidate.canonicalize() {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let root_canonical = match state.template_folder_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, "folder gone").into_response(),
    };
    if !canonical.starts_with(&root_canonical) {
        return (StatusCode::FORBIDDEN, "path traversal").into_response();
    }
    match std::fs::read(&canonical) {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            let ext = canonical
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let ct = match ext.as_str() {
                "html" | "htm" => "text/html; charset=utf-8",
                "css" => "text/css; charset=utf-8",
                "js" => "application/javascript",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "svg" => "image/svg+xml",
                _ => "application/octet-stream",
            };
            headers.insert(header::CONTENT_TYPE, ct.parse().unwrap());
            (headers, bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ============================================================================
// FormSessionRegistry — multi-session ownership for the command layer (P1 Task 8)
// ============================================================================

/// Owns every open [`FormSession`] keyed by a per-open token. The token is
/// minted by [`FormSessionRegistry::open`] (16 hex chars from `rand::random`
/// matching the modem-consent-token shape) and is the lookup key used by
/// the `close_webview_form_server(token)` Tauri command + the
/// `compose-form-<token>` webview label.
///
/// ## Why the token is NOT in the URL
///
/// Per the 2026-06-01 WLE snapshot recon (see module docs above), the
/// form's `<form action="http://{FormServer}:{FormPort}">` substitution
/// gives the submit endpoint NO path component — so a path-embedded token
/// can't survive the WLE template contract. The token here is intra-tuxlink
/// only: it scopes the registry lookup + the child webview's label.
/// Loopback + ephemeral-port + the capability scope + the Origin header
/// check on POST are the security boundary, not the token.
///
/// ## Concurrency
///
/// `tokio::sync::Mutex` (not `std::sync::Mutex`) so the async command
/// handlers can hold the guard across the `FormSession::open` await
/// without blocking the runtime. Sessions are stored as `Option` slots so
/// the `submit_rx` can be `take()`n out for the forwarder task at
/// open-time without removing the session entry from the map (the entry
/// is removed by `close`).
pub struct FormSessionRegistry {
    sessions: tokio::sync::Mutex<std::collections::HashMap<String, FormSession>>,
}

/// Receiver half handed to the command layer's forwarder task on open.
/// Owning this struct gives the caller exclusive access to submit
/// notifications until either `close()` drops the session or the
/// frontend's child webview closes (which the operator-flow eventually
/// triggers via `close_webview_form_server`).
pub struct OpenedSession {
    pub token: String,
    pub port: u16,
    pub submit_rx: mpsc::UnboundedReceiver<ParsedBody>,
}

impl FormSessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Open a new form session and register it. Returns the token + bound
    /// port + the submit receiver. Caller (the `open_webview_form` Tauri
    /// command) MUST spawn a forwarder task that drains `submit_rx` onto
    /// the `form-submitted` event scoped to the child webview's label.
    pub async fn open(&self, template: Template) -> Result<OpenedSession, String> {
        let mut session = FormSession::open(template).await?;
        let port = session.port;
        // Take the receiver out so the registry only retains the abort
        // handle + port (the receiver is owned by the forwarder task).
        let submit_rx = session.take_submit_rx().ok_or_else(|| {
            "FormSession::take_submit_rx returned None on a fresh session".to_string()
        })?;
        let token = mint_session_token();
        // Token collisions in a 16-hex-char namespace are astronomically
        // rare, but be defensive: keep minting until we hit an empty slot.
        let mut guard = self.sessions.lock().await;
        let mut tok = token;
        while guard.contains_key(&tok) {
            tok = mint_session_token();
        }
        guard.insert(tok.clone(), session);
        Ok(OpenedSession {
            token: tok,
            port,
            submit_rx,
        })
    }

    /// Tear down a registered session. Idempotent: closing an unknown
    /// token is a no-op (returns `Ok(())`) so the frontend's cleanup path
    /// can call this without bothering to know whether the session was
    /// already collapsed by a Drop / runtime shutdown.
    pub async fn close(&self, token: &str) -> Result<(), String> {
        let mut guard = self.sessions.lock().await;
        // Drop the FormSession; its Drop impl aborts the serve task.
        let _ = guard.remove(token);
        Ok(())
    }

    /// Test helper — returns the number of currently-registered sessions.
    #[cfg(test)]
    pub async fn session_count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

impl Default for FormSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Mint a 16-hex-char session token. Mirrors the modem-consent-token
/// shape (see `modem_status::mint_consent_token`): enough for in-process
/// uniqueness, NOT a secret — security on this surface is loopback +
/// ephemeral port + capability scope + the per-request Origin check.
fn mint_session_token() -> String {
    (0..16)
        .map(|_| {
            let n: u8 = rand::random::<u8>() & 0xF;
            std::char::from_digit(n as u32, 16).unwrap()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::wle_templates::TemplateSource;
    use axum::body::Body;
    use axum::http::Request;
    use tempfile::TempDir;
    use tower::ServiceExt;

    /// Test port — used both for state.port AND for the synthesized
    /// Origin header on POST tests so the origin-matches-local check
    /// passes for happy-path requests.
    const TEST_PORT: u16 = 34567;

    fn make_state(html: &str) -> (Arc<SessionState>, mpsc::UnboundedReceiver<ParsedBody>) {
        let td = TempDir::new().unwrap();
        let folder = td.path().to_path_buf();
        // Leak the tempdir so the path stays valid for the test's lifetime.
        // (A test-helper struct that holds the TempDir would be cleaner, but
        // for the SessionState tests we don't actually read files.)
        std::mem::forget(td);
        let (tx, rx) = mpsc::unbounded_channel();
        let state = Arc::new(SessionState {
            form_html: html.to_string(),
            template_folder_path: folder,
            submit_tx: tx,
            port: TEST_PORT,
        });
        (state, rx)
    }

    fn local_origin() -> String {
        format!("http://127.0.0.1:{}", TEST_PORT)
    }

    #[tokio::test]
    async fn get_root_serves_form_html() {
        let html = "<html><head></head><body>test form</body></html>";
        let (state, _rx) = make_state(html);
        let router = build_router(state);
        let resp = router
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 64_000).await.unwrap();
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("test form"));
    }

    #[tokio::test]
    async fn skin_route_serves_css() {
        let (state, _rx) = make_state("<html></html>");
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/skin.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "text/css; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn post_root_urlencoded_dispatches_to_channel() {
        let (state, mut rx) = make_state("");
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .header("Origin", local_origin())
                    .body(Body::from("Subject=Hi&Submit=Submit"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let parsed = rx.recv().await.unwrap();
        assert_eq!(parsed.fields["Subject"][0], "Hi");
        assert_eq!(parsed.submitter, Some("Submit".to_string()));
    }

    #[tokio::test]
    async fn post_root_rejects_missing_origin() {
        let (state, _rx) = make_state("");
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    // No Origin header — should be 403 per Codex 2026-06-01 P1 #3.
                    .body(Body::from("Subject=Hi"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_root_rejects_foreign_origin() {
        let (state, _rx) = make_state("");
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .header("Origin", "http://evil.example.com")
                    .body(Body::from("Subject=Hi"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_root_response_carries_csp_header() {
        let (state, _rx) = make_state("<html><head></head></html>");
        let router = build_router(state);
        let resp = router
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = resp
            .headers()
            .get(header::CONTENT_SECURITY_POLICY)
            .expect("CSP header expected on form HTML response")
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("form-action 'self'"));
        assert!(csp.contains("connect-src 'self'"));
    }

    #[tokio::test]
    async fn post_root_multipart_dispatches_to_channel() {
        let (state, mut rx) = make_state("");
        let router = build_router(state);
        let boundary = "----testb";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"Subject\"\r\n\r\nHi\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"Submit\"\r\n\r\nSubmit\r\n\
             --{b}--\r\n",
            b = boundary
        );
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header(
                        "Content-Type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .header("Origin", local_origin())
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let parsed = rx.recv().await.unwrap();
        assert_eq!(parsed.fields["Subject"][0], "Hi");
        assert_eq!(parsed.submitter, Some("Submit".to_string()));
    }

    #[tokio::test]
    async fn folder_route_rejects_path_traversal() {
        let (state, _rx) = make_state("");
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/folder/abc/../../../etc/passwd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Either FORBIDDEN (traversal detected post-canonicalize) or
        // NOT_FOUND (canonicalize on a non-existent path fails) — both
        // are acceptable defenses.
        assert!(
            resp.status() == StatusCode::FORBIDDEN
                || resp.status() == StatusCode::NOT_FOUND,
            "path traversal must not return 200; got: {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let (state, _rx) = make_state("");
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn substitute_template_replaces_form_server_and_form_port() {
        let raw = r#"<form action="http://{FormServer}:{FormPort}"></form>"#;
        let out = substitute_template(raw, 34567, "ICS Forms");
        assert!(out.contains("http://127.0.0.1:34567"));
        assert!(!out.contains("{FormServer}"));
        assert!(!out.contains("{FormPort}"));
    }

    #[test]
    fn substitute_template_replaces_form_folder_url_encoded() {
        let raw = r#"<a href="{FormFolder}/foo.html">x</a>"#;
        let out = substitute_template(raw, 1, "ICS Forms");
        // "ICS Forms" → "ICS%20Forms" (NON_ALPHANUMERIC encodes the space)
        assert!(out.contains("/folder/ICS%20Forms"));
    }

    #[test]
    fn inject_skin_link_adds_link_into_head() {
        let html = "<html><head></head><body></body></html>";
        let out = inject_skin_link(html);
        assert!(out.contains(r#"<link rel="stylesheet" href="/skin.css">"#));
        // Link is placed inside <head>, not at the start.
        let head_idx = out.find("<head>").unwrap();
        let link_idx = out.find("/skin.css").unwrap();
        assert!(link_idx > head_idx);
    }

    #[test]
    fn inject_skin_link_handles_no_head() {
        let html = "<html><body></body></html>";
        let out = inject_skin_link(html);
        assert!(out.contains("/skin.css"));
    }

    #[tokio::test]
    async fn submit_with_oversized_body_returns_413() {
        let (state, _rx) = make_state("");
        let router = build_router(state);
        let huge = "A".repeat(MAX_SUBMIT_BODY_BYTES + 10);
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .header("Origin", local_origin())
                    .body(Body::from(huge))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    // ============================================================
    // Integration test: real bind + serve + fetch
    // ============================================================

    #[tokio::test]
    async fn full_open_session_serves_form_html_via_real_tcp() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("Test.html");
        std::fs::write(&path, "<html><head></head><body>OK</body></html>").unwrap();
        let template = Template {
            id: "Test".to_string(),
            label: "Test".to_string(),
            folder: "".to_string(),
            source: TemplateSource::Bundled,
            path: path.clone(),
        };
        let mut session = FormSession::open(template).await.unwrap();
        let url = session.url();
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(body.contains("OK"));
        assert!(body.contains("/skin.css"));
        session.close();
        // After close, a fresh GET should fail (the listener is gone).
        // tolerate either a TCP-refuse error or a timeout — both signal
        // the listener is down.
        let after = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap()
            .get(&url)
            .send()
            .await;
        assert!(after.is_err(), "listener should be down after close()");
    }

    // ============================================================
    // FormSessionRegistry — P1 Task 8 command-layer plumbing
    // ============================================================

    fn make_template_on_disk(td: &TempDir, name: &str) -> Template {
        let path = td.path().join(format!("{name}.html"));
        std::fs::write(
            &path,
            "<html><head></head><body>registry-test</body></html>",
        )
        .unwrap();
        Template {
            id: name.to_string(),
            label: name.to_string(),
            folder: "".to_string(),
            source: TemplateSource::Bundled,
            path,
        }
    }

    #[tokio::test]
    async fn registry_open_returns_token_port_and_receiver() {
        let td = TempDir::new().unwrap();
        let template = make_template_on_disk(&td, "RegOpen");
        let registry = FormSessionRegistry::new();
        let opened = registry.open(template).await.unwrap();
        assert!(!opened.token.is_empty(), "token must be non-empty");
        assert_eq!(opened.token.len(), 16, "token shape: 16 hex chars");
        assert!(
            opened.token.chars().all(|c| c.is_ascii_hexdigit()),
            "token must be hex"
        );
        assert!(opened.port != 0, "port must be the bound ephemeral port");
        assert_eq!(registry.session_count().await, 1);
    }

    #[tokio::test]
    async fn registry_open_mints_distinct_tokens_for_concurrent_sessions() {
        let td = TempDir::new().unwrap();
        let t1 = make_template_on_disk(&td, "Multi1");
        let t2 = make_template_on_disk(&td, "Multi2");
        let registry = FormSessionRegistry::new();
        let a = registry.open(t1).await.unwrap();
        let b = registry.open(t2).await.unwrap();
        assert_ne!(a.token, b.token, "concurrent sessions get distinct tokens");
        assert_ne!(
            a.port, b.port,
            "concurrent sessions bind distinct ephemeral ports"
        );
        assert_eq!(registry.session_count().await, 2);
    }

    #[tokio::test]
    async fn registry_close_drops_the_session_and_is_idempotent() {
        let td = TempDir::new().unwrap();
        let template = make_template_on_disk(&td, "Closing");
        let registry = FormSessionRegistry::new();
        let opened = registry.open(template).await.unwrap();
        let url = format!("http://127.0.0.1:{}/", opened.port);
        assert_eq!(registry.session_count().await, 1);

        registry.close(&opened.token).await.unwrap();
        assert_eq!(registry.session_count().await, 0, "session entry removed");

        // Idempotent: closing again is fine.
        registry.close(&opened.token).await.unwrap();
        assert_eq!(registry.session_count().await, 0);

        // Listener is down (FormSession::drop -> abort).
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap();
        let after = client.get(&url).send().await;
        assert!(after.is_err(), "listener must be down after close()");
    }

    #[tokio::test]
    async fn registry_close_unknown_token_is_ok() {
        let registry = FormSessionRegistry::new();
        // No session ever opened; closing an unknown token must succeed.
        registry.close("0000000000000000").await.unwrap();
        assert_eq!(registry.session_count().await, 0);
    }

    #[tokio::test]
    async fn registry_take_submit_rx_is_consumed_by_open() {
        // After registry.open, the receiver lives with the caller; the
        // session retained in the registry has no rx (the forwarder task
        // owns it). Sanity: re-taking from the retained session would
        // return None.
        let td = TempDir::new().unwrap();
        let template = make_template_on_disk(&td, "Taken");
        let registry = FormSessionRegistry::new();
        let _opened = registry.open(template).await.unwrap();
        let mut guard = registry.sessions.lock().await;
        let session = guard.values_mut().next().unwrap();
        assert!(
            session.take_submit_rx().is_none(),
            "submit_rx already moved into the OpenedSession"
        );
    }

    #[test]
    fn mint_session_token_returns_16_hex_chars() {
        let t = mint_session_token();
        assert_eq!(t.len(), 16);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
