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

use tuxlink_routines::action::{Action, ActionDescriptor, OutputSpec, ParamSpec, ValueType};
use tuxlink_routines::error::StepError;

use crate::winlink_backend::OutboundMessage;

use super::{LocalService, StationContext};

// ============================================================================
// local.compose
// ============================================================================

pub(crate) const LOCAL_COMPOSE: &str = "local.compose";

/// The resolved `@template:` object shape (`MonolithEntityResolver`'s
/// `"template"` arm, `routines/resolver.rs`): `{id, name, subjectTemplate,
/// bodyTemplate}`. Only the two rendered fields are declared here — serde
/// ignores unrecognized JSON keys by default, matching cat.rs's
/// `PresetParam`'s "declare only what this file actually uses" precedent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateParam {
    /// The bundled form's canonical id, when this reference came from
    /// `@template:` (the resolver emits it; see `routines/resolver.rs`).
    ///
    /// Its PRESENCE is what distinguishes a real form send from a plain
    /// templated message, and it used to be dropped on the floor here — serde
    /// ignores unrecognized keys, so a resolved `@template:Winlink_Check-In`
    /// arrived carrying its id and this struct threw it away. The consequence
    /// was tuxlink-3ddk2's silent interop failure: the body rendered as
    /// "Winlink Check-in / 0. HEADER / 0a: Organization: ..." and staged with
    /// NO form XML, so a human reading it sees a check-in and Winlink Express
    /// does not. Nothing on our side is malformed, so nothing we own catches
    /// it; the failure lands on the recipient's machine, over RF.
    ///
    /// `None` is a hand-written inline template (`{"bodyTemplate": "...",
    /// "subjectTemplate": "..."}`), which is a legitimate plain-text message
    /// and keeps the original behaviour.
    #[serde(default)]
    id: Option<String>,
    /// Only consulted on the inline path. When `id` names a bundled form the
    /// real templates come from the bundle, so these default to empty and a
    /// step may simply write `{"id": "Winlink_Check-In"}` rather than
    /// reproducing a template it does not own. A resolved `@template:`
    /// reference supplies all four keys and is unaffected.
    #[serde(default)]
    body_template: String,
    #[serde(default)]
    subject_template: String,
    /// The saved answers a `@draft:<slot_id>` reference brings with it.
    ///
    /// The draft resolves to this WHOLE object — the form and its answers
    /// together — so a step writes `"template": "@draft:<slot_id>"` and the
    /// two cannot get out of step. Naming the form separately would let a
    /// routine reference form A and fill it with answers saved against form B.
    ///
    /// The step's own `vars` override these, so a routine can reuse a saved
    /// check-in and change one line for a particular net.
    #[serde(default)]
    values: Map<String, Value>,
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

