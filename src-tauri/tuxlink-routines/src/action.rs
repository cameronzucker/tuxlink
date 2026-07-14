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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ActionDescriptor {
    pub name: &'static str,
    pub needs_radio: bool,
    pub transmits: bool,
    pub needs_internet: bool,
}

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
