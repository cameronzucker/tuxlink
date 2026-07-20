//! The Action port: what a step DOES (spec §6).
//!
//! Real implementations (plan 2) wrap transports/CAT/local features; the
//! dry-run layer (plan 3) and tests substitute fakes through the same
//! registry — one mechanism (spec §10, §15).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::error::StepError;

/// The closed value-shape vocabulary for declared params and outputs
/// (tuxlink-3nvvl). Deliberately NOT a JSON-schema engine: a handful of
/// shapes the validator can lint, the human designer can render as typed
/// fields, and the agent catalog can teach. `Object` is the escape hatch for
/// genuinely nested params (compose vars, identity blobs) — it lints only
/// presence, never inner shape.
///
/// The list kinds matter to `$ref` type checking: executor substitution
/// (`resolve_params`) replaces a whole `"$path"` string with the referenced
/// value and resolves array ELEMENTS in place without flattening — so a
/// list-typed param accepts a bare `"$sN.key"` string only when the
/// referenced output is itself list-typed, and `["$sN.key"]` where `key` is
/// a list is an array-of-arrays that dies at runtime (the GLM/122b stations
/// divergence this type system exists to catch at save time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    String,
    Number,
    Boolean,
    /// List of free strings.
    StringList,
    /// List of band labels ("20m", "40m", …) — renderable as a band picker.
    BandList,
    /// List of station callsigns — renderable as a station picker and the
    /// canonical target of `$sN.callsigns` refs.
    StationList,
    /// List of objects (e.g. `find_stations`' `gateways`, docs-search hits).
    /// Element shape is not linted.
    ObjectList,
    /// Nested object; presence-only linting.
    Object,
}

impl ValueType {
    /// Stable snake_case token for catalog projections ("string",
    /// "band_list", "station_list", …) — matches this enum's serde
    /// `rename_all` by construction (shape-tested in the projections).
    pub fn token(self) -> &'static str {
        match self {
            ValueType::String => "string",
            ValueType::Number => "number",
            ValueType::Boolean => "boolean",
            ValueType::StringList => "string_list",
            ValueType::BandList => "band_list",
            ValueType::StationList => "station_list",
            ValueType::ObjectList => "object_list",
            ValueType::Object => "object",
        }
    }
}

/// One declared parameter of an action (tuxlink-3nvvl). The single source
/// both audiences project: the validator lints against it at save time, the
/// designer renders a typed field from it, the MCP catalog teaches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ParamSpec {
    pub key: &'static str,
    #[serde(rename = "type")]
    pub ty: ValueType,
    /// Required at deserialization time — absence is a RUNTIME failure, so
    /// the validator flags it as an error at save.
    pub required: bool,
    /// One-line human description (also the field help text in the designer).
    pub description: &'static str,
    /// Closed vocabulary for `String` params or the ELEMENTS of list params.
    /// Generalizes the legacy single-param `allowed_values` tuple.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<&'static [&'static str]>,
    /// Compact JSON example for THIS param alone (e.g. `"[\"20m\"]"`).
    pub example: &'static str,
}

/// One declared output of an action (tuxlink-3nvvl): what `$sN.<key>` can
/// reference and what type it resolves to. The `$ref` type lint and the
/// designer's insert-result picker both read this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct OutputSpec {
    pub key: &'static str,
    #[serde(rename = "type")]
    pub ty: ValueType,
    pub description: &'static str,
    /// True when this output can be `null` or ABSENT depending on the run's
    /// path (a band-less connect's `band`, a fresh config's `old`). The ref
    /// lint warns when a nullable output feeds a REQUIRED param, and both
    /// catalogs project the flag so authors see it (Codex adrev 2026-07-20,
    /// both models independently).
    pub nullable: bool,
}