/// The field ids a scheduled run fills for itself, and what fills them.
///
/// ## Why anything is filled at all
///
/// A saved form draft is PARTIAL BY DESIGN. `CheckInForm.tsx` saves the
/// "which net is this" metadata — organization, recipient, contact name,
/// status, service, band, session — and deliberately excludes the timestamp,
/// the sender callsign, and every position field. Those are the ones that must
/// not be stale, so the compose window recomputes them at open time and the
/// draft never stores them. A routine has no compose window, so if nothing
/// filled them a scheduled check-in would go out with a blank Date/Time and a
/// blank From — a defective form, sent every morning, silently.
///
/// ## Where the values come from, and why this is not a new policy
///
/// Every one of them is an existing setting read at run time:
///
/// - The timestamp is the moment the run fires. There is no other defensible
///   answer for a scheduled message, and the XML envelope's
///   `submission_datetime` is already derived exactly this way.
/// - The sender callsign is the step's `from_identity` when it set one, else
///   the station's configured callsign. Same precedence the compose window
///   uses.
/// - The locator is [`StationContext::on_air_grid`], which came from
///   `position::effective_broadcast_locator`. That function applies the
///   operator's position-precision setting AND `gps_state`, so a routine
///   transmits exactly the position they have already said may be
///   transmitted — including none, when they have said none. Routing through
///   the standing control is not a decision about what routines assert; it is
///   the operator's own decision, honoured from a context with nobody at the
///   keyboard.
///
/// ## Two rules that keep this honest
///
/// A value supplied in the step's own `vars` ALWAYS wins — an explicit
/// authored value is never overwritten by a derived one. And a field is only
/// filled when the form actually declares it, so an ICS-213 does not sprout a
/// `locationsource` and a Position Report does not sprout a `datetime`.
///
/// `lat`/`lon` on the Position Report are the centre of the broadcast grid
/// square, matching the square-centre convention `resolve_operator_broadcast_grid`
/// already documents for distance ranking. A 4-character square centre is a
/// coarse position, which is the point: it is as precise as the operator
/// allows.
fn form_field_values(
    form: &crate::forms::types::FormDef,
    saved: &Map<String, Value>,
    vars: &Option<Map<String, Value>>,
    station: &StationContext,
    from_identity: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> HashMap<String, String> {
    // Three layers, most specific first: the step's own `vars`, then whatever
    // a `@draft:` reference carried, then the run-time fills below. A routine
    // can therefore reuse a saved check-in and change one line for a
    // particular net without editing the draft everyone else uses.
    let mut values = vars_to_field_values(&Some(saved.clone()));
    values.extend(vars_to_field_values(vars));

    let declares = |id: &str| form.fields.iter().any(|f| f.id == id);
    let mut fill = |id: &str, value: String| {
        if declares(id) && !value.is_empty() {
            values.entry(id.to_string()).or_insert(value);
        }
    };

    // WLE's Check-In DateTime is operator-facing UTC in this exact shape; the
    // Position Report's `thetime` is the same idea under a different name.
    let stamp = now.format("%Y-%m-%d %H:%M").to_string();
    fill("datetime", stamp.clone());
    fill("thetime", stamp);

    let callsign = from_identity
        .map(str::to_string)
        .or_else(|| station.callsign.clone())
        .unwrap_or_default();
    fill("msgsender", callsign);

    fill("grid", station.on_air_grid.clone());
    if let Some(src) = &station.location_source {
        fill("locationsource", src.clone());
    }
    if !station.on_air_grid.is_empty() {
        if let Some((lat, lon)) = crate::position::grid_to_lat_lon(&station.on_air_grid) {
            fill("lat", format!("{lat:.4}"));
            fill("lon", format!("{lon:.4}"));
        }
    }

    values
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
            writes_config: false,
            name: LOCAL_COMPOSE,
            label: "Compose message",
            description: "Stage a Winlink message in the outbox. Staged messages are SENT \
                          by the next radio.connect that starts AFTER this step - place \
                          compose BEFORE the radio.connect that should carry it; a compose \
                          after the connect (or inside its success branch) waits for a \
                          future connection.",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
            example_params: Some(r#"{"to":["W1AW"],"subject":"Status report","body":"All quiet."}"#),
            allowed_values: None,
            params: &[
                ParamSpec {
                    key: "to",
                    ty: ValueType::StationList,
                    required: true,
                    description: "Recipient callsigns/addresses",
                    allowed: None,
                    example: r#"["W1AW"]"#,
                },
                ParamSpec {
                    key: "subject",
                    ty: ValueType::String,
                    required: false,
                    description: "Message subject; with template, rendered from it when absent",
                    allowed: None,
                    example: r#""Status report""#,
                },
                ParamSpec {
                    key: "body",
                    ty: ValueType::String,
                    required: false,
                    description: "Verbatim message body — exactly one of body or template",
                    allowed: None,
                    example: r#""All quiet.""#,
                },
                ParamSpec {
                    key: "template",
                    ty: ValueType::Object,
                    required: false,
                    description: "Winlink form reference — exactly one of body or template. \
                                  An @template:<form-id> reference stages a REAL form: the \
                                  message carries the form XML a Winlink Express recipient \
                                  needs to render it, and the run fills the timestamp, sender \
                                  and position for you. An inline {bodyTemplate, \
                                  subjectTemplate} object with no id stages a plain templated \
                                  text message instead.",
                    allowed: None,
                    example: r#"{"id":"Winlink_Check-In"}"#,
                },
                ParamSpec {
                    key: "vars",
                    ty: ValueType::Object,
                    required: false,
                    description: "Substitution values for the template's <var> tokens",
                    allowed: None,
                    example: r#"{"status":"green"}"#,
                },
                ParamSpec {
                    key: "from_identity",
                    ty: ValueType::Object,
                    required: false,
                    description: "Sending identity override; defaults to the active identity",
                    allowed: None,
                    example: r#"{"callsign":"N0CALL-1"}"#,
                },
            ],
            outputs: &[
                OutputSpec {
                    key: "staged",
                    ty: ValueType::Boolean,
                    description: "Whether the message was staged to the outbox",
                    nullable: false,
                },
                OutputSpec {
                    key: "mid",
                    ty: ValueType::String,
                    description: "Winlink message id of the staged message",
                    nullable: false,
                },
            ],
            dry_run_shape: None,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: ComposeParams =
            serde_json::from_value(params)
                .map_err(|e| StepError::invalid(LOCAL_COMPOSE, format!("invalid params: {e}")))?;

        if parsed.to.is_empty() {
            return Err(StepError::invalid(
                LOCAL_COMPOSE,
                "to must have at least one recipient",
                ));
        }

        let from = parsed.from_identity.map(|f| f.callsign);

        let msg = match (parsed.template, parsed.body) {
            (Some(_), Some(_)) => {
                return Err(StepError::invalid(
                    LOCAL_COMPOSE,
                    "template and body are mutually exclusive — supply exactly one",
                    ));
            }
            (None, None) => {
                return Err(StepError::invalid(
                    LOCAL_COMPOSE,
                    "exactly one of template or body is required",
                    ));
            }
            // A reference to a REAL bundled form. Send an actual form: XML
            // attachment, field structure, the lot. See `TemplateParam::id`.
            (
                Some(TemplateParam {
                    id: Some(id),
                    values: saved,
                    ..
                }),
                None,
            ) => {
                let form = crate::forms::catalog::find_form(&id).ok_or_else(|| {
                    // Loud, never a fallback to prose. A broken form reference
                    // that quietly degrades to a text message is precisely the
                    // failure this whole path exists to prevent.
                    StepError::invalid(
                        LOCAL_COMPOSE,
                        format!(
                            "unknown form: {id}. A template reference must name a bundled form; \
                             refusing rather than sending an unstructured message that reads \
                             like one."
                        ),
                    )
                })?;
                let station = self.local.station_context().await;
                let now = chrono::Utc::now();
                let field_values =
                    form_field_values(form, &saved, &parsed.vars, &station, from.as_deref(), now);
                let senders_callsign = from
                    .clone()
                    .or_else(|| station.callsign.clone())
                    .unwrap_or_default();
                crate::forms::outbound::build_native_form_message(
                    form,
                    crate::forms::outbound::FormSend {
                        field_values: &field_values,
                        to: parsed.to,
                        cc: Vec::new(),
                        senders_callsign,
                        subject_override: parsed.subject,
                        now,
                    },
                    station.on_air_grid.clone(),
                )
            }
            // Neither a form id nor any template text. Name both valid shapes
            // rather than letting serde report a missing field, so one
            // rejection carries the whole contract.
            (Some(t), None) if t.body_template.is_empty() && t.subject_template.is_empty() => {
                return Err(StepError::invalid(
                    LOCAL_COMPOSE,
                    "template needs either an id naming a bundled form, e.g. \
                     {\"id\": \"Winlink_Check-In\"} (stages a real form with its XML), \
                     or inline text, e.g. {\"bodyTemplate\": \"...\", \
                     \"subjectTemplate\": \"...\"} (stages a plain message). \
                     It carried neither.",
                ));
            }
            // A hand-written inline template: a plain templated text message,
            // which is a legitimate thing to want and unchanged in behaviour.
            (Some(template), None) => {
                let field_values = vars_to_field_values(&parsed.vars);
                let body = crate::forms::serialize::render_body_template(
                    &template.body_template,
                    &field_values,
                );
                let subject = parsed.subject.unwrap_or_else(|| {
                    crate::forms::serialize::render_body_template(
                        &template.subject_template,
                        &field_values,
                    )
                });
                OutboundMessage {
                    to: parsed.to,
                    cc: Vec::new(),
                    subject,
                    body,
                    date: chrono::Utc::now().to_rfc3339(),
                    attachments: Vec::new(),
                }
            }
            (None, Some(body)) => OutboundMessage {
                to: parsed.to,
                cc: Vec::new(),
                subject: parsed.subject.unwrap_or_default(),
                body,
                date: chrono::Utc::now().to_rfc3339(),
                attachments: Vec::new(),
            },
        };

        let mid = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.local.compose_stage(msg, from) => res,
        }
        .map_err(|cause| StepError::service(LOCAL_COMPOSE, cause))?;

        Ok(json!({ "staged": true, "mid": mid }))
    }
}

