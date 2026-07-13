//! `local.compose` / `local.compose_catalog_request` / `local.set_identity`
//! / `local.log` / `local.notify` — spec §6 "Local actions" (plan Task 4d).
//! None of these actions declare any capability flag (`needs_radio: false`,
//! `transmits: false`, `needs_internet: false` on all five) — every one is
//! either a local write (outbox stage, session log, desktop notification) or
//! pure in-memory logic (`local.set_identity`). Every impl here delegates
//! through the narrow [`super::LocalService`] port declared in
//! `actions/mod.rs`; NONE of this file re-implements B2F message
//! composition, the session-log ring buffer, or desktop-notification
//! plumbing — those live behind the real seams [`MonolithLocalService`]
//! below wraps.
//!
//! ## Recon: the real seams (plan Task 4d)
//!
//! - **The B2F composer + outbox path** — `crate::winlink::compose::compose_message_with_files`
//!   builds a `Message` (headers + body + attachments) from plain fields;
//!   `crate::winlink_backend::WinlinkBackend::send_message` is the ONE real
//!   queueing verb every existing "stage a message" surface already uses:
//!   `ui_commands::message_send` (the Compose window) and
//!   `catalog::commands::catalog_send_inquiry` (the Catalog Request menu,
//!   the exact KM4ACK use case `local.compose_catalog_request` mirrors) both
//!   build a `winlink_backend::OutboundMessage { to, cc, subject, body,
//!   date, attachments }` and hand it to `backend.send_message(msg)`. This
//!   file's [`LocalService::compose_stage`] wraps that SAME call — no new
//!   compose/stage logic, just the same two-line "build `OutboundMessage`,
//!   call `send_message`" every existing caller already does.
//!
//! - **`from_identity`'s real seam gap, and how it's closed.** Spec §6's
//!   "Set identity" row promises run-scoped identity: `local.compose` can
//!   author a message under a tactical call for THIS run only, without ever
//!   touching the app's shared identity state. But `send_message` itself
//!   ALWAYS derives `from` from `NativeBackend::active_identity()` (the
//!   process-wide, session-shared slot `set_active_identity`/
//!   `clear_active_identity` mutate on login/logout) — falling back to
//!   `live_config().identity.active_full` — and `OutboundMessage` carries no
//!   per-call override field at all. Mutating the shared `active_identity`
//!   slot around a single compose call would be exactly the race spec §6
//!   explicitly calls out run-scoping to prevent ("parallel runs with
//!   different tactical calls" safe). There was genuinely no seam for a
//!   per-call `from` override before this task. **Closed in
//!   `winlink_backend.rs`, this revision:** a new `WinlinkBackend` trait
//!   method, `send_message_as(msg, from: Option<String>)`, with a
//!   backward-compatible DEFAULT implementation (`from: None` delegates to
//!   `send_message`, matching this trait's existing "unimplemented
//!   override, `NativeBackend` supplies the real behavior" convention —
//!   e.g. `abort`/`restore_message`'s own no-op/`NotImplemented` defaults).
//!   `NativeBackend` overrides it for real: `from: Some(callsign)` composes
//!   + queues under that exact callsign via the SAME `compose_message_with_files`
//!   + `Outbox` store call `send_message` itself makes (refactored into a
//!     shared private `queue_message` helper), entirely bypassing
//!     `active_identity()`/config resolution — the override never reads or
//!     writes the shared slot. `WinlinkBackend` has exactly one production
//!     implementor (`NativeBackend`), so this addition is non-breaking.
//!
//! - **The catalog-request wire format** — `crate::catalog::composer`
//!   (verified empirically against a real N7CPZ WLE outbox, per that
//!   module's own doc comment): `To: INQUIRY@winlink.org`, `Subject:
//!   REQUEST`, body = one filename per line
//!   (`build_inquiry_body(&filenames)`, newline-joined). This is the exact
//!   KM4ACK "request the station/mode listing" flow — a routine step
//!   staging this message is spec §6's "Compose catalog request" row
//!   verbatim: *"Sending is whatever Connect attempt comes next; the
//!   response arrives on a later connection"* — this action stages ONLY;
//!   it never itself dials.
//!
//! - **Template rendering delegates to the REAL forms renderer, which uses
//!   `<var field_id>` tokens, NOT `{field}` curly braces.** The plan's
//!   illustrative wording ("substitute `{placeholders}` from vars") is
//!   prose shorthand, not a wire-format spec — the actual recon instruction
//!   was "match however the forms system renders body_template," and the
//!   ONE real template-body renderer in the codebase is
//!   `crate::forms::serialize::render_body_template(template: &str,
//!   field_values: &HashMap<String, String>) -> String`, which scans for
//!   `<var X>` spans and substitutes `field_values[X.trim().to_lowercase()]`
//!   (empty string when a var is unset — never an error, never the token's
//!   own literal text; it also strips XML-1.0-illegal control chars from
//!   the substituted value). This is not a coincidence: plan Task 3's
//!   `MonolithEntityResolver` already resolved `@template:<name>` to
//!   `{id, name, subjectTemplate, bodyTemplate}` sourced from
//!   `forms::catalog::find_form` (the bundled Standard Forms catalog —
//!   ICS-213, ICS-309, Bulletin, …; see `routines/resolver.rs`'s own doc
//!   comment for why that, not a dead "Templates" menu item, is the real
//!   `@template:` seam). A `bodyTemplate` resolved that way is LITERALLY a
//!   `FormDef.body_template` string, and `render_body_template` is the one
//!   function anywhere in the codebase that knows how to render it — so
//!   delegating to it is the "if the forms system has a renderer, delegate"
//!   branch of the plan's instruction, not the frontend-only fallback
//!   branch. **This action does NOT build the form's XML attachment or use
//!   `send_form`'s full HTML-Forms pipeline** — spec §6 frames "Compose
//!   message" as a plain templated text message ("Template + routine
//!   variables (ICS-213/309, wx tabular)"), distinct from
//!   `ui_commands::send_form`'s XML-attachment flow, which is a different,
//!   unrelated Tauri command this plan does not touch.
//!
//! - **Station/session log** — `crate::session_log::SessionLogState::append_operator_line`
//!   (`Arc<SessionLogState>` managed state) is the exact append call
//!   `mcp_ports.rs`'s `MonolithLogPort`/`EgressGate::audit_abort` already
//!   use for a non-UI-originated log line. `local.log` writes at
//!   `LogLevel::Info`/`LogSource::Backend` — the same level/source
//!   `audit_abort` uses for its own non-interactive forensic line.
//!
//! - **Desktop notification** — Tauri v2 core has NO built-in notification
//!   API (moved to a plugin in v2); nothing in the existing dependency tree
//!   provided one. This revision adds `tauri-plugin-notification = "2"`
//!   (`Cargo.toml`/`Cargo.lock` regenerated via `cargo add`, matching every
//!   other `tauri-plugin-*` dependency already in the tree) and registers
//!   it in `lib.rs`'s plugin chain alongside `tauri_plugin_dialog`/
//!   `tauri_plugin_shell`. [`MonolithLocalService::notify`] below uses the
//!   plugin's `NotificationExt::notification().builder().title(..).body(..).show()`
//!   — a synchronous, non-blocking-I/O call (a D-Bus round-trip under
//!   Linux), called directly rather than via `spawn_blocking`, matching
//!   `resolver.rs`'s own documented "at routine-authoring scale, a
//!   `spawn_blocking` wrapper would add complexity for no measurable
//!   benefit" reasoning for similarly-cheap synchronous calls.
//!
//! - **`local.set_identity` takes NO seam at all — not even a read one.**
//!   Spec §6: *"Switch to a tactical call for subsequent steps. Run-scoped:
//!   affects later steps in this run only; never mutates the app's global
//!   identity."* The mechanism spec §6 hands this action is the step OUTPUT
//!   itself (`{"identity": <object>}`), consumed by a later step's params
//!   (`$stepid.identity`, e.g. `local.compose`'s `from_identity`) via the
//!   engine's own variable-resolution machinery (`tuxlink_routines::vars::RunVars`)
//!   — NOT a config write this action makes. [`SetIdentity`] is therefore a
//!   pure validate-and-echo: it holds no `Arc<dyn ...>` field whatsoever
//!   (there is structurally nothing it COULD write through, config or
//!   otherwise — see this struct's own doc comment and this file's test
//!   `set_identity_holds_no_seam_it_could_write_a_global_through`).
//!
//! Plan: `docs/superpowers/plans/2026-07-13-routines-02-actions-arbiter-mount.md`
//! Task 4. Spec: `docs/superpowers/specs/2026-07-13-routines-design.md` §6.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::{Action, ActionDescriptor};
use tuxlink_routines::error::StepError;

