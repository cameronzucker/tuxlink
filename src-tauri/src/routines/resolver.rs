//! `MonolithEntityResolver`: the production [`EntityResolver`] a mounted
//! Engine uses to resolve `@`-reference tokens at run-start snapshot time
//! (spec §7, `tuxlink-routines::snapshot::resolve_snapshot`).
//!
//! ## Recon: the real service seam per entity kind (plan 2 Task 3)
//!
//! The plan's Task 3 required grepping the ACTUAL codebase for each entity
//! kind's storage before writing this resolver, rather than assuming a
//! service that doesn't exist. Findings:
//!
//! - **`@preset:<name>`** — `super::presets::RadioPresetStore` (plan 2 Task
//!   1), CRUD over `radio-presets.json`. Straightforward: the store already
//!   exists exactly as spec'd.
//! - **`@station-set:<name>`** — **no named station-set/group concept
//!   exists anywhere in the codebase.** `config::RelayFavorite` /
//!   `network_po_favorites` is a single Network Post Office relay endpoint,
//!   not a named collection of ordinary callsigns. The station-listing
//!   cache (`catalog::stations`/`stations_cache`) and Find-a-Station are a
//!   live-polled ranked result set, not an operator-curated group. Per the
//!   plan's explicit fallback instruction, [`super::station_sets`] is a
//!   NEW small store (same shape/discipline as `presets.rs`:
//!   `station-sets.json` beside `config.json`, atomic writes,
//!   `Vec<String>` callsigns per name) rather than bolting this onto an
//!   unrelated service.
//! - **`@identity:<name>`** — `crate::identity::IdentityStore`, loaded from
//!   `crate::config::identity_store_path()` (`identities.json` beside
//!   `config.json`). `name` is matched against a FULL identity's callsign
//!   first (`IdentityStore::full()`), then a tactical identity's label
//!   (`IdentityStore::tactical()`) — both are flat, already-validated
//!   string fields on the store's own records, so no `Callsign`/`Address`
//!   re-parsing is needed at the resolve boundary. `IdentityStore` holds NO
//!   secrets (see `identity/store.rs` module doc) — nothing keyring-backed
//!   is ever exposed through a routine's resolved snapshot.
//! - **`@template:<name>`** — the plan's recon prompt ("Templates menu item
//!   exists; find its storage") does not hold: the Tools → Templates menu
//!   entry was removed as dead scaffolding (`tuxlink-esb65`,
//!   `src/shell/chrome/menuModel.ts`) because nothing populated it. Two
//!   candidate real services remain: (a) the bundled Standard Forms catalog
//!   (`forms::catalog::find_form`, `FormDef { id, name, subject_template,
//!   body_template, .. }`) — a fixed set of named, ID-addressable message
//!   templates (ICS-213, ICS-309, Bulletin, etc.), reached today from
//!   Compose's form picker; (b) `forms::draft_library::FormDraftLibrary`
//!   — operator-saved FIELD VALUES for a specific `form_id`, keyed by a
//!   minted `slot_id` with a `label` that is not globally unique and is
//!   listed per-`form_id`, not looked up by a single flat name. (b) is the
//!   wrong shape for a single `@template:<name>` token (it stores filled
//!   answers, not a template body, and has no name→single-record lookup).
//!   (a) is chosen: `forms::catalog::find_form(name)` is exactly a
//!   name(id)-addressable message template with a body. `FormDef` does not
//!   derive `Serialize` (its fields are `&'static str`/slices used for
//!   compile-time bundling), so this resolver hand-builds the JSON object
//!   from the fields a routine action needs to compose a message.
//!
//! ## Async trait, sync I/O
//!
//! `EntityResolver::resolve` is `async fn` (object-safety via
//! `async_trait`), but every implementation below is a synchronous file
//! read (`RadioPresetStore`/`StationSetStore`/`IdentityStore` are all
//! bare `std::fs` calls, no tokio I/O) or a static in-memory table lookup
//! (`forms::catalog::find_form`). This is deliberate, not an oversight: at
//! routine-authoring scale (a handful of presets/station-sets/identities/
//! templates, resolved once per run start, not a hot request path) a
//! `tokio::task::spawn_blocking` wrapper would add complexity for no
//! measurable benefit. If a future entity kind's real backing service
//! becomes a genuinely slow or blocking call, wrap THAT call, not this
//! trait boundary.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use tuxlink_routines::error::SnapshotError;
use tuxlink_routines::refs::EntityRef;
use tuxlink_routines::snapshot::EntityResolver;

