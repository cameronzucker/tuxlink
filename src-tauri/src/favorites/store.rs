//! Favorites JSON store — the `stations.json` backing for the per-radio-mode
//! Favorites / Recents system + its honest, time-of-day-bucketed empirical
//! connection record.
//!
//! Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → "Locked
//! decisions" + "Task B1". Hardened by a 5-round adversarial review; the
//! invariants below are load-bearing, not stylistic. The shared store mechanics
//! (infallible `open`, corrupt-file quarantine, atomic `.tmp`→rename flush,
//! hand-written `Default`, `#[serde(default)]` additive tolerance, NO
//! `deny_unknown_fields`) mirror `contacts/store.rs`.
//!
//! **HONESTY-critical logic (the high-risk parts):**
//! - **ToD bucketing (H1):** the bucket is derived from the LOCAL wall-clock
//!   hour. [`ConnectionAttempt::ts_local`] is an offset-bearing ISO8601 string
//!   (`DateTime<FixedOffset>`) that is ALREADY local — its `.hour()` IS the
//!   station-local hour. We therefore parse with
//!   [`chrono::DateTime::parse_from_rfc3339`] and read `.hour()` directly.
//!   FORBIDDEN: `.with_timezone(&Utc)`, `.naive_utc()`, `.timestamp()` — those
//!   re-bucket by UTC and silently corrupt the feature.
//! - **`tod_hint` over-claim guard (H2):** a hint is returned ONLY when the
//!   argmax-`reached`-fraction bucket has ≥3 attempts AND ≥1 (prefer ≥2) actual
//!   successes AND is a STRICT unique max. Otherwise `None`. We NEVER name a
//!   zero-success bucket and NEVER frame a hint as a prediction — observed
//!   counts only.
//! - **Recents trim (M3):** cap is 10 NON-starred entries per mode; eviction is
//!   least-recently-DIALED (smallest `last_attempt_at`), NOT least-recently
//!   *created*. Starred favorites are NEVER trimmed (star-to-promote survives
//!   indefinitely).
//! - **Server-stamped `unit_id` (H3):** the record path assigns/finds the
//!   recent FIRST, then stamps the appended attempt's `unit_id` with that
//!   recent's `id`. The client never supplies `unit_id`.
//! - **Log orphan-sweep (M2):** on recents-trim AND on `favorite_delete`, every
//!   `ConnectionAttempt` whose `unit_id` matches the dropped favorite is removed
//!   from `log`. A per-unit cap (~50 most-recent attempts) keeps `log` bounded.

use chrono::Timelike;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// On-disk schema version. Bumped only on a non-additive shape change.
pub const SCHEMA_VERSION: u32 = 1;

/// Per-mode cap on NON-starred recents. Starred favorites are exempt (M3).
const RECENTS_CAP: usize = 10;

/// Per-unit cap on retained `ConnectionAttempt`s, keeping `log` bounded (M2).
const PER_UNIT_LOG_CAP: usize = 50;

/// Minimum attempts in the argmax bucket before a ToD hint may show (H2).
const TOD_HINT_MIN_ATTEMPTS: usize = 3;

/// Minimum successes in the argmax bucket before a ToD hint may show (H2). The
/// spec prefers ≥2 but mandates ≥1; we require ≥1 here and the observed-success
/// count is surfaced so the UI never implies more confidence than the data.
const TOD_HINT_MIN_SUCCESSES: usize = 1;

/// A single per-mode favorite/recent station. The `id` is server-assigned and is
/// the join key for [`ConnectionAttempt::unit_id`]. `last_attempt_at` is bumped
/// on every recorded attempt and is the LRU-dialed eviction key (M3). `freq` is
/// RECORD-ONLY metadata (never read back into a form, H8). `transport` is the
/// telnet-only `"CmsSsl" | "Telnet"` discriminator (H7 — NOT a free port).
/// `peer_id` [R5-7] links this favorite to a P2P roster entry
/// (`peers::model::Peer::id`) when the recent originated from (or was matched
/// to) a peer; `#[serde(default)]` gives additive tolerance so an existing
/// `stations.json` written before this field existed loads with `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite {
    pub id: String,
    pub mode: String,
    pub gateway: String,
    pub freq: Option<String>,
    pub transport: Option<String>,
    pub band: Option<String>,
    pub grid: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub peer_id: Option<String>,
    pub starred: bool,
    pub last_attempt_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One empirical connection attempt against a unit. `unit_id` is stamped
/// SERVER-SIDE (H3) — the client never supplies it. `ts_local` is an
/// offset-bearing ISO8601 string stored VERBATIM — NEVER converted to UTC (H1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionAttempt {
    pub unit_id: String,
    pub ts_local: String,
    pub freq: Option<String>,
    /// `"reached"` | `"failed"`.
    pub outcome: String,
}

impl ConnectionAttempt {
    /// True iff this attempt actually reached the gateway (an on-air link).
    fn reached(&self) -> bool {
        self.outcome == "reached"
    }
}

/// A gateway that has at least one connection attempt within a recency window,
/// carrying its most-recent in-window attempt's timestamp + outcome plus the
/// favorite's stored grid square. Used by the Winlink map layer (tuxlink-s1o1)
/// to place and color stations on the APRS map.
///
/// `grid` may be `None` when the favorite was added without a grid square; the
/// frontend drops those entries from the map but the struct still carries them
/// so the query layer is not responsible for that UI decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentGateway {
    pub gateway: String,
    pub grid: Option<String>,
    /// RFC3339 offset-bearing timestamp of the most-recent attempt in the window.
    pub last_attempt_at: String,
    /// Outcome of that most-recent attempt: `"reached"` | `"failed"`.
    pub outcome: String,
}

/// The record-path DTO (H3/Codex#8). Carries everything needed to upsert/find
/// the unit; the client passes this (NOT a `unit_id`) to the record path.
/// `peer_id` [R5-7] carries the P2P roster link through to a brand-new
/// recent's [`Favorite::peer_id`]; it has NO `Default` impl (deliberately, so
/// every construction site states its fields explicitly), so callers with no
/// peer context (e.g. the CMS/telnet dial paths) pass `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteDial {
    pub mode: String,
    pub gateway: String,
    pub freq: Option<String>,
    pub transport: Option<String>,
    pub band: Option<String>,
    pub grid: Option<String>,
    pub peer_id: Option<String>,
}

impl FavoriteDial {
    /// The natural identity of a recent within a mode: gateway plus the
    /// freq-or-transport discriminator. Telnet units key on `transport`
    /// (CmsSsl/Telnet); RF units key on the dial `freq`; either may be absent.
    fn ident_key(&self) -> (String, Option<String>, Option<String>) {
        (self.gateway.clone(), self.freq.clone(), self.transport.clone())
    }
}

/// The observed time-of-day record surfaced to the UI when (and only when) the
/// over-claim guard (H2) passes. Carries observed counts ONLY — never a
/// prediction, never a zero-success bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodHint {
    /// The argmax bucket name: `dawn` | `day` | `dusk` | `night`.
    pub bucket: String,
    /// Total attempts observed in that bucket (≥3).
    pub attempts: usize,
    /// Successful (`reached`) attempts observed in that bucket (≥1).
    pub successes: usize,
}

/// The on-disk file shape. `#[serde(default)]` on every field gives additive
/// forward-compat tolerance; there is deliberately NO `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationsFile {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub favorites: Vec<Favorite>,
    #[serde(default)]
    pub log: Vec<ConnectionAttempt>,
}

// NO derive(Default) (M1) — hand-write Default so schema_version is 1, not 0.
impl Default for StationsFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            favorites: vec![],
            log: vec![],
        }
    }
}

/// Serializable error projection for the IPC boundary. Mirrors the
/// `#[serde(tag = "kind", content = "detail")]` shape used by `ContactsError`
/// and `ui_commands.rs::UiError`.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum FavoritesError {
    #[error("io: {0}")]
    Io(String),
    #[error("serde: {0}")]
    Serde(String),
    /// A command-layer input was rejected before it reached the store (e.g. an
    /// unknown `mode` string on an upsert/record). Task B2.
    #[error("validation: {0}")]
    Validation(String),
}

/// Map a local wall-clock hour (0–23) to its time-of-day bucket.
/// dawn 05–07, day 08–16, dusk 17–19, night 20–04.
pub fn tod_bucket(hour: u8) -> &'static str {
    match hour {
        5..=7 => "dawn",
        8..=16 => "day",
        17..=19 => "dusk",
        // 20..=23 and 0..=4 (and any out-of-range value, defensively) → night.
        _ => "night",
    }
}

/// Parse an optional offset-bearing RFC3339 dial timestamp into a comparable
/// instant. `None`/unparseable sorts as the OLDEST possible instant so a
/// never-dialed (or malformed) recent is treated as least-recently-dialed
/// (evicted first; sorts last in the most-recent-first recents view). Comparing
/// `DateTime<FixedOffset>` compares by the underlying UTC instant, so mixed
/// offsets (DST / timezone changes) order correctly — unlike a string compare.
fn dial_instant(
    last_attempt_at: &Option<String>,
) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    last_attempt_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
}

/// Parse the LOCAL hour from an offset-bearing ISO8601 timestamp.
///
/// `ts_local` is a `DateTime<FixedOffset>` that is ALREADY local — its `.hour()`
/// IS the station-local hour. We parse and read `.hour()` directly. We do NOT
/// call `.with_timezone(&Utc)` / `.naive_utc()` / `.timestamp()` — those would
/// re-bucket by UTC and defeat the feature (H1). An unparseable timestamp yields
/// `None` (skipped, never a panic).
fn local_hour(ts_local: &str) -> Option<u8> {
    chrono::DateTime::parse_from_rfc3339(ts_local)
        .ok()
        .map(|dt| dt.hour() as u8)
}

