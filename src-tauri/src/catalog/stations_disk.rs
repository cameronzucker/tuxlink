//! Atomic on-disk persistence for the station-listing cache.
//!
//! Mirrors the `FavoritesStore::flush` / `FavoritesStore::open` hardened patterns:
//! - `save`: serialize → write `<path>.tmp` → rename (atomic; no partial writes).
//! - `load`: missing file → empty; unparseable → quarantine-log + empty (never panic).
//! - Entries whose `listing.parsed_ok == false` are dropped on load (never resurrect
//!   a bad parse into the UI).
//!
//! On-disk schema: a single JSON object with a `schema` discriminator and a
//! `Vec`-of-entries payload (a `HashMap<CacheKey, …>` can't round-trip as a JSON
//! object because `CacheKey` is a struct, not a string).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::stations::StationListing;
use super::stations_cache::CacheKey;

const SCHEMA: &str = "tuxlink-station-cache-v1";

#[derive(Serialize, Deserialize)]
struct PersistedCache {
    schema: String,
    entries: Vec<PersistedEntry>,
}

#[derive(Serialize, Deserialize)]
struct PersistedEntry {
    key: CacheKey,
    listing: StationListing,
    last_attempt_ms: Option<u64>,
}

/// Load the persisted cache from `path`.
///
/// - Missing file → both maps empty (not an error).
/// - Present but unparseable → `eprintln!` a quarantine note, both maps empty
///   (never panics, never deletes the file — the original bytes are preserved for
///   operator inspection, mirroring `FavoritesStore::open`).
/// - Any entry with `listing.parsed_ok == false` is silently dropped — never
///   resurrect a bad parse into the UI.
pub fn load(path: &Path) -> (HashMap<CacheKey, StationListing>, HashMap<CacheKey, u64>) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (HashMap::new(), HashMap::new()),
        Err(e) => {
            eprintln!(
                "stations_disk: failed to read {}: {e} — starting with empty cache",
                path.display()
            );
            return (HashMap::new(), HashMap::new());
        }
    };

    let persisted: PersistedCache = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "stations_disk: {} is unparseable, starting with empty cache (original preserved): {e}",
                path.display()
            );
            return (HashMap::new(), HashMap::new());
        }
    };

    let mut data: HashMap<CacheKey, StationListing> = HashMap::new();
    let mut attempts: HashMap<CacheKey, u64> = HashMap::new();

    for entry in persisted.entries {
        // Drop entries that did not parse OK — never resurrect a bad parse.
        if !entry.listing.parsed_ok {
            continue;
        }
        if let Some(ms) = entry.last_attempt_ms {
            attempts.insert(entry.key.clone(), ms);
        }
        data.insert(entry.key, entry.listing);
    }

    (data, attempts)
}