use crate::winlink_backend::OutboundMessage;

use super::LocalService;

// ============================================================================
// local.compose
// ============================================================================

const LOCAL_COMPOSE: &str = "local.compose";

/// The resolved `@template:` object shape (`MonolithEntityResolver`'s
/// `"template"` arm, `routines/resolver.rs`): `{id, name, subjectTemplate,
/// bodyTemplate}`. Only the two rendered fields are declared here — serde
/// ignores unrecognized JSON keys by default, matching cat.rs's
/// `PresetParam`'s "declare only what this file actually uses" precedent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateParam {
    body_template: String,
    subject_template: String,
}

/// `local.compose`'s optional `from_identity` param — "object with
/// callsign" per plan Task 4d's instruction. Deliberately narrow: whatever
/// richer shape `@identity:`/`local.set_identity`'s resolved identity object
/// actually carries (`label`, `has_cms_account`, `cms`/`parent`, …), this
/// action only ever reads `callsign` — the one field
/// `WinlinkBackend::send_message_as` needs. Extra keys are ignored, so the
/// FULL resolved `@identity:`/`local.set_identity` output object can be
/// passed here directly without a routine author having to hand-pick just
/// the callsign field out of it first.
#[derive(Debug, Clone, Deserialize)]
struct FromIdentityParam {
    callsign: String,
}

#[derive(Debug, Deserialize)]
struct ComposeParams {
    to: Vec<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    template: Option<TemplateParam>,
    #[serde(default)]
    body: Option<String>,
    /// Substitution values for `template`'s `<var …>` tokens. Only consumed
    /// on the template path — a plain `body` is staged verbatim and `vars`
    /// is ignored (no substitution is attempted on raw bodies).
    #[serde(default)]
    vars: Option<Map<String, Value>>,
    #[serde(default)]
    from_identity: Option<FromIdentityParam>,
}

