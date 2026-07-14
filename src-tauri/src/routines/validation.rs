//! `MonolithValidationContext`: the production
//! [`ValidationContext`](tuxlink_routines::validate::ValidationContext) the
//! validator (`tuxlink_routines::validate::{validate, validate_fleet}`, plan 3)
//! reasons over — the read-only port that answers "does this `@`-token resolve",
//! "what does this action declare", "what else is enabled", and "what can this
//! station actually do".
//!
//! ## Why it lands here (plan 2 Task 6), not in plan 3
//!
//! Plan 3 built the validator against its own `StaticContext` test double and
//! left the real adapter to "plan 4 or the executing controller" — but the
//! command layer's **validate-on-save** contract (spec §10: continuous
//! validation, one validator, no privileged path) needs it NOW: `routines_save`
//! validates every definition as it is written, and `routines_set_enabled`
//! refuses to enable a routine with errors. Without this adapter, both would
//! have to either skip validation (shipping an unvalidated authoring surface) or
//! re-derive a second, parallel notion of "what exists" — the exact drift the
//! single-validator rule exists to prevent.
//!
//! ## The four seams, and where each answer comes from
//!
//! | Port method | Source of truth |
//! |---|---|
//! | `entity_exists` | the SAME stores [`super::resolver::MonolithEntityResolver`] resolves against at snapshot time — presets, station-sets, `identities.json`, the bundled forms catalog |
//! | `action_descriptor` | the SAME [`ActionRegistry`] the executor resolves actions from (`RoutinesState.registry`) — never a name-sniffing heuristic |
//! | `routine_def` / `enabled_routines` | [`super::store::DefinitionStore`] + its `enabled.json` sidecar |
//! | `station_profile` | [`station_profile_from_config`] — the operator's config |
//!
//! Every one of those is the same source the RUNTIME uses. A validator that
//! consulted a different registry or a different store could pass a routine the
//! executor then chokes on; by construction, this one cannot.
//!
//! ## `station_profile` is a DECLARED posture, not a live probe
//!
//! `has_internet` reads `config.connect.connect_to_cms` — the operator's own
//! "this station reaches the internet" declaration (`false` = offline-only
//! deployment, `config.rs`). It is deliberately NOT a live reachability probe:
//! validation runs on every keystroke-scale save and must be instant,
//! deterministic, and honest about a station that is off-grid *by design* rather
//! than merely off-grid *right now*. Both findings this feeds
//! (`NEEDS_INTERNET_OFFGRID`, `NO_RIG_CONFIGURED`) are WARNINGS, not errors —
//! they never block a save or a run, so a stale-by-a-minute answer costs a
//! nudge, not a capability.
//!
//! `rigs` is `["default"]` when the operator has configured a rig model + CAT
//! serial (`modem_commands::rig_config_from` — the same predicate the real
//! `rig.*` actions use to decide "rig control not configured"), else empty. The
//! single-rig station is v1's model throughout ([`super::actions`]'s
//! `DEFAULT_RIG_ID`), so the list is 0 or 1 long by construction.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tuxlink_routines::action::{ActionDescriptor, ActionRegistry};
use tuxlink_routines::refs::EntityRef;
use tuxlink_routines::types::RoutineDef;
use tuxlink_routines::validate::{StationProfile, ValidationContext};

use crate::forms::catalog::find_form;
use crate::identity::IdentityStore;

use super::actions::DEFAULT_RIG_ID;
use super::presets::RadioPresetStore;
use super::session::RoutinesState;
use super::station_sets::StationSetStore;
use super::store::DefinitionStore;

/// The production [`ValidationContext`]. Cheap to build (it clones `Arc`s and
/// reads the config once), so the command layer constructs a fresh one per
/// validate call rather than caching one — a routine saved a second after a
/// preset was created must see that preset.
pub struct MonolithValidationContext {
    presets: Arc<RadioPresetStore>,
    station_sets: Arc<StationSetStore>,
    identity_store_path: PathBuf,
    store: Arc<DefinitionStore>,
    registry: Arc<ActionRegistry>,
    profile: StationProfile,
}