/// Atomically persist the cache.
///
/// Build a `PersistedCache` from the two maps (zipping by key), serialize to
/// pretty JSON, `create_dir_all(parent)`, write to `<path>.tmp`, then
/// `rename` over `path`. Mirrors `FavoritesStore::flush` exactly.
pub fn save(
    path: &Path,
    data: &HashMap<CacheKey, StationListing>,
    attempts: &HashMap<CacheKey, u64>,
) -> std::io::Result<()> {
    let entries: Vec<PersistedEntry> = data
        .iter()
        .map(|(key, listing)| PersistedEntry {
            key: key.clone(),
            listing: listing.clone(),
            last_attempt_ms: attempts.get(key).copied(),
        })
        .collect();

    let cache = PersistedCache {
        schema: SCHEMA.to_string(),
        entries,
    };

    let json = serde_json::to_string_pretty(&cache)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // "<name>.tmp" — same suffix logic as FavoritesStore::flush (NOT with_extension).
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "station_cache.json".to_string());
    let tmp = path.with_file_name(format!("{name}.tmp"));

    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::stations::{Gateway, ListingMode};
    use tempfile::tempdir;

    // ---- helpers ----------------------------------------------------------------

    fn key(mode: ListingMode) -> CacheKey {
        CacheKey {
            mode,
            service_codes: "PUBLIC".to_string(),
            history_hours: 168,
        }
    }

    fn good_listing(mode: ListingMode) -> StationListing {
        StationListing {
            mode,
            title: Some("WINLINK VARA HF CHANNEL LISTING".to_string()),
            gateways: vec![Gateway {
                channel: "AI4Y.WINLINK".to_string(),
                callsign: "AI4Y".to_string(),
                sysop_name: Some("Test Sysop".to_string()),
                grid: Some("FM07CC".to_string()),
                location: Some("Wirtz, VA".to_string()),
                frequencies_khz: vec![14105.0],
                last_update: Some("Sat, 06 Jun 2026 08:47:00 GMT".to_string()),
                email: Some("test@example.com".to_string()),
                homepage: None,
                antenna: None,
            }],
            raw: "raw body text".to_string(),
            parsed_ok: true,
            fetched_at_ms: Some(1_000),
        }
    }

    fn bad_listing(mode: ListingMode) -> StationListing {
        StationListing {
            mode,
            title: None,
            gateways: vec![],
            raw: "<!DOCTYPE html>not a listing".to_string(),
            parsed_ok: false,
            fetched_at_ms: Some(2_000),
        }
    }

    // ---- RED tests (written first; will fail until implementation exists) ------

    /// A populated data map + attempts map survives a save+load round-trip,
    /// including exact `fetched_at_ms` and attempt timestamps.
    #[test]
    fn round_trips_a_populated_cache() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("station_cache.json");

        let mut data = HashMap::new();
        let mut attempts = HashMap::new();

        let k1 = key(ListingMode::VaraHf);
        let k2 = key(ListingMode::ArdopHf);
        let l1 = good_listing(ListingMode::VaraHf);
        let l2 = good_listing(ListingMode::ArdopHf);

        data.insert(k1.clone(), l1.clone());
        data.insert(k2.clone(), l2.clone());
        attempts.insert(k1.clone(), 9_000_u64);
        // k2 intentionally has no attempt entry

        save(&path, &data, &attempts).expect("save must succeed");

        let (loaded_data, loaded_attempts) = load(&path);

        assert_eq!(loaded_data.len(), 2, "both entries must round-trip");
        let r1 = loaded_data.get(&k1).expect("VaraHf entry missing after load");
        assert_eq!(r1.fetched_at_ms, Some(1_000), "fetched_at_ms must survive the round-trip");
        assert_eq!(r1.gateways.len(), 1, "gateways must survive");
        assert_eq!(r1.gateways[0].callsign, "AI4Y");

        assert_eq!(loaded_attempts.get(&k1), Some(&9_000_u64), "attempt ms must round-trip");
        assert_eq!(loaded_attempts.get(&k2), None, "key with no attempt must remain absent");
    }

    /// Loading from a non-existent path returns two empty maps without panicking.
    #[test]
    fn missing_file_loads_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does_not_exist.json");

        let (data, attempts) = load(&path);
        assert!(data.is_empty(), "data must be empty for a missing file");
        assert!(attempts.is_empty(), "attempts must be empty for a missing file");
    }

    /// Corrupt JSON content is quarantined (logged) and both maps come back empty,
    /// no panic, original file is NOT deleted.
    #[test]
    fn corrupt_file_quarantines_to_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("station_cache.json");
        std::fs::write(&path, b"{ not valid json at all").unwrap();

        let (data, attempts) = load(&path);
        assert!(data.is_empty(), "data must be empty after corrupt-file quarantine");
        assert!(attempts.is_empty(), "attempts must be empty after corrupt-file quarantine");

        // File must still exist — load never deletes it.
        assert!(path.exists(), "load must NOT delete the corrupt file");
    }

    /// An entry with `parsed_ok: false` is dropped on load; `parsed_ok: true` survives.
    #[test]
    fn dropped_bad_parse_on_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("station_cache.json");

        let k_good = key(ListingMode::VaraHf);
        let k_bad = key(ListingMode::ArdopHf);

        let mut data = HashMap::new();
        data.insert(k_good.clone(), good_listing(ListingMode::VaraHf));
        data.insert(k_bad.clone(), bad_listing(ListingMode::ArdopHf));
        let attempts = HashMap::new();

        save(&path, &data, &attempts).expect("save must succeed");

        let (loaded_data, _) = load(&path);
        assert_eq!(loaded_data.len(), 1, "bad-parse entry must be dropped on load");
        assert!(loaded_data.contains_key(&k_good), "good entry must survive");
        assert!(!loaded_data.contains_key(&k_bad), "bad-parse entry must be absent");
    }

    /// After `save`, the `.tmp` sibling must not exist and the real file must exist.
    #[test]
    fn save_is_atomic_no_tmp_left() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("station_cache.json");

        let mut data = HashMap::new();
        data.insert(key(ListingMode::VaraHf), good_listing(ListingMode::VaraHf));
        save(&path, &data, &HashMap::new()).expect("save must succeed");

        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "no .tmp file must remain after save");
        assert!(path.exists(), "the real file must exist after save");
    }
}