/// Declared capabilities the validator and arbiter reason over (spec §6).
///
/// `PartialEq`/`Eq` are hand-written to EXCLUDE `dry_run_shape`: comparing fn
/// pointers is meaningless (their addresses are not guaranteed unique, and
/// clippy denies it) and the shape is not part of a descriptor's identity — two
/// descriptors with the same declared capabilities are equal regardless of
/// which shape fn they carry.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ActionDescriptor {
    pub name: &'static str,
    /// Human-readable palette/inspector label (tuxlink-5lfxk). Empty in test
    /// fakes; the UI falls back to `name` when empty.
    pub label: &'static str,
    /// One-line human description for the palette/inspector (tuxlink-5lfxk).
    pub description: &'static str,
    pub needs_radio: bool,
    pub transmits: bool,
    /// Declares that the action MUTATES persisted station configuration (the
    /// `config.*` write family, D5+). Like `transmits`, it is a consent class:
    /// an attended run parks a `writes_config` step for operator confirmation
    /// BEFORE it runs (spec §4, O3/O4 round). `transmits && writes_config` is a
    /// transmit park (transmit copy dominates); `writes_config && !transmits`
    /// is a `ParkKind::Write` park.
    pub writes_config: bool,
    pub needs_internet: bool,
    /// A canonical example `params` object (compact JSON string) the authoring
    /// UI seeds when this action is dropped onto the canvas (D6). `None` for
    /// actions that take no params, or whose params are self-evidently empty.
    pub example_params: Option<&'static str>,
    /// A closed vocabulary for ONE string param: `(param_key, &[allowed…])`.
    /// The validator's `UNKNOWN_READ_SOURCE` lint (D6) fires when a LITERAL
    /// (non-`$ref`) value for `param_key` is outside this set. Today only
    /// `data.read`'s `source` carries one. `None` = no closed vocabulary.
    pub allowed_values: Option<(&'static str, &'static [&'static str])>,
    /// Declared parameters (tuxlink-3nvvl). Empty = the action declares no
    /// param contract yet; the params lint skips it entirely (no false
    /// UNKNOWN_PARAM storms from an undeclared surface).
    pub params: &'static [ParamSpec],
    /// Declared step outputs (tuxlink-3nvvl): the `$sN.<key>` surface.
    pub outputs: &'static [OutputSpec],
    /// A pure function mapping RESOLVED params to this action's shape-true
    /// dry-run output (D6, round-2 P1-5). Consulted by the dry-run registry's
    /// default path when nothing was scripted for this action; a fn pointer so
    /// `ActionDescriptor` stays `Copy` + `'static` (MSRV-safe). `None` = fall
    /// back to the optimistic default (`{"dry_run": true}` plus
    /// `"connected": true` for a radio action).
    #[serde(skip)]
    pub dry_run_shape: Option<fn(&serde_json::Value) -> serde_json::Value>,
}

impl PartialEq for ActionDescriptor {
    fn eq(&self, other: &Self) -> bool {
        // Every field EXCEPT `dry_run_shape` (fn-pointer identity is not a
        // descriptor's identity — see the type doc).
        self.name == other.name
            && self.label == other.label
            && self.description == other.description
            && self.needs_radio == other.needs_radio
            && self.transmits == other.transmits
            && self.writes_config == other.writes_config
            && self.needs_internet == other.needs_internet
            && self.example_params == other.example_params
            && self.allowed_values == other.allowed_values
            && self.params == other.params
            && self.outputs == other.outputs
    }
}

impl Eq for ActionDescriptor {}

#[async_trait]
pub trait Action: Send + Sync {
    fn descriptor(&self) -> ActionDescriptor;

    /// Execute with resolved params. MUST return promptly on `cancel`;
    /// MUST surface underlying failures verbatim in `StepError::Action`.
    async fn execute(
        &self,
        params: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, StepError>;
}

#[derive(Default)]
pub struct ActionRegistry {
    actions: HashMap<&'static str, Arc<dyn Action>>,
}

impl ActionRegistry {
    pub fn register(&mut self, action: Arc<dyn Action>) {
        self.actions.insert(action.descriptor().name, action);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Action>> {
        self.actions.get(name).cloned()
    }

    pub fn descriptors(&self) -> Vec<ActionDescriptor> {
        self.actions.values().map(|a| a.descriptor()).collect()
    }

    /// Every registered action, in arbitrary order. The monolith's consent
    /// layer (plan 2 Task 5b) consumes this to rebuild a registry in which
    /// every `transmits: true` action is wrapped in a consent gate — the
    /// wrapper preserves the inner descriptor (including `name`), so a
    /// re-`register` of the wrapped action keys under the same catalog name.
    pub fn actions(&self) -> impl Iterator<Item = Arc<dyn Action>> + '_ {
        self.actions.values().cloned()
    }
}