/// Converts a JSON `vars` object into the `HashMap<String, String>`
/// [`crate::forms::serialize::render_body_template`] consumes, lower-casing
/// every key — that renderer itself lower-cases the `<var X>` token it
/// extracts from the template before looking the field up (see its own doc
/// comment), so a routine author's `vars` object matches regardless of how
/// they capitalized a key. Non-string values are stringified: numbers/bools
/// render via `Value`'s own `Display` (`serde_json::Value` implements
/// `Display` as compact JSON text, which for a bare number/bool is
/// indistinguishable from the plain value), `null` becomes an empty string
/// (matching how an UNSET var already renders — see the renderer's own doc
/// comment), and a nested array/object (a routine-author error — vars are
/// meant to be flat) serializes to its own compact JSON text rather than
/// silently vanishing or panicking.
fn vars_to_field_values(vars: &Option<Map<String, Value>>) -> HashMap<String, String> {
    let Some(vars) = vars else {
        return HashMap::new();
    };
    vars.iter()
        .map(|(k, v)| {
            let s = match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            (k.to_ascii_lowercase(), s)
        })
        .collect()
}

/// `local.compose` — stage a B2F message via the real composer + outbox
/// path (spec §6 "Compose message"). `template` XOR `body` is required
/// (exactly one, never both, never neither) — this module's doc comment
/// covers the real template renderer `template` delegates through.
/// `subject` is independently optional in EITHER shape: with `template`,
/// an absent `subject` renders `template.subjectTemplate` the same way the
/// body renders; with `body`, an absent `subject` is an empty string (the
/// real `compose_message`/`OutboundMessage` pipeline has no non-empty-
/// subject requirement — `ui_commands::message_send`'s own `OutboundDraftDto.subject`
/// threads through unchecked). No capability flags — `needs_radio: false`,
/// `transmits: false` (queueing to the Outbox is not transmitting — see
/// `send_message`'s own doc comment), `needs_internet: false`.
pub struct ComposeMessage {
    local: Arc<dyn LocalService>,
}

impl ComposeMessage {
    pub fn new(local: Arc<dyn LocalService>) -> Self {
        Self { local }
    }
}

#[async_trait]
impl Action for ComposeMessage {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: LOCAL_COMPOSE,
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: ComposeParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: LOCAL_COMPOSE.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        if parsed.to.is_empty() {
            return Err(StepError::Action {
                action: LOCAL_COMPOSE.to_string(),
                cause: "to must have at least one recipient".to_string(),
            });
        }

        let (subject, body) = match (parsed.template, parsed.body) {
            (Some(_), Some(_)) => {
                return Err(StepError::Action {
                    action: LOCAL_COMPOSE.to_string(),
                    cause: "template and body are mutually exclusive — supply exactly one"
                        .to_string(),
                });
            }
            (None, None) => {
                return Err(StepError::Action {
                    action: LOCAL_COMPOSE.to_string(),
                    cause: "exactly one of template or body is required".to_string(),
                });
            }
            (Some(template), None) => {
                let field_values = vars_to_field_values(&parsed.vars);
                let rendered_body = crate::forms::serialize::render_body_template(
                    &template.body_template,
                    &field_values,
                );
                let subject = parsed.subject.unwrap_or_else(|| {
                    crate::forms::serialize::render_body_template(
                        &template.subject_template,
                        &field_values,
                    )
                });
                (subject, rendered_body)
            }
            (None, Some(body)) => (parsed.subject.unwrap_or_default(), body),
        };

        let from = parsed.from_identity.map(|f| f.callsign);

        let msg = OutboundMessage {
            to: parsed.to,
            cc: Vec::new(),
            subject,
            body,
            date: chrono::Utc::now().to_rfc3339(),
            attachments: Vec::new(),
        };

        let mid = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.local.compose_stage(msg, from) => res,
        }
        .map_err(|cause| StepError::Action {
            action: LOCAL_COMPOSE.to_string(),
            cause,
        })?;

        Ok(json!({ "staged": true, "mid": mid }))
    }
}

// ============================================================================
// local.compose_catalog_request
// ============================================================================

const LOCAL_COMPOSE_CATALOG_REQUEST: &str = "local.compose_catalog_request";

#[derive(Debug, Deserialize)]
struct CatalogRequestParams {
    /// Zero or more inquiry filenames directly (e.g. `["PUB_PACKET",
    /// "PUB_VARA"]` — the KM4ACK "station/mode listing" request).
    #[serde(default)]
    filenames: Vec<String>,
    /// Singular sugar for a one-item `catalog_item` selection (a bundled
    /// `catalog::parser::CatalogEntry.filename`).
    #[serde(default)]
    catalog_item: Option<String>,
    /// Singular sugar for an ad-hoc inquiry keyword not in the bundled
    /// catalog. `catalog_item` and `query` collapse to the SAME underlying
    /// shape once resolved: `catalog::composer::build_inquiry_body` never
    /// validates a filename against the bundled catalog (see this module's
    /// doc comment) — a WL2K inquiry request is just "one or more filename
    /// strings," and the real seam does not distinguish where a caller's
    /// filename string came from.
    #[serde(default)]
    query: Option<String>,
}

impl CatalogRequestParams {
    /// Flattens `filenames`/`catalog_item`/`query` into the single ordered
    /// list `build_inquiry_body` wants — `filenames` first (as supplied),
    /// then `catalog_item`, then `query`, so a routine author combining all
    /// three gets a deterministic, documented order rather than
    /// HashMap-style nondeterminism.
    fn resolved_filenames(self) -> Vec<String> {
        let mut out = self.filenames;
        if let Some(item) = self.catalog_item {
            out.push(item);
        }
        if let Some(q) = self.query {
            out.push(q);
        }
        out
    }
}