impl MonolithValidationContext {
    /// Assemble from resolved parts — the injectable seam tests use (tempdir
    /// stores, a fake registry, a hand-built profile).
    pub fn new(
        presets: Arc<RadioPresetStore>,
        station_sets: Arc<StationSetStore>,
        identity_store_path: PathBuf,
        store: Arc<DefinitionStore>,
        registry: Arc<ActionRegistry>,
        profile: StationProfile,
    ) -> Self {
        Self {
            presets,
            station_sets,
            identity_store_path,
            store,
            registry,
            profile,
        }
    }

    /// The production constructor: every store + the action registry come from
    /// the managed [`RoutinesState`], the station profile from the operator's
    /// config. This is what the Tauri command layer calls.
    pub fn for_state(state: &RoutinesState) -> Self {
        Self::new(
            state.presets.clone(),
            state.station_sets.clone(),
            state.identity_store_path.clone(),
            state.store.clone(),
            state.registry.clone(),
            station_profile_from_config(),
        )
    }

    /// Does an `@identity:<name>` token name a real identity? Matched exactly as
    /// [`super::resolver::MonolithEntityResolver::resolve`] matches it — a FULL
    /// identity's callsign, then a tactical identity's label — so "the validator
    /// says it resolves" and "the snapshot resolves it" can never disagree.
    fn identity_exists(&self, name: &str) -> bool {
        let Ok(store) = IdentityStore::load(&self.identity_store_path) else {
            return false;
        };
        store.full().iter().any(|f| f.callsign.as_str() == name)
            || store.tactical().iter().any(|t| t.label == name)
    }
}

impl ValidationContext for MonolithValidationContext {
    fn entity_exists(&self, r: &EntityRef) -> bool {
        match r.kind.as_str() {
            "preset" => self.presets.get(&r.name).is_some(),
            "station-set" => self.station_sets.get(&r.name).is_some(),
            "identity" => self.identity_exists(&r.name),
            "template" => find_form(&r.name).is_some(),
            // An unknown KIND does not exist — never silently pass through
            // (`UNRESOLVED_REF` names the verbatim token). Mirrors the
            // resolver's own unknown-kind arm.
            _ => false,
        }
    }

    fn action_descriptor(&self, name: &str) -> Option<ActionDescriptor> {
        self.registry.get(name).map(|a| a.descriptor())
    }

    fn routine_def(&self, name: &str) -> Option<RoutineDef> {
        self.store.get(name)
    }

    fn enabled_routines(&self) -> Vec<RoutineDef> {
        self.store
            .list()
            .into_iter()
            .filter(|s| s.enabled)
            .filter_map(|s| self.store.get(&s.routine))
            .collect()
    }

    fn station_profile(&self) -> StationProfile {
        self.profile.clone()
    }
}

/// The station's declared capabilities, read from the operator's config. A
/// config that fails to read (absent / corrupt — a pre-wizard install) yields
/// the conservative default: no internet, no rig. That produces WARNINGS on
/// internet/radio steps, never errors, so a fresh install can still author and
/// save routines before the wizard has run.
pub fn station_profile_from_config() -> StationProfile {
    let Ok(cfg) = crate::config::read_config() else {
        return StationProfile::default();
    };
    let rigs = if crate::modem_commands::rig_config_from(&cfg.rig).is_some() {
        vec![DEFAULT_RIG_ID.to_string()]
    } else {
        Vec::new()
    };
    StationProfile {
        has_internet: cfg.connect.connect_to_cms,
        rigs,
    }
}

