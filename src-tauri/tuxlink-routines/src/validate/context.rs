//! `ValidationContext` port (spec §10): everything `validate()` needs to
//! know about the world outside a single `RoutineDef` — whether a
//! `@`-reference resolves, what an action declares about itself, sibling
//! routine definitions (for `Call` closure checks), the currently-enabled
//! fleet, and the station's radio/internet profile.
//!
//! This crate is Tauri-free, so the trait is the seam: the monolith wires a
//! real adapter over its own stores (plan 4, alongside the MCP port so
//! `routines_validate` serves both UI and MCP from one implementation).
//! `StaticContext` is this task's test double, public like `fakes.rs`'s
//! `FakeAction`/`FakeResolver` so later validate-module tasks and the
//! plan-4 adapter's own tests reuse it instead of re-deriving a fake.

use std::collections::{HashMap, HashSet};

use crate::action::ActionDescriptor;
use crate::refs::EntityRef;
use crate::types::RoutineDef;

/// What the station can actually do right now (spec §9's capability
/// checks compare `ActionDescriptor.{needs_radio,needs_internet}` against
/// this).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StationProfile {
    pub has_internet: bool,
    pub rigs: Vec<String>,
}

/// Read-only port `validate()` and `validate_fleet()` reason over. No
/// mutation, no I/O contract beyond lookup — implementations may be
/// entirely in-memory (`StaticContext`) or backed by the monolith's real
/// stores.
pub trait ValidationContext: Send + Sync {
    /// Does this `@`-token resolve to a real configured entity?
    fn entity_exists(&self, r: &EntityRef) -> bool;
    /// The catalog descriptor for an action name, if it exists in the
    /// registry (spec §6). `None` means `ActionStep.action` is unknown.
    fn action_descriptor(&self, name: &str) -> Option<ActionDescriptor>;
    /// A sibling routine's definition by name, for `Call` closure walks
    /// (consent + recursion checks, tasks 3-4).
    fn routine_def(&self, name: &str) -> Option<RoutineDef>;
    /// Every routine currently enabled, for the fleet check (task 5).
    fn enabled_routines(&self) -> Vec<RoutineDef>;
    /// The station's current radio/internet capabilities.
    fn station_profile(&self) -> StationProfile;
}

/// Builder-style test double mirroring `fakes.rs`'s conventions
/// (`FakeAction`, `FakeResolver`): seed entities/actions/routines/profile
/// via chained `with_*` calls, then hand `&StaticContext` to `validate()`.
/// Public so every later validate-module task's unit tests, and the plan-4
/// monolith adapter's own tests, share one fake instead of re-deriving it.
#[derive(Debug, Clone, Default)]
pub struct StaticContext {
    entities: HashSet<EntityRef>,
    actions: HashMap<String, ActionDescriptor>,
    routines: HashMap<String, RoutineDef>,
    enabled: Vec<String>,
    profile: StationProfile,
}

impl StaticContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed an entity that `entity_exists` will report as present, e.g.
    /// `.with_entity("station-set", "or-gateways")` for `@station-set:or-gateways`.
    pub fn with_entity(mut self, kind: &str, name: &str) -> Self {
        self.entities.insert(EntityRef {
            kind: kind.to_string(),
            name: name.to_string(),
        });
        self
    }

    /// Seed an action descriptor `action_descriptor(name)` will return.
    pub fn with_action(mut self, descriptor: ActionDescriptor) -> Self {
        self.actions.insert(descriptor.name.to_string(), descriptor);
        self
    }

    /// Register a sibling routine definition `routine_def(name)` will
    /// return, keyed by its own `routine` field.
    pub fn with_routine(mut self, def: RoutineDef) -> Self {
        self.routines.insert(def.routine.clone(), def);
        self
    }

    /// Mark a routine (already registered via `with_routine`) enabled —
    /// included in `enabled_routines()` for the fleet check (task 5).
    pub fn with_enabled(mut self, routine: &str) -> Self {
        self.enabled.push(routine.to_string());
        self
    }

    /// Set the station profile `station_profile()` returns (default:
    /// no internet, no rigs configured).
    pub fn with_profile(mut self, profile: StationProfile) -> Self {
        self.profile = profile;
        self
    }
}

impl ValidationContext for StaticContext {
    fn entity_exists(&self, r: &EntityRef) -> bool {
        self.entities.contains(r)
    }

    fn action_descriptor(&self, name: &str) -> Option<ActionDescriptor> {
        self.actions.get(name).copied()
    }

    fn routine_def(&self, name: &str) -> Option<RoutineDef> {
        self.routines.get(name).cloned()
    }

    fn enabled_routines(&self) -> Vec<RoutineDef> {
        self.enabled
            .iter()
            .filter_map(|name| self.routines.get(name).cloned())
            .collect()
    }

    fn station_profile(&self) -> StationProfile {
        self.profile.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_routine(name: &str) -> RoutineDef {
        RoutineDef {
            routine: name.to_string(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: crate::types::TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: crate::types::OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![crate::types::Trigger::Manual],
            tracks: vec![],
        }
    }

    #[test]
    fn empty_context_knows_nothing() {
        let ctx = StaticContext::new();
        let r = EntityRef {
            kind: "station-set".into(),
            name: "or-gateways".into(),
        };
        assert!(!ctx.entity_exists(&r));
        assert!(ctx.action_descriptor("radio.connect").is_none());
        assert!(ctx.routine_def("r1").is_none());
        assert!(ctx.enabled_routines().is_empty());
        assert_eq!(ctx.station_profile(), StationProfile::default());
    }

    #[test]
    fn seeded_entity_is_found_and_others_are_not() {
        let ctx = StaticContext::new().with_entity("station-set", "or-gateways");
        assert!(ctx.entity_exists(&EntityRef {
            kind: "station-set".into(),
            name: "or-gateways".into()
        }));
        assert!(!ctx.entity_exists(&EntityRef {
            kind: "station-set".into(),
            name: "other".into()
        }));
        assert!(!ctx.entity_exists(&EntityRef {
            kind: "preset".into(),
            name: "or-gateways".into()
        }));
    }

    #[test]
    fn seeded_action_descriptor_round_trips() {
        let descriptor = ActionDescriptor {
            name: "radio.connect",
            needs_radio: true,
            transmits: true,
            needs_internet: false,
        };
        let ctx = StaticContext::new().with_action(descriptor);
        assert_eq!(ctx.action_descriptor("radio.connect"), Some(descriptor));
        assert_eq!(ctx.action_descriptor("unknown.action"), None);
    }

    #[test]
    fn routine_def_and_enabled_routines_track_separately() {
        let a = manual_routine("routine-a");
        let b = manual_routine("routine-b");
        let ctx = StaticContext::new()
            .with_routine(a.clone())
            .with_routine(b.clone())
            .with_enabled("routine-a");

        assert_eq!(ctx.routine_def("routine-a"), Some(a.clone()));
        assert_eq!(ctx.routine_def("routine-b"), Some(b));
        assert_eq!(ctx.routine_def("routine-c"), None);

        let enabled = ctx.enabled_routines();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0], a);
    }

    #[test]
    fn station_profile_defaults_to_offline_no_rigs() {
        let ctx = StaticContext::new();
        let profile = ctx.station_profile();
        assert!(!profile.has_internet);
        assert!(profile.rigs.is_empty());
    }

    #[test]
    fn station_profile_reflects_seeded_value() {
        let profile = StationProfile {
            has_internet: true,
            rigs: vec!["FT-710".into()],
        };
        let ctx = StaticContext::new().with_profile(profile.clone());
        assert_eq!(ctx.station_profile(), profile);
    }
}
