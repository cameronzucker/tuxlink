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
//!   (Form sessions only — Viewer sessions return 404)
//! - `GET /skin.css` → serve the static tuxlink skin (`forms::skin`)
//! - `GET /folder/<path>/<file>` → serve adjacent assets from the
//!   template's folder (P1 minimal support for `{FormFolder}` references;
//!   path-traversal rejected via canonicalize)
//! - anything else → 404
//!
//! ## Session kinds (Task 11)
//!
//! Sessions come in two kinds — [`SessionKind::Form`] (the historical
//! authoring path, with a submit channel) and [`SessionKind::Viewer`] (the
//! P1 Task 11 receive-side fallback for unknown-form-id messages, with no
//! submit channel; the POST route returns 404 in this mode). The kind is
//! captured in [`SessionState`] and consulted by the root handler. Viewer
//! sessions additionally inject FormPayload field values via a JS snippet
//! appended before `</body>` so the form's hidden inputs and `{var X}`
//! inline placeholders display the received data.
//!
//! Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md
//!       Task 6 (Form mode) + Task 11 (Viewer mode).

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

/// Per bd tuxlink-4g2n: largest asset the form-adjacent /folder/<path> route
/// will serve. 8 MiB comfortably accommodates WLE template-bundled images,
/// CSS, and JS while preventing a custom-form directory from letting any
/// local client allocate arbitrarily-large memory per request.
const MAX_FOLDER_ASSET_BYTES: usize = 8 * 1_048_576;

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

/// Session mode discriminator (P1 Task 11).
///
/// Form sessions are the historical send-side authoring path: the form's
/// `<form action="http://{FormServer}:{FormPort}">` POST lands at `/` and
/// produces a `ParsedBody` on the submit channel. Viewer sessions are the
/// receive-side fallback for unknown form_ids — the field values are
/// pre-bound into the HTML server-side, and the POST route returns 404 so
/// the operator cannot resubmit a received form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    Form,
    Viewer,
}

/// State shared with the axum router. Cheap to clone (Arc-wrapped channel).
#[derive(Clone)]
struct SessionState {
    /// Discriminator branching the POST handler (Form = parse + emit;
    /// Viewer = 404). Captured at open time; immutable for the session.
    kind: SessionKind,
    /// Pre-substituted form HTML, ready to serve at GET /. For Viewer
    /// sessions this also has FormPayload field values bound via an
    /// appended `<script>` tag (see [`inject_field_value_script`]).
    form_html: String,
    /// Absolute path to the template's parent folder, for /folder/* asset serving.
    template_folder_path: PathBuf,
    /// Channel for emitting parsed submissions back to the caller. Always
    /// present, but for Viewer sessions the receiver is dropped at open
    /// time (no forwarder task is spawned) and the POST handler returns
    /// 404 before ever touching this channel.
    /// Bounded — per bd tuxlink-rk6s, the channel holds at most one in-flight
    /// submission. WLE forms close themselves after a successful submit, so
    /// any second submit on an open session is anomalous (either a buggy
    /// template or a same-host adversary that bypassed the origin check). A
    /// bounded channel + try_send + 503-on-full prevents a local flood from
    /// growing memory while the compose receiver catches up.
    submit_tx: mpsc::Sender<ParsedBody>,
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
    /// obtain the receiver via `take_submit_rx()`.
    submit_rx: Option<mpsc::Receiver<ParsedBody>>,
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

