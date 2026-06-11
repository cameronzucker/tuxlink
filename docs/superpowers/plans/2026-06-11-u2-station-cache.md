# U2 — Persistent station-list cache (offline last-known-good)

> Unit U2 of Find-a-Station (umbrella `tuxlink-axq0`, bd `tuxlink-dx57`).
> Spec §6 of `docs/design/2026-06-10-find-a-station-propagation-map-design.md` (authoritative).
> "Small TDD-against-spec backend unit" (§9) — no cross-provider adrev ceremony.

**Goal:** persist the last-known-good per-mode station listing to disk so a cold,
offline launch shows stations immediately. Today `StationsCache` is in-memory only
(`Mutex<HashMap<CacheKey, StationListing>>`); a cold offline launch has nothing.

**Definition of done:** a cold offline launch (no network) shows the last-known-good
station list with the existing freshness caption ("as of <original fetch time>"),
with no blocking call and no modal.

**Backend-only.** The frontend already has the wire shape: `StationListing.fetchedAtMs`
exists in both the Rust struct and `src/catalog/stationTypes.ts`, and `StationResults.tsx`'s
`StaleCaption` already renders "as of HH:MM (cached — may be stale)" from it. U2 does NOT
change the DTO or any TS. It only makes the cache survive a restart.

**Grounding (verified 2026-06-11, current origin/main):**
- `StationsCache` (`src-tauri/src/catalog/stations_cache.rs`): `data: Mutex<HashMap<CacheKey, StationListing>>`, `attempts: Mutex<HashMap<CacheKey, u64>>`, `locks`, `ttl_ms`, `min_refetch_ms`, `clock: Arc<dyn Clock>`. Constructed `StationsCache::new(ttl, min_refetch, clock)` at `lib.rs:252-258`, `.manage()`d BEFORE `.setup()`.
- `CacheKey { mode: ListingMode, service_codes: String, history_hours: u32 }` — derives `Debug, Clone, PartialEq, Eq, Hash` (NO serde yet).
- `StationListing { mode, title, gateways, raw, parsed_ok, fetched_at_ms: Option<u64> }` — already `Serialize/Deserialize`, camelCase.
- "Cache only good parses": only `parsed_ok=true` listings reach the `Ok(mut listing)` insert arm of `get_or_fetch` (~lines 122-124), where `fetched_at_ms` is stamped `clock.now_millis()` then inserted. Errors hit the stale-on-error path and are never stored.
- Disk-store template (atomic .tmp→rename, `std::fs`, NOT tokio::fs): `favorites/store.rs::flush()`; resolved via `app.path().app_data_dir()` in `.setup()` (`lib.rs:297` arm, alongside `contacts.json` / `stations.json`).
- `Clock` trait: `fn now_millis(&self) -> u64`; tests use `MockClock(AtomicU64)` + `.advance(d)`.
- Corrupt-file quarantine pattern: `FavoritesStore::open()` (start empty + log on parse failure, never crash).
- Tests: `stations_cache.rs` uses `#[tokio::test]` + MockClock; `favorites/store.rs` tests use `tempfile::tempdir()` + reopen-after-write + corrupt-file.

**Reused:** `Clock`/`SystemClock` (stations_cache.rs), `app_data_dir()` setup arm, `tempfile` (dev-dep), `serde_json`.

---

## Task 1: On-disk format + persistence helper (load/save, atomic, quarantine)

**Files:** create `src-tauri/src/catalog/stations_disk.rs`; modify `stations_cache.rs` (add `#[derive(Serialize, Deserialize)]` to `CacheKey`); declare `mod stations_disk;` in `catalog/mod.rs`.