/// `local.compose_catalog_request` — the KM4ACK use case: stage a WL2K
/// catalog/inquiry request (spec §6 "Compose catalog request"). Stages
/// ONLY; the response arrives on a later connection (modeled by a
/// subsequent `radio.connect` step — this action never dials). No
/// capability flags.
pub struct ComposeCatalogRequest {
    local: Arc<dyn LocalService>,
}

impl ComposeCatalogRequest {
    pub fn new(local: Arc<dyn LocalService>) -> Self {
        Self { local }
    }
}

#[async_trait]
impl Action for ComposeCatalogRequest {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: LOCAL_COMPOSE_CATALOG_REQUEST,
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: CatalogRequestParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: LOCAL_COMPOSE_CATALOG_REQUEST.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        let filenames = parsed.resolved_filenames();
        let filename_refs: Vec<&str> = filenames.iter().map(String::as_str).collect();
        // `build_inquiry_body` itself rejects an empty list / an embedded
        // newline / a whitespace-only filename — its own error variants are
        // already operator-facing text (Global Constraints: verbatim, never
        // paraphrased), so this is passed straight through, not re-validated
        // here first.
        let body = crate::catalog::composer::build_inquiry_body(&filename_refs).map_err(|e| {
            StepError::Action {
                action: LOCAL_COMPOSE_CATALOG_REQUEST.to_string(),
                cause: e.to_string(),
            }
        })?;

        let msg = OutboundMessage {
            to: vec![crate::catalog::composer::INQUIRY_RECIPIENT.to_string()],
            cc: Vec::new(),
            subject: crate::catalog::composer::INQUIRY_SUBJECT.to_string(),
            body,
            date: chrono::Utc::now().to_rfc3339(),
            attachments: Vec::new(),
        };

        // No `from_identity` override for the catalog-request path — spec §6
        // doesn't call one out for this row, and the real UI's
        // `catalog_send_inquiry` command never took one either; the app's
        // current identity applies, same as that existing surface.
        let mid = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.local.compose_stage(msg, None) => res,
        }
        .map_err(|cause| StepError::Action {
            action: LOCAL_COMPOSE_CATALOG_REQUEST.to_string(),
            cause,
        })?;

        Ok(json!({ "staged": true, "mid": mid }))
    }
}

// ============================================================================
// local.set_identity
// ============================================================================

const LOCAL_SET_IDENTITY: &str = "local.set_identity";

#[derive(Debug, Deserialize)]
struct SetIdentityParams {
    identity: Value,
}

/// `local.set_identity` — spec §6 "Set identity": run-scoped only. Holds NO
/// fields — see this module's doc comment for why this action structurally
/// cannot write anywhere (there is no `Arc<dyn ...>` seam field for it to
/// hold, config-write or otherwise). `execute` validates `params.identity`
/// is an object carrying a non-empty `callsign` string (the one field
/// `local.compose`'s `from_identity` reads back out — see
/// [`FromIdentityParam`]) and echoes it verbatim as the step's OUTPUT
/// (`{"identity": <the same object>}`) — the mechanism spec §6 hands later
/// steps for consuming it (`$stepid.identity` via the engine's
/// `RunVars`/params substitution, wired by Task 5). No capability flags.
#[derive(Default)]
pub struct SetIdentity;

impl SetIdentity {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Action for SetIdentity {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: LOCAL_SET_IDENTITY,
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        if cancel.is_cancelled() {
            return Err(StepError::Cancelled);
        }

        let parsed: SetIdentityParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: LOCAL_SET_IDENTITY.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        let callsign = parsed
            .identity
            .as_object()
            .and_then(|obj| obj.get("callsign"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if callsign.is_none() {
            return Err(StepError::Action {
                action: LOCAL_SET_IDENTITY.to_string(),
                cause: "identity must be an object with a non-empty \"callsign\" string"
                    .to_string(),
            });
        }

        Ok(json!({ "identity": parsed.identity }))
    }
}

// ============================================================================
// local.log
// ============================================================================

const LOCAL_LOG: &str = "local.log";

#[derive(Debug, Deserialize)]
struct LogParams {
    message: String,
}

/// `local.log` — write a line to the real station/session log (spec §6
/// "Log entry / Notify"). No capability flags. Output `{}`.
pub struct LogEntry {
    local: Arc<dyn LocalService>,
}

impl LogEntry {
    pub fn new(local: Arc<dyn LocalService>) -> Self {
        Self { local }
    }
}

#[async_trait]
impl Action for LogEntry {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: LOCAL_LOG,
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: LogParams = serde_json::from_value(params).map_err(|e| StepError::Action {
            action: LOCAL_LOG.to_string(),
            cause: format!("invalid params: {e}"),
        })?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.local.log_append(parsed.message) => res,
        }
        .map_err(|cause| StepError::Action {
            action: LOCAL_LOG.to_string(),
            cause,
        })?;

        Ok(json!({}))
    }
}

// ============================================================================
// local.notify
// ============================================================================

const LOCAL_NOTIFY: &str = "local.notify";

#[derive(Debug, Deserialize)]
struct NotifyParams {
    #[serde(default)]
    title: Option<String>,
    message: String,
}