// ============================================================================
// local.compose_catalog_request
// ============================================================================

pub(crate) const LOCAL_COMPOSE_CATALOG_REQUEST: &str = "local.compose_catalog_request";

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
            writes_config: false,
            name: LOCAL_COMPOSE_CATALOG_REQUEST,
            label: "Compose catalog request",
            description: "Stage a WL2K catalog request message in the outbox. Like \
                          local.compose, it must run BEFORE the radio.connect that should \
                          carry it; the catalog response arrives on a LATER connection \
                          (typically a second radio.connect after a delay).",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
            example_params: Some(r#"{"filenames":["PUB_PACKET"]}"#),
            allowed_values: None,
            params: &[
                ParamSpec {
                    key: "filenames",
                    ty: ValueType::StringList,
                    required: false,
                    description: "Catalog file names to request",
                    allowed: None,
                    example: r#"["PUB_PACKET"]"#,
                },
                ParamSpec {
                    key: "catalog_item",
                    ty: ValueType::String,
                    required: false,
                    description: "Single catalog item shorthand (alternative to filenames)",
                    allowed: None,
                    example: r#""PUB_PACKET""#,
                },
                ParamSpec {
                    key: "query",
                    ty: ValueType::String,
                    required: false,
                    description: "Ad-hoc inquiry keyword not in the built-in catalog (staged \
                                  the same as a filename — Codex adrev 2026-07-20 P2 #3)",
                    allowed: None,
                    example: r#""CMS_TRAFFIC""#,
                },
            ],
            outputs: &[
                OutputSpec {
                    key: "staged",
                    ty: ValueType::Boolean,
                    description: "Whether the request message was staged to the outbox",
                    nullable: false,
                },
                OutputSpec {
                    key: "mid",
                    ty: ValueType::String,
                    description: "Winlink message id of the staged request",
                    nullable: false,
                },
            ],
            dry_run_shape: None,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: CatalogRequestParams =
            serde_json::from_value(params)
                .map_err(|e| StepError::invalid(LOCAL_COMPOSE_CATALOG_REQUEST, format!("invalid params: {e}")))?;

        let filenames = parsed.resolved_filenames();
        let filename_refs: Vec<&str> = filenames.iter().map(String::as_str).collect();
        // `build_inquiry_body` itself rejects an empty list / an embedded
        // newline / a whitespace-only filename — its own error variants are
        // already operator-facing text (Global Constraints: verbatim, never
        // paraphrased), so this is passed straight through, not re-validated
        // here first.
        let body = crate::catalog::composer::build_inquiry_body(&filename_refs)
            .map_err(|e| StepError::invalid(LOCAL_COMPOSE_CATALOG_REQUEST, e.to_string()))?;

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
        .map_err(|cause| StepError::service(LOCAL_COMPOSE_CATALOG_REQUEST, cause))?;

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
            writes_config: false,
            name: LOCAL_SET_IDENTITY,
            label: "Set station identity",
            description: "Switch the active callsign for the rest of this run only.",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
            example_params: Some(r#"{"identity":{"callsign":"N0CALL-1"}}"#),
            allowed_values: None,
            params: &[ParamSpec {
                key: "identity",
                ty: ValueType::Object,
                required: true,
                description: "The station identity object to activate",
                allowed: None,
                example: r#"{"callsign":"N0CALL-1"}"#,
            }],
            outputs: &[OutputSpec {
                key: "identity",
                ty: ValueType::Object,
                description: "The identity as applied",
                nullable: false,
            }],
            dry_run_shape: None,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        if cancel.is_cancelled() {
            return Err(StepError::Cancelled);
        }

        let parsed: SetIdentityParams =
            serde_json::from_value(params)
                .map_err(|e| StepError::invalid(LOCAL_SET_IDENTITY, format!("invalid params: {e}")))?;

        let callsign = parsed
            .identity
            .as_object()
            .and_then(|obj| obj.get("callsign"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if callsign.is_none() {
            return Err(StepError::invalid(
                LOCAL_SET_IDENTITY,
                "identity must be an object with a non-empty \"callsign\" string",
            ));
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
            writes_config: false,
            name: LOCAL_LOG,
            label: "Write log entry",
            description: "Write a line to the station log.",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
            example_params: Some(r#"{"message":"Hourly check complete"}"#),
            allowed_values: None,
            params: &[ParamSpec {
                key: "message",
                ty: ValueType::String,
                required: true,
                description: "Line to append to the station/session log. A value that IS a \
                              \"$sN.key\" ref substitutes the typed value; refs embedded \
                              inside longer text interpolate as text (6epl8). Unresolvable \
                              embedded tokens are left verbatim; an unset whole-string ref \
                              fails the step.",
                allowed: None,
                example: r#""Hourly check complete""#,
            }],
            outputs: &[],
            dry_run_shape: None,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: LogParams = serde_json::from_value(params)
            .map_err(|e| StepError::invalid(LOCAL_LOG, format!("invalid params: {e}")))?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.local.log_append(parsed.message) => res,
        }
        .map_err(|cause| StepError::service(LOCAL_LOG, cause))?;

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
            writes_config: false,
            name: LOCAL_NOTIFY,
            label: "Show notification",
            description: "Show a desktop notification.",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
            example_params: Some(r#"{"message":"New messages retrieved"}"#),
            allowed_values: None,
            params: &[
                ParamSpec {
                    key: "message",
                    ty: ValueType::String,
                    required: true,
                    description: "Desktop notification body",
                    allowed: None,
                    example: r#""New messages retrieved""#,
                },
                ParamSpec {
                    key: "title",
                    ty: ValueType::String,
                    required: false,
                    description: "Notification title (app default when omitted)",
                    allowed: None,
                    example: r#""Tuxlink""#,
                },
            ],
            outputs: &[],
            dry_run_shape: None,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: NotifyParams =
            serde_json::from_value(params)
                .map_err(|e| StepError::invalid(LOCAL_NOTIFY, format!("invalid params: {e}")))?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.local.notify(parsed.title, parsed.message) => res,
        }
        .map_err(|cause| StepError::service(LOCAL_NOTIFY, cause))?;

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

    async fn station_context(&self) -> StationContext {
        use crate::config::PositionSource;

        let arbiter = self.app.state::<Arc<crate::position::PositionArbiter>>();
        let arbiter: &crate::position::PositionArbiter = &arbiter;

        // A missing config is the pre-wizard case: nothing configured, so
        // nothing to sign as and nothing to broadcast. The form still goes out
        // with those fields empty rather than the run failing, matching how the
        // compose window behaves before the wizard has run.
        let Ok(cfg) = crate::config::read_config() else {
            return StationContext::default();
        };

        let on_air_grid = crate::position::effective_broadcast_locator(&cfg, Some(arbiter));
        // "GPS" only when a live fix is what actually produced the locator.
        // That needs all three of `effective_broadcast_locator`'s conditions to
        // have held, not just the source chip: under a suppressing gps_state it
        // returns the stored config grid, and even on the GPS branch
        // `broadcast_grid` falls back to the manual grid when the fix has gone
        // stale. Either fallback is operator-entered, and reporting it as GPS
        // would misstate the provenance on a form that asks for it.
        let from_live_fix = arbiter.source() == PositionSource::Gps
            && cfg.privacy.gps_state == crate::config::GpsState::BroadcastAtPrecision
            && arbiter.has_fresh_fix();
        let location_source = if on_air_grid.is_empty() {
            None
        } else if from_live_fix {
            Some("GPS".to_string())
        } else {
            Some("Operator".to_string())
        };

        StationContext {
            callsign: cfg
                .identity
                .active_full
                .clone()
                .or_else(|| cfg.identity.identifier.clone()),
            on_air_grid,
            location_source,
        }
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
        station: StationContext,
    }

    impl Default for FakeLocalService {
        fn default() -> Self {
            Self {
                compose: Box::new(|_, _| panic!("compose_stage not expected in this test")),
                log: Box::new(|_| panic!("log_append not expected in this test")),
                notify: Box::new(|_, _| panic!("notify not expected in this test")),
                station: StationContext::default(),
            }
        }
    }

    impl FakeLocalService {
        fn with_station(mut self, station: StationContext) -> Self {
            self.station = station;
            self
        }
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
        async fn station_context(&self) -> StationContext {
            self.station.clone()
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
            StepError::Action { action, cause, .. } => {
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
            StepError::Action { action, cause, .. } => {
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
                    // No `id`: a hand-written inline template, which stays a
                    // plain templated text message. A payload WITH an id is a
                    // reference to a real bundled form and takes the form path
                    // instead — see the form-send tests below.
                    "template": {
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

    // ======================================================================
    // local.compose — FORM sends (tuxlink-3ddk2)
    //
    // A step referencing a real bundled form must produce a real form: XML
    // attachment, field structure, the parts a Winlink Express receiver needs
    // to recognise it. Before this, the reference rendered the form's body
    // text and staged it with no attachment, so the recipient got something
    // that reads like an ICS-213 to a person and is not one to their client.
    // ======================================================================

    /// The resolved-`@template:` shape, exactly as `routines/resolver.rs`
    /// emits it.
    fn resolved_template(id: &str) -> Value {
        let form = crate::forms::catalog::find_form(id).expect("bundled form");
        json!({
            "id": form.id,
            "name": form.name,
            "subjectTemplate": form.subject_template,
            "bodyTemplate": form.body_template,
        })
    }

    fn station_at(grid: &str, callsign: &str, source: &str) -> StationContext {
        StationContext {
            callsign: Some(callsign.to_string()),
            on_air_grid: grid.to_string(),
            location_source: Some(source.to_string()),
        }
    }

    async fn stage_form(
        station: StationContext,
        params: Value,
    ) -> Result<OutboundMessage, StepError> {
        let observed: Arc<Mutex<Option<OutboundMessage>>> = Arc::new(Mutex::new(None));
        let o = observed.clone();
        let local = FakeLocalService::default()
            .with_station(station)
            .with_compose(move |msg, _from| {
                *o.lock().unwrap() = Some(msg);
                Ok("m1".to_string())
            });
        ComposeMessage::new(Arc::new(local))
            .execute(params, CancellationToken::new())
            .await?;
        let msg = observed.lock().unwrap().clone().expect("staged a message");
        Ok(msg)
    }

    fn xml_of(msg: &OutboundMessage) -> String {
        assert_eq!(
            msg.attachments.len(),
            1,
            "a form send stages exactly one attachment, its XML"
        );
        String::from_utf8_lossy(&msg.attachments[0].bytes).into_owned()
    }

    /// THE defect. Reverting the id-bearing arm makes this fail with zero
    /// attachments, which is the shipped behaviour it replaces.
    #[tokio::test]
    async fn a_form_reference_stages_a_real_form_not_prose_that_resembles_one() {
        let msg = stage_form(
            station_at("CN87", "N0CALL", "Operator"),
            json!({
                "to": ["NET@winlink.org"],
                "template": resolved_template("Winlink_Check-In"),
                "vars": {"organization": "Cascadia Net", "msgto": "NET CONTROL",
                         "newsubject": "Morning check-in", "status": "EXERCISE"}
            }),
        )
        .await
        .expect("a bundled form reference must stage");

        assert_eq!(
            msg.attachments[0].filename,
            "RMS_Express_Form_Winlink_Check-In.xml"
        );
        let xml = xml_of(&msg);
        assert!(xml.contains("<RMS_Express_Form>"));
        assert!(xml.contains("<msgto>NET CONTROL</msgto>"));
        assert!(xml.contains("<organization>Cascadia Net</organization>"));
        // The readable body is still produced — it is the fallback for clients
        // that cannot render the XML, not a substitute for it.
        assert!(msg.body.contains("NET CONTROL"), "body: {}", msg.body);
    }

    /// The draft deliberately does not store these, so the run supplies them.
    /// An empty Date/Time on a daily check-in is a defective form sent every
    /// morning.
    #[tokio::test]
    async fn the_run_fills_the_fields_a_saved_draft_deliberately_omits() {
        let msg = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["NET@winlink.org"],
                "template": resolved_template("Winlink_Check-In"),
                "vars": {"msgto": "NET CONTROL"}
            }),
        )
        .await
        .expect("staged");
        let xml = xml_of(&msg);

        assert!(xml.contains("<msgsender>N0CALL</msgsender>"), "{xml}");
        assert!(xml.contains("<grid>CN87</grid>"), "{xml}");
        assert!(xml.contains("<locationsource>GPS</locationsource>"), "{xml}");
        // The timestamp is the moment the run fired, in the shape WLE's
        // Check-In uses. Assert the shape rather than a literal clock value.
        let dt_open = xml.find("<datetime>").expect("datetime element") + "<datetime>".len();
        let dt_close = xml[dt_open..].find("</datetime>").unwrap() + dt_open;
        let dt = &xml[dt_open..dt_close];
        assert_eq!(dt.len(), 16, "expected YYYY-MM-DD HH:MM, got {dt:?}");
        assert!(dt.contains('-') && dt.contains(':'), "got {dt:?}");
    }

    /// An authored value is never overwritten by a derived one.
    #[tokio::test]
    async fn an_explicit_var_wins_over_the_runtime_fill() {
        let msg = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["NET@winlink.org"],
                "template": resolved_template("Winlink_Check-In"),
                "vars": {"grid": "DM33", "msgsender": "N0CALL-7", "datetime": "2026-01-01 00:00"}
            }),
        )
        .await
        .expect("staged");
        let xml = xml_of(&msg);
        assert!(xml.contains("<grid>DM33</grid>"), "{xml}");
        assert!(xml.contains("<msgsender>N0CALL-7</msgsender>"), "{xml}");
        assert!(xml.contains("<datetime>2026-01-01 00:00</datetime>"), "{xml}");
    }

    /// The operator has said no position may be transmitted. A routine is not
    /// an exception to that: it is the same standing setting, read at run time.
    #[tokio::test]
    async fn nothing_broadcastable_means_no_position_on_the_form() {
        let station = StationContext {
            callsign: Some("N0CALL".into()),
            on_air_grid: String::new(),
            location_source: None,
        };
        let msg = stage_form(
            station,
            json!({
                "to": ["NET@winlink.org"],
                "template": resolved_template("Winlink_Check-In"),
                "vars": {"msgto": "NET CONTROL"}
            }),
        )
        .await
        .expect("staged");
        let xml = xml_of(&msg);
        assert!(xml.contains("<grid></grid>"), "{xml}");
        assert!(xml.contains("<locationsource></locationsource>"), "{xml}");
        assert!(
            xml.contains("<grid_square></grid_square>"),
            "the envelope carries no position either: {xml}"
        );
    }

    /// A field the form does not declare is never invented. An ICS-213 has no
    /// location, so it does not sprout one.
    #[tokio::test]
    async fn a_form_without_a_field_does_not_grow_one() {
        let msg = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["W1AW"],
                "template": resolved_template("ICS213_Initial"),
                "vars": {"subjectline": "Road closure", "message": "Route 9 blocked"}
            }),
        )
        .await
        .expect("staged");
        let xml = xml_of(&msg);
        assert!(!xml.contains("<locationsource>"), "{xml}");
        assert!(!xml.contains("<grid>"), "{xml}");
        assert!(xml.contains("<message>Route 9 blocked</message>"), "{xml}");
    }

    /// `from_identity` is a run-scoped tactical call and outranks the station's
    /// configured callsign in BOTH the envelope and the form's own From field.
    #[tokio::test]
    async fn a_run_scoped_identity_signs_the_form() {
        let msg = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["NET@winlink.org"],
                "template": resolved_template("Winlink_Check-In"),
                "from_identity": {"callsign": "SHELTER-1"}
            }),
        )
        .await
        .expect("staged");
        let xml = xml_of(&msg);
        assert!(xml.contains("<msgsender>SHELTER-1</msgsender>"), "{xml}");
        assert!(
            xml.contains("<senders_callsign>SHELTER-1</senders_callsign>"),
            "{xml}"
        );
    }

    /// The `@draft:` shape: the resolved reference carries the form AND the
    /// operator's saved answers in one object, and the step's own vars still
    /// override a single line of it.
    #[tokio::test]
    async fn a_resolved_draft_supplies_the_answers_and_vars_still_win() {
        let mut template = resolved_template("Winlink_Check-In");
        template["values"] = json!({
            "organization": "Cascadia Net",
            "msgto": "NET CONTROL",
            "band": "40m",
        });
        template["draft"] = json!({"slotId": "abc", "label": "Cascadia Morning Net"});

        let msg = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["NET@winlink.org"],
                "template": template,
                // Tonight is on 80m; everything else comes from the draft.
                "vars": {"band": "80m"}
            }),
        )
        .await
        .expect("staged");
        let xml = xml_of(&msg);
        assert!(xml.contains("<organization>Cascadia Net</organization>"), "{xml}");
        assert!(xml.contains("<msgto>NET CONTROL</msgto>"), "{xml}");
        assert!(
            xml.contains("<band>80m</band>"),
            "the step's own vars override the saved answer: {xml}"
        );
    }

    /// A step may name the form and nothing else. The templates belong to the
    /// bundle, so a routine author should not have to reproduce them.
    #[tokio::test]
    async fn a_bare_form_id_is_enough() {
        let msg = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["NET@winlink.org"],
                "template": {"id": "Winlink_Check-In"},
                "vars": {"msgto": "NET CONTROL", "newsubject": "Morning check-in"}
            }),
        )
        .await
        .expect("a bare id must be sufficient");
        assert_eq!(msg.subject, "Morning check-in");
        assert!(xml_of(&msg).contains("<msgto>NET CONTROL</msgto>"));
    }

    /// An empty template object names neither shape. One rejection carries
    /// both, rather than serde reporting a missing field at a time.
    #[tokio::test]
    async fn an_empty_template_object_names_both_valid_shapes() {
        let err = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({"to": ["W1AW"], "template": {}}),
        )
        .await
        .expect_err("an empty template must not stage");
        match err {
            StepError::Action { cause, .. } => {
                assert!(cause.contains("\"id\""), "cause: {cause}");
                assert!(cause.contains("bodyTemplate"), "cause: {cause}");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    /// A reference naming a form we do not have fails the step. Falling back to
    /// a plain message would ship exactly the bug this path exists to prevent.
    #[tokio::test]
    async fn an_unresolvable_form_reference_fails_the_step() {
        let err = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["W1AW"],
                "template": {"id": "Not_A_Real_Form", "subjectTemplate": "s", "bodyTemplate": "b"}
            }),
        )
        .await
        .expect_err("an unknown form must not stage anything");
        match err {
            StepError::Action { action, cause, .. } => {
                assert_eq!(action, "local.compose");
                assert!(cause.contains("Not_A_Real_Form"), "cause: {cause}");
                assert!(cause.contains("unknown form"), "cause: {cause}");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    /// An explicit subject still wins on the form path, same as everywhere
    /// else.
    #[tokio::test]
    async fn an_explicit_subject_wins_on_the_form_path_too() {
        let msg = stage_form(
            station_at("CN87", "N0CALL", "GPS"),
            json!({
                "to": ["NET@winlink.org"],
                "subject": "Cascadia Net check-in",
                "template": resolved_template("Winlink_Check-In"),
                "vars": {"newsubject": "Morning check-in"}
            }),
        )
        .await
        .expect("staged");
        assert_eq!(msg.subject, "Cascadia Net check-in");
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
            StepError::Action { action, cause, .. } => {
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
        // tuxlink-5lfxk: every shipped action carries human palette copy.
        assert!(!d.label.is_empty() && !d.description.is_empty());
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
            StepError::Action { action, cause, .. } => {
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
            StepError::Action { action, cause, .. } => {
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
        // tuxlink-5lfxk: every shipped action carries human palette copy.
        assert!(!d.label.is_empty() && !d.description.is_empty());
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
            StepError::Action { action, cause, .. } => {
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
        // tuxlink-5lfxk: every shipped action carries human palette copy.
        assert!(!d.label.is_empty() && !d.description.is_empty());
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
            StepError::Action { action, cause, .. } => {
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
        // tuxlink-5lfxk: every shipped action carries human palette copy.
        assert!(!d.label.is_empty() && !d.description.is_empty());
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
            StepError::Action { action, cause, .. } => {
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
        // tuxlink-5lfxk: every shipped action carries human palette copy.
        assert!(!d.label.is_empty() && !d.description.is_empty());
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }

    /// tuxlink-3nvvl: every descriptor's example_params must pass its own
    /// declared ParamSpecs — locks the registry backfill mechanically.
    #[test]
    fn descriptor_examples_pass_their_own_param_specs() {
        use tuxlink_routines::validate::params::example_self_check;
        let actions: Vec<tuxlink_routines::action::ActionDescriptor> = vec![
            ComposeMessage::new(Arc::new(FakeLocalService::default())).descriptor(),
            ComposeCatalogRequest::new(Arc::new(FakeLocalService::default())).descriptor(),
            SetIdentity.descriptor(),
            LogEntry::new(Arc::new(FakeLocalService::default())).descriptor(),
            Notify::new(Arc::new(FakeLocalService::default())).descriptor(),
        ];
        for d in actions {
            let f = example_self_check(&d);
            assert!(f.is_empty(), "{}: {f:?}", d.name);
        }
    }

}