TDD. The on-disk shape (a JSON map keyed by a struct can't serialize, so use a Vec of entries):
```rust
#[derive(Serialize, Deserialize)]
struct PersistedCache {
    schema: String,                  // "tuxlink-station-cache-v1"
    entries: Vec<PersistedEntry>,
}
#[derive(Serialize, Deserialize)]
struct PersistedEntry {
    key: CacheKey,
    listing: StationListing,
    last_attempt_ms: Option<u64>,
}
```
- `pub fn load(path: &Path) -> (HashMap<CacheKey, StationListing>, HashMap<CacheKey, u64>)` — missing file → empty maps; unparseable → empty maps + `eprintln!` quarantine (never panic). Only restore entries whose `listing.parsed_ok` is true (defensive: never resurrect a bad parse).
- `pub fn save(path: &Path, data: &HashMap<CacheKey, StationListing>, attempts: &HashMap<CacheKey, u64>) -> std::io::Result<()>` — `create_dir_all(parent)`, `to_string_pretty`, write `.tmp`, `rename` (atomic; mirror FavoritesStore::flush exactly).

**Tests** (tempdir): round-trips a populated cache (entry's `fetched_at_ms` preserved exactly); missing file → empty; corrupt file (`"{not json"`) → empty + no panic; a `parsed_ok=false` entry in the file is dropped on load.

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog::stations_disk` + `cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings` (re-run clippy to exit 0).
Commit: `feat(catalog): on-disk station-listing cache format (load/save, atomic, quarantine)`.

## Task 2: Wire persistence into StationsCache

**Files:** modify `stations_cache.rs`.

TDD. Add an OPTIONAL persistence path so existing in-memory tests are unchanged:
- Add field `persist_path: Option<PathBuf>`.
- Keep `new(ttl, min_refetch, clock)` as-is (path None). Add `pub fn new_persistent(ttl, min_refetch, clock, path: PathBuf) -> Self` that calls `stations_disk::load(&path)` and seeds `data` + `attempts` from disk.
- After the successful insert in `get_or_fetch` (the `Ok` arm), call a private `fn persist(&self)` that snapshots `data` + `attempts` (brief locks, clone out) and calls `stations_disk::save(...)` when `persist_path` is `Some`. Persist errors are logged, never propagated (a cache that can't write disk still serves in-memory).

**Tests** (tempdir + MockClock + a fake fetch fn):
- `cold_load_serves_last_known_good`: pre-write a disk file with one good entry; `new_persistent` from it; call `get_or_fetch` with a FAILING fetch → returns the disk entry with its original `fetched_at_ms` (the core U2 value — offline cold start serves disk).
- `good_fetch_persists_to_disk`: `new_persistent` empty; `get_or_fetch` with an `Ok` fetch → the file now exists and reloads to the same entry.
- `failed_fetch_does_not_persist`: empty cache, failing fetch → no file / no entry written (only-good-parses).
- `in_memory_mode_unchanged`: `new(...)` (no path) still passes the existing behavior (no disk I/O).

Run the cache tests + clippy. Commit: `feat(catalog): persist station cache to disk; cold offline launch serves last-known-good`.

## Task 3: Wire into lib.rs .setup() (app_data_dir path + graceful fallback)

**Files:** modify `lib.rs`.

Move the `StationsCache` `.manage()` from the pre-`.setup()` chain (lib.rs:252-258) into the `.setup()` `app_data_dir()` arm (where contacts/favorites are opened): construct `StationsCache::new_persistent(30*60*1000, 15*60*1000, Arc::new(SystemClock), data_dir.join("station-listings-cache.json"))`. If `app_data_dir()` is unavailable (the existing `Err` arm), fall back to the in-memory `StationsCache::new(...)` so launch never breaks (mirror the existing degrade pattern). Ensure the cache is managed exactly once on every path.

Run: `cargo build` + full `cargo test --manifest-path src-tauri/Cargo.toml --lib catalog` + `cargo test --doc` + `cargo clippy --all-targets -- -D warnings` (the CI verify gate — incl. doctests, per the PR #575 lesson). Commit: `feat(catalog): register persistent station cache under app_data_dir (graceful fallback)`.

## Review loop (after Task 3)
Two perspectives (read-only): spec compliance (§6: offline-first, no modal, only-good-parses, honest "as of") + a quick correctness pass (lock-across-await safety, atomic write, corrupt-file resilience, the once-managed invariant). Fix, re-verify, push, PR.
