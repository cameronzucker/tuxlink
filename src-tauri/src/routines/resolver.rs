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
    /// The saved-form slot library, for `@draft:`. `None` when it is not
    /// available — it is opened at launch as a sibling SQLite file and that
    /// open is deliberately non-fatal (`lib.rs`), so a session can legitimately
    /// run without it. A `@draft:` reference then fails loudly at resolve time
    /// rather than the whole routines subsystem refusing to start.
    ///
    /// Passed in as the already-managed handle rather than opened here by path:
    /// `DraftLibrary::open` CREATES the database when it is absent, and a
    /// resolver reading a reference must not bring the operator's saved-form
    /// store into existence as a side effect.
    drafts: Option<Arc<crate::forms::draft_library::DraftLibrary>>,
}

impl MonolithEntityResolver {
    pub fn new(
        presets: Arc<RadioPresetStore>,
        station_sets: Arc<StationSetStore>,
        identity_store_path: PathBuf,
        drafts: Option<Arc<crate::forms::draft_library::DraftLibrary>>,
    ) -> Self {
        Self {
            presets,
            station_sets,
            identity_store_path,
            drafts,
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
            "draft" => self.resolve_draft(&entity.name),
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
    /// `@draft:<slot_id>` — a form the operator has already FILLED IN and saved
    /// in the compose window, so a routine can file it rather than only being
    /// able to send an empty template.
    ///
    /// ## Why the address is the slot id and never the label
    ///
    /// `slot_id` is a UUID v4 and the PRIMARY KEY of `form_draft_slots`
    /// (`forms/draft_library.rs`), so it is globally unique on its own — no
    /// `form_id` qualifier, no disambiguation. Labels are NOT unique and are
    /// listed per form, so accepting one as an address would reintroduce the
    /// guess-then-get-rejected loop that `tuxlink-0rc3h` removed from the tool
    /// surface. The label comes BACK in the resolved value for readability; it
    /// is never accepted as input.
    ///
    /// ## Resolved live, not pinned
    ///
    /// The draft is read at run start, so editing the saved check-in in the
    /// form UI changes what tomorrow's run sends. That is the same behaviour
    /// `@preset`, `@station-set` and `@identity` already have, and the run is
    /// still reproducible after the fact because the engine journals the
    /// resolved snapshot (`RunStarted`). Pinning would turn a reference into a
    /// fork: the form UI would keep showing "Morning Check-In" while the
    /// routine executed an invisible copy of it.
    ///
    /// ## The shape, and why it is the whole template
    ///
    /// It resolves to `local.compose`'s `template` object — id, name,
    /// templates — PLUS the saved `values` and non-authoritative `draft`
    /// metadata. One token therefore carries both the form and its answers, so
    /// a step writes `"template": "@draft:<slot_id>"` and cannot get the pair
    /// out of step. Splitting it (a `@template:` beside a separate values
    /// reference) would let a routine name form A and fill it with answers
    /// saved against form B.
    fn resolve_draft(&self, slot_id: &str) -> Result<serde_json::Value, SnapshotError> {
        let token = format!("@draft:{slot_id}");
        let Some(drafts) = self.drafts.as_ref() else {
            return Err(SnapshotError::Io(format!(
                "{token}: the saved-form library is unavailable in this session, \
                 so a saved draft cannot be read"
            )));
        };
        let slot = drafts
            .get(slot_id)
            .map_err(|e| SnapshotError::Io(format!("{token}: reading the draft library: {e}")))?
            .ok_or_else(|| SnapshotError::UnresolvedRef(token.clone()))?;

        // The draft names a form we no longer bundle. It resolved as an id and
        // still cannot be sent, and the operator cannot work that out from the
        // token, so say which form is missing. Refusing is the only correct
        // answer: falling back to an empty form would transmit structurally
        // plausible garbage, and falling back to the unfilled template would
        // hide the broken reference while changing what the message says.
        let form = find_form(&slot.form_id).ok_or_else(|| {
            SnapshotError::UnresolvedRef(format!(
                "{token} (\"{label}\") is saved against form \"{form_id}\", which this build \
                 does not have. Re-save the draft against a bundled form.",
                label = slot.label,
                form_id = slot.form_id,
            ))
        })?;

        Ok(json!({
            "id": form.id,
            "name": form.name,
            "subjectTemplate": form.subject_template,
            "bodyTemplate": form.body_template,
            "values": slot.payload,
            // Non-authoritative: for display and for the journal, so a run can
            // be read back knowing WHICH saved draft it used and when that
            // draft last changed. Nothing branches on it.
            "draft": {
                "slotId": slot.slot_id,
                "label": slot.label,
                "updatedAt": slot.updated_at,
            },
        }))
    }

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
        let (dir, resolver, _) = resolver_with_drafts();
        (dir, resolver)
    }

    /// A resolver over tempdir stores INCLUDING a real (empty) draft library,
    /// handed back so a test can seed it.
    fn resolver_with_drafts() -> (
        tempfile::TempDir,
        MonolithEntityResolver,
        Arc<crate::forms::draft_library::DraftLibrary>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let presets = Arc::new(RadioPresetStore::open(
            dir.path().join("radio-presets.json"),
        ));
        let station_sets = Arc::new(StationSetStore::open(dir.path().join("station-sets.json")));
        let identity_store_path = dir.path().join("identities.json");
        let drafts = Arc::new(
            crate::forms::draft_library::DraftLibrary::open(dir.path().join("drafts.db")).unwrap(),
        );
        let resolver = MonolithEntityResolver::new(
            presets,
            station_sets,
            identity_store_path,
            Some(drafts.clone()),
        );
        (dir, resolver, drafts)
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

    // ── @draft: — a form the operator already filled in (tuxlink-3ddk2) ──

    #[tokio::test]
    async fn a_draft_resolves_to_the_whole_form_plus_its_saved_answers() {
        let (_dir, resolver, drafts) = resolver_with_drafts();
        let slot = drafts
            .upsert(
                None,
                "Winlink_Check-In".into(),
                "Cascadia Morning Net".into(),
                json!({"organization": "Cascadia Net", "msgto": "NET CONTROL",
                       "status": "EXERCISE", "band": "40m"}),
            )
            .unwrap();

        let value = resolver
            .resolve(&EntityRef::parse(&format!("@draft:{}", slot.slot_id)).unwrap())
            .await
            .unwrap();

        // The form half: this IS local.compose's `template` object, so one
        // token supplies both the form and its answers and they cannot get
        // out of step.
        assert_eq!(value["id"], json!("Winlink_Check-In"));
        assert_eq!(value["name"], json!("Winlink Check-In"));
        assert!(value["bodyTemplate"].as_str().unwrap().contains("<var"));
        // The answers half.
        assert_eq!(value["values"]["msgto"], json!("NET CONTROL"));
        assert_eq!(value["values"]["organization"], json!("Cascadia Net"));
        // Non-authoritative metadata, for display and for the journal.
        assert_eq!(value["draft"]["label"], json!("Cascadia Morning Net"));
        assert_eq!(value["draft"]["slotId"], json!(slot.slot_id));
        assert!(value["draft"]["updatedAt"].is_string());
    }

    /// A deleted draft fails the run. It must never fall back to an empty
    /// form (structurally plausible garbage), to the unfilled template (hides
    /// the broken reference while changing what the message says), or to a
    /// last-known copy (silently converts a live reference into a pinned one
    /// at the exact moment of failure).
    #[tokio::test]
    async fn a_missing_draft_refuses_rather_than_falling_back() {
        let (_dir, resolver, drafts) = resolver_with_drafts();
        let slot = drafts
            .upsert(
                None,
                "Winlink_Check-In".into(),
                "Gone".into(),
                json!({"msgto": "NET CONTROL"}),
            )
            .unwrap();
        drafts.delete(&slot.slot_id).unwrap();

        let token = format!("@draft:{}", slot.slot_id);
        let err = resolver
            .resolve(&EntityRef::parse(&token).unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(&err, SnapshotError::UnresolvedRef(t) if *t == token),
            "got {err:?}"
        );
    }

    /// The draft exists and still cannot be used. The token alone cannot tell
    /// the operator why, so the message names the form that is missing.
    #[tokio::test]
    async fn a_draft_for_a_form_we_no_longer_bundle_says_which_form() {
        let (_dir, resolver, drafts) = resolver_with_drafts();
        let slot = drafts
            .upsert(
                None,
                "Retired_Form_2019".into(),
                "Old Net".into(),
                json!({"msgto": "NET CONTROL"}),
            )
            .unwrap();

        let err = resolver
            .resolve(&EntityRef::parse(&format!("@draft:{}", slot.slot_id)).unwrap())
            .await
            .unwrap_err();
        let SnapshotError::UnresolvedRef(detail) = &err else {
            panic!("expected UnresolvedRef, got {err:?}");
        };
        assert!(detail.contains("Retired_Form_2019"), "detail: {detail}");
        assert!(detail.contains("Old Net"), "detail: {detail}");
    }

    /// A label is not an address. Labels are not unique and are listed per
    /// form, so accepting one would reintroduce the guess-then-get-rejected
    /// loop the tool-surface work removed.
    #[tokio::test]
    async fn a_label_is_refused_as_an_address() {
        let (_dir, resolver, drafts) = resolver_with_drafts();
        drafts
            .upsert(
                None,
                "Winlink_Check-In".into(),
                "Cascadia Morning Net".into(),
                json!({"msgto": "NET CONTROL"}),
            )
            .unwrap();

        let err = resolver
            .resolve(&EntityRef::parse("@draft:Cascadia Morning Net").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(&err, SnapshotError::UnresolvedRef(_)), "got {err:?}");
    }

    /// Two drafts may share a label. The slot id is the primary key, so each
    /// still addresses exactly one record — which is why the address is flat
    /// and needs no form qualifier.
    #[tokio::test]
    async fn two_drafts_sharing_a_label_still_address_distinctly() {
        let (_dir, resolver, drafts) = resolver_with_drafts();
        let a = drafts
            .upsert(
                None,
                "Winlink_Check-In".into(),
                "Evening Net".into(),
                json!({"msgto": "ALPHA"}),
            )
            .unwrap();
        let b = drafts
            .upsert(
                None,
                "ICS213_Initial".into(),
                "Evening Net".into(),
                json!({"subjectline": "BRAVO"}),
            )
            .unwrap();
        assert_ne!(a.slot_id, b.slot_id);

        let va = resolver
            .resolve(&EntityRef::parse(&format!("@draft:{}", a.slot_id)).unwrap())
            .await
            .unwrap();
        let vb = resolver
            .resolve(&EntityRef::parse(&format!("@draft:{}", b.slot_id)).unwrap())
            .await
            .unwrap();
        assert_eq!(va["id"], json!("Winlink_Check-In"));
        assert_eq!(va["values"]["msgto"], json!("ALPHA"));
        assert_eq!(vb["id"], json!("ICS213_Initial"));
        assert_eq!(vb["values"]["subjectline"], json!("BRAVO"));
    }

    /// The library failed to open at launch (a non-fatal condition in lib.rs).
    /// A `@draft:` reference says so rather than looking like a missing draft.
    #[tokio::test]
    async fn no_draft_library_is_reported_as_such_not_as_a_missing_draft() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = MonolithEntityResolver::new(
            Arc::new(RadioPresetStore::open(dir.path().join("p.json"))),
            Arc::new(StationSetStore::open(dir.path().join("s.json"))),
            dir.path().join("identities.json"),
            None,
        );
        let err = resolver
            .resolve(&EntityRef::parse("@draft:0f3c-whatever").unwrap())
            .await
            .unwrap_err();
        match err {
            SnapshotError::Io(msg) => assert!(msg.contains("unavailable"), "msg: {msg}"),
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