use crate::forms::catalog::find_form;
use crate::identity::IdentityStore;

use super::presets::RadioPresetStore;
use super::station_sets::StationSetStore;

/// The production `EntityResolver`. Constructed once (Task 5's engine
/// mount) and shared as an `Arc` between the engine and any Tauri command
/// that needs standalone resolution (e.g. a "preview this routine" UI
/// affordance).
pub struct MonolithEntityResolver {
    presets: Arc<RadioPresetStore>,
    station_sets: Arc<StationSetStore>,
    /// `crate::config::identity_store_path()` — passed in rather than
    /// resolved internally so tests can point at a tempdir's
    /// `identities.json` without touching the real XDG config dir. Loaded
    /// fresh on every `resolve` call (matches `RadioPresetStore`/
    /// `StationSetStore`'s no-cache discipline — a concurrent identity
    /// add/remove is never served stale by this resolver).
    identity_store_path: PathBuf,
}

impl MonolithEntityResolver {
    pub fn new(
        presets: Arc<RadioPresetStore>,
        station_sets: Arc<StationSetStore>,
        identity_store_path: PathBuf,
    ) -> Self {
        Self {
            presets,
            station_sets,
            identity_store_path,
        }
    }
}

#[async_trait]
impl EntityResolver for MonolithEntityResolver {
    async fn resolve(&self, entity: &EntityRef) -> Result<serde_json::Value, SnapshotError> {
        match entity.kind.as_str() {
            "preset" => {
                let preset = self
                    .presets
                    .get(&entity.name)
                    .ok_or_else(|| SnapshotError::UnresolvedRef(entity.to_string()))?;
                serde_json::to_value(&preset)
                    .map_err(|e| SnapshotError::Io(format!("preset serialize: {e}")))
            }
            "station-set" => {
                let set = self
                    .station_sets
                    .get(&entity.name)
                    .ok_or_else(|| SnapshotError::UnresolvedRef(entity.to_string()))?;
                Ok(json!(set.callsigns))
            }
            "identity" => self
                .resolve_identity(&entity.name)
                .await
                .ok_or_else(|| SnapshotError::UnresolvedRef(entity.to_string())),
            "template" => {
                let form = find_form(&entity.name)
                    .ok_or_else(|| SnapshotError::UnresolvedRef(entity.to_string()))?;
                Ok(json!({
                    "id": form.id,
                    "name": form.name,
                    "subjectTemplate": form.subject_template,
                    "bodyTemplate": form.body_template,
                }))
            }
            // Unknown kind — never silently pass through. `substitute()` in
            // `tuxlink_routines::snapshot` overwrites `UnresolvedRef`'s
            // payload with the original verbatim token regardless of what
            // we put here, but this resolver does not rely on that: the
            // string it constructs is itself already verbatim.
            _ => Err(SnapshotError::UnresolvedRef(entity.to_string())),
        }
    }
}