/// `local.notify` — a Tauri desktop notification (spec §6 "Log entry /
/// Notify"). No capability flags. Output `{}`.
pub struct Notify {
    local: Arc<dyn LocalService>,
}

impl Notify {
    pub fn new(local: Arc<dyn LocalService>) -> Self {
        Self { local }
    }
}

#[async_trait]
impl Action for Notify {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: LOCAL_NOTIFY,
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: NotifyParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: LOCAL_NOTIFY.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.local.notify(parsed.title, parsed.message) => res,
        }
        .map_err(|cause| StepError::Action {
            action: LOCAL_NOTIFY.to_string(),
            cause,
        })?;

        Ok(json!({}))
    }
}

// ============================================================================
// Real seam adapter — MonolithLocalService. Follows the `mcp_ports.rs`
// egress-port pattern: holds an `AppHandle`, resolves `.state::<T>()` fresh
// at call time — the same pattern every other Monolith*Service adapter in
// this module family uses.
// ============================================================================

/// Real [`LocalService`]. `compose_stage` delegates to
/// `WinlinkBackend::send_message_as` (this module's doc comment covers the
/// new trait method + its `from_identity` rationale). `log_append` delegates
/// to `SessionLogState::append_operator_line`. `notify` delegates to
/// `tauri_plugin_notification`'s `NotificationExt`.
pub struct MonolithLocalService {
    app: AppHandle,
}

impl MonolithLocalService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl LocalService for MonolithLocalService {
    async fn compose_stage(
        &self,
        msg: OutboundMessage,
        from: Option<String>,
    ) -> Result<String, String> {
        let backend = self
            .app
            .state::<crate::app_backend::BackendState>()
            .current()
            .ok_or_else(|| "backend offline".to_string())?;
        let mid = backend
            .send_message_as(msg, from)
            .await
            .map_err(|e| e.to_string())?;
        Ok(mid.0)
    }

    async fn log_append(&self, message: String) -> Result<(), String> {
        use crate::winlink_backend::{LogLevel, LogSource};
        let log = self.app.state::<Arc<crate::session_log::SessionLogState>>();
        log.append_operator_line(LogLevel::Info, LogSource::Backend, message);
        Ok(())
    }

    async fn notify(&self, title: Option<String>, message: String) -> Result<(), String> {
        use tauri_plugin_notification::NotificationExt;
        let mut builder = self.app.notification().builder().body(message);
        if let Some(title) = title {
            builder = builder.title(title);
        }
        builder.show().map_err(|e| e.to_string())
    }
}