        let (submit_tx, submit_rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html,
            template_folder_path: folder,
            submit_tx,
            port,
        });

        tracing::info!(
            target: "tuxlink::forms",
            port,
            kind = "form",
            "form session opened",
        );
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

    /// Open a Viewer-mode session for the receive-side fallback (P1 Task 11).
    ///
    /// `viewer_path` points to a WLE `*_Viewer.html` template; the file
    /// stem isn't required to match a Template id (catalog `Template`s key
    /// off the *form* file stem like `ICS213_Initial`, but Viewer files
    /// have suffixes like `_Viewer` or sometimes a free-form name like
    /// `Bulletin Viewer.html`). The folder is resolved from the parent dir
    /// and used for `{FormFolder}` substitution + adjacent-asset serving.
    ///
    /// `field_values` is the parsed FormPayload's `(field_id, value)`
    /// mapping. Two substitution passes bind it into the HTML:
    /// 1. WLE `{var Name}` placeholders → `value` (server-side string
    ///    substitution; matches the WLE viewer convention for inline
    ///    text display).
    /// 2. A `<script>` tag appended before `</body>` runs on
    ///    `DOMContentLoaded` and assigns `document.querySelectorAll(
    ///    '[name="Name"]').forEach(el => el.value = "value")` for each
    ///    field. This covers hidden inputs the viewer relies on (some
    ///    viewers use `<input name="..." />` for round-tripping data
    ///    instead of `{var}` placeholders).
    ///
    /// POST `/` returns 404 in this mode — the operator cannot resubmit
    /// a received form. The submit channel is created (it lives on
    /// [`SessionState`]) but never drained; the receiver returned in
    /// [`FormSession::submit_rx`] is `None` so callers can't accidentally
    /// wire up a forwarder task against a dead channel.
    pub async fn open_viewer(
        viewer_path: PathBuf,
        folder: String,
        field_values: &std::collections::HashMap<String, String>,
    ) -> Result<Self, String> {
        let raw = std::fs::read_to_string(&viewer_path)
            .map_err(|e| format!("read viewer template: {e}"))?;
        let folder_path = viewer_path
            .parent()
            .ok_or_else(|| "viewer template has no parent folder".to_string())?
            .to_path_buf();

        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .map_err(|e| format!("bind 127.0.0.1:0: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {e}"))?
            .port();

        let substituted = substitute_template(&raw, port, &folder);
        let with_vars = substitute_var_placeholders(&substituted, field_values);
        let form_html = inject_field_value_script(&with_vars, field_values);

        let (submit_tx, _submit_rx) = mpsc::channel(1);
        // _submit_rx is dropped here: Viewer mode has no submit forwarder.
        // The POST handler rejects with 404 before ever sending on submit_tx,
        // but the tx is retained on SessionState because SessionState's shape
        // is shared with Form mode (cleaner than two near-identical structs).
        let state = Arc::new(SessionState {
            kind: SessionKind::Viewer,
            form_html,
            template_folder_path: folder_path,
            submit_tx,
            port,
        });

        tracing::info!(
            target: "tuxlink::forms",
            port,
            kind = "viewer",
            "form session opened",
        );
        let router = build_router(state);
        let serve_handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let abort = serve_handle.abort_handle();
        Ok(Self {
            port,
            // Viewer sessions never expose a receiver — there's nothing to
            // forward, the POST handler always 404s.
            submit_rx: None,
            abort,
        })
    }

    /// Open an editable Form session **pre-bound** with field values
    /// (tuxlink-hhfx / G10 reply threading). The union of [`FormSession::open`]
    /// (editable + a live submit channel) and [`FormSession::open_viewer`]
    /// (server-side value binding): the SendReply authoring HTML is served with
    /// the original form's field values pre-filled (so the operator sees the
    /// request they're replying to) AND the POST submit path is live (so the
    /// operator's filled reply round-trips back to Compose like any authoring
    /// form, producing a `ParsedBody` on the submit channel).
    ///
    /// `html_path` is the SendReply authoring HTML; `folder` is its
    /// `{FormFolder}` for adjacent-asset serving; `field_values` are the
    /// original form's values (plus `MsgOriginalBody`) to pre-bind. Binding uses
    /// the same two passes as [`open_viewer`](FormSession::open_viewer)
    /// (`{var X}` server-side substitution + a DOM-injection script for
    /// `[name="X"]` inputs), so the SendReply's hidden round-trip inputs carry
    /// the original data back through the POST untouched.
    pub async fn open_form_prebound(
        html_path: PathBuf,
        folder: String,
        field_values: &std::collections::HashMap<String, String>,
    ) -> Result<Self, String> {
        let raw = std::fs::read_to_string(&html_path)
            .map_err(|e| format!("read reply template: {e}"))?;
        let folder_path = html_path
            .parent()
            .ok_or_else(|| "reply template has no parent folder".to_string())?
            .to_path_buf();

        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .map_err(|e| format!("bind 127.0.0.1:0: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {e}"))?
            .port();

        // Form-mode placeholder substitution, then the two viewer-style binding
        // passes so the pre-filled values appear in the served HTML and on POST.
        let substituted = substitute_template(&raw, port, &folder);
        let with_vars = substitute_var_placeholders(&substituted, field_values);
        let form_html = inject_field_value_script(&with_vars, field_values);

        let (submit_tx, submit_rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html,
            template_folder_path: folder_path,
            submit_tx,
            port,
        });

        tracing::info!(
            target: "tuxlink::forms",
            port,
            kind = "form-prebound",
            "form session opened",
        );
        let router = build_router(state);
        let serve_handle: JoinHandle<()> = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let abort = serve_handle.abort_handle();
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
    pub fn take_submit_rx(&mut self) -> Option<mpsc::Receiver<ParsedBody>> {
        self.submit_rx.take()
    }

    /// Explicit shutdown. Aborts the serve task; the listener is dropped
    /// + port released. Idempotent.
    pub fn close(&mut self) {
        tracing::info!(
            target: "tuxlink::forms",
            port = self.port,
            "form session closed",
        );
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
/// {FormFolder} → /folder   (NO trailing folder name — see below)
///
/// Per bd tuxlink-gheo, `{FormFolder}` used to expand to
/// `/folder/<url-encoded-folder>`, which broke nested-folder templates: a
/// folder like `Cat1/Sub1` percent-encodes to `Cat1%2FSub1` but axum decodes
/// wildcard captures before reaching the handler. The handler's
/// `splitn(2, '/')` then sliced "Cat1" off the front and treated
/// "Sub1/file.css" as the file_path relative to the already-pinned folder
/// — double-counting the depth. By emitting just `/folder` here and treating
/// the entire wildcard rest as the file path in `folder_handler`, the session
/// works the same way regardless of nesting depth. The `_folder` parameter
/// is kept on this fn for API stability and is intentionally unused.
fn substitute_template(raw: &str, port: u16, _folder: &str) -> String {
    let with_subs = raw
        .replace("{FormServer}", "127.0.0.1")
        .replace("{FormPort}", &port.to_string())
        .replace("{FormFolder}", "/folder")
        // tuxlink-2tom / G12-C: `{SeqNum}` is the WLE serial-number placeholder
        // (e.g. `<input value="{SeqNum}" name="SeqNum">`). tuxlink assigns the
        // serial authoritatively at SEND time from the persisted counter, so the
        // field opens blank here — leaving the literal `{SeqNum}` on screen would
        // be a defect. Forms without the placeholder are unaffected (no-op).
        .replace("{SeqNum}", "");
    inject_skin_link(&with_subs)
}

/// Substitute WLE Viewer `{var Name}` placeholders with the received field
/// values (P1 Task 11). Used by Viewer mode only — Form mode never has
/// pre-bound values.
///
/// Lookup is case-insensitive on the placeholder name to tolerate the
/// viewer-vs-XML casing drift in WLE's catalog (the inbound XML may carry
/// field IDs as `subjectline` while the viewer template references
/// `{var Subjectline}`). Field values are HTML-attribute-escaped before
/// substitution to neutralize XSS in payload values from untrusted senders;
/// the CSP `form-action 'self'` + scoped capability + loopback origin are
/// the primary defense, but the input passes through `<` / `>` / `&` /
/// quotes regardless and HTML-escaping costs nothing.
///
/// Placeholders that don't match any field value are replaced with an empty
/// string, matching WLE's behavior for missing fields. Without this, the
/// raw `{var Foo}` text would render in the viewer.
///
/// 2026-06-04 Codex adrev P2.2: Some bundled viewers put `{var X}` inside
/// `<script>` blocks (e.g. `s = "{var Comments}"` in Hawaii Siren Report
/// Viewer). HTML-escaping the substituted value leaves raw newlines /
/// backslashes / unescaped quotes that corrupt the JS string. To avoid
/// that, this function tracks `<script>` nesting and SKIPS substitution
/// inside script blocks — the field values are bound by the DOM-injection
/// path (`inject_field_value_script`) instead, which JS-escapes correctly.
/// Substituting `{var X}` inside a `<script>` block to "" leaves
/// syntactically-valid (if semantically empty) JS, which is safer than
/// the corrupt-string alternative.
fn substitute_var_placeholders(
    html: &str,
    field_values: &std::collections::HashMap<String, String>,
) -> String {
    // Lowercase the keys once for case-insensitive matching.
    let lowered: std::collections::HashMap<String, &str> = field_values
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.as_str()))
        .collect();
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut in_script = false;
    loop {
        // Find the next salient event: a `{var ` placeholder, an opening
        // `<script` (when outside a script), or a closing `</script>`
        // (when inside one). Whichever has the smallest offset in `rest`
        // wins; we copy everything up to it, advance past it, and repeat.
        let next_var = rest.find("{var ");
        let next_script_open = if !in_script {
            // Match `<script` (case-insensitive, with optional attributes
            // and the trailing `>` or whitespace). The cheapest reliable
            // detection in pure Rust without bringing a parser in: search
            // for the lowercased substring and verify the character before
            // wasn't alphanumeric (so we don't match `<noscript` etc).
            find_tag_open(rest, "script")
        } else {
            None
        };
        let next_script_close = if in_script {
            find_tag_close(rest, "script")
        } else {
            None
        };

        // Pick the earliest event among the three.
        let event = [
            next_var.map(|i| (i, Event::Var)),
            next_script_open.map(|i| (i, Event::ScriptOpen)),
            next_script_close.map(|i| (i, Event::ScriptClose)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(i, _)| *i);

        let Some((idx, kind)) = event else {
            // No more events; flush the remainder and return.
            out.push_str(rest);
            return out;
        };

        match kind {
            Event::Var => {
                out.push_str(&rest[..idx]);
                let after_start = &rest[idx + "{var ".len()..];
                if let Some(end) = after_start.find('}') {
                    let name = after_start[..end].trim();
                    let value = lowered
                        .get(&name.to_lowercase())
                        .copied()
                        .unwrap_or("");
                    if in_script {
                        // Skip substitution inside script blocks. Emit an
                        // empty string in place of the placeholder so the
                        // resulting JS is syntactically valid (e.g.
                        // `s = "{var Comments}"` becomes `s = ""` rather
                        // than `s = "raw\nfield\nvalue"` corrupting the
                        // string literal). The DOM-injection path
                        // (`inject_field_value_script`) re-binds the same
                        // field value via querySelectorAll('[name="X"]'),
                        // so any hidden input that the JS reads via
                        // `document.getElementById('X').value` still gets
                        // the right value at DOMContentLoaded.
                    } else {
                        out.push_str(&html_escape(value));
                    }
                    rest = &after_start[end + 1..];
                } else {
                    // Unterminated placeholder; emit the raw `{var ` and
                    // continue. This is malformed-template tolerance.
                    out.push_str("{var ");
                    rest = after_start;
                }
            }
            Event::ScriptOpen => {
                // Copy through the opening tag (we don't transform tags
                // themselves; just track state). Advance past the `>`
                // that terminates the open tag.
                let after_open = idx + "<script".len();
                if let Some(rel_end) = rest[after_open..].find('>') {
                    let absolute_end = after_open + rel_end + 1;
                    out.push_str(&rest[..absolute_end]);
                    rest = &rest[absolute_end..];
                    in_script = true;
                } else {
                    // Malformed: `<script` with no closing `>`. Copy the
                    // rest and bail.
                    out.push_str(rest);
                    return out;
                }
            }
            Event::ScriptClose => {
                // Copy through the closing tag.
                let close_len = "</script>".len();
                let absolute_end = idx + close_len;
                out.push_str(&rest[..absolute_end]);
                rest = &rest[absolute_end..];
                in_script = false;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Event {
    Var,
    ScriptOpen,
    ScriptClose,
}

/// Find the next `<tag` opener (case-insensitive) in `s`. Returns the
/// byte offset of the `<`. Verifies the next char after the tag name
/// is non-alphanumeric so `<script` doesn't match `<scriptingFoo`
/// (rare in real HTML but defensive).
fn find_tag_open(s: &str, tag: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    let needle = format!("<{tag}");
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find(&needle) {
        let abs = search_from + pos;
        let after = abs + needle.len();
        // The byte right after `<script` must be `>`, whitespace, or `/`
        // (self-closing) to qualify as the real script tag.
        match lower.as_bytes().get(after) {
            Some(&b) if b == b'>' || b == b'/' || b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' => {
                return Some(abs);
            }
            None => return None,
            _ => {
                // Not a real tag; advance past this match.
                search_from = abs + 1;
            }
        }
    }
    None
}

/// Find the next `</tag>` (case-insensitive) in `s`. Returns the byte
/// offset of the `<`.
fn find_tag_close(s: &str, tag: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    let needle = format!("</{tag}>");
    lower.find(&needle)
}

/// HTML-escape a string for insertion into a text context (and as a basic
/// guard for attribute contexts where the substituted placeholder might be
/// inside `<input value="{var X}">`). Conservative: escapes `&`, `<`, `>`,
/// `"`, and `'`. Not for use on URLs.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Escape a string for embedding inside a JavaScript double-quoted string
/// literal. Handles backslash, double-quote, newline, carriage-return, the
/// `</` sequence (which would otherwise close the surrounding `<script>` tag
/// from inside a JS string — the classic XSS via `</script>` payload), and the
/// Unicode line terminators U+2028 / U+2029.
///
/// tuxlink-2590 (receiving-end appsec audit): U+2028 (LINE SEPARATOR) and
/// U+2029 (PARAGRAPH SEPARATOR) terminate a string literal in pre-ES2019 JS
/// engines. WebKitGTK's JavaScriptCore is ES2019+ (they are legal inside
/// string literals there, so this is not an exploitable bypass on the shipping
/// engine), but escaping them is cheap defense-in-depth: it removes a
/// binder-script parse-failure (silent no-op) on any non-ES2019 engine and
/// keeps `js_escape` sound for reuse in any future context. The quote /
/// backslash / `<` / newline escapes above are what actually gate breakout.
fn js_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            // Escape `<` to prevent `</script>` from terminating the script
            // element when injected payload contains it. < is the
            // standard JS escape for `<` inside string literals.
            '<' => out.push_str("\\u003C"),
            // Unicode line terminators — escape so they can never terminate the
            // string literal on a non-ES2019 engine (tuxlink-2590).
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

/// Append a `<script>` tag inside the HTML that, on `DOMContentLoaded`,
/// binds each `field_values` entry into matching `[name="X"]` form inputs.
///
/// This complements [`substitute_var_placeholders`] for WLE viewers that
/// round-trip data through hidden inputs (`<input type="hidden" name="X">`)
/// rather than `{var X}` inline placeholders. The script:
///
/// - Waits for `DOMContentLoaded` so all inputs are in the DOM
/// - Iterates every field value, calls
///   `document.querySelectorAll('[name="X"]')` and sets `.value = ...`
/// - Skips fields whose name contains characters that would break the
///   CSS selector (defensive — WLE field IDs are alphanumeric+underscore
///   in practice, but a hostile sender could craft a payload).
///
/// The script is appended immediately before `</body>` if present; otherwise
/// at the end of the document (browsers tolerate trailing scripts outside
/// `<body>`).
fn inject_field_value_script(
    html: &str,
    field_values: &std::collections::HashMap<String, String>,
) -> String {
    // Build the JS object literal that drives the per-field assignments.
    // Skip names with characters that aren't safe inside our quoted
    // selector — those are payloads, not WLE field IDs.
    let safe_name = |n: &str| n.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    let mut entries: Vec<(String, String)> = field_values
        .iter()
        .filter(|(k, _)| safe_name(k))
        .map(|(k, v)| (js_escape(k), js_escape(v)))
        .collect();
    // Sort for deterministic output (test stability).
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut pairs = String::new();
    for (k, v) in entries {
        pairs.push_str(&format!("    [\"{k}\", \"{v}\"],\n"));
    }

    let script = format!(
        r#"<script>
(function() {{
  var bind = function() {{
    var fields = [
{pairs}    ];
    for (var i = 0; i < fields.length; i++) {{
      var name = fields[i][0];
      var value = fields[i][1];
      var nodes = document.querySelectorAll('[name="' + name + '"]');
      for (var j = 0; j < nodes.length; j++) {{
        nodes[j].value = value;
      }}
    }}
  }};
  if (document.readyState === 'loading') {{
    document.addEventListener('DOMContentLoaded', bind);
  }} else {{
    bind();
  }}
}})();
</script>
"#
    );

    // Prefer to insert before `</body>` (case-insensitive).
    let lower = html.to_lowercase();
    if let Some(pos) = lower.rfind("</body>") {
        let mut out = String::with_capacity(html.len() + script.len());
        out.push_str(&html[..pos]);
        out.push_str(&script);
        out.push_str(&html[pos..]);
        out
    } else {
        // No `</body>`; append at the end. Browsers parse this as if it
        // were a trailing script element.
        let mut out = String::with_capacity(html.len() + script.len());
        out.push_str(html);
        out.push_str(&script);
        out
    }
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

/// GET / serves the form; POST / accepts the submit (Form mode) OR returns
/// 404 (Viewer mode, P1 Task 11 — received forms are read-only).
async fn root_handler(
    State(state): State<Arc<SessionState>>,
    req: Request<Body>,
) -> Response {
    match *req.method() {
        Method::GET => html_with_csp(&state.form_html),
        Method::POST => match state.kind {
            SessionKind::Form => submit_handler(state, req).await,
            // Viewer sessions: the operator can't resubmit a received form.
            // 404 (not 405) so a misbehaving form template that POSTs by
            // accident gets the same response shape as an unmapped route.
            SessionKind::Viewer => (StatusCode::NOT_FOUND, "not found").into_response(),
        },
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
    // Log field count only — never field names or values (privacy boundary).
    tracing::info!(
        target: "tuxlink::forms",
        port = state.port,
        field_count = parsed.fields.len(),
        has_submitter = parsed.submitter.is_some(),
        "form submission received",
    );
    // Bounded channel (capacity 1) per bd tuxlink-rk6s. Use try_send so we
    // don't block the request thread waiting for the compose receiver to
    // drain — a slow receiver shouldn't gate the form's UI response.
    match state.submit_tx.try_send(parsed) {
        Ok(()) => html_with_csp(SUBMITTED_HTML),
        Err(mpsc::error::TrySendError::Closed(_)) => {
            // Receiver dropped; the session is closing. Return success
            // anyway so the form's onsubmit doesn't show a confusing error
            // (matches the pre-bd-rk6s behavior on a closed channel).
            html_with_csp(SUBMITTED_HTML)
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            // Channel full: a prior submission is queued and the compose
            // receiver hasn't drained yet. Return 503 so the form surfaces
            // "in flight" rather than silently dropping. Per the rk6s
            // analysis: WLE forms close themselves after a successful submit,
            // so any second submit during one session is anomalous (template
            // bug or local-host adversary past the origin check).
            (StatusCode::SERVICE_UNAVAILABLE, "submission already in flight")
                .into_response()
        }
    }
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
///
/// Per bd tuxlink-gheo, the entire `rest` capture IS the file path relative
/// to `template_folder_path`. The prior implementation stripped a leading
/// folder segment via `splitn(2, '/')`, which broke nested-folder templates
/// (axum decodes percent-encoded slashes before the handler runs, so a
/// nested-folder URL had MORE slashes than expected and the splitn ate
/// one of them). Since `substitute_template` now emits just `/folder`
/// (not `/folder/<encoded-folder>`), there's no folder segment to strip.
///
/// Per bd tuxlink-4g2n, refuse files whose metadata.len() exceeds
/// MAX_FOLDER_ASSET_BYTES (8 MiB) before reading them. The WLE form-adjacent
/// asset use case is small images / CSS / JS; bounding the per-file size
/// prevents a custom-form directory with a large file from letting any local
/// client allocate the entire file repeatedly until the process OOMs.
async fn folder_handler(
    State(state): State<Arc<SessionState>>,
    AxumPath(rest): AxumPath<String>,
) -> Response {
    if rest.is_empty() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let candidate = state.template_folder_path.join(rest.as_str());
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
    // tuxlink-4g2n: pre-flight size cap. Cheaper than read-then-discard,
    // and a malicious large file never gets read into the response heap.
    //
    // Codex 2026-06-05 P2: a FIFO / socket / device file in a custom forms
    // folder reports md.len() == 0 (it has no fixed size), so the cap check
    // passes and the subsequent std::fs::read blocks the async worker waiting
    // for EOF or reads arbitrary content past the 8 MiB cap. Reject any
    // non-regular file BEFORE the size check so the cap can't be bypassed
    // by file-type sleight-of-hand.
    match std::fs::metadata(&canonical) {
        Ok(md) if !md.is_file() => {
            return (StatusCode::FORBIDDEN, "not a regular file").into_response();
        }
        Ok(md) if md.len() > MAX_FOLDER_ASSET_BYTES as u64 => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "asset too large").into_response();
        }
        Ok(_) => {}
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    }
    // tuxlink-z0le §11.5: refuse to serve scriptable text types from /folder/*.
    // Imported (untrusted) forms can ship assets here; HTML/SVG are script +
    // exfil sinks, so adjacent assets are restricted to css/js/images. Combined
    // with the CSP+nosniff below, this closes the residual network-exfil channel
    // the empty forms-webview capability leaves open.
    let ext = canonical
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "html" | "htm" | "svg") {
        return (StatusCode::FORBIDDEN, "asset type not allowed").into_response();
    }
    match std::fs::read(&canonical) {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            let ct = match ext.as_str() {
                "css" => "text/css; charset=utf-8",
                "js" => "application/javascript",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                _ => "application/octet-stream",
            };
            headers.insert(header::CONTENT_TYPE, ct.parse().unwrap());
            // Lock origin to its own assets + stop content-type sniffing from
            // re-interpreting a served file as HTML/script.
            headers.insert(header::CONTENT_SECURITY_POLICY, FORM_CSP.parse().unwrap());
            headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
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
/// `tokio::sync::Mutex` (not `std::sync::Mutex`) is forward-defensive:
/// the current `open` and `close` methods don't hold the guard across an
/// await, but if a future change does, a `std::sync::Mutex` would block
/// the tokio runtime worker — `tokio::sync::Mutex` won't. The receiver
/// is `take()`n out of the session at open-time into a forwarder task;
/// the session itself stays in the map until `close` removes it.
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
    pub submit_rx: mpsc::Receiver<ParsedBody>,
}

/// Result of [`FormSessionRegistry::open_viewer`] (P1 Task 11). No submit
/// receiver: Viewer sessions are read-only — the http_server returns 404 on
/// POST `/`, so there is nothing for a forwarder task to drain.
pub struct OpenedViewerSession {
    pub token: String,
    pub port: u16,
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

    /// Open a Viewer-mode session for the receive-side fallback (P1 Task 11)
    /// and register it. Returns the token + bound port. There is no submit
    /// receiver: Viewer sessions 404 the POST route, so no forwarder task
    /// is needed.
    ///
    /// Caller is responsible for resolving the Viewer template path. Native
    /// forms can look up `FormDef::display_form` from `forms::catalog`; pure-
    /// catalog forms fall back to the `<form_id>_Viewer.html` convention
    /// (the same convention `send_webview_form` uses when minting the
    /// outbound XML's `display_form` field).
    pub async fn open_viewer(
        &self,
        viewer_path: PathBuf,
        folder: String,
        field_values: &std::collections::HashMap<String, String>,
    ) -> Result<OpenedViewerSession, String> {
        let session = FormSession::open_viewer(viewer_path, folder, field_values).await?;
        let port = session.port;
        let token = mint_session_token();
        let mut guard = self.sessions.lock().await;
        let mut tok = token;
        while guard.contains_key(&tok) {
            tok = mint_session_token();
        }
        guard.insert(tok.clone(), session);
        Ok(OpenedViewerSession { token: tok, port })
    }

    /// Open an editable, pre-bound Form session (tuxlink-hhfx / G10) and register
    /// it. Like [`open`](FormSessionRegistry::open), the POST route is live
    /// (SessionKind::Form), so the caller MUST spawn a forwarder task draining
    /// the returned `submit_rx` onto the `form-submitted` event scoped to the
    /// child webview's label — unlike a viewer, this session round-trips a
    /// submission.
    pub async fn open_form_prebound(
        &self,
        html_path: PathBuf,
        folder: String,
        field_values: &std::collections::HashMap<String, String>,
    ) -> Result<OpenedSession, String> {
        let mut session =
            FormSession::open_form_prebound(html_path, folder, field_values).await?;
        let port = session.port;
        let submit_rx = session.take_submit_rx().ok_or_else(|| {
            "FormSession::take_submit_rx returned None on a fresh prebound session".to_string()
        })?;
        let token = mint_session_token();
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

    fn make_state(html: &str) -> (Arc<SessionState>, mpsc::Receiver<ParsedBody>) {
        make_state_with_kind(html, SessionKind::Form)
    }

    /// Same as [`make_state`] but lets the caller pick the session kind. Used
    /// by Viewer-mode tests (P1 Task 11) to assert that POST / returns 404
    /// in Viewer mode without otherwise affecting the router shape.
    fn make_state_with_kind(
        html: &str,
        kind: SessionKind,
    ) -> (Arc<SessionState>, mpsc::Receiver<ParsedBody>) {
        let td = TempDir::new().unwrap();
        let folder = td.path().to_path_buf();
        // Leak the tempdir so the path stays valid for the test's lifetime.
        // (A test-helper struct that holds the TempDir would be cleaner, but
        // for the SessionState tests we don't actually read files.)
        std::mem::forget(td);
        let (tx, rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind,
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

    /// bd tuxlink-gheo regression: an asset inside a nested template folder
    /// (e.g. `General Forms/SubCategory/icon.png`) must be served correctly
    /// when the request hits `/folder/icon.png`. Previously the handler
    /// stripped a leading "folder segment" via splitn(2, '/'), which broke
    /// nested-folder templates: axum decodes %2F → / before the handler runs,
    /// so the splitn ate an extra path segment when the folder name itself
    /// contained a slash post-decode.
    #[tokio::test]
    async fn folder_route_serves_file_in_nested_folder() {
        let td = TempDir::new().unwrap();
        // Mimic a nested WLE folder like "Standard_Forms/General Forms/Sub".
        let nested = td.path().join("Standard_Forms").join("General Forms").join("Sub");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("icon.png"), b"\x89PNG\r\n\x1a\n_fake").unwrap();
        let template_folder = nested.clone();
        // Leak the TempDir so the folder survives for the test's await
        // calls (same pattern as make_state).
        std::mem::forget(td);
        let (tx, _rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html: String::new(),
            template_folder_path: template_folder,
            submit_tx: tx,
            port: TEST_PORT,
        });
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/folder/icon.png")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "nested-folder asset must resolve");
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(ct, "image/png");
    }

    /// tuxlink-z0le §11.5: /folder/* must refuse scriptable text types
    /// (HTML/HTM/SVG) — exfil + script sinks that imported untrusted forms
    /// could otherwise ship as adjacent "assets".
    #[tokio::test]
    async fn folder_route_refuses_html_htm_svg() {
        let td = TempDir::new().unwrap();
        let folder = td.path().to_path_buf();
        std::fs::write(folder.join("page.html"), b"<form>x</form>").unwrap();
        std::fs::write(folder.join("page.htm"), b"<form>x</form>").unwrap();
        std::fs::write(folder.join("icon.svg"), b"<svg/>").unwrap();
        std::mem::forget(td);
        let (tx, _rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html: String::new(),
            template_folder_path: folder,
            submit_tx: tx,
            port: TEST_PORT,
        });
        for uri in ["/folder/page.html", "/folder/page.htm", "/folder/icon.svg"] {
            let resp = build_router(state.clone())
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{uri} must be refused");
        }
    }

    /// tuxlink-z0le §11.5: served /folder/* assets carry the form CSP +
    /// X-Content-Type-Options: nosniff.
    #[tokio::test]
    async fn folder_route_sets_csp_and_nosniff_on_assets() {
        let td = TempDir::new().unwrap();
        let folder = td.path().to_path_buf();
        std::fs::write(folder.join("style.css"), b"body{}").unwrap();
        std::mem::forget(td);
        let (tx, _rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html: String::new(),
            template_folder_path: folder,
            submit_tx: tx,
            port: TEST_PORT,
        });
        let resp = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/folder/style.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers().get(header::CONTENT_SECURITY_POLICY).is_some(),
            "CSP present on served asset"
        );
        assert_eq!(
            resp.headers()
                .get("X-Content-Type-Options")
                .and_then(|v| v.to_str().ok()),
            Some("nosniff")
        );
    }

    /// bd tuxlink-4g2n regression: a file whose metadata.len() exceeds
    /// MAX_FOLDER_ASSET_BYTES must be rejected with 413 BEFORE std::fs::read
    /// pulls it into memory.
    #[tokio::test]
    async fn folder_route_rejects_asset_over_size_cap() {
        let td = TempDir::new().unwrap();
        let folder = td.path().to_path_buf();
        // Write a file larger than the 8 MiB cap (use 8 MiB + 1 KiB so the
        // pre-flight metadata check trips). Zero-filled is fine; the
        // handler is supposed to reject before reading content.
        let oversized_path = folder.join("oversized.bin");
        let oversized = vec![0u8; MAX_FOLDER_ASSET_BYTES + 1024];
        std::fs::write(&oversized_path, &oversized).unwrap();
        std::mem::forget(td);
        let (tx, _rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html: String::new(),
            template_folder_path: folder,
            submit_tx: tx,
            port: TEST_PORT,
        });
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/folder/oversized.bin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// Codex 2026-06-05 P2 (post-tuxlink-4g2n): a non-regular file (FIFO /
    /// socket / device) reports metadata().len() == 0 since it has no fixed
    /// size, so the size-cap check passes and the subsequent std::fs::read
    /// could block the async worker waiting for EOF (or read arbitrary
    /// content past the 8 MiB cap). Reject !is_file() before the size check.
    #[cfg(unix)]
    #[tokio::test]
    async fn folder_route_rejects_non_regular_file() {
        use std::os::unix::fs::FileTypeExt;
        let td = TempDir::new().unwrap();
        let folder = td.path().to_path_buf();
        let fifo_path = folder.join("pipe");
        // Create a FIFO via nix-style mkfifo (libc; in std via raw syscall).
        // The std lib doesn't expose mkfifo, so shell out to `mkfifo` —
        // available on every POSIX system the dev/CI matrix targets.
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo_path)
            .status()
            .expect("mkfifo command must exist");
        assert!(status.success(), "mkfifo must succeed");
        let md = std::fs::metadata(&fifo_path).unwrap();
        assert!(md.file_type().is_fifo(), "test setup must produce a FIFO");
        assert_eq!(md.len(), 0, "FIFO reports size 0 — the bypass surface");
        std::mem::forget(td);
        let (tx, _rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html: String::new(),
            template_folder_path: folder,
            submit_tx: tx,
            port: TEST_PORT,
        });
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/folder/pipe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // The handler must refuse the FIFO BEFORE any std::fs::read — if
        // the test hangs here, the guard is missing and read() is blocking
        // on the empty FIFO waiting for a writer.
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// bd tuxlink-4g2n: a file at exactly the cap (or below) must still
    /// succeed — verify the comparison is strict-greater-than.
    #[tokio::test]
    async fn folder_route_serves_asset_at_size_cap() {
        let td = TempDir::new().unwrap();
        let folder = td.path().to_path_buf();
        let path = folder.join("at-cap.bin");
        // 1 MiB — well under the 8 MiB cap. (Writing 8 MiB on every test run
        // is wasteful disk + slow on the Pi; the bounded behavior is the
        // same.)
        std::fs::write(&path, vec![0u8; 1_048_576]).unwrap();
        std::mem::forget(td);
        let (tx, _rx) = mpsc::channel(1);
        let state = Arc::new(SessionState {
            kind: SessionKind::Form,
            form_html: String::new(),
            template_folder_path: folder,
            submit_tx: tx,
            port: TEST_PORT,
        });
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/folder/at-cap.bin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// bd tuxlink-rk6s regression: when the submission channel is full
    /// (one in-flight, receiver hasn't drained), a second POST must return
    /// 503 instead of being silently queued onto an unbounded buffer.
    #[tokio::test]
    async fn post_root_second_submit_returns_503_when_channel_full() {
        let (state, _rx) = make_state("");
        let router = build_router(state.clone());
        let body = "Subject=Hi&Submit=Submit";
        let mk_req = || {
            Request::builder()
                .method("POST")
                .uri("/")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Origin", local_origin())
                .body(Body::from(body))
                .unwrap()
        };
        // First submit fills the bounded channel (capacity 1). Receiver
        // intentionally NOT drained — _rx is held in scope so the channel
        // stays open but full.
        let first = router.clone().oneshot(mk_req()).await.unwrap();
        assert_eq!(first.status(), StatusCode::OK, "first submit should succeed");
        // Second submit (channel full) must surface 503 — the prior
        // unbounded channel would silently queue this.
        let second = router.oneshot(mk_req()).await.unwrap();
        assert_eq!(
            second.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "second submit on full bounded channel must return 503"
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
    fn substitute_template_emits_bare_folder_root() {
        // bd tuxlink-gheo: {FormFolder} now expands to just `/folder` so
        // that the wildcard rest on the handler side IS the file path
        // relative to the per-session template folder. Previously this
        // emitted `/folder/<url-encoded-folder>` which broke nested-folder
        // templates (axum decoded %2F → / before the handler saw it, and
        // the splitn-based prefix-strip ate one segment).
        let raw = r#"<a href="{FormFolder}/foo.html">x</a>"#;
        let out = substitute_template(raw, 1, "ICS Forms");
        assert!(out.contains(r#"<a href="/folder/foo.html">x</a>"#));
        // Folder name is no longer in the URL — verify it's absent.
        assert!(!out.contains("ICS%20Forms"));
        assert!(!out.contains("ICS Forms"));
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

    // ============================================================
    // Viewer mode — P1 Task 11 receive-side fallback
    // ============================================================

    fn make_viewer_template_on_disk(td: &TempDir, name: &str, body: &str) -> PathBuf {
        let path = td.path().join(format!("{name}.html"));
        std::fs::write(&path, body).unwrap();
        path
    }

    #[tokio::test]
    async fn viewer_session_post_root_returns_404() {
        // The 404 is the canonical "read-only" signal — a Viewer session
        // doesn't accept resubmissions. Test via direct router construction
        // so we don't depend on the on-disk template path.
        let (state, _rx) =
            make_state_with_kind("<html><body>viewer</body></html>", SessionKind::Viewer);
        let router = build_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .header("Origin", local_origin())
                    .body(Body::from("Subject=AfterReceive"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn viewer_session_get_root_still_serves_html() {
        // Read-only mode still serves the HTML on GET — only POST is locked
        // down. This guards against a regression that, e.g., over-restricts
        // the router and refuses the legitimate fetch from the loaded
        // webview.
        let (state, _rx) = make_state_with_kind(
            "<html><body>viewer content here</body></html>",
            SessionKind::Viewer,
        );
        let router = build_router(state);
        let resp = router
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 64_000).await.unwrap();
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("viewer content here"));
    }

    #[test]
    fn substitute_template_blanks_seqnum_placeholder() {
        // tuxlink-2tom: the WLE `{SeqNum}` serial placeholder opens blank (the
        // serial is assigned at send), never rendered literally.
        let out = substitute_template(
            "<input value=\"{SeqNum}\" name=\"SeqNum\" id=\"Number\">",
            34567,
            "",
        );
        assert!(!out.contains("{SeqNum}"), "literal {{SeqNum}} must not survive: {out}");
        assert!(out.contains("value=\"\""), "the serial input opens blank: {out}");
    }

    #[test]
    fn substitute_var_placeholders_replaces_known_keys() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("Subjectline".to_string(), "Hello".to_string());
        fv.insert("Name".to_string(), "Net Control".to_string());
        let html = "<p>Subj: {var Subjectline}, From: {var Name}</p>";
        let out = substitute_var_placeholders(html, &fv);
        assert_eq!(out, "<p>Subj: Hello, From: Net Control</p>");
    }

    #[test]
    fn substitute_var_placeholders_is_case_insensitive() {
        // WLE viewers often use PascalCase placeholders (`{var Subjectline}`)
        // while inbound XML field IDs are typically lowercase
        // (`subjectline`). The lookup MUST tolerate either casing or the
        // viewer will render blank for every field on real-world payloads.
        let mut fv = std::collections::HashMap::new();
        fv.insert("subjectline".to_string(), "Hello".to_string());
        let out = substitute_var_placeholders("Subj: {var Subjectline}", &fv);
        assert_eq!(out, "Subj: Hello");
    }

    #[test]
    fn substitute_var_placeholders_empties_unknown_keys() {
        // Missing keys substitute to "" (matches WLE's behavior). Without
        // this, raw `{var Foo}` text would survive into the rendered page.
        let fv = std::collections::HashMap::new();
        let out = substitute_var_placeholders("<p>{var Missing}</p>", &fv);
        assert_eq!(out, "<p></p>");
    }

    #[test]
    fn substitute_var_placeholders_escapes_html() {
        // Defense-in-depth against payload XSS: HTML-escape the substituted
        // value. Even though CSP + capability scope contain the blast
        // radius, escaping costs nothing and prevents in-viewer HTML
        // injection from confusing the operator (e.g. a payload with a
        // fake "Submit" button).
        let mut fv = std::collections::HashMap::new();
        fv.insert(
            "Name".to_string(),
            r#"<script>alert("x")</script>"#.to_string(),
        );
        let out = substitute_var_placeholders("<p>{var Name}</p>", &fv);
        assert!(out.contains("&lt;script&gt;"));
        assert!(!out.contains("<script>"));
    }

    /// 2026-06-04 Codex adrev P2.2: `{var X}` placeholders inside
    /// `<script>` blocks are NOT substituted (they collapse to an empty
    /// string). HTML-escaping the substituted value would leave raw
    /// newlines / unescaped quotes that corrupt the JS string literal.
    /// The DOM-injection path (`inject_field_value_script`) re-binds
    /// the same values via `querySelectorAll('[name="X"]').value = ...`
    /// at DOMContentLoaded so hidden-input round-trips still work.
    #[test]
    fn substitute_var_placeholders_skips_inside_script_blocks() {
        let mut fv = std::collections::HashMap::new();
        // A multi-line field value — naively HTML-escaped, it would
        // leave raw newlines inside the JS string literal and break the
        // surrounding `<script>` block.
        fv.insert("comments".to_string(), "line one\nline two".to_string());

        let html = r#"<html><body>
<script type="text/javascript">
function DoCheck() {
s = "{var Comments}";
}
</script>
<p>Inline: {var Comments}</p>
</body></html>"#;

        let out = substitute_var_placeholders(html, &fv);

        // The `s = "{var Comments}"` inside the script block must
        // collapse to `s = ""` — NOT `s = "line one\nline two"` (raw
        // newline) which would be invalid JS.
        assert!(
            out.contains(r#"s = """#),
            "script-context substitution must yield empty string, got: {out}"
        );
        // The script literal must NOT contain the raw value.
        let script_start = out.find("<script").unwrap();
        let script_end = out.find("</script>").unwrap();
        let script_block = &out[script_start..script_end];
        assert!(
            !script_block.contains("line one"),
            "raw field value leaked into script block: {script_block}"
        );

        // The inline `<p>{var Comments}</p>` (outside the script) is
        // still substituted (HTML-escaped, newlines preserved verbatim).
        assert!(
            out.contains("line one\nline two") || out.contains("line one") && out.contains("line two"),
            "inline substitution outside script must still happen, got: {out}"
        );
    }

    /// Substitutions OUTSIDE script blocks are unchanged by the
    /// script-context skip. Re-checks the existing behavior survives
    /// the P2.2 fix.
    #[test]
    fn substitute_var_placeholders_outside_script_unchanged() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("subjectline".to_string(), "Hello".to_string());
        let html = r#"<html><head><title>{var Subjectline}</title></head>
<body>
<p>Subject: {var Subjectline}</p>
<script>// trailing script with no var placeholder</script>
<p>Footer: {var Subjectline}</p>
</body></html>"#;
        let out = substitute_var_placeholders(html, &fv);
        // All three NON-script occurrences substituted to "Hello".
        // (Two in the body + one in the head before the script.)
        let occurrences = out.matches("Hello").count();
        assert_eq!(occurrences, 3, "expected 3 substitutions outside script, got: {out}");
    }

    /// Multiple `<script>` blocks in the same document are each tracked
    /// correctly — placeholders between them (in body text) still get
    /// substituted.
    #[test]
    fn substitute_var_placeholders_multiple_script_blocks() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("name".to_string(), "Alice".to_string());

        let html = r#"<html><body>
<script>var x = "{var Name}";</script>
<p>Hello, {var Name}!</p>
<script>var y = "{var Name}";</script>
<p>Goodbye, {var Name}.</p>
</body></html>"#;

        let out = substitute_var_placeholders(html, &fv);

        // Script-block placeholders → empty:
        assert!(out.contains(r#"var x = """#), "first script block not skipped: {out}");
        assert!(out.contains(r#"var y = """#), "second script block not skipped: {out}");
        // Body-text placeholders → "Alice":
        assert!(out.contains("Hello, Alice!"), "first inline substitution missing: {out}");
        assert!(out.contains("Goodbye, Alice."), "second inline substitution missing: {out}");
    }

    /// `<script>` tags with attributes (e.g. `<script type="text/javascript">`,
    /// `<script src="x.js">`) are still detected.
    #[test]
    fn substitute_var_placeholders_script_with_attributes() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("comments".to_string(), "raw\nnewline".to_string());

        let html = r#"<script type="text/javascript" defer>
s = "{var Comments}";
</script>"#;

        let out = substitute_var_placeholders(html, &fv);
        assert!(
            out.contains(r#"s = """#),
            "attributed script block not skipped: {out}"
        );
        assert!(!out.contains("raw\nnewline"), "raw value leaked into script: {out}");
    }

    #[test]
    fn inject_field_value_script_emits_dom_content_loaded_listener() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("subjectline".to_string(), "Test".to_string());
        let html = "<html><body></body></html>";
        let out = inject_field_value_script(html, &fv);
        assert!(
            out.contains("DOMContentLoaded"),
            "field-value injector must wait for DOMContentLoaded"
        );
        assert!(out.contains(r#"querySelectorAll('[name="' + name + '"]')"#));
    }

    #[test]
    fn inject_field_value_script_serializes_field_pairs() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("subjectline".to_string(), "Hello".to_string());
        fv.insert("inc_name".to_string(), "Waldo".to_string());
        let out = inject_field_value_script("<html><body></body></html>", &fv);
        assert!(out.contains(r#"["subjectline", "Hello"]"#));
        assert!(out.contains(r#"["inc_name", "Waldo"]"#));
    }

    #[test]
    fn inject_field_value_script_inserts_before_body_close() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("a".to_string(), "1".to_string());
        let out = inject_field_value_script("<html><body><p>x</p></body></html>", &fv);
        let script_pos = out.find("<script>").expect("script injected");
        let body_close = out.find("</body>").expect("body close present");
        assert!(
            script_pos < body_close,
            "script must appear before </body>; got script@{script_pos} body@{body_close}"
        );
    }

    #[test]
    fn inject_field_value_script_appends_when_no_body_close() {
        let mut fv = std::collections::HashMap::new();
        fv.insert("a".to_string(), "1".to_string());
        let out = inject_field_value_script("<html><p>x</p></html>", &fv);
        assert!(out.contains("<script>"), "script must still be present");
    }

    #[test]
    fn inject_field_value_script_escapes_quotes_and_close_script() {
        // The classic XSS via `</script>` in a JS string literal: the
        // payload terminates the host script tag and starts a new one.
        // js_escape encodes < as < so this can't happen.
        let mut fv = std::collections::HashMap::new();
        fv.insert(
            "a".to_string(),
            r#"</script><script>alert(1)</script>"#.to_string(),
        );
        let out = inject_field_value_script("<html><body></body></html>", &fv);
        // Confirm raw `</script>` from the payload doesn't appear (it would
        // mean we leaked the injected payload as live HTML markup).
        // Note: the host `<script>` tag closes with `</script>` itself, so
        // we count "</script>" occurrences and assert there's exactly one
        // (the host tag's own closing).
        let close_count = out.matches("</script>").count();
        assert_eq!(
            close_count, 1,
            "exactly one </script> expected (the host tag's close); found {close_count}"
        );
        // And the escaped form should be present.
        assert!(out.contains("\\u003C"));
    }

    #[test]
    fn js_escape_escapes_unicode_line_terminators() {
        // tuxlink-2590 (receiving-end appsec audit): U+2028 (LINE SEPARATOR) and
        // U+2029 (PARAGRAPH SEPARATOR) terminate a JS string literal on
        // pre-ES2019 engines. They must be escaped so a hostile field value can
        // never split the binder string literal there. (Not exploitable on
        // WebKitGTK's ES2019+ JavaScriptCore — defense-in-depth.)
        let out = js_escape("a\u{2028}b\u{2029}c");
        assert_eq!(out, "a\\u2028b\\u2029c");
        assert!(!out.contains('\u{2028}'), "raw U+2028 must not survive");
        assert!(!out.contains('\u{2029}'), "raw U+2029 must not survive");
    }

    #[test]
    fn inject_field_value_script_escapes_unicode_line_terminators() {
        // End-to-end: a received field value carrying U+2028 lands escaped in
        // the injected binder script, never as a raw terminator (tuxlink-2590).
        let mut fv = std::collections::HashMap::new();
        fv.insert("a".to_string(), "x\u{2028}y".to_string());
        let out = inject_field_value_script("<html><body></body></html>", &fv);
        assert!(
            out.contains("\\u2028"),
            "U+2028 in a field value must be escaped in the injected script"
        );
        assert!(
            !out.contains('\u{2028}'),
            "raw U+2028 must not appear in the injected script"
        );
    }

    #[test]
    fn inject_field_value_script_skips_unsafe_field_names() {
        // CSS-selector-unsafe characters in a field name are a payload
        // signal, not a WLE field ID. Skip those entries entirely rather
        // than risk constructing a malformed querySelectorAll.
        let mut fv = std::collections::HashMap::new();
        fv.insert(r#"a"]&[name="b"#.to_string(), "evil".to_string());
        fv.insert("legitimate".to_string(), "ok".to_string());
        let out = inject_field_value_script("<html><body></body></html>", &fv);
        assert!(out.contains(r#"["legitimate", "ok"]"#));
        assert!(!out.contains(r#"a"]"#));
    }

    #[tokio::test]
    async fn full_open_viewer_session_serves_viewer_html_via_real_tcp() {
        // Mirror full_open_session_serves_form_html_via_real_tcp for Viewer
        // mode: end-to-end bind + serve a viewer template, assert the
        // served body carries the substituted `{var X}` placeholders + the
        // injected field-value script, and that POST returns 404.
        let td = TempDir::new().unwrap();
        let viewer_path = make_viewer_template_on_disk(
            &td,
            "Test_Viewer",
            "<html><head></head><body>Subject: {var Subjectline}</body></html>",
        );
        let mut fv = std::collections::HashMap::new();
        fv.insert("subjectline".to_string(), "Hello World".to_string());

        let mut session = FormSession::open_viewer(
            viewer_path,
            String::new(), // no folder substitution needed in this template
            &fv,
        )
        .await
        .unwrap();
        let url = session.url();

        // GET / serves the viewer HTML with both substitution paths applied.
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(
            body.contains("Subject: Hello World"),
            "{{var Subjectline}} must be substituted; got body: {body}"
        );
        assert!(
            body.contains("DOMContentLoaded"),
            "JS injection script must be present"
        );

        // POST / returns 404 in Viewer mode.
        let client = reqwest::Client::new();
        let post_resp = client
            .post(&url)
            .header("Origin", &url[..url.len() - 1]) // strip trailing /
            .body("Subject=Retry")
            .send()
            .await
            .unwrap();
        assert_eq!(post_resp.status(), 404);

        // submit_rx is always None for viewer sessions.
        assert!(
            session.take_submit_rx().is_none(),
            "viewer sessions never expose a submit_rx"
        );
        session.close();
    }

    #[tokio::test]
    async fn registry_open_viewer_returns_token_and_port() {
        let td = TempDir::new().unwrap();
        let viewer_path = make_viewer_template_on_disk(
            &td,
            "RegViewer",
            "<html><body>viewer</body></html>",
        );
        let registry = FormSessionRegistry::new();
        let fv = std::collections::HashMap::new();
        let opened = registry
            .open_viewer(viewer_path, String::new(), &fv)
            .await
            .unwrap();
        assert_eq!(opened.token.len(), 16);
        assert!(opened.token.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(opened.port != 0);
        assert_eq!(registry.session_count().await, 1);
    }

    #[tokio::test]
    async fn registry_open_viewer_then_close_works() {
        let td = TempDir::new().unwrap();
        let viewer_path = make_viewer_template_on_disk(
            &td,
            "ClosingViewer",
            "<html><body>viewer</body></html>",
        );
        let registry = FormSessionRegistry::new();
        let fv = std::collections::HashMap::new();
        let opened = registry
            .open_viewer(viewer_path, String::new(), &fv)
            .await
            .unwrap();
        let url = format!("http://127.0.0.1:{}/", opened.port);
        assert_eq!(registry.session_count().await, 1);

        registry.close(&opened.token).await.unwrap();
        assert_eq!(registry.session_count().await, 0);

        // Listener is down.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap();
        let after = client.get(&url).send().await;
        assert!(after.is_err(), "listener must be down after close()");
    }

    // ---- open_form_prebound (G10 reply threading) ---------------------

    #[tokio::test]
    async fn full_open_form_prebound_serves_bound_html_and_accepts_post() {
        // The union of Form (POST live) + Viewer (values pre-bound): a SendReply
        // authoring page served with the original field values filled in, whose
        // submit still round-trips back through the submit channel.
        let td = TempDir::new().unwrap();
        let html_path = make_viewer_template_on_disk(
            &td,
            "ICS213_SendReply",
            "<html><head></head><body>Orig subject: {var Subjectline}\
             <input name=\"MsgOriginalBody\" type=\"hidden\" /></body></html>",
        );
        let mut fv = std::collections::HashMap::new();
        fv.insert("subjectline".to_string(), "Road status".to_string());
        fv.insert("MsgOriginalBody".to_string(), "original body text".to_string());

        let mut session =
            FormSession::open_form_prebound(html_path, String::new(), &fv)
                .await
                .unwrap();
        let url = session.url();

        // GET / serves the bound HTML: {var Subjectline} substituted + the
        // DOM-injection script present (to fill the hidden MsgOriginalBody input).
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(
            body.contains("Orig subject: Road status"),
            "{{var}} must be substituted in a prebound form; got: {body}"
        );
        assert!(
            body.contains("DOMContentLoaded"),
            "field-value injection script must be present so hidden inputs round-trip"
        );

        // POST / is LIVE (unlike a viewer): it dispatches onto the submit channel.
        let client = reqwest::Client::new();
        let post = client
            .post(&url)
            .header("Origin", &url[..url.len() - 1])
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("Reply=Roger&MsgOriginalBody=original+body+text&Submit=Submit")
            .send()
            .await
            .unwrap();
        assert_eq!(post.status(), 200, "prebound form must accept POST (Form kind)");

        let rx = session.take_submit_rx();
        assert!(rx.is_some(), "prebound form exposes a live submit_rx");
        let parsed = rx.unwrap().recv().await.unwrap();
        assert_eq!(parsed.fields["Reply"][0], "Roger");
        assert_eq!(parsed.fields["MsgOriginalBody"][0], "original body text");
        session.close();
    }

    #[tokio::test]
    async fn registry_open_form_prebound_returns_token_port_and_receiver() {
        let td = TempDir::new().unwrap();
        let html_path = make_viewer_template_on_disk(
            &td,
            "RegPrebound",
            "<html><body>{var X}</body></html>",
        );
        let registry = FormSessionRegistry::new();
        let mut fv = std::collections::HashMap::new();
        fv.insert("x".to_string(), "bound".to_string());
        let opened = registry
            .open_form_prebound(html_path, String::new(), &fv)
            .await
            .unwrap();
        assert_eq!(opened.token.len(), 16);
        assert!(opened.port != 0);
        assert_eq!(registry.session_count().await, 1);
        // The receiver is handed to the caller (forwarder task), not retained.
        drop(opened.submit_rx);
        registry.close(&opened.token).await.unwrap();
        assert_eq!(registry.session_count().await, 0);
    }
}
