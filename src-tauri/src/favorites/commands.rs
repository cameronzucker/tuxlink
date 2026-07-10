//! Favorites tauri commands + id/timestamp stamping + the M12 merge — Task B2.
//!
//! Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → "Locked
//! decisions" + "### Task B2". The [`store`](super::store) layer owns the
//! durable mechanics (atomic flush, recents trim, log orphan-sweep, ToD); THIS
//! (command) layer is the thin IPC surface that stamps ids/timestamps and routes
//! the upsert through the M12 anti-clobber merge.
//!
//! Mirrors the just-completed `contacts/commands.rs` (A2) for signature style:
//! every command takes a managed `State<Arc<Mutex<FavoritesStore>>>`; the lock
//! is scoped in a block and dropped before return; `.expect("…poisoned")` is the
//! accepted convention.
//!
//! **M12 — the key correctness fix.** `favorite_upsert` MERGES only the
//! operator-editable fields (`gateway`, `freq`, `transport`, `band`, `grid`,
//! `note`) into the existing record by id, PRESERVING `starred`, `created_at`,
//! `last_attempt_at`, `mode`, and the connection log. `favorite_star` and
//! `favorite_record_attempt` are the ONLY writers of `starred` /
//! `last_attempt_at` / the log. A STALE whole-object upsert therefore can never
//! revert a concurrent star or rewind the dial clock — the protected fields are
//! read from the LIVE record, never the caller's payload.
//!
//! **No cross-window event (intentional, YAGNI).** Unlike contacts (which fan
//! out to a separate Compose window via `contacts:changed`), favorites are a
//! single-window radio-dock surface — there is no second window observing the
//! same store, so no `favorites:changed` event is emitted.

use std::sync::{Arc, Mutex};

use super::store::{
    Favorite, FavoriteDial, FavoritesError, FavoritesStore, StationsFile, TodHint,
};

/// The radio modes a favorite may belong to (mirrors the frontend `RadioMode`
/// union: `vara-hf` | `vara-fm` | `ardop-hf` | `packet` | `telnet`). An upsert
/// carrying any other mode string is rejected at the command boundary so a
/// typo / future-mode payload can't silently create an unreachable recent.
const VALID_MODES: [&str; 5] = ["vara-hf", "vara-fm", "ardop-hf", "packet", "telnet"];

/// Mint a fresh uuid-v4 string id (mirrors `contacts/commands.rs::new_id`).
fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The current wall-clock instant as an RFC3339 UTC string. Used only for the
/// server-side `created_at` / `updated_at` bookkeeping stamps — NEVER for
/// `ts_local`, which is the offset-bearing local timestamp built by the frontend
/// (H1: that one is stored verbatim, never generated or converted here).
fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Reject an unknown mode string before it reaches the store.
fn validate_mode(mode: &str) -> Result<(), FavoritesError> {
    if VALID_MODES.contains(&mode) {
        Ok(())
    } else {
        Err(FavoritesError::Validation(format!("unknown mode: {mode:?}")))
    }
}