// ============================================================================
// Tests — trait fakes, no hardware/tauri. Per plan Task 4's test contract:
// seam fakes, template XOR body validation, placeholder substitution,
// catalog request message shape, set_identity emits the resolved object and
// touches no config seam, verbatim errors.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ---- FakeLocalService ---------------------------------------------------
    // Builder-style: every method panics by default ("not expected in this
    // test") unless overridden, matching data.rs's `FakeDataService`
    // precedent — a test exercising the wrong seam method fails loudly.

    type ComposeFn =
        dyn Fn(OutboundMessage, Option<String>) -> Result<String, String> + Send + Sync;
    type LogFn = dyn Fn(String) -> Result<(), String> + Send + Sync;
    type NotifyFn = dyn Fn(Option<String>, String) -> Result<(), String> + Send + Sync;

    struct FakeLocalService {
        compose: Box<ComposeFn>,
        log: Box<LogFn>,
        notify: Box<NotifyFn>,
    }

    impl Default for FakeLocalService {
        fn default() -> Self {
            Self {
                compose: Box::new(|_, _| panic!("compose_stage not expected in this test")),
                log: Box::new(|_| panic!("log_append not expected in this test")),
                notify: Box::new(|_, _| panic!("notify not expected in this test")),
            }
        }
    }

    impl FakeLocalService {
        fn with_compose(
            mut self,
            f: impl Fn(OutboundMessage, Option<String>) -> Result<String, String>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            self.compose = Box::new(f);
            self
        }
        fn with_log(
            mut self,
            f: impl Fn(String) -> Result<(), String> + Send + Sync + 'static,
        ) -> Self {
            self.log = Box::new(f);
            self
        }
        fn with_notify(
            mut self,
            f: impl Fn(Option<String>, String) -> Result<(), String> + Send + Sync + 'static,
        ) -> Self {
            self.notify = Box::new(f);
            self
        }
    }

    #[async_trait]
    impl LocalService for FakeLocalService {
        async fn compose_stage(
            &self,
            msg: OutboundMessage,
            from: Option<String>,
        ) -> Result<String, String> {
            (self.compose)(msg, from)
        }
        async fn log_append(&self, message: String) -> Result<(), String> {
            (self.log)(message)
        }
        async fn notify(&self, title: Option<String>, message: String) -> Result<(), String> {
            (self.notify)(title, message)
        }
    }

    // ======================================================================
    // local.compose
    // ======================================================================

    #[tokio::test]
    async fn compose_body_path_happy_output_shape() {
        let local = FakeLocalService::default().with_compose(|_msg, _from| Ok("m1".to_string()));
        let action = ComposeMessage::new(Arc::new(local));
        let out = action
            .execute(
                json!({"to": ["W7DEF-10"], "subject": "Sitrep", "body": "all quiet"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["staged"], json!(true));
        assert_eq!(out["mid"], json!("m1"));
    }

    #[tokio::test]
    async fn compose_empty_to_is_a_step_error() {
        let action = ComposeMessage::new(Arc::new(FakeLocalService::default()));
        let err = action
            .execute(json!({"to": [], "body": "x"}), CancellationToken::new())
            .await
            .expect_err("empty to must error");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[tokio::test]
    async fn compose_template_and_body_together_is_mutually_exclusive_error() {
        let action = ComposeMessage::new(Arc::new(FakeLocalService::default()));
        let err = action
            .execute(
                json!({
                    "to": ["W7DEF-10"],
                    "body": "x",
                    "template": {"bodyTemplate": "b", "subjectTemplate": "s"}
                }),
                CancellationToken::new(),
            )
            .await
            .expect_err("template + body together must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.compose");
                assert!(cause.contains("mutually exclusive"));
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn compose_neither_template_nor_body_is_an_error() {
        let action = ComposeMessage::new(Arc::new(FakeLocalService::default()));
        let err = action
            .execute(json!({"to": ["W7DEF-10"]}), CancellationToken::new())
            .await
            .expect_err("neither template nor body must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.compose");
                assert!(cause.contains("exactly one of template or body"));
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn compose_template_placeholder_substitution_renders_body_and_default_subject() {
        let observed_msg: Arc<Mutex<Option<OutboundMessage>>> = Arc::new(Mutex::new(None));
        let om = observed_msg.clone();
        let local = FakeLocalService::default().with_compose(move |msg, _from| {
            *om.lock().unwrap() = Some(msg);
            Ok("m1".to_string())
        });
        let action = ComposeMessage::new(Arc::new(local));
        action
            .execute(
                json!({
                    "to": ["W7DEF-10"],
                    "template": {
                        "id": "ICS213_Initial",
                        "name": "ICS-213 General Message",
                        "subjectTemplate": "ICS-213: <var subjectline>",
                        "bodyTemplate": "To: <var inc_name>\nMsg: <var message>"
                    },
                    "vars": {"subjectline": "Road closure", "inc_name": "Fire Camp 3", "message": "Route 9 blocked"}
                }),
                CancellationToken::new(),
            )
            .await
            .expect("happy path must succeed");
        let msg = observed_msg.lock().unwrap().clone().unwrap();
        assert_eq!(msg.subject, "ICS-213: Road closure");
        assert_eq!(msg.body, "To: Fire Camp 3\nMsg: Route 9 blocked");
    }

    #[tokio::test]
    async fn compose_template_explicit_subject_overrides_subject_template() {
        let observed_msg: Arc<Mutex<Option<OutboundMessage>>> = Arc::new(Mutex::new(None));
        let om = observed_msg.clone();
        let local = FakeLocalService::default().with_compose(move |msg, _from| {
            *om.lock().unwrap() = Some(msg);
            Ok("m1".to_string())
        });
        let action = ComposeMessage::new(Arc::new(local));
        action
            .execute(
                json!({
                    "to": ["W7DEF-10"],
                    "subject": "Custom subject",
                    "template": {"subjectTemplate": "Ignored: <var x>", "bodyTemplate": "<var x>"},
                    "vars": {"x": "hi"}
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            observed_msg.lock().unwrap().clone().unwrap().subject,
            "Custom subject"
        );
    }

    #[tokio::test]
    async fn compose_template_unset_var_renders_empty_not_the_token_text() {
        let observed_msg: Arc<Mutex<Option<OutboundMessage>>> = Arc::new(Mutex::new(None));
        let om = observed_msg.clone();
        let local = FakeLocalService::default().with_compose(move |msg, _from| {
            *om.lock().unwrap() = Some(msg);
            Ok("m1".to_string())
        });
        let action = ComposeMessage::new(Arc::new(local));
        action
            .execute(
                json!({
                    "to": ["W7DEF-10"],
                    "template": {"subjectTemplate": "s", "bodyTemplate": "before[<var missing>]after"}
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            observed_msg.lock().unwrap().clone().unwrap().body,
            "before[]after",
            "an unset var renders empty, never its own literal name"
        );
    }

    #[tokio::test]
    async fn compose_body_path_absent_subject_defaults_to_empty_string() {
        let observed_msg: Arc<Mutex<Option<OutboundMessage>>> = Arc::new(Mutex::new(None));
        let om = observed_msg.clone();
        let local = FakeLocalService::default().with_compose(move |msg, _from| {
            *om.lock().unwrap() = Some(msg);
            Ok("m1".to_string())
        });
        let action = ComposeMessage::new(Arc::new(local));
        action
            .execute(
                json!({"to": ["W7DEF-10"], "body": "x"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(observed_msg.lock().unwrap().clone().unwrap().subject, "");
    }

    #[tokio::test]
    async fn compose_from_identity_absent_passes_none_the_apps_current_identity_applies() {
        let observed_from: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
        let of = observed_from.clone();
        let local = FakeLocalService::default().with_compose(move |_msg, from| {
            *of.lock().unwrap() = Some(from);
            Ok("m1".to_string())
        });
        let action = ComposeMessage::new(Arc::new(local));
        action
            .execute(
                json!({"to": ["W7DEF-10"], "body": "x"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(*observed_from.lock().unwrap(), Some(None));
    }

    #[tokio::test]
    async fn compose_from_identity_present_threads_callsign_through() {
        let observed_from: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
        let of = observed_from.clone();
        let local = FakeLocalService::default().with_compose(move |_msg, from| {
            *of.lock().unwrap() = Some(from);
            Ok("m1".to_string())
        });
        let action = ComposeMessage::new(Arc::new(local));
        action
            .execute(
                json!({
                    "to": ["W7DEF-10"],
                    "body": "x",
                    "from_identity": {"callsign": "EOC-3-TAC", "label": "ignored"}
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            *observed_from.lock().unwrap(),
            Some(Some("EOC-3-TAC".to_string())),
            "from_identity.callsign must thread through verbatim; extra fields (label) ignored"
        );
    }

    #[tokio::test]
    async fn compose_verbatim_error_passthrough() {
        let local =
            FakeLocalService::default().with_compose(|_, _| Err("backend offline".to_string()));
        let action = ComposeMessage::new(Arc::new(local));
        let err = action
            .execute(
                json!({"to": ["W7DEF-10"], "body": "x"}),
                CancellationToken::new(),
            )
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.compose");
                assert_eq!(cause, "backend offline");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn compose_observes_cancellation_promptly() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let action = ComposeMessage::new(Arc::new(FakeLocalService::default()));
        let err = action
            .execute(json!({"to": ["W7DEF-10"], "body": "x"}), cancel)
            .await
            .expect_err("a pre-cancelled token must not stage");
        assert!(matches!(err, StepError::Cancelled));
    }

    #[test]
    fn compose_descriptor_has_no_capabilities() {
        let action = ComposeMessage::new(Arc::new(FakeLocalService::default()));
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }

    // ======================================================================
    // vars_to_field_values (pure)
    // ======================================================================

    #[test]
    fn vars_to_field_values_lowercases_keys_and_stringifies_values() {
        let vars = Some(
            json!({"Subjectline": "Road closure", "Count": 3, "Ok": true, "Note": null})
                .as_object()
                .unwrap()
                .clone(),
        );
        let got = vars_to_field_values(&vars);
        assert_eq!(got.get("subjectline"), Some(&"Road closure".to_string()));
        assert_eq!(got.get("count"), Some(&"3".to_string()));
        assert_eq!(got.get("ok"), Some(&"true".to_string()));
        assert_eq!(got.get("note"), Some(&String::new()));
    }

    #[test]
    fn vars_to_field_values_none_is_empty_map() {
        assert!(vars_to_field_values(&None).is_empty());
    }

    // ======================================================================
    // local.compose_catalog_request
    // ======================================================================

    #[tokio::test]
    async fn catalog_request_single_filename_message_shape() {
        let observed_msg: Arc<Mutex<Option<OutboundMessage>>> = Arc::new(Mutex::new(None));
        let om = observed_msg.clone();
        let local = FakeLocalService::default().with_compose(move |msg, from| {
            *om.lock().unwrap() = Some(msg);
            assert_eq!(from, None, "catalog request never overrides from_identity");
            Ok("m1".to_string())
        });
        let action = ComposeCatalogRequest::new(Arc::new(local));
        let out = action
            .execute(
                json!({"filenames": ["PUB_PACKET"]}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["staged"], json!(true));
        assert_eq!(out["mid"], json!("m1"));
        let msg = observed_msg.lock().unwrap().clone().unwrap();
        assert_eq!(msg.to, vec!["INQUIRY@winlink.org".to_string()]);
        assert_eq!(msg.subject, "REQUEST");
        assert_eq!(msg.body, "PUB_PACKET");
    }

    #[tokio::test]
    async fn catalog_request_multi_filename_body_is_newline_joined() {
        let observed_msg: Arc<Mutex<Option<OutboundMessage>>> = Arc::new(Mutex::new(None));
        let om = observed_msg.clone();
        let local = FakeLocalService::default().with_compose(move |msg, _from| {
            *om.lock().unwrap() = Some(msg);
            Ok("m1".to_string())
        });
        let action = ComposeCatalogRequest::new(Arc::new(local));
        action
            .execute(
                json!({"filenames": ["PUB_PACKET"], "catalog_item": "PUB_VARA", "query": "CMS_TRAFFIC"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let msg = observed_msg.lock().unwrap().clone().unwrap();
        assert_eq!(msg.body, "PUB_PACKET\nPUB_VARA\nCMS_TRAFFIC");
    }

    #[tokio::test]
    async fn catalog_request_no_filenames_at_all_is_a_verbatim_error() {
        let action = ComposeCatalogRequest::new(Arc::new(FakeLocalService::default()));
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("empty filenames must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.compose_catalog_request");
                assert_eq!(cause, "no filenames selected");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn catalog_request_verbatim_error_passthrough_from_compose_stage() {
        let local =
            FakeLocalService::default().with_compose(|_, _| Err("backend offline".to_string()));
        let action = ComposeCatalogRequest::new(Arc::new(local));
        let err = action
            .execute(
                json!({"filenames": ["PUB_PACKET"]}),
                CancellationToken::new(),
            )
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.compose_catalog_request");
                assert_eq!(cause, "backend offline");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[test]
    fn catalog_request_descriptor_has_no_capabilities() {
        let action = ComposeCatalogRequest::new(Arc::new(FakeLocalService::default()));
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }

    // ======================================================================
    // local.set_identity
    // ======================================================================

    #[tokio::test]
    async fn set_identity_emits_the_resolved_object_verbatim() {
        let action = SetIdentity::new();
        let identity = json!({"callsign": "EOC-3-TAC", "label": "EOC-3", "cms": "Unknown"});
        let out = action
            .execute(
                json!({"identity": identity.clone()}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out, json!({"identity": identity}));
    }

    #[tokio::test]
    async fn set_identity_missing_callsign_is_a_step_error() {
        let action = SetIdentity::new();
        let err = action
            .execute(
                json!({"identity": {"label": "EOC-3"}}),
                CancellationToken::new(),
            )
            .await
            .expect_err("identity without callsign must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.set_identity");
                assert!(cause.contains("callsign"));
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_identity_blank_callsign_is_a_step_error() {
        let action = SetIdentity::new();
        let err = action
            .execute(
                json!({"identity": {"callsign": "   "}}),
                CancellationToken::new(),
            )
            .await
            .expect_err("whitespace-only callsign must error");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[tokio::test]
    async fn set_identity_non_object_identity_is_a_step_error() {
        let action = SetIdentity::new();
        let err = action
            .execute(json!({"identity": "W1ABC"}), CancellationToken::new())
            .await
            .expect_err("a bare string identity must error — object required");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[tokio::test]
    async fn set_identity_observes_cancellation_promptly() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let action = SetIdentity::new();
        let err = action
            .execute(json!({"identity": {"callsign": "W1ABC"}}), cancel)
            .await
            .expect_err("a pre-cancelled token must not even validate");
        assert!(matches!(err, StepError::Cancelled));
    }

    /// Per plan Task 4's explicit test-contract wording: "assert the fake
    /// config seam is never called — or better, no config seam exists to
    /// call." [`SetIdentity`] is a unit struct with NO fields — there is
    /// structurally no `Arc<dyn ...>` config-write (or any other) seam for
    /// it to hold, so "never calls a config seam" is a compile-time
    /// invariant, not a runtime assertion. This test exercises the action
    /// end-to-end and documents that guarantee at the call site.
    #[tokio::test]
    async fn set_identity_holds_no_seam_it_could_write_a_global_through() {
        let action = SetIdentity;
        let out = action
            .execute(
                json!({"identity": {"callsign": "W1ABC"}}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["identity"]["callsign"], json!("W1ABC"));
    }

    #[test]
    fn set_identity_descriptor_has_no_capabilities() {
        let action = SetIdentity::new();
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }

    // ======================================================================
    // local.log
    // ======================================================================

    #[tokio::test]
    async fn log_happy_path_threads_message_and_outputs_empty_object() {
        let observed: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let o = observed.clone();
        let local = FakeLocalService::default().with_log(move |msg| {
            *o.lock().unwrap() = Some(msg);
            Ok(())
        });
        let action = LogEntry::new(Arc::new(local));
        let out = action
            .execute(
                json!({"message": "Net check-in at 1800Z"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out, json!({}));
        assert_eq!(
            observed.lock().unwrap().clone(),
            Some("Net check-in at 1800Z".to_string())
        );
    }

    #[tokio::test]
    async fn log_verbatim_error_passthrough() {
        let local =
            FakeLocalService::default().with_log(|_| Err("session log unavailable".to_string()));
        let action = LogEntry::new(Arc::new(local));
        let err = action
            .execute(json!({"message": "x"}), CancellationToken::new())
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.log");
                assert_eq!(cause, "session log unavailable");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn log_invalid_params_is_a_step_error() {
        let action = LogEntry::new(Arc::new(FakeLocalService::default()));
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("missing message must error");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[test]
    fn log_descriptor_has_no_capabilities() {
        let action = LogEntry::new(Arc::new(FakeLocalService::default()));
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }

    // ======================================================================
    // local.notify
    // ======================================================================

    #[tokio::test]
    async fn notify_happy_path_threads_title_and_message() {
        #[allow(clippy::type_complexity)] // observed-tuple capture in a test
        let observed: Arc<Mutex<Option<(Option<String>, String)>>> = Arc::new(Mutex::new(None));
        let o = observed.clone();
        let local = FakeLocalService::default().with_notify(move |title, message| {
            *o.lock().unwrap() = Some((title, message));
            Ok(())
        });
        let action = Notify::new(Arc::new(local));
        let out = action
            .execute(
                json!({"title": "Routine done", "message": "WWV capture complete"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out, json!({}));
        assert_eq!(
            observed.lock().unwrap().clone(),
            Some((
                Some("Routine done".to_string()),
                "WWV capture complete".to_string()
            ))
        );
    }

    #[tokio::test]
    async fn notify_absent_title_passes_none() {
        let observed: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
        let o = observed.clone();
        let local = FakeLocalService::default().with_notify(move |title, _message| {
            *o.lock().unwrap() = Some(title);
            Ok(())
        });
        let action = Notify::new(Arc::new(local));
        action
            .execute(json!({"message": "x"}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(*observed.lock().unwrap(), Some(None));
    }

    #[tokio::test]
    async fn notify_verbatim_error_passthrough() {
        let local = FakeLocalService::default()
            .with_notify(|_, _| Err("notification backend unavailable".to_string()));
        let action = Notify::new(Arc::new(local));
        let err = action
            .execute(json!({"message": "x"}), CancellationToken::new())
            .await
            .expect_err("must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "local.notify");
                assert_eq!(cause, "notification backend unavailable");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn notify_invalid_params_is_a_step_error() {
        let action = Notify::new(Arc::new(FakeLocalService::default()));
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("missing message must error");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[test]
    fn notify_descriptor_has_no_capabilities() {
        let action = Notify::new(Arc::new(FakeLocalService::default()));
        let d = action.descriptor();
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }
}