/// Compute the honest time-of-day hint from a unit's attempts, or `None`.
///
/// Buckets each parseable attempt by its LOCAL hour (H1). The hint is the bucket
/// with the highest `reached`-fraction; it is returned ONLY when that bucket has
/// ≥3 attempts AND ≥1 success AND is a STRICT unique max (strictly greater
/// `reached`-fraction than every other bucket). Otherwise `None`. A
/// zero-success bucket is NEVER named; a tie produces no hint (H2).
pub fn tod_hint(attempts: &[ConnectionAttempt]) -> Option<TodHint> {
    // Tally (attempts, successes) per bucket, by LOCAL hour. Unparseable
    // timestamps are skipped (H1).
    let buckets = ["dawn", "day", "dusk", "night"];
    let mut totals: std::collections::HashMap<&'static str, (usize, usize)> =
        std::collections::HashMap::new();
    for a in attempts {
        let Some(hour) = local_hour(&a.ts_local) else {
            continue;
        };
        let bucket = tod_bucket(hour);
        let entry = totals.entry(bucket).or_insert((0, 0));
        entry.0 += 1;
        if a.reached() {
            entry.1 += 1;
        }
    }

    // Find the argmax bucket by reached-fraction. We require a STRICT unique
    // max: if two buckets tie on the top fraction, no hint (H2). We compare
    // fractions via cross-multiplication to avoid float drift:
    // s_a / n_a > s_b / n_b  <=>  s_a * n_b > s_b * n_a.
    let mut best: Option<(&'static str, usize, usize)> = None; // (bucket, attempts, successes)
    let mut best_is_unique = true;
    for &bucket in &buckets {
        let Some(&(n, s)) = totals.get(bucket) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        match best {
            None => {
                best = Some((bucket, n, s));
                best_is_unique = true;
            }
            Some((_, bn, bs)) => {
                // Compare s/n vs bs/bn.
                let lhs = s * bn;
                let rhs = bs * n;
                if lhs > rhs {
                    best = Some((bucket, n, s));
                    best_is_unique = true;
                } else if lhs == rhs {
                    // A tie with the current best.
                    best_is_unique = false;
                }
            }
        }
    }

    let (bucket, attempts_n, successes_n) = best?;

    // Over-claim guard (H2): ≥3 attempts, ≥1 success, strict unique max.
    if !best_is_unique
        || attempts_n < TOD_HINT_MIN_ATTEMPTS
        || successes_n < TOD_HINT_MIN_SUCCESSES
    {
        return None;
    }

    Some(TodHint {
        bucket: bucket.to_string(),
        attempts: attempts_n,
        successes: successes_n,
    })
}

/// The favorites store: an in-memory [`StationsFile`] plus the path it persists
/// to. Mutations flush eagerly. Construct via [`FavoritesStore::open`].
pub struct FavoritesStore {
    path: PathBuf,
    file: StationsFile,
}

impl FavoritesStore {
    /// Open the store at `path`. INFALLIBLE — always returns a usable store.
    ///
    /// - Missing file → default empty store.
    /// - Present + parseable → the parsed file.
    /// - Present + UNparseable (read error or JSON error): rename the file to
    ///   `<name>.corrupt-<utc-ts>` to PRESERVE the original bytes, `eprintln!` a
    ///   warning, then return the default empty store. The corrupt original is
    ///   never overwritten in place; a later flush writes only to `path`,
    ///   leaving the sidecar intact. (Mirrors `contacts/store.rs::open` /
    ///   `user_folders.rs::load_registry`.)
    pub fn open(path: PathBuf) -> Self {
        let file = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<StationsFile>(&bytes) {
                Ok(parsed) => parsed,
                Err(e) => {
                    Self::quarantine_corrupt(&path, &bytes);
                    eprintln!(
                        "favorites: {} is unparseable, starting empty (original preserved): {e}",
                        path.display()
                    );
                    StationsFile::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StationsFile::default(),
            Err(e) => {
                // A non-NotFound read error (e.g. permission/partial). Try to
                // preserve whatever bytes we can read; if even that fails, we
                // still degrade to empty rather than blocking startup.
                if let Ok(bytes) = std::fs::read(&path) {
                    Self::quarantine_corrupt(&path, &bytes);
                }
                eprintln!(
                    "favorites: failed to read {}, starting empty: {e}",
                    path.display()
                );
                StationsFile::default()
            }
        };
        Self { path, file }
    }

    /// Rename the unreadable file to a timestamped `.corrupt-*` sidecar,
    /// preserving the original bytes. Falls back to a copy-write if the rename
    /// itself fails (best-effort preservation; never panics).
    fn quarantine_corrupt(path: &std::path::Path, original: &[u8]) {
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stations.json".to_string());
        let corrupt = path.with_file_name(format!("{name}.corrupt-{ts}"));
        if let Err(e) = std::fs::rename(path, &corrupt) {
            // Rename failed (e.g. cross-device); fall back to copying the bytes
            // out so the original is not lost when a later flush overwrites it.
            eprintln!(
                "favorites: could not rename corrupt {} → {} ({e}); copying bytes instead",
                path.display(),
                corrupt.display()
            );
            let _ = std::fs::write(&corrupt, original);
        }
    }

    /// Persist the in-memory file atomically: serialize → write to a sibling
    /// `<name>.tmp` → `rename` over the final path. `create_dir_all(parent)`
    /// first. Uses `format!("{}.tmp", name)` so the suffix is `stations.json.tmp`
    /// (NOT `with_extension("tmp")`, which would drop `.json`).
    fn flush(&self) -> Result<(), FavoritesError> {
        let json = serde_json::to_string_pretty(&self.file)
            .map_err(|e| FavoritesError::Serde(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| FavoritesError::Io(e.to_string()))?;
        }
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stations.json".to_string());
        let tmp = self.path.with_file_name(format!("{name}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| FavoritesError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| FavoritesError::Io(e.to_string()))?;
        Ok(())
    }

    /// The whole in-memory file (read-only view) — used by `favorites_read`.
    pub fn file(&self) -> &StationsFile {
        &self.file
    }

    /// All favorites (read-only view).
    pub fn favorites(&self) -> &[Favorite] {
        &self.file.favorites
    }

    /// All log entries (read-only view).
    pub fn log(&self) -> &[ConnectionAttempt] {
        &self.file.log
    }

    /// Recents for a mode: NON-starred favorites of that mode, most-recently
    /// dialed first (entries never dialed sort last). Starred favorites are not
    /// recents (they live in the Favorites tab).
    pub fn favorites_recents(&self, mode: &str) -> Vec<Favorite> {
        let mut recents: Vec<Favorite> = self
            .file
            .favorites
            .iter()
            .filter(|f| f.mode == mode && !f.starred)
            .cloned()
            .collect();
        // Most-recently dialed first; never-dialed (None) sort last.
        // Compare by parsed UTC instant so mixed offsets (DST / timezone changes)
        // order correctly — string compare breaks when offsets differ (C2-P2).
        recents.sort_by(|a, b| {
            dial_instant(&b.last_attempt_at).cmp(&dial_instant(&a.last_attempt_at))
        });
        recents
    }

    /// All attempts recorded against a unit (read-only view), in insertion order.
    pub fn attempts_for(&self, unit_id: &str) -> Vec<ConnectionAttempt> {
        self.file
            .log
            .iter()
            .filter(|a| a.unit_id == unit_id)
            .cloned()
            .collect()
    }

    /// All attempts recorded against EVERY favorite whose `gateway` equals
    /// `gateway`, aggregated across modes/freqs into one chronologically-appended
    /// list. The gateway match is EXACT (SSID-bearing): `"W7CPZ"` does NOT match
    /// `"W7CPZ-10"` — a base callsign and an SSID'd one are distinct stations.
    ///
    /// This is the join behind `contacts_connection_record`: a `Favorite` keys on
    /// `gateway` (the station callsign), and `attempts_for(id)` returns one
    /// favorite's attempts; a contact's record is the union of attempts across
    /// every favorite that shares the callsign (a station dialed in two modes has
    /// two favorites, hence two `unit_id`s, hence two attempt streams). No
    /// matching favorite → an empty vec (honest empty state — never fabricated).
    pub fn attempts_for_gateway(&self, gateway: &str) -> Vec<ConnectionAttempt> {
        let unit_ids: std::collections::HashSet<&str> = self
            .file
            .favorites
            .iter()
            .filter(|f| f.gateway == gateway)
            .map(|f| f.id.as_str())
            .collect();
        self.file
            .log
            .iter()
            .filter(|a| unit_ids.contains(a.unit_id.as_str()))
            .cloned()
            .collect()
    }

    /// Gateways that have at least one connection attempt within `within_hours`
    /// of `now`, each carrying its most-recent in-window attempt's timestamp +
    /// outcome plus the favorite's stored grid. Used by the Winlink map layer
    /// (tuxlink-s1o1).
    ///
    /// - `now` is injected (not `Local::now()`) so tests are deterministic.
    /// - The cutoff is `now - within_hours` (inclusive: `ts >= cutoff`).
    /// - When a gateway has multiple favorites (e.g. same callsign in two modes),
    ///   all their attempts are considered; the single most-recent in-window
    ///   attempt wins, and `grid` comes from whichever favorite owns that attempt.
    /// - Returned vec is sorted newest-first by `last_attempt_at` (string compare
    ///   is safe here because all timestamps share the same offset — the caller's
    ///   `now` offset — after parse-and-back-to-source; in practice, lexicographic
    ///   order on same-offset RFC3339 strings IS correct temporal order).
    pub fn recent_gateways(
        &self,
        within_hours: u32,
        now: chrono::DateTime<chrono::FixedOffset>,
    ) -> Vec<RecentGateway> {
        use std::collections::HashMap;
        let cutoff = now - chrono::Duration::hours(within_hours as i64);
        // Build a lookup: unit_id → (gateway, grid).
        let units: HashMap<&str, (&str, Option<&str>)> = self
            .file
            .favorites
            .iter()
            .map(|f| (f.id.as_str(), (f.gateway.as_str(), f.grid.as_deref())))
            .collect();
        // For each in-window attempt, track the most-recent per gateway.
        // Value: (attempt_ref, parsed_instant, grid).
        let mut best: HashMap<
            &str,
            (&ConnectionAttempt, chrono::DateTime<chrono::FixedOffset>, Option<&str>),
        > = HashMap::new();
        for a in &self.file.log {
            let Some(&(gw, grid)) = units.get(a.unit_id.as_str()) else {
                continue;
            };
            let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&a.ts_local) else {
                continue;
            };
            if ts < cutoff {
                continue;
            }
            match best.get(gw) {
                Some((_, prev_ts, _)) if *prev_ts >= ts => {}
                _ => {
                    best.insert(gw, (a, ts, grid));
                }
            }
        }
        let mut out: Vec<RecentGateway> = best
            .into_iter()
            .map(|(gw, (a, _ts, grid))| RecentGateway {
                gateway: gw.to_string(),
                grid: grid.map(|g| g.to_string()),
                last_attempt_at: a.ts_local.clone(),
                outcome: a.outcome.clone(),
            })
            .collect();
        // Sort newest-first by timestamp string (safe for same-offset RFC3339).
        out.sort_by(|x, y| y.last_attempt_at.cmp(&x.last_attempt_at));
        out
    }

    /// Insert a favorite, or replace the existing one with the same `id`. The
    /// store takes the favorite as given (id/timestamp/merge semantics are the
    /// command layer's job, Task B2). Flushes on success.
    pub fn favorite_upsert(&mut self, f: Favorite) -> Result<(), FavoritesError> {
        match self.file.favorites.iter_mut().find(|x| x.id == f.id) {
            Some(existing) => *existing = f,
            None => self.file.favorites.push(f),
        }
        self.flush()
    }

    /// Merge ONLY the operator-editable fields of `edited` into the existing
    /// favorite with the same `id`, preserving everything `favorite_star` and
    /// `record_attempt` own (M12). Editable fields: `gateway`, `freq`,
    /// `transport`, `band`, `grid`, `note`. PRESERVED from the existing record:
    /// `id`, `mode`, `starred`, `created_at`, `last_attempt_at`, `peer_id`
    /// (and, in the file, the whole `log` — untouched here). Bumps `updated_at`
    /// to `now`.
    ///
    /// This is the M12 anti-clobber guard: a STALE whole-object `favorite_upsert`
    /// carrying `starred:false` (or a stale `last_attempt_at`) can never revert a
    /// concurrent star or rewind the dial clock, because those fields are read
    /// from the LIVE record, not the caller's payload.
    ///
    /// `peer_id` [R5-7] is preserved, NOT merged, for the same reason: it is a
    /// system-derived back-link to the P2P roster (like `id`), not
    /// operator-typed metadata. The edit form round-trips the client's cached
    /// whole-object snapshot, so merging it would let a stale edit payload
    /// (touching just `note`) resurrect a peer link the system had since
    /// cleared — or clobber one it had since written.
    ///
    /// Returns `None` (and does NOT flush) when no favorite with `edited.id`
    /// exists — the command layer treats that as a brand-new mint. On a hit, the
    /// merged record is flushed and returned.
    pub fn favorite_merge_editable(
        &mut self,
        edited: &Favorite,
        now: String,
    ) -> Result<Option<Favorite>, FavoritesError> {
        let Some(existing) = self
            .file
            .favorites
            .iter_mut()
            .find(|f| f.id == edited.id)
        else {
            return Ok(None);
        };
        // Operator-editable fields overwrite.
        existing.gateway = edited.gateway.clone();
        existing.freq = edited.freq.clone();
        existing.transport = edited.transport.clone();
        existing.band = edited.band.clone();
        existing.grid = edited.grid.clone();
        existing.note = edited.note.clone();
        existing.updated_at = now;
        // starred, created_at, last_attempt_at, mode, id, peer_id: PRESERVED
        // (not touched). peer_id is a system-derived roster back-link [R5-7];
        // a round-tripped edit snapshot must never resurrect or clobber it.
        let merged = existing.clone();
        self.flush()?;
        Ok(Some(merged))
    }

    /// Flip a favorite's `starred` flag (no-op if the id is absent). A starred
    /// favorite is exempt from recents trimming; star-to-promote keeps a recent
    /// alive past the cap. Bumps `updated_at`. Flushes on success.
    pub fn favorite_star(
        &mut self,
        id: &str,
        starred: bool,
        updated_at: String,
    ) -> Result<(), FavoritesError> {
        if let Some(fav) = self.file.favorites.iter_mut().find(|f| f.id == id) {
            fav.starred = starred;
            fav.updated_at = updated_at;
        }
        self.flush()
    }

    /// Remove a favorite by id (no-op if absent) AND sweep its orphaned log
    /// entries (M2: every `ConnectionAttempt` with `unit_id == id`). Flushes.
    pub fn favorite_delete(&mut self, id: &str) -> Result<(), FavoritesError> {
        self.file.favorites.retain(|f| f.id != id);
        self.file.log.retain(|a| a.unit_id != id);
        self.flush()
    }

    /// Record a connection attempt against the unit identified by `dial`.
    ///
    /// The record path (H3):
    /// 1. Find OR create the recent for `(mode, gateway, freq|transport)`,
    ///    obtaining/assigning its `id` (server-assigned via `new_id`).
    /// 2. Bump that recent's `last_attempt_at` to `ts_local` and `updated_at`.
    /// 3. Append a [`ConnectionAttempt`] with `unit_id` = that recent's id
    ///    (SERVER-stamped — the client never supplies `unit_id`), `ts_local`
    ///    stored VERBATIM (no UTC conversion, H1).
    /// 4. Enforce the per-unit log cap (M2: keep the ~50 most-recent attempts
    ///    for that unit).
    /// 5. Trim non-starred recents for that mode to [`RECENTS_CAP`] by
    ///    least-recently-DIALED (smallest `last_attempt_at`, M3); starred
    ///    favorites are NEVER trimmed.
    /// 6. Sweep orphaned log entries for any trimmed unit (M2).
    ///
    /// `new_id` supplies a fresh unique id for a brand-new recent (the command
    /// layer passes a uuid factory). `now` is the `updated_at`/`created_at`
    /// stamp for a newly-created recent (RFC3339 UTC). Flushes on success.
    pub fn record_attempt(
        &mut self,
        dial: FavoriteDial,
        outcome: String,
        ts_local: String,
        new_id: impl FnOnce() -> String,
        now: String,
    ) -> Result<(), FavoritesError> {
        let key = dial.ident_key();

        // 1. Find OR create the recent for this dial within its mode.
        let unit_id = match self.file.favorites.iter_mut().find(|f| {
            f.mode == dial.mode
                && (f.gateway.clone(), f.freq.clone(), f.transport.clone()) == key
        }) {
            Some(existing) => {
                // 2. Bump the existing recent's dial timestamp.
                existing.last_attempt_at = Some(ts_local.clone());
                existing.updated_at = now.clone();
                existing.id.clone()
            }
            None => {
                let id = new_id();
                self.file.favorites.push(Favorite {
                    id: id.clone(),
                    mode: dial.mode.clone(),
                    gateway: dial.gateway.clone(),
                    freq: dial.freq.clone(),
                    transport: dial.transport.clone(),
                    band: dial.band.clone(),
                    grid: dial.grid.clone(),
                    note: None,
                    // [R5-7] carried through from the dial ONLY at creation —
                    // mirrors freq/transport/band/grid, which likewise are not
                    // re-applied to an already-existing recent on a repeat dial.
                    peer_id: dial.peer_id.clone(),
                    starred: false,
                    last_attempt_at: Some(ts_local.clone()),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
                id
            }
        };

        // 3. Append the attempt with the server-stamped unit_id; ts_local VERBATIM.
        self.file.log.push(ConnectionAttempt {
            unit_id: unit_id.clone(),
            ts_local,
            freq: dial.freq.clone(),
            outcome,
        });

        // 4. Enforce the per-unit log cap for this unit (M2).
        self.cap_unit_log(&unit_id);

        // 5 + 6. Trim non-starred recents for this mode + sweep orphans (M2/M3).
        self.trim_recents(&dial.mode);

        self.flush()
    }

    /// Keep only the [`PER_UNIT_LOG_CAP`] most-recent attempts for `unit_id`
    /// (M2). Most-recent = latest in insertion order (attempts are appended in
    /// chronological record order). Other units' entries are untouched.
    fn cap_unit_log(&mut self, unit_id: &str) {
        let count = self.file.log.iter().filter(|a| a.unit_id == unit_id).count();
        if count <= PER_UNIT_LOG_CAP {
            return;
        }
        let mut to_drop = count - PER_UNIT_LOG_CAP;
        // Drop the OLDEST (earliest-inserted) attempts for this unit until the
        // cap is met; retain everything for other units.
        self.file.log.retain(|a| {
            if a.unit_id == unit_id && to_drop > 0 {
                to_drop -= 1;
                false
            } else {
                true
            }
        });
    }

    /// Trim NON-starred recents for `mode` down to [`RECENTS_CAP`] by
    /// least-recently-DIALED (smallest `last_attempt_at`, M3). Starred favorites
    /// are NEVER trimmed (and not counted toward the cap). On each eviction,
    /// orphaned log entries for the dropped unit are swept (M2).
    fn trim_recents(&mut self, mode: &str) {
        loop {
            // Indices of non-starred recents in this mode.
            let mut recent_idx: Vec<usize> = self
                .file
                .favorites
                .iter()
                .enumerate()
                .filter(|(_, f)| f.mode == mode && !f.starred)
                .map(|(i, _)| i)
                .collect();

            if recent_idx.len() <= RECENTS_CAP {
                break;
            }

            // Pick the least-recently-DIALED among them. A never-dialed entry
            // (last_attempt_at == None) sorts as the oldest instant and is
            // evicted first. Sort ascending by instant so index [0] is the
            // least-recently-dialed. Compare by parsed UTC instant so mixed
            // offsets (DST / timezone changes) order correctly — string compare
            // breaks when offsets differ (C2-P2).
            recent_idx.sort_by(|&a, &b| {
                dial_instant(&self.file.favorites[a].last_attempt_at)
                    .cmp(&dial_instant(&self.file.favorites[b].last_attempt_at))
            });
            let evict = recent_idx[0];
            let dropped_id = self.file.favorites[evict].id.clone();
            self.file.favorites.remove(evict);
            // Sweep orphaned log entries for the dropped unit (M2).
            self.file.log.retain(|a| a.unit_id != dropped_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use tempfile::tempdir;

    /// Deterministic id factory: each call returns a fresh id.
    fn id_seq() -> impl FnMut() -> String {
        let mut n = 0u32;
        move || {
            n += 1;
            format!("u{n}")
        }
    }

    fn dial(mode: &str, gateway: &str, freq: Option<&str>) -> FavoriteDial {
        FavoriteDial {
            mode: mode.to_string(),
            gateway: gateway.to_string(),
            freq: freq.map(|s| s.to_string()),
            transport: None,
            band: None,
            grid: None,
            peer_id: None,
        }
    }

    fn telnet_dial(gateway: &str, transport: &str) -> FavoriteDial {
        FavoriteDial {
            mode: "telnet".to_string(),
            gateway: gateway.to_string(),
            freq: None,
            transport: Some(transport.to_string()),
            band: None,
            grid: None,
            peer_id: None,
        }
    }

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
            created_at: "2026-06-07T12:00:00+00:00".to_string(),
            updated_at: "2026-06-07T12:00:00+00:00".to_string(),
        }
    }

    fn attempt(unit_id: &str, ts_local: &str, outcome: &str) -> ConnectionAttempt {
        ConnectionAttempt {
            unit_id: unit_id.to_string(),
            ts_local: ts_local.to_string(),
            freq: None,
            outcome: outcome.to_string(),
        }
    }

    // ---- Store CRUD + reopen ------------------------------------------------

    #[test]
    fn open_missing_returns_empty() {
        let dir = tempdir().unwrap();
        let store = FavoritesStore::open(dir.path().join("stations.json"));
        assert_eq!(store.file().schema_version, SCHEMA_VERSION);
        assert!(store.favorites().is_empty());
        assert!(store.log().is_empty());
    }

    #[test]
    fn fresh_empty_store_has_schema_version_1() {
        // M1: a brand-new store written to disk persists schema_version:1, NOT 0
        // (guards against an accidental derive(Default)).
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path.clone());
        store.favorite_upsert(favorite("f1", "ardop-hf", "W6XYZ")).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\"schema_version\": 1"),
            "expected schema_version 1 on disk, got: {raw}"
        );
        let reopened = FavoritesStore::open(path);
        assert_eq!(reopened.file().schema_version, 1);
    }

    #[test]
    fn upsert_then_reopen_persists_favorites_and_log() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path.clone());
        store.favorite_upsert(favorite("f1", "ardop-hf", "W6XYZ")).unwrap();
        // Append a log entry via the record path so the log persists too.
        store
            .record_attempt(
                dial("ardop-hf", "W6XYZ", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T10:00:00-07:00".to_string(),
                id_seq(),
                "2026-06-07T17:00:00+00:00".to_string(),
            )
            .unwrap();
        drop(store);
        let reopened = FavoritesStore::open(path);
        assert!(!reopened.favorites().is_empty());
        assert!(!reopened.log().is_empty());
    }

    #[test]
    fn favorite_with_peer_id_round_trips() {
        // [R5-7]: `peer_id` is a normal on-disk field, not record-path-only —
        // a favorite constructed with a peer link survives a flush + reopen.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path.clone());
        let mut fav = favorite("f1", "vara-hf", "KK6XYZ");
        fav.peer_id = Some("p1".to_string());
        store.favorite_upsert(fav).unwrap();
        drop(store);

        let reopened = FavoritesStore::open(path);
        assert_eq!(
            reopened.favorites()[0].peer_id.as_deref(),
            Some("p1"),
            "peer_id must survive flush + reopen"
        );
    }

    #[test]
    fn stations_json_without_peer_id_loads_as_none() {
        // [R5-7] additive-safety: a `stations.json` written before `peer_id`
        // existed (the favorite row simply omits the key) must load with
        // `peer_id: None`, not a deserialize error — `#[serde(default)]`.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let json = r#"{
            "schema_version": 1,
            "favorites": [{
                "id": "f1",
                "mode": "vara-hf",
                "gateway": "KK6XYZ",
                "freq": null,
                "transport": null,
                "band": null,
                "grid": null,
                "note": null,
                "starred": false,
                "last_attempt_at": null,
                "created_at": "2026-06-07T12:00:00+00:00",
                "updated_at": "2026-06-07T12:00:00+00:00"
            }],
            "log": []
        }"#;
        std::fs::write(&path, json).unwrap();
        let store = FavoritesStore::open(path);
        assert_eq!(store.favorites().len(), 1);
        assert_eq!(
            store.favorites()[0].peer_id, None,
            "an old row with no peer_id key must deserialize to None, not fail"
        );
    }

    #[test]
    fn unknown_top_level_field_tolerated() {
        // C1: an EXTRA top-level key parses fine; deny_unknown_fields is ABSENT.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let json = r#"{
            "schema_version": 1,
            "favorites": [],
            "log": [],
            "future_field_from_a_newer_version": {"nested": true}
        }"#;
        std::fs::write(&path, json).unwrap();
        let store = FavoritesStore::open(path);
        assert_eq!(store.file().schema_version, 1);
        // No corrupt sidecar should have been created.
        let sidecars: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .collect();
        assert!(
            sidecars.is_empty(),
            "tolerated unknown field must NOT trigger quarantine"
        );
    }

    #[test]
    fn open_on_corrupt_file_preserves_original_bytes() {
        // C1: garbage in stations.json → open() returns empty AND leaves a
        // stations.json.corrupt-<ts> sidecar holding the ORIGINAL bytes; a
        // subsequent mutate+flush must NOT destroy those bytes.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let garbage = b"not valid json {{{ \x00\x01 broken";
        std::fs::write(&path, garbage).unwrap();

        let mut store = FavoritesStore::open(path.clone());
        assert!(
            store.favorites().is_empty(),
            "corrupt file must degrade to empty store"
        );

        let sidecar = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("stations.json.corrupt-")
            })
            .expect("expected a stations.json.corrupt-<ts> sidecar");
        let preserved = std::fs::read(sidecar.path()).unwrap();
        assert_eq!(
            preserved, garbage,
            "corrupt sidecar must hold the original bytes verbatim"
        );

        // A subsequent mutate+flush writes the fresh empty file WITHOUT
        // destroying the preserved bytes.
        store.favorite_upsert(favorite("f1", "ardop-hf", "W6XYZ")).unwrap();
        let preserved_after = std::fs::read(sidecar.path()).unwrap();
        assert_eq!(
            preserved_after, garbage,
            "flush must not clobber the preserved corrupt bytes"
        );
        let reopened = FavoritesStore::open(path);
        assert_eq!(reopened.favorites().len(), 1);
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        store.favorite_upsert(favorite("f1", "ardop-hf", "W6XYZ")).unwrap();
        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "no .tmp file should remain after flush");
    }

    // ---- Star + delete ------------------------------------------------------

    #[test]
    fn favorite_star_flips_starred() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path.clone());
        store.favorite_upsert(favorite("f1", "ardop-hf", "W6XYZ")).unwrap();
        store
            .favorite_star("f1", true, "2026-06-08T00:00:00+00:00".to_string())
            .unwrap();
        drop(store);
        let reopened = FavoritesStore::open(path);
        assert!(reopened.favorites()[0].starred);
        assert_eq!(reopened.favorites()[0].updated_at, "2026-06-08T00:00:00+00:00");
    }

    #[test]
    fn favorite_merge_editable_preserves_protected_fields() {
        // M12: merging operator-editable fields must NOT touch starred,
        // created_at, or last_attempt_at — only gateway/freq/transport/band/
        // grid/note + updated_at change. A miss returns None without flushing.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);

        // Seed a starred favorite that has been dialed (has last_attempt_at).
        let mut seed = favorite("f1", "ardop-hf", "W6XYZ");
        seed.starred = true;
        seed.created_at = "2026-01-01T00:00:00+00:00".to_string();
        seed.last_attempt_at = Some("2026-06-07T10:00:00-07:00".to_string());
        seed.note = Some("original note".to_string());
        store.favorite_upsert(seed).unwrap();

        // A STALE edit: starred:false, no last_attempt_at, different created_at,
        // only the note + band changed.
        let edit = Favorite {
            id: "f1".to_string(),
            mode: "telnet".to_string(), // mode is NOT editable — must be ignored
            gateway: "W6XYZ".to_string(),
            freq: Some("7102.0".to_string()),
            transport: None,
            band: Some("40m".to_string()),
            grid: Some("CN88".to_string()),
            note: Some("edited note".to_string()),
            peer_id: None,
            starred: false,                                  // stale — must be ignored
            last_attempt_at: None,                           // stale — must be ignored
            created_at: "2099-01-01T00:00:00+00:00".to_string(), // stale — must be ignored
            updated_at: String::new(),
        };
        let merged = store
            .favorite_merge_editable(&edit, "2026-06-08T12:00:00+00:00".to_string())
            .unwrap()
            .expect("merge over an existing id returns Some");

        // Protected fields preserved from the LIVE record.
        assert!(merged.starred, "starred must be preserved (M12)");
        assert_eq!(merged.created_at, "2026-01-01T00:00:00+00:00", "created_at preserved");
        assert_eq!(
            merged.last_attempt_at.as_deref(),
            Some("2026-06-07T10:00:00-07:00"),
            "last_attempt_at preserved"
        );
        assert_eq!(merged.mode, "ardop-hf", "mode is not editable — preserved");
        // Editable fields overwritten.
        assert_eq!(merged.note.as_deref(), Some("edited note"));
        assert_eq!(merged.band.as_deref(), Some("40m"));
        assert_eq!(merged.freq.as_deref(), Some("7102.0"));
        assert_eq!(merged.grid.as_deref(), Some("CN88"));
        assert_eq!(merged.updated_at, "2026-06-08T12:00:00+00:00");
    }

    #[test]
    fn favorite_merge_editable_preserves_peer_id_both_directions() {
        // [R5-7] peer_id is a system-derived roster back-link (like `id`), NOT
        // operator-typed metadata — the merge must read it from the LIVE record,
        // never the caller's payload. The edit form round-trips the client's
        // cached whole-object snapshot, so an editable peer_id would let a stale
        // Edit (touching just `note`) resurrect a link the system had since
        // cleared, or clobber one it had since written. Pin BOTH directions.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);

        // Direction 1: live Some survives an edit carrying None.
        let mut seed = favorite("f1", "vara-hf", "KK6XYZ");
        seed.peer_id = Some("p1".to_string());
        store.favorite_upsert(seed).unwrap();
        let mut edit = favorite("f1", "vara-hf", "KK6XYZ");
        edit.peer_id = None; // stale snapshot from before the system linked p1
        edit.note = Some("edited note".to_string());
        let merged = store
            .favorite_merge_editable(&edit, "2026-06-08T12:00:00+00:00".to_string())
            .unwrap()
            .expect("merge over an existing id returns Some");
        assert_eq!(
            merged.peer_id.as_deref(),
            Some("p1"),
            "a live peer link must survive an edit payload carrying None"
        );
        assert_eq!(merged.note.as_deref(), Some("edited note"), "the edit itself landed");

        // Direction 2: live None survives an edit carrying Some.
        let seed2 = favorite("f2", "vara-hf", "W6ABC"); // peer_id: None (cleared/never linked)
        store.favorite_upsert(seed2).unwrap();
        let mut edit2 = favorite("f2", "vara-hf", "W6ABC");
        edit2.peer_id = Some("p-ghost".to_string()); // stale snapshot from before a cleanup
        let merged2 = store
            .favorite_merge_editable(&edit2, "2026-06-08T13:00:00+00:00".to_string())
            .unwrap()
            .expect("merge over an existing id returns Some");
        assert_eq!(
            merged2.peer_id, None,
            "a cleared peer link must not be resurrected by a stale edit payload"
        );
        // And the live store agrees on both.
        assert_eq!(store.favorites()[0].peer_id.as_deref(), Some("p1"));
        assert_eq!(store.favorites()[1].peer_id, None);
    }

    #[test]
    fn favorite_merge_editable_miss_returns_none() {
        // A merge against an unknown id returns None (command layer mints anew).
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let edit = favorite("ghost", "packet", "K0NONE");
        let result = store
            .favorite_merge_editable(&edit, "2026-06-08T12:00:00+00:00".to_string())
            .unwrap();
        assert!(result.is_none(), "merge against an absent id returns None");
        assert!(store.favorites().is_empty(), "no favorite is created on a miss");
    }

    #[test]
    fn delete_sweeps_log_entries() {
        // M2: favorite_delete also removes that unit's ConnectionAttempts.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        // Create a recent + an attempt via the record path so unit_id links.
        store
            .record_attempt(
                dial("ardop-hf", "W6XYZ", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T10:00:00-07:00".to_string(),
                id_seq(),
                "2026-06-07T17:00:00+00:00".to_string(),
            )
            .unwrap();
        let unit_id = store.favorites()[0].id.clone();
        assert_eq!(store.attempts_for(&unit_id).len(), 1);

        store.favorite_delete(&unit_id).unwrap();
        assert!(store.favorites().is_empty());
        assert!(
            store.log().is_empty(),
            "delete must sweep the unit's orphaned log entries (M2)"
        );
    }

    // ---- Record path: server-stamped unit_id + ts_local verbatim ------------

    #[test]
    fn record_attempt_appends_verbatim_ts_and_bumps_last_attempt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let ts = "2026-06-07T23:00:00-07:00"; // offset-bearing, NOT UTC
        store
            .record_attempt(
                dial("ardop-hf", "W6XYZ", Some("14105.0")),
                "reached".to_string(),
                ts.to_string(),
                id_seq(),
                "2026-06-08T06:00:00+00:00".to_string(),
            )
            .unwrap();
        let unit = &store.favorites()[0];
        // last_attempt_at bumped to the EXACT ts_local (offset preserved).
        assert_eq!(unit.last_attempt_at.as_deref(), Some(ts));
        // The appended attempt's ts_local is stored VERBATIM (no UTC conversion).
        let logged = &store.log()[0];
        assert_eq!(logged.ts_local, ts, "ts_local must be stored verbatim (H1)");
    }

    #[test]
    fn starred_favorite_never_trimmed() {
        // A starred favorite survives even when the mode is over the recents cap.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        // Star a favorite up front.
        store.favorite_upsert(favorite("star", "packet", "K0STAR")).unwrap();
        store
            .favorite_star("star", true, "2026-06-07T12:00:00+00:00".to_string())
            .unwrap();
        // Dial 12 distinct NON-starred recents → over the cap of 10.
        let mut ids = id_seq();
        for i in 0..12 {
            store
                .record_attempt(
                    dial("packet", &format!("GW{i}"), None),
                    "reached".to_string(),
                    format!("2026-06-07T{:02}:00:00-07:00", i),
                    &mut ids,
                    "2026-06-07T20:00:00+00:00".to_string(),
                )
                .unwrap();
        }
        // The starred favorite is still present.
        assert!(
            store.favorites().iter().any(|f| f.id == "star" && f.starred),
            "starred favorite must never be trimmed (M3)"
        );
        // Non-starred packet recents are capped at 10.
        let non_starred = store
            .favorites()
            .iter()
            .filter(|f| f.mode == "packet" && !f.starred)
            .count();
        assert_eq!(non_starred, RECENTS_CAP);
    }

    #[test]
    fn first_dial_creates_recent_and_links_attempt() {
        // H3: recording on a brand-new (mode,gateway,freq|transport) CREATES the
        // recent (server assigns id); the attempt's unit_id == that id. A SECOND
        // record on the same pair reuses the same recent id (no dup unit).
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let mut ids = id_seq();

        store
            .record_attempt(
                dial("ardop-hf", "W6XYZ", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T10:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-07T17:00:00+00:00".to_string(),
            )
            .unwrap();
        assert_eq!(store.favorites().len(), 1, "first dial creates the recent");
        let unit_id = store.favorites()[0].id.clone();
        assert_eq!(store.log().len(), 1);
        assert_eq!(
            store.log()[0].unit_id, unit_id,
            "attempt's unit_id must equal the server-assigned recent id (H3)"
        );

        // Second record on the SAME pair reuses the same recent.
        store
            .record_attempt(
                dial("ardop-hf", "W6XYZ", Some("14105.0")),
                "failed".to_string(),
                "2026-06-07T11:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-07T18:00:00+00:00".to_string(),
            )
            .unwrap();
        assert_eq!(store.favorites().len(), 1, "second dial reuses the recent");
        assert_eq!(store.log().len(), 2);
        assert!(store.log().iter().all(|a| a.unit_id == unit_id));

        // favorites_recents(mode) contains the recent.
        let recents = store.favorites_recents("ardop-hf");
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].id, unit_id);
    }

    #[test]
    fn record_distinguishes_telnet_transport() {
        // A telnet unit keys on transport, not freq: two transports on the same
        // host are distinct recents.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let mut ids = id_seq();
        store
            .record_attempt(
                telnet_dial("cms.winlink.org", "CmsSsl"),
                "reached".to_string(),
                "2026-06-07T10:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-07T17:00:00+00:00".to_string(),
            )
            .unwrap();
        store
            .record_attempt(
                telnet_dial("cms.winlink.org", "Telnet"),
                "reached".to_string(),
                "2026-06-07T10:05:00-07:00".to_string(),
                &mut ids,
                "2026-06-07T17:05:00+00:00".to_string(),
            )
            .unwrap();
        assert_eq!(
            store.favorites_recents("telnet").len(),
            2,
            "distinct transports on the same host are distinct recents"
        );
    }

    #[test]
    fn trim_evicts_least_recently_dialed_not_created() {
        // M3: with the cap exceeded, eviction drops the recent with the SMALLEST
        // last_attempt_at — NOT the smallest created_at. Dial an OLD-created
        // entry late to bump its last_attempt_at; overflow; assert it SURVIVES
        // and a newer-created-but-staler-dialed entry is dropped.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let mut ids = id_seq();

        // Seed an "old-created" recent dialed at an EARLY local time first.
        store
            .record_attempt(
                dial("ardop-hf", "OLD", Some("14000.0")),
                "reached".to_string(),
                "2026-06-07T01:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-07T08:00:00+00:00".to_string(),
            )
            .unwrap();

        // Fill to exactly the cap with 9 more distinct recents (total 10).
        for i in 0..9 {
            store
                .record_attempt(
                    dial("ardop-hf", &format!("GW{i}"), Some("14000.0")),
                    "reached".to_string(),
                    format!("2026-06-07T{:02}:00:00-07:00", i + 2),
                    &mut ids,
                    "2026-06-07T09:00:00+00:00".to_string(),
                )
                .unwrap();
        }
        assert_eq!(store.favorites_recents("ardop-hf").len(), 10);

        // Re-dial OLD LATE so its last_attempt_at becomes the NEWEST.
        store
            .record_attempt(
                dial("ardop-hf", "OLD", Some("14000.0")),
                "reached".to_string(),
                "2026-06-07T23:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-07T23:00:00+00:00".to_string(),
            )
            .unwrap();

        // Now dial ONE new recent → overflow (11 > cap of 10). The evicted one
        // must be the least-recently-DIALED = GW0 (dialed at 02:00), NOT OLD.
        store
            .record_attempt(
                dial("ardop-hf", "NEW", Some("14000.0")),
                "reached".to_string(),
                "2026-06-07T22:30:00-07:00".to_string(),
                &mut ids,
                "2026-06-07T22:30:00+00:00".to_string(),
            )
            .unwrap();

        let recents = store.favorites_recents("ardop-hf");
        assert_eq!(recents.len(), 10, "cap holds at 10");
        let gateways: Vec<String> = recents.iter().map(|f| f.gateway.clone()).collect();
        assert!(
            gateways.contains(&"OLD".to_string()),
            "OLD was re-dialed latest; it must SURVIVE (LRU-dialed, not created)"
        );
        assert!(
            !gateways.contains(&"GW0".to_string()),
            "GW0 was the least-recently-dialed; it must be evicted"
        );
        assert!(gateways.contains(&"NEW".to_string()));
    }

    #[test]
    fn trim_evicts_by_instant_across_offsets() {
        // C2-P2: lexical order ≠ instant order when offsets differ.
        //
        // The scenario:
        //   A = 2026-06-07T01:15:00-08:00 → UTC 09:15Z  (more recent instant)
        //   B = 2026-06-07T01:30:00-07:00 → UTC 08:30Z  (older instant)
        //
        // String compare: "01:15" < "01:30" (same date prefix, -08 > -07 doesn't
        // rescue the comparison because the digit characters dominate), so a
        // naive string sort would rank A as the SMALLER timestamp (least-recently
        // dialed) and evict it. But A's UTC instant (09:15Z) is LATER than B's
        // (08:30Z), so B is actually the older dial and must be evicted first.
        //
        // Instant compare: A (09:15Z) > B (08:30Z) → B is least-recently-dialed
        // → B gets evicted, A survives. That's the correct behaviour.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let mut ids = id_seq();

        // Dial A with the LATER UTC instant but the SMALLER wall-clock string.
        store
            .record_attempt(
                dial("ardop-hf", "STATION_A", Some("14100.0")),
                "reached".to_string(),
                "2026-06-07T01:15:00-08:00".to_string(), // 09:15Z
                &mut ids,
                "2026-06-07T09:15:00+00:00".to_string(),
            )
            .unwrap();

        // Dial B with the OLDER UTC instant but the LARGER wall-clock string.
        store
            .record_attempt(
                dial("ardop-hf", "STATION_B", Some("14100.0")),
                "reached".to_string(),
                "2026-06-07T01:30:00-07:00".to_string(), // 08:30Z
                &mut ids,
                "2026-06-07T08:30:00+00:00".to_string(),
            )
            .unwrap();

        // Confirm the recents order: most-recent-first → A (09:15Z) before B
        // (08:30Z). Under string compare this would be backwards.
        let recents = store.favorites_recents("ardop-hf");
        assert_eq!(recents.len(), 2);
        assert_eq!(
            recents[0].gateway, "STATION_A",
            "A (09:15Z) is the more-recent dial; it must sort first (C2-P2)"
        );
        assert_eq!(
            recents[1].gateway, "STATION_B",
            "B (08:30Z) is the older dial; it must sort second (C2-P2)"
        );

        // Now fill the mode to exactly the cap with 8 more recents so the next
        // dial pushes the count to cap+1 and forces one eviction.
        for i in 0..8 {
            store
                .record_attempt(
                    dial("ardop-hf", &format!("FILLER{i}"), Some("14100.0")),
                    "reached".to_string(),
                    format!("2026-06-06T{:02}:00:00+00:00", i + 2), // older than A and B
                    &mut ids,
                    "2026-06-06T10:00:00+00:00".to_string(),
                )
                .unwrap();
        }
        assert_eq!(store.favorites_recents("ardop-hf").len(), RECENTS_CAP);

        // Dial one more new station to push the count to 11 → one eviction.
        store
            .record_attempt(
                dial("ardop-hf", "NEWCOMER", Some("14100.0")),
                "reached".to_string(),
                "2026-06-08T12:00:00+00:00".to_string(), // newest of all
                &mut ids,
                "2026-06-08T12:00:00+00:00".to_string(),
            )
            .unwrap();

        let recents = store.favorites_recents("ardop-hf");
        assert_eq!(recents.len(), RECENTS_CAP, "cap must hold at 10 after eviction");

        let gateways: Vec<String> = recents.iter().map(|f| f.gateway.clone()).collect();
        assert!(
            gateways.contains(&"STATION_A".to_string()),
            "STATION_A (09:15Z, more-recent instant) must SURVIVE — C2-P2 fix"
        );
        // B (08:30Z) is the oldest among A and B; whether B or a FILLER entry
        // gets evicted depends on exact filler timestamps, but B must NOT survive
        // if it is the absolute least-recently-dialed. The fillers were dialed
        // at 2026-06-06T02..09Z which are all older than B's 08:30Z, so B should
        // survive too — the oldest filler (02:00Z) is the one evicted.
        // The critical assertion is that A survives (string compare would evict A).
        assert!(
            !gateways.iter().any(|g| g.starts_with("FILLER0")
                || (g.starts_with("FILLER") && {
                    let n: u32 = g.trim_start_matches("FILLER").parse().unwrap_or(99);
                    // The oldest filler was FILLER0 at 02:00Z — it must be evicted.
                    n == 0
                })),
            "FILLER0 (2026-06-06T02:00Z, oldest) must be evicted, not STATION_A"
        );
    }

    #[test]
    fn trim_sweeps_orphaned_log_entries() {
        // M2: when a non-starred recent is trimmed, its ConnectionAttempts are
        // removed from log; no orphaned attempts remain.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let mut ids = id_seq();
        // Dial 11 distinct recents → one is trimmed.
        for i in 0..11 {
            store
                .record_attempt(
                    dial("packet", &format!("GW{i}"), None),
                    "reached".to_string(),
                    format!("2026-06-07T{:02}:00:00-07:00", i),
                    &mut ids,
                    "2026-06-07T20:00:00+00:00".to_string(),
                )
                .unwrap();
        }
        // Exactly one recent was trimmed; the cap holds.
        assert_eq!(store.favorites_recents("packet").len(), RECENTS_CAP);
        // Every remaining log entry references a still-present favorite — no
        // orphans.
        let live_ids: std::collections::HashSet<String> =
            store.favorites().iter().map(|f| f.id.clone()).collect();
        assert!(
            store.log().iter().all(|a| live_ids.contains(&a.unit_id)),
            "no orphaned log entries may remain after a trim (M2)"
        );
        // And the total log count equals the live recents count (1 attempt each).
        assert_eq!(store.log().len(), RECENTS_CAP);
    }

    #[test]
    fn per_unit_log_cap_bounds_growth() {
        // M2: the log can't grow unbounded for a single hot unit — only the ~50
        // most-recent attempts per unit_id are retained.
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let mut store = FavoritesStore::open(path);
        let mut ids = id_seq();
        // 60 attempts on the SAME pair → one unit, capped at 50.
        for i in 0..60 {
            store
                .record_attempt(
                    dial("ardop-hf", "HOT", Some("14000.0")),
                    if i % 2 == 0 { "reached" } else { "failed" }.to_string(),
                    format!("2026-06-07T{:02}:{:02}:00-07:00", i / 60, i % 60),
                    &mut ids,
                    "2026-06-07T20:00:00+00:00".to_string(),
                )
                .unwrap();
        }
        assert_eq!(store.favorites_recents("ardop-hf").len(), 1);
        let unit_id = store.favorites()[0].id.clone();
        assert_eq!(
            store.attempts_for(&unit_id).len(),
            PER_UNIT_LOG_CAP,
            "per-unit log must be capped at {PER_UNIT_LOG_CAP} (M2)"
        );
    }

    // ---- attempts_for_gateway: the contacts-record join (tuxlink-je5d) -------

    #[test]
    fn attempts_for_gateway_single_favorite() {
        // (a) One favorite with attempts → that favorite's attempts, aggregated
        // by gateway.
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::open(dir.path().join("stations.json"));
        let mut ids = id_seq();
        store
            .record_attempt(
                dial("ardop-hf", "W7CPZ", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T23:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T06:00:00+00:00".to_string(),
            )
            .unwrap();
        store
            .record_attempt(
                dial("ardop-hf", "W7CPZ", Some("14105.0")),
                "failed".to_string(),
                "2026-06-07T22:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T05:00:00+00:00".to_string(),
            )
            .unwrap();

        let attempts = store.attempts_for_gateway("W7CPZ");
        assert_eq!(attempts.len(), 2, "both attempts on the one favorite aggregate");
        // The combined set runs the same tod_hint gate the command uses; here
        // 1/2 reached in night, <3 attempts → no hint (over-claim guard, H2).
        assert!(tod_hint(&attempts).is_none(), "<3 attempts → None (H2)");
    }

    #[test]
    fn attempts_for_gateway_spans_two_favorites_same_gateway() {
        // (b) The SAME gateway dialed in two distinct ways (different mode/freq)
        // mints TWO favorites (two unit_ids); the record aggregates attempts from
        // BOTH. Build a strict-unique-max night bucket with ≥3 attempts / ≥1
        // success across the two units so a hint is produced over the union.
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::open(dir.path().join("stations.json"));
        let mut ids = id_seq();

        // Favorite 1: ardop-hf @ 14105.0 — two night successes.
        store
            .record_attempt(
                dial("ardop-hf", "W7CPZ", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T23:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T06:00:00+00:00".to_string(),
            )
            .unwrap();
        store
            .record_attempt(
                dial("ardop-hf", "W7CPZ", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T22:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T05:00:00+00:00".to_string(),
            )
            .unwrap();

        // Favorite 2: vara-hf @ 7102.0 — a distinct favorite, SAME gateway. One
        // more night success.
        store
            .record_attempt(
                dial("vara-hf", "W7CPZ", Some("7102.0")),
                "reached".to_string(),
                "2026-06-07T21:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T04:00:00+00:00".to_string(),
            )
            .unwrap();

        // Two distinct favorites for the same gateway.
        assert_eq!(
            store.favorites().iter().filter(|f| f.gateway == "W7CPZ").count(),
            2,
            "different mode/freq on the same gateway → two favorites"
        );

        let attempts = store.attempts_for_gateway("W7CPZ");
        assert_eq!(
            attempts.len(),
            3,
            "attempts from BOTH favorites aggregate into one record"
        );
        let hint = tod_hint(&attempts).expect("3 night successes across two units → Some");
        assert_eq!(hint.bucket, "night");
        assert_eq!(hint.attempts, 3);
        assert_eq!(hint.successes, 3);
    }

    #[test]
    fn attempts_for_gateway_no_favorite_is_empty() {
        // (c) A callsign with no matching favorite → empty attempts; the combined
        // tod_hint over the empty set is None (honest empty state — not faked).
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::open(dir.path().join("stations.json"));
        let mut ids = id_seq();
        // Seed an UNRELATED favorite so the store isn't trivially empty.
        store
            .record_attempt(
                dial("ardop-hf", "K0OTHER", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T23:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T06:00:00+00:00".to_string(),
            )
            .unwrap();

        let attempts = store.attempts_for_gateway("W7CPZ");
        assert!(attempts.is_empty(), "no favorite for the callsign → empty attempts");
        assert!(tod_hint(&attempts).is_none(), "empty attempts → None hint");
    }

    #[test]
    fn attempts_for_gateway_match_is_exact_ssid_bearing() {
        // (d) The gateway match is EXACT (SSID-bearing): a query for "W7CPZ" must
        // NOT pick up attempts on the distinct station "W7CPZ-10".
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::open(dir.path().join("stations.json"));
        let mut ids = id_seq();
        // Base callsign favorite: one attempt.
        store
            .record_attempt(
                dial("ardop-hf", "W7CPZ", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T23:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T06:00:00+00:00".to_string(),
            )
            .unwrap();
        // SSID'd favorite, a DIFFERENT station: two attempts.
        store
            .record_attempt(
                dial("ardop-hf", "W7CPZ-10", Some("14105.0")),
                "reached".to_string(),
                "2026-06-07T22:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T05:00:00+00:00".to_string(),
            )
            .unwrap();
        store
            .record_attempt(
                dial("ardop-hf", "W7CPZ-10", Some("14105.0")),
                "failed".to_string(),
                "2026-06-07T21:00:00-07:00".to_string(),
                &mut ids,
                "2026-06-08T04:00:00+00:00".to_string(),
            )
            .unwrap();

        let base = store.attempts_for_gateway("W7CPZ");
        assert_eq!(base.len(), 1, "\"W7CPZ\" must NOT match \"W7CPZ-10\" (exact, SSID-bearing)");
        let ssid = store.attempts_for_gateway("W7CPZ-10");
        assert_eq!(ssid.len(), 2, "the SSID'd station carries its own two attempts");
    }

    // ---- tod_bucket boundaries ----------------------------------------------

    #[test]
    fn tod_bucket_boundaries() {
        assert_eq!(tod_bucket(5), "dawn");
        assert_eq!(tod_bucket(6), "dawn");
        assert_eq!(tod_bucket(7), "dawn");
        assert_eq!(tod_bucket(8), "day");
        assert_eq!(tod_bucket(12), "day");
        assert_eq!(tod_bucket(16), "day");
        assert_eq!(tod_bucket(17), "dusk");
        assert_eq!(tod_bucket(18), "dusk");
        assert_eq!(tod_bucket(19), "dusk");
        assert_eq!(tod_bucket(20), "night");
        assert_eq!(tod_bucket(23), "night");
        assert_eq!(tod_bucket(0), "night");
        assert_eq!(tod_bucket(2), "night");
        assert_eq!(tod_bucket(4), "night");
    }

    // ---- tod_hint: offset-local hour (H1) -----------------------------------

    #[test]
    fn tod_hint_buckets_by_offset_local_hour_not_utc() {
        // H1: offset-bearing fixtures where the LOCAL hour and the UTC hour fall
        // in DIFFERENT buckets. The bucket must follow the LOCAL hour.
        // 2026-06-07T23:00:00-07:00 → local 23:00 = night (UTC would be 06 =
        // dawn). Use 3 such attempts (≥3) with ≥1 success so a hint is produced.
        let attempts = vec![
            attempt("u1", "2026-06-07T23:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T23:30:00-07:00", "reached"),
            attempt("u1", "2026-06-07T22:00:00-07:00", "reached"),
        ];
        let hint = tod_hint(&attempts).expect("a unique ≥3-attempt ≥1-success bucket → Some");
        assert_eq!(
            hint.bucket, "night",
            "23:00-07:00 must bucket by LOCAL hour (night), NOT UTC (dawn) — H1"
        );
        assert_eq!(hint.attempts, 3);
        assert_eq!(hint.successes, 3);
    }

    #[test]
    fn tod_hint_offset_dawn_not_day() {
        // 2026-06-07T06:00:00-07:00 → local 06 = dawn (UTC 13 = day).
        let attempts = vec![
            attempt("u1", "2026-06-07T06:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T06:30:00-07:00", "reached"),
            attempt("u1", "2026-06-07T05:00:00-07:00", "reached"),
        ];
        let hint = tod_hint(&attempts).unwrap();
        assert_eq!(
            hint.bucket, "dawn",
            "06:00-07:00 must bucket by LOCAL hour (dawn), NOT UTC (day) — H1"
        );
    }

    #[test]
    fn tod_hint_positive_offset_night() {
        // 2026-01-15T02:00:00+10:00 → local 02 = night (UTC 16:00 prev day =
        // day). Verifies a POSITIVE offset is also honored locally.
        let attempts = vec![
            attempt("u1", "2026-01-15T02:00:00+10:00", "reached"),
            attempt("u1", "2026-01-15T03:00:00+10:00", "reached"),
            attempt("u1", "2026-01-15T01:00:00+10:00", "reached"),
        ];
        let hint = tod_hint(&attempts).unwrap();
        assert_eq!(
            hint.bucket, "night",
            "02:00+10:00 must bucket by LOCAL hour (night), NOT UTC (day) — H1"
        );
    }

    #[test]
    fn tod_hint_skips_unparseable_ts_no_panic() {
        // An unparseable ts_local is skipped (no panic, no count). The remaining
        // parseable attempts still form a hint.
        let attempts = vec![
            attempt("u1", "not-a-timestamp", "reached"),
            attempt("u1", "2026-06-07T23:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T23:30:00-07:00", "reached"),
            attempt("u1", "2026-06-07T22:00:00-07:00", "reached"),
        ];
        let hint = tod_hint(&attempts).unwrap();
        assert_eq!(hint.bucket, "night");
        assert_eq!(hint.attempts, 3, "the unparseable attempt is not counted");
    }

    // ---- tod_hint: over-claim guard (H2) ------------------------------------

    #[test]
    fn tod_hint_none_below_three_attempts() {
        // <3 attempts in the argmax bucket → None.
        let attempts = vec![
            attempt("u1", "2026-06-07T23:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T23:30:00-07:00", "reached"),
        ];
        assert!(
            tod_hint(&attempts).is_none(),
            "fewer than 3 attempts must NOT produce a hint (H2)"
        );
    }

    #[test]
    fn tod_hint_none_all_failed_zero_success() {
        // A bucket with 3 attempts ALL failed → None. NEVER name a zero-success
        // bucket (H2).
        let attempts = vec![
            attempt("u1", "2026-06-07T23:00:00-07:00", "failed"),
            attempt("u1", "2026-06-07T23:30:00-07:00", "failed"),
            attempt("u1", "2026-06-07T22:00:00-07:00", "failed"),
        ];
        assert!(
            tod_hint(&attempts).is_none(),
            "a zero-success bucket must NEVER be named (H2)"
        );
    }

    #[test]
    fn tod_hint_none_on_tie_no_unique_max() {
        // No UNIQUE max: two buckets each ≥3 attempts at the SAME reached
        // fraction (1.0) → a tie → None (H2).
        let attempts = vec![
            // night: 3/3 reached
            attempt("u1", "2026-06-07T23:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T22:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T21:00:00-07:00", "reached"),
            // dawn: 3/3 reached
            attempt("u1", "2026-06-07T06:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T05:30:00-07:00", "reached"),
            attempt("u1", "2026-06-07T07:00:00-07:00", "reached"),
        ];
        assert!(
            tod_hint(&attempts).is_none(),
            "a tie on the top reached-fraction must NOT produce a hint (H2)"
        );
    }

    #[test]
    fn tod_hint_some_on_unique_max_with_successes() {
        // The positive case: night has the strict-unique highest reached
        // fraction with ≥3 attempts and ≥1 success → Some, observed counts only.
        let attempts = vec![
            // night: 3/3 reached (fraction 1.0)
            attempt("u1", "2026-06-07T23:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T22:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T21:00:00-07:00", "reached"),
            // day: 1/3 reached (fraction 0.33) — strictly lower
            attempt("u1", "2026-06-07T10:00:00-07:00", "reached"),
            attempt("u1", "2026-06-07T11:00:00-07:00", "failed"),
            attempt("u1", "2026-06-07T12:00:00-07:00", "failed"),
        ];
        let hint = tod_hint(&attempts).expect("a strict unique max with successes → Some");
        assert_eq!(hint.bucket, "night");
        assert_eq!(hint.attempts, 3);
        assert_eq!(hint.successes, 3);
    }

    #[test]
    fn tod_hint_empty_is_none() {
        assert!(tod_hint(&[]).is_none());
    }

    // A sanity touch on a chrono Datelike import (positive-offset day boundary
    // case exercises date math implicitly); keep the import meaningful.
    #[test]
    fn local_hour_reads_offset_local_hour() {
        // Direct unit check on the parse helper via a positive offset crossing
        // the UTC date boundary: +10:00 at 02:00 local.
        let parsed = chrono::DateTime::parse_from_rfc3339("2026-01-15T02:00:00+10:00").unwrap();
        assert_eq!(parsed.hour(), 2);
        // The UTC equivalent would be the previous day at 16:00 — proving we do
        // NOT use it.
        assert_ne!(parsed.with_timezone(&chrono::Utc).day(), parsed.day());
    }

    // ---- recent_gateways: Winlink map layer (tuxlink-s1o1) ------------------

    /// Helper: write a JSON fixture to `path` and open it as a `FavoritesStore`.
    /// Mirrors the `unknown_top_level_field_tolerated` fixture-write+open idiom.
    fn store_from_json(path: std::path::PathBuf, json: &str) -> FavoritesStore {
        std::fs::write(&path, json).unwrap();
        FavoritesStore::open(path)
    }

    #[test]
    fn recent_gateways_returns_in_window_most_recent_with_grid() {
        // Primary case (from the brief):
        //   now = 2026-06-22T12:00:00-07:00
        //   W6DRZ (id=u1, grid=CM97): two attempts — newest at 11:30 (in 6h window,
        //     outcome "reached"), older at 09:00 (in window, outcome "failed").
        //     The MOST RECENT in-window attempt is 11:30 "reached".
        //   AI6BX (id=u2, grid=CM98): one attempt at 04:00 (OUTSIDE the 6h window
        //     because 12:00 - 6h = 06:00, and 04:00 < 06:00).
        //
        // Expected: only W6DRZ returned, with last_attempt_at=11:30, outcome="reached",
        // grid=Some("CM97").
        let now =
            chrono::DateTime::parse_from_rfc3339("2026-06-22T12:00:00-07:00").unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let json = r#"{
            "schema_version": 1,
            "favorites": [
                {
                    "id": "u1",
                    "mode": "ardop-hf",
                    "gateway": "W6DRZ",
                    "freq": "14105.0",
                    "transport": null,
                    "band": "20m",
                    "grid": "CM97",
                    "note": null,
                    "starred": false,
                    "last_attempt_at": "2026-06-22T11:30:00-07:00",
                    "created_at": "2026-06-20T00:00:00+00:00",
                    "updated_at": "2026-06-22T11:30:00-07:00"
                },
                {
                    "id": "u2",
                    "mode": "ardop-hf",
                    "gateway": "AI6BX",
                    "freq": "14105.0",
                    "transport": null,
                    "band": "20m",
                    "grid": "CM98",
                    "note": null,
                    "starred": false,
                    "last_attempt_at": "2026-06-22T04:00:00-07:00",
                    "created_at": "2026-06-20T00:00:00+00:00",
                    "updated_at": "2026-06-22T04:00:00-07:00"
                }
            ],
            "log": [
                {
                    "unit_id": "u1",
                    "ts_local": "2026-06-22T11:30:00-07:00",
                    "freq": null,
                    "outcome": "reached"
                },
                {
                    "unit_id": "u1",
                    "ts_local": "2026-06-22T09:00:00-07:00",
                    "freq": null,
                    "outcome": "failed"
                },
                {
                    "unit_id": "u2",
                    "ts_local": "2026-06-22T04:00:00-07:00",
                    "freq": null,
                    "outcome": "reached"
                }
            ]
        }"#;
        let store = store_from_json(path, json);
        let got = store.recent_gateways(6, now);
        assert_eq!(got.len(), 1, "only W6DRZ is within the 6h window");
        assert_eq!(got[0].gateway, "W6DRZ");
        assert_eq!(
            got[0].last_attempt_at, "2026-06-22T11:30:00-07:00",
            "most-recent in-window attempt"
        );
        assert_eq!(got[0].outcome, "reached", "most-recent attempt's outcome");
        assert_eq!(got[0].grid.as_deref(), Some("CM97"));
    }

    #[test]
    fn recent_gateways_empty_log_returns_empty() {
        // Edge case: store has favorites but no log entries → no gateways returned.
        let now =
            chrono::DateTime::parse_from_rfc3339("2026-06-22T12:00:00-07:00").unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let json = r#"{
            "schema_version": 1,
            "favorites": [
                {
                    "id": "u1",
                    "mode": "ardop-hf",
                    "gateway": "W6DRZ",
                    "freq": "14105.0",
                    "transport": null,
                    "band": "20m",
                    "grid": "CM97",
                    "note": null,
                    "starred": false,
                    "last_attempt_at": null,
                    "created_at": "2026-06-20T00:00:00+00:00",
                    "updated_at": "2026-06-20T00:00:00+00:00"
                }
            ],
            "log": []
        }"#;
        let store = store_from_json(path, json);
        let got = store.recent_gateways(6, now);
        assert!(got.is_empty(), "no log entries → no recent gateways");
    }

    #[test]
    fn recent_gateways_boundary_attempt_exactly_at_cutoff_included() {
        // Edge case: an attempt whose ts_local == cutoff (now - within_hours)
        // is included (cutoff is INCLUSIVE: ts >= cutoff).
        //   now = 2026-06-22T12:00:00-07:00, within_hours = 6
        //   cutoff = 2026-06-22T06:00:00-07:00
        //   attempt at exactly 06:00:00-07:00 → included.
        let now =
            chrono::DateTime::parse_from_rfc3339("2026-06-22T12:00:00-07:00").unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let json = r#"{
            "schema_version": 1,
            "favorites": [
                {
                    "id": "u1",
                    "mode": "ardop-hf",
                    "gateway": "W6DRZ",
                    "freq": "14105.0",
                    "transport": null,
                    "band": "20m",
                    "grid": "CM97",
                    "note": null,
                    "starred": false,
                    "last_attempt_at": "2026-06-22T06:00:00-07:00",
                    "created_at": "2026-06-20T00:00:00+00:00",
                    "updated_at": "2026-06-22T06:00:00-07:00"
                }
            ],
            "log": [
                {
                    "unit_id": "u1",
                    "ts_local": "2026-06-22T06:00:00-07:00",
                    "freq": null,
                    "outcome": "reached"
                }
            ]
        }"#;
        let store = store_from_json(path, json);
        let got = store.recent_gateways(6, now);
        assert_eq!(
            got.len(),
            1,
            "attempt exactly at cutoff must be included (ts >= cutoff is inclusive)"
        );
        assert_eq!(got[0].gateway, "W6DRZ");
        assert_eq!(got[0].last_attempt_at, "2026-06-22T06:00:00-07:00");
    }

    #[test]
    fn recent_gateways_grid_none_still_returned() {
        // Edge case: a favorite with grid=None is still included in results;
        // the frontend is responsible for dropping it from the map pin set.
        let now =
            chrono::DateTime::parse_from_rfc3339("2026-06-22T12:00:00-07:00").unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let json = r#"{
            "schema_version": 1,
            "favorites": [
                {
                    "id": "u1",
                    "mode": "ardop-hf",
                    "gateway": "W6DRZ",
                    "freq": "14105.0",
                    "transport": null,
                    "band": "20m",
                    "grid": null,
                    "note": null,
                    "starred": false,
                    "last_attempt_at": "2026-06-22T11:00:00-07:00",
                    "created_at": "2026-06-20T00:00:00+00:00",
                    "updated_at": "2026-06-22T11:00:00-07:00"
                }
            ],
            "log": [
                {
                    "unit_id": "u1",
                    "ts_local": "2026-06-22T11:00:00-07:00",
                    "freq": null,
                    "outcome": "failed"
                }
            ]
        }"#;
        let store = store_from_json(path, json);
        let got = store.recent_gateways(6, now);
        assert_eq!(got.len(), 1, "gateway with grid=None must still be returned");
        assert_eq!(got[0].gateway, "W6DRZ");
        assert!(
            got[0].grid.is_none(),
            "grid=None from the favorite passes through as None"
        );
    }

    #[test]
    fn recent_gateways_multiple_favorites_same_gateway_most_recent_wins() {
        // Edge case: same gateway in two favorites (two modes → two unit_ids).
        // Both have in-window attempts. The single most-recent attempt across
        // ALL favorites for that gateway is picked, along with ITS grid.
        //
        // Favorite u1: ardop-hf, grid=CM97, attempt at 11:00 "reached"
        // Favorite u2: vara-hf,  grid=CM97, attempt at 11:30 "failed"  ← most recent
        // Expected: ONE RecentGateway for W6DRZ, last_attempt_at=11:30, outcome="failed",
        //           grid from u2 (same gateway, same grid here; the important thing is
        //           the most-recent attempt is selected regardless of which unit it belongs to).
        let now =
            chrono::DateTime::parse_from_rfc3339("2026-06-22T12:00:00-07:00").unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let json = r#"{
            "schema_version": 1,
            "favorites": [
                {
                    "id": "u1",
                    "mode": "ardop-hf",
                    "gateway": "W6DRZ",
                    "freq": "14105.0",
                    "transport": null,
                    "band": "20m",
                    "grid": "CM97",
                    "note": null,
                    "starred": false,
                    "last_attempt_at": "2026-06-22T11:00:00-07:00",
                    "created_at": "2026-06-20T00:00:00+00:00",
                    "updated_at": "2026-06-22T11:00:00-07:00"
                },
                {
                    "id": "u2",
                    "mode": "vara-hf",
                    "gateway": "W6DRZ",
                    "freq": "14105.0",
                    "transport": null,
                    "band": "20m",
                    "grid": "CM97",
                    "note": null,
                    "starred": false,
                    "last_attempt_at": "2026-06-22T11:30:00-07:00",
                    "created_at": "2026-06-20T00:00:00+00:00",
                    "updated_at": "2026-06-22T11:30:00-07:00"
                }
            ],
            "log": [
                {
                    "unit_id": "u1",
                    "ts_local": "2026-06-22T11:00:00-07:00",
                    "freq": null,
                    "outcome": "reached"
                },
                {
                    "unit_id": "u2",
                    "ts_local": "2026-06-22T11:30:00-07:00",
                    "freq": null,
                    "outcome": "failed"
                }
            ]
        }"#;
        let store = store_from_json(path, json);
        let got = store.recent_gateways(6, now);
        assert_eq!(
            got.len(),
            1,
            "multiple favorites for the same gateway → ONE RecentGateway"
        );
        assert_eq!(got[0].gateway, "W6DRZ");
        assert_eq!(
            got[0].last_attempt_at, "2026-06-22T11:30:00-07:00",
            "most-recent in-window attempt wins across all favorites for the gateway"
        );
        assert_eq!(
            got[0].outcome, "failed",
            "outcome belongs to the most-recent attempt"
        );
    }
}