/// Build a context over an explicit config directory — the shape the command
/// tests use (tempdir stores, no `AppHandle`, no real config file).
pub fn context_for_dir(
    config_dir: &Path,
    store: Arc<DefinitionStore>,
    registry: Arc<ActionRegistry>,
    profile: StationProfile,
) -> MonolithValidationContext {
    MonolithValidationContext::new(
        Arc::new(RadioPresetStore::open(
            config_dir.join("radio-presets.json"),
        )),
        Arc::new(StationSetStore::open(config_dir.join("station-sets.json"))),
        config_dir.join("identities.json"),
        store,
        registry,
        profile,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::presets::RadioPreset;
    use crate::routines::station_sets::StationSet;
    use std::sync::Arc;
    use tuxlink_routines::fakes::FakeAction;
    use tuxlink_routines::types::{
        OnInterrupted, RoutineDef, Track, TransmitMode, Trigger, SUPPORTED_SCHEMA_VERSION,
    };

    fn def(name: &str) -> RoutineDef {
        RoutineDef {
            routine: name.into(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![],
            }],
        }
    }

    fn ctx_over(dir: &Path) -> (Arc<DefinitionStore>, MonolithValidationContext) {
        let store = Arc::new(DefinitionStore::open(dir.join("routines")));
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(
            FakeAction::new("radio.connect").with_capabilities(true, true, false),
        ));
        let ctx = context_for_dir(
            dir,
            store.clone(),
            Arc::new(reg),
            StationProfile {
                has_internet: true,
                rigs: vec!["default".into()],
            },
        );
        (store, ctx)
    }

    #[test]
    fn entity_exists_answers_from_the_real_stores_per_kind() {
        let dir = tempfile::tempdir().unwrap();
        let (_store, ctx) = ctx_over(dir.path());

        // Nothing seeded yet: every token is unresolved.
        assert!(!ctx.entity_exists(&EntityRef::parse("@preset:40m-ardop").unwrap()));
        assert!(!ctx.entity_exists(&EntityRef::parse("@station-set:or-gateways").unwrap()));

        RadioPresetStore::open(dir.path().join("radio-presets.json"))
            .save(&RadioPreset {
                name: "40m-ardop".into(),
                frequency_hz: 7_070_000,
                mode: "ARDOP".into(),
                power_w: None,
                atu: None,
            })
            .unwrap();
        StationSetStore::open(dir.path().join("station-sets.json"))
            .save(&StationSet {
                name: "or-gateways".into(),
                callsigns: vec!["W7DEF-10".into()],
            })
            .unwrap();

        assert!(ctx.entity_exists(&EntityRef::parse("@preset:40m-ardop").unwrap()));
        assert!(ctx.entity_exists(&EntityRef::parse("@station-set:or-gateways").unwrap()));
        // A bundled Standard Form IS a template.
        assert!(ctx.entity_exists(&EntityRef::parse("@template:ICS213_Initial").unwrap()));
        assert!(!ctx.entity_exists(&EntityRef::parse("@template:No_Such_Form").unwrap()));
        // Unknown kind never passes through.
        assert!(!ctx.entity_exists(&EntityRef::parse("@mystery:whatever").unwrap()));
    }

    #[test]
    fn action_descriptor_comes_from_the_registry_with_capability_flags_intact() {
        let dir = tempfile::tempdir().unwrap();
        let (_store, ctx) = ctx_over(dir.path());

        let d = ctx
            .action_descriptor("radio.connect")
            .expect("registered action must be described");
        assert!(d.needs_radio);
        assert!(d.transmits);
        assert!(!d.needs_internet);
        assert!(
            ctx.action_descriptor("radio.mystery").is_none(),
            "an unregistered action is UNKNOWN, not defaulted"
        );
    }

    #[test]
    fn routine_def_and_enabled_routines_track_the_store_and_its_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let (store, ctx) = ctx_over(dir.path());

        store.save(&def("alpha")).unwrap();
        store.save(&def("beta")).unwrap();
        assert_eq!(ctx.routine_def("alpha"), Some(def("alpha")));
        assert!(ctx.routine_def("nope").is_none());

        // Saved-but-not-enabled is NOT in the fleet.
        assert!(ctx.enabled_routines().is_empty());
        store.set_enabled("beta", true).unwrap();
        let enabled = ctx.enabled_routines();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].routine, "beta");
    }

    #[test]
    fn station_profile_is_the_injected_posture() {
        let dir = tempfile::tempdir().unwrap();
        let (_store, ctx) = ctx_over(dir.path());
        let p = ctx.station_profile();
        assert!(p.has_internet);
        assert_eq!(p.rigs, vec!["default".to_string()]);
    }
}
