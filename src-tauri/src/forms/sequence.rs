//! Per-form message serial counters (tuxlink-2tom / G12-C — `SeqInc:`).
//!
//! WLE forms carrying the `SeqInc:` directive auto-number each send: a per-form
//! serial counter increments on every send and is substituted into the message's
//! `{SeqNum}` / `<var SeqNum>` placeholders (radiogram / RRI / net-log serials).
//! The counter is durable (survives restarts) and resettable via Settings.
//!
//! State model: the store maps `form_id -> last_used` serial. [`allocate`] returns
//! `last_used + 1` and persists it, so the NEXT send gets the following number.
//! Allocation happens at SEND time and persists BEFORE the network send, so a
//! failed send burns a number (a serial gap) rather than risking a duplicate from
//! a concurrent retry — gaps are operationally harmless; duplicate serials are not.
//!
//! [`allocate`]: SeqCounterStore::allocate

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Persisted per-form serial counters. Mirrors the infallible-open / atomic-save
/// contract of `ContactsStore` / `FavoritesStore`: a read/parse error degrades to
/// an empty store (never blocks startup), and saves write-tmp-then-rename.
#[derive(Debug)]
pub struct SeqCounterStore {
    path: PathBuf,
    /// `form_id -> last-used serial`. `BTreeMap` for deterministic status order.
    counters: BTreeMap<String, u64>,
}

impl SeqCounterStore {
    /// Open the store at `path`, loading existing counters. INFALLIBLE: a missing
    /// file yields an empty store; a malformed file logs + degrades to empty
    /// (preserving the on-disk bytes until the next successful save).
    pub fn open(path: PathBuf) -> Self {
        let counters = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                eprintln!(
                    "forms::sequence: {} is malformed, starting empty: {e}",
                    path.display()
                );
                BTreeMap::new()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => {
                eprintln!(
                    "forms::sequence: failed to read {}: {e}",
                    path.display()
                );
                BTreeMap::new()
            }
        };
        Self { path, counters }
    }

    /// Allocate the next serial for `form_id`: increment last-used, persist, and
    /// return the new value. First allocation for an unseen form returns 1.
    pub fn allocate(&mut self, form_id: &str) -> u64 {
        let n = self.counters.entry(form_id.to_string()).or_insert(0);
        *n += 1;
        let value = *n;
        self.save();
        value
    }

    /// The next serial `form_id` would receive (`last_used + 1`) without consuming
    /// it. Unseen forms return 1.
    pub fn peek(&self, form_id: &str) -> u64 {
        self.counters.get(form_id).copied().unwrap_or(0) + 1
    }

    /// Set `form_id`'s NEXT serial to `next` (so the following [`allocate`] returns
    /// `next`). `next` is clamped to `>= 1`. Persists.
    ///
    /// [`allocate`]: SeqCounterStore::allocate
    pub fn set_next(&mut self, form_id: &str, next: u64) {
        let last = next.max(1) - 1;
        self.counters.insert(form_id.to_string(), last);
        self.save();
    }

    /// Every form with a counter, as `(form_id, next_serial)` pairs ordered by id.
    pub fn status(&self) -> Vec<(String, u64)> {
        self.counters
            .iter()
            .map(|(k, v)| (k.clone(), v + 1))
            .collect()
    }

    /// Atomic write: serialize to `<path>.json.tmp`, then rename over. Best-effort
    /// — a write failure is logged, not propagated (matches the degrade-and-keep-
    /// running contract; the in-memory counter stays authoritative for the run).
    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("json.tmp");
        match serde_json::to_string_pretty(&self.counters) {
            Ok(json) => {
                if let Err(e) =
                    std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, &self.path))
                {
                    eprintln!(
                        "forms::sequence: failed to persist {}: {e}",
                        self.path.display()
                    );
                }
            }
            Err(e) => eprintln!("forms::sequence: serialize failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &std::path::Path) -> SeqCounterStore {
        SeqCounterStore::open(dir.join("sequence_counters.json"))
    }

    #[test]
    fn open_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let s = store_in(dir.path());
        assert!(s.status().is_empty());
        assert_eq!(s.peek("ICS213"), 1, "unseen form's next serial is 1");
    }

    #[test]
    fn allocate_increments_from_one_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = store_in(dir.path());
            assert_eq!(s.allocate("IARU"), 1);
            assert_eq!(s.allocate("IARU"), 2);
            assert_eq!(s.allocate("IARU"), 3);
            // A different form has its own independent counter.
            assert_eq!(s.allocate("RRI"), 1);
        }
        // Reopen: counters survive (durable across "restart").
        let s2 = store_in(dir.path());
        assert_eq!(s2.peek("IARU"), 4, "next after 3 allocations is 4");
        assert_eq!(s2.peek("RRI"), 2);
    }

    #[test]
    fn peek_does_not_consume() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store_in(dir.path());
        s.allocate("X"); // last_used = 1
        assert_eq!(s.peek("X"), 2);
        assert_eq!(s.peek("X"), 2, "peek is non-mutating");
    }

    #[test]
    fn set_next_controls_the_following_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store_in(dir.path());
        s.allocate("X"); // 1
        s.set_next("X", 100);
        assert_eq!(s.peek("X"), 100);
        assert_eq!(s.allocate("X"), 100, "set_next makes the next allocate return it");
        assert_eq!(s.allocate("X"), 101);
        // Clamp: next < 1 is treated as 1.
        s.set_next("X", 0);
        assert_eq!(s.allocate("X"), 1);
    }

    #[test]
    fn status_lists_next_serial_per_form_ordered() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store_in(dir.path());
        s.allocate("Zebra"); // next 2
        s.allocate("Alpha");
        s.allocate("Alpha"); // next 3
        let st = s.status();
        assert_eq!(st, vec![("Alpha".to_string(), 3), ("Zebra".to_string(), 2)]);
    }

    #[test]
    fn malformed_file_degrades_to_empty_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sequence_counters.json"), b"{not json").unwrap();
        let s = store_in(dir.path());
        assert!(s.status().is_empty());
    }
}