impl MonolithEntityResolver {
    /// `name` is matched against a FULL identity's callsign first, then a
    /// tactical identity's label — an exact string match against the
    /// store's own already-validated fields (no re-parsing through
    /// `Callsign::parse`/`Address::tactical`, which would reject a name
    /// that is merely case-different or otherwise wouldn't re-validate
    /// identically to how it was originally stored).
    async fn resolve_identity(&self, name: &str) -> Option<serde_json::Value> {
        let store = IdentityStore::load(&self.identity_store_path).ok()?;
        if let Some(full) = store.full().iter().find(|f| f.callsign.as_str() == name) {
            return serde_json::to_value(full).ok();
        }
        if let Some(tactical) = store.tactical().iter().find(|t| t.label == name) {
            return serde_json::to_value(tactical).ok();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Callsign, FullIdentity, TacticalCmsState, TacticalIdentity};
    use crate::routines::presets::RadioPreset;
    use crate::routines::station_sets::StationSet;

    fn resolver_with_tempdirs() -> (tempfile::TempDir, MonolithEntityResolver) {
        let dir = tempfile::tempdir().unwrap();
        let presets = Arc::new(RadioPresetStore::open(
            dir.path().join("radio-presets.json"),
        ));
        let station_sets = Arc::new(StationSetStore::open(dir.path().join("station-sets.json")));
        let identity_store_path = dir.path().join("identities.json");
        let resolver = MonolithEntityResolver::new(presets, station_sets, identity_store_path);
        (dir, resolver)
    }

    #[tokio::test]
    async fn resolves_preset_as_json_object() {
        let (_dir, resolver) = resolver_with_tempdirs();
        resolver
            .presets
            .save(&RadioPreset {
                name: "40m-ardop".into(),
                frequency_hz: 7_070_000,
                mode: "ARDOP".into(),
                power_w: Some(20),
                atu: Some(true),
            })
            .unwrap();

        let value = resolver
            .resolve(&EntityRef::parse("@preset:40m-ardop").unwrap())
            .await
            .unwrap();
        assert_eq!(value["frequencyHz"], json!(7_070_000));
        assert_eq!(value["mode"], json!("ARDOP"));
        assert_eq!(value["powerW"], json!(20));
    }

    #[tokio::test]
    async fn resolves_station_set_as_array_of_callsigns() {
        let (_dir, resolver) = resolver_with_tempdirs();
        resolver
            .station_sets
            .save(&StationSet {
                name: "or-gateways".into(),
                callsigns: vec!["W7DEF-10".into(), "K7ABC-10".into()],
            })
            .unwrap();

        let value = resolver
            .resolve(&EntityRef::parse("@station-set:or-gateways").unwrap())
            .await
            .unwrap();
        assert_eq!(value, json!(["W7DEF-10", "K7ABC-10"]));
    }

    #[tokio::test]
    async fn resolves_full_identity_by_callsign() {
        let (_dir, resolver) = resolver_with_tempdirs();
        let mut store = IdentityStore::load(&resolver.identity_store_path).unwrap();
        store
            .add_full(FullIdentity {
                callsign: Callsign::parse("W1ABC").unwrap(),
                label: Some("Home".into()),
                has_cms_account: true,
                cms_registered: true,
            })
            .unwrap();
        store.save().unwrap();

        let value = resolver
            .resolve(&EntityRef::parse("@identity:W1ABC").unwrap())
            .await
            .unwrap();
        assert_eq!(value["callsign"], json!("W1ABC"));
        assert_eq!(value["label"], json!("Home"));
    }

    #[tokio::test]
    async fn resolves_tactical_identity_by_label() {
        let (_dir, resolver) = resolver_with_tempdirs();
        let mut store = IdentityStore::load(&resolver.identity_store_path).unwrap();
        store
            .add_full(FullIdentity {
                callsign: Callsign::parse("W1ABC").unwrap(),
                label: None,
                has_cms_account: true,
                cms_registered: true,
            })
            .unwrap();
        store
            .add_tactical(TacticalIdentity {
                label: "EOC-3".into(),
                parent: Callsign::parse("W1ABC").unwrap(),
                cms: TacticalCmsState::Unknown,
            })
            .unwrap();
        store.save().unwrap();

        let value = resolver
            .resolve(&EntityRef::parse("@identity:EOC-3").unwrap())
            .await
            .unwrap();
        assert_eq!(value["label"], json!("EOC-3"));
        assert_eq!(value["parent"], json!("W1ABC"));
    }

    #[tokio::test]
    async fn resolves_template_body_from_bundled_forms_catalog() {
        let (_dir, resolver) = resolver_with_tempdirs();
        let value = resolver
            .resolve(&EntityRef::parse("@template:ICS213_Initial").unwrap())
            .await
            .unwrap();
        assert_eq!(value["id"], json!("ICS213_Initial"));
        assert_eq!(value["name"], json!("ICS-213 General Message"));
        assert!(value["bodyTemplate"].is_string());
    }

    #[tokio::test]
    async fn unknown_name_is_unresolved_ref_per_kind() {
        let (_dir, resolver) = resolver_with_tempdirs();
        for token in [
            "@preset:no-such-preset",
            "@station-set:no-such-set",
            "@identity:W9NONE",
            "@template:No_Such_Form",
        ] {
            let err = resolver
                .resolve(&EntityRef::parse(token).unwrap())
                .await
                .unwrap_err();
            assert!(
                matches!(&err, SnapshotError::UnresolvedRef(t) if t == token),
                "expected UnresolvedRef({token:?}), got {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn unknown_kind_is_unresolved_ref_not_silently_passed_through() {
        let (_dir, resolver) = resolver_with_tempdirs();
        let err = resolver
            .resolve(&EntityRef::parse("@mystery-kind:whatever").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(&err, SnapshotError::UnresolvedRef(t) if t == "@mystery-kind:whatever"));
    }
}
