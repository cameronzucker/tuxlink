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