/// Read the whole stations file (favorites + log + schema_version).
#[tauri::command]
pub fn favorites_read(
    svc: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<StationsFile, FavoritesError> {
    let store = svc.lock().expect("favorites store mutex poisoned");
    Ok(store.file().clone())
}

/// Insert a NEW favorite or MERGE operator edits into an existing one (M12).
///
/// - **New** (empty `id`, or an `id` not present in the store): mint a uuid-v4
///   id, stamp `created_at == updated_at == now`, force `starred:false`, and
///   persist the caller's fields. (`favorite_star` is the only way to star.)
/// - **Existing** (`id` present): merge ONLY `gateway`/`freq`/`transport`/
///   `band`/`grid`/`note` + bump `updated_at`; `starred`, `created_at`,
///   `last_attempt_at`, `mode`, and the log are preserved from the LIVE record.
///   A stale whole-object payload cannot clobber a concurrent star (M12).
///
/// Returns the STORED favorite so the caller learns the assigned id + the
/// preserved/merged field values.
#[tauri::command]
pub fn favorite_upsert(
    favorite: Favorite,
    svc: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<Favorite, FavoritesError> {
    validate_mode(&favorite.mode)?;
    let mut store = svc.lock().expect("favorites store mutex poisoned");

    // EXISTING record → merge editable fields only (M12). A non-empty id that is
    // actually present takes the merge path; anything else is a fresh mint.
    if !favorite.id.trim().is_empty() {
        if let Some(merged) = store.favorite_merge_editable(&favorite, now_utc())? {
            return Ok(merged);
        }
    }

    // NEW record → mint id + stamp timestamps + force starred:false.
    let now = now_utc();
    let mut fresh = favorite;
    fresh.id = new_id();
    fresh.starred = false;
    fresh.last_attempt_at = None;
    fresh.created_at = now.clone();
    fresh.updated_at = now;
    store.favorite_upsert(fresh.clone())?;
    Ok(fresh)
}

/// Delete a favorite by id (no-op if absent). The store also sweeps that unit's
/// orphaned `ConnectionAttempt`s from the log (M2).
#[tauri::command]
pub fn favorite_delete(
    id: String,
    svc: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<(), FavoritesError> {
    let mut store = svc.lock().expect("favorites store mutex poisoned");
    store.favorite_delete(&id)
}

/// Set/clear a favorite's `starred` flag (no-op if the id is absent). This is the
/// ONLY writer of `starred` — a starred recent survives the recents cap
/// (star-to-promote). Bumps `updated_at`.
#[tauri::command]
pub fn favorite_star(
    id: String,
    starred: bool,
    svc: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<(), FavoritesError> {
    let mut store = svc.lock().expect("favorites store mutex poisoned");
    store.favorite_star(&id, starred, now_utc())
}

/// Record an empirical connection attempt against the unit identified by `dial`
/// (H3/Codex#8). The store finds-or-creates the `(mode, gateway, freq|transport)`
/// recent, server-stamps the attempt's `unit_id`, bumps `last_attempt_at`,
/// appends the attempt, then trims/sweeps.
///
/// `ts_local` is the OFFSET-BEARING local timestamp built by the FRONTEND (M4) —
/// it is stored VERBATIM, never generated or converted to UTC here (H1).
/// `outcome` is `"reached"` | `"failed"`.
#[tauri::command]
pub fn favorite_record_attempt(
    dial: FavoriteDial,
    outcome: String,
    ts_local: String,
    svc: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<(), FavoritesError> {
    validate_mode(&dial.mode)?;
    let mut store = svc.lock().expect("favorites store mutex poisoned");
    store.record_attempt(dial, outcome, ts_local, new_id, now_utc())
}

/// Mode-filtered recents (NON-starred favorites of `mode`, most-recently dialed
/// first).
#[tauri::command]
pub fn favorites_recents(
    mode: String,
    svc: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<Vec<Favorite>, FavoritesError> {
    let store = svc.lock().expect("favorites store mutex poisoned");
    Ok(store.favorites_recents(&mode))
}

/// Return the gated time-of-day hint for the unit `unit_id`, or None.
///
/// Reads that unit's recorded attempts from the store and runs the EXISTING
/// `tod_hint` gate (≥3 attempts, ≥1 success, strict unique max — H2; offset-local
/// bucketing — H1). The whole honesty gate lives in Rust by design; this command
/// is a thin read-only IPC shim so the B5 frontend never re-implements the
/// bucketing in JS. An unknown `unit_id` yields an empty attempt set → None.
#[tauri::command]
pub fn favorite_tod_hint(
    unit_id: String,
    svc: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<Option<TodHint>, FavoritesError> {
    let store = svc.lock().expect("favorites store mutex poisoned");
    Ok(super::store::tod_hint(&store.attempts_for(&unit_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn favorite(id: &str, mode: &str, gateway: &str) -> Favorite {
        Favorite {
            id: id.to_string(),
            mode: mode.to_string(),
            gateway: gateway.to_string(),
            freq: Some("14105.0".to_string()),
            transport: None,
            band: Some("20m".to_string()),
            grid: Some("CN87".to_string()),
            note: None,
            peer_id: None,
            starred: false,
            last_attempt_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    // ---- mode validation ----------------------------------------------------

    #[test]
    fn validate_mode_accepts_known_modes() {
        for m in VALID_MODES {
            assert!(validate_mode(m).is_ok(), "{m} must be accepted");
        }
    }

    #[test]
    fn validate_mode_rejects_unknown_mode() {
        let err = validate_mode("bogus-mode").unwrap_err();
        match err {
            FavoritesError::Validation(msg) => assert!(msg.contains("bogus-mode")),
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    // ---- id / timestamp stamping (new mint) --------------------------------

    #[test]
    fn new_id_is_nonempty_and_unique() {
        let a = new_id();
        let b = new_id();
        assert!(!a.is_empty() && !b.is_empty());
        assert_ne!(a, b, "uuid-v4 ids must be unique");
    }

    // The command-layer upsert is tested directly by constructing a store in a
    // tempdir and calling the inner logic via the store API — the thin
    // `#[tauri::command]` wrapper only adds State extraction. To exercise the
    // mint/merge decision without a Tauri harness, we replicate the command body
    // against a raw store here.

    /// Run the same new-vs-merge decision the `favorite_upsert` command makes,
    /// against a raw store (no Tauri State). Keeps the M12 assertion harness-free.
    fn upsert_via_command_logic(
        store: &mut FavoritesStore,
        favorite: Favorite,
        now: &str,
    ) -> Favorite {
        validate_mode(&favorite.mode).expect("valid mode in test");
        if !favorite.id.trim().is_empty() {
            if let Some(merged) = store
                .favorite_merge_editable(&favorite, now.to_string())
                .unwrap()
            {
                return merged;
            }
        }
        let mut fresh = favorite;
        fresh.id = "minted-id".to_string();
        fresh.starred = false;
        fresh.last_attempt_at = None;
        fresh.created_at = now.to_string();
        fresh.updated_at = now.to_string();
        store.favorite_upsert(fresh.clone()).unwrap();
        fresh
    }

    #[test]
    fn upsert_new_mints_id_and_equal_timestamps_and_unstarred() {
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::open(dir.path().join("stations.json"));
        let mut input = favorite("", "ardop-hf", "W6XYZ");
        input.starred = true; // a caller cannot star via upsert — forced false.
        let stored = upsert_via_command_logic(&mut store, input, "2026-06-08T12:00:00+00:00");
        assert_eq!(stored.id, "minted-id", "empty id is minted");
        assert!(!stored.starred, "a new favorite is never starred via upsert");
        assert!(stored.last_attempt_at.is_none());
        assert_eq!(stored.created_at, stored.updated_at, "new: created == updated");
        assert_eq!(stored.created_at, "2026-06-08T12:00:00+00:00");
    }

    #[test]
    fn upsert_preserves_starred_and_created_at() {
        // M12 (the key correctness fix): a STALE favorite_upsert carrying
        // starred:false over an already-starred favorite leaves starred:true and
        // preserves created_at + last_attempt_at + the log. This proves a stale
        // whole-object upsert can't clobber a concurrent star.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);

        // 1. Seed a recent + an attempt via the record path, then star it. This
        //    gives us a real starred favorite with a created_at, a
        //    last_attempt_at, AND a linked log entry.
        store
            .record_attempt(
                FavoriteDial {
                    mode: "ardop-hf".to_string(),
                    gateway: "W6XYZ".to_string(),
                    freq: Some("14105.0".to_string()),
                    transport: None,
                    band: Some("20m".to_string()),
                    grid: Some("CN87".to_string()),
                    peer_id: None,
                },
                "reached".to_string(),
                "2026-06-07T10:00:00-07:00".to_string(),
                || "fav-1".to_string(),
                "2026-06-07T17:00:00+00:00".to_string(),
            )
            .unwrap();
        let fav_id = store.favorites()[0].id.clone();
        let original_created = store.favorites()[0].created_at.clone();
        let original_last_attempt = store.favorites()[0].last_attempt_at.clone();
        assert!(original_last_attempt.is_some(), "precondition: was dialed");
        assert_eq!(store.attempts_for(&fav_id).len(), 1, "precondition: has a log entry");

        store
            .favorite_star(&fav_id, true, "2026-06-07T18:00:00+00:00".to_string())
            .unwrap();
        assert!(store.favorites()[0].starred, "precondition: starred");

        // 2. Issue a STALE whole-object upsert: starred:false, no last_attempt_at,
        //    a bogus created_at, and only an edited `note`.
        let stale = Favorite {
            id: fav_id.clone(),
            mode: "ardop-hf".to_string(),
            gateway: "W6XYZ".to_string(),
            freq: Some("14105.0".to_string()),
            transport: None,
            band: Some("20m".to_string()),
            grid: Some("CN87".to_string()),
            note: Some("edited note".to_string()),
            peer_id: None,
            starred: false,                                       // STALE
            last_attempt_at: None,                                // STALE
            created_at: "1999-01-01T00:00:00+00:00".to_string(), // STALE
            updated_at: String::new(),
        };
        let result =
            upsert_via_command_logic(&mut store, stale, "2026-06-08T12:00:00+00:00");

        // 3. The protected fields survived the stale upsert.
        assert!(result.starred, "starred:true must survive a stale upsert (M12)");
        assert_eq!(result.created_at, original_created, "created_at preserved (M12)");
        assert_eq!(
            result.last_attempt_at, original_last_attempt,
            "last_attempt_at preserved (M12)"
        );
        // The edited field DID land.
        assert_eq!(result.note.as_deref(), Some("edited note"), "the new note is merged");
        // updated_at advanced.
        assert_eq!(result.updated_at, "2026-06-08T12:00:00+00:00");
        // The log entry was NOT clobbered.
        assert_eq!(
            store.attempts_for(&fav_id).len(),
            1,
            "the connection log must survive a stale upsert (M12)"
        );
        // And the live store agrees.
        let live = &store.favorites()[0];
        assert!(live.starred);
        assert_eq!(live.created_at, original_created);
    }

    // ---- favorite_tod_hint delegation contract ------------------------------

    /// Documents that `favorite_tod_hint`'s body delegates correctly to
    /// `store::tod_hint` without re-implementing the bucketing logic. Exercises
    /// the command body against a raw store (no Tauri harness needed — the
    /// `#[tauri::command]` wrapper only adds State extraction).
    #[test]
    fn tod_hint_command_delegates_to_store() {
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::open(dir.path().join("stations.json"));

        // Record 3 `reached` attempts on the same dial at offset-local night
        // hours (23:00, 22:00, 21:00 at -07:00 → local hour falls in "night").
        let dial = FavoriteDial {
            mode: "ardop-hf".to_string(),
            gateway: "W6XYZ".to_string(),
            freq: Some("14105.0".to_string()),
            transport: None,
            band: Some("20m".to_string()),
            grid: Some("CN87".to_string()),
            peer_id: None,
        };
        store
            .record_attempt(
                dial.clone(),
                "reached".to_string(),
                "2026-06-07T23:00:00-07:00".to_string(),
                || "u1".to_string(),
                "2026-06-08T06:00:00+00:00".to_string(),
            )
            .unwrap();
        store
            .record_attempt(
                dial.clone(),
                "reached".to_string(),
                "2026-06-07T22:00:00-07:00".to_string(),
                || "u1".to_string(),
                "2026-06-08T05:00:00+00:00".to_string(),
            )
            .unwrap();
        store
            .record_attempt(
                dial.clone(),
                "reached".to_string(),
                "2026-06-07T21:00:00-07:00".to_string(),
                || "u1".to_string(),
                "2026-06-08T04:00:00+00:00".to_string(),
            )
            .unwrap();

        let unit_id = store.favorites()[0].id.clone();

        // Replicate the command body: lock (raw store here) → attempts_for → tod_hint.
        // `crate::favorites::store::tod_hint` is the same fn the production command
        // calls as `super::store::tod_hint` (super = favorites from the command level,
        // super = commands from inside this test module).
        let hint = crate::favorites::store::tod_hint(&store.attempts_for(&unit_id));
        let hint = hint.expect("3 night reached attempts → Some");
        assert_eq!(hint.bucket, "night", "offset-local bucketing (H1): 23:00-07:00 = night");
        assert_eq!(hint.attempts, 3);
        assert_eq!(hint.successes, 3);

        // An unknown unit_id yields empty attempts → None (the command's unknown-id contract).
        let none_hint = crate::favorites::store::tod_hint(&store.attempts_for("nope"));
        assert!(none_hint.is_none(), "unknown unit_id → empty attempts → None");
    }
}
