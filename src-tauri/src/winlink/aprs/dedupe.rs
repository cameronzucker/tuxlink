use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupeKey {
    pub src: String,  // "CALL-SSID"
    pub kind: String, // "msg" | "ack" | "rej" | "ackout"
    pub id: String,   // msgid, or a text hash for msgid-less messages
}

/// Time-windowed duplicate suppressor. `seen` returns true if this key was seen
/// within `window_ms` before `now_ms`; otherwise records it and returns false.
pub struct DedupeCache {
    window_ms: u64,
    last_seen: HashMap<DedupeKey, u64>,
}

impl DedupeCache {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            last_seen: HashMap::new(),
        }
    }

    pub fn seen(&mut self, key: DedupeKey, now_ms: u64) -> bool {
        // Opportunistic prune so the map can't grow unbounded on a busy channel.
        self.last_seen
            .retain(|_, &mut t| now_ms.saturating_sub(t) <= self.window_ms);
        match self.last_seen.get(&key) {
            Some(&t) if now_ms.saturating_sub(t) <= self.window_ms => {
                self.last_seen.insert(key, now_ms);
                true
            }
            _ => {
                self.last_seen.insert(key, now_ms);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_identical_within_window_is_duplicate() {
        let mut c = DedupeCache::new(30_000); // 30s window
        let key = DedupeKey {
            src: "N0CALL-9".into(),
            kind: "msg".into(),
            id: "01".into(),
        };
        assert!(!c.seen(key.clone(), 1000)); // first sighting at t=1s
        assert!(c.seen(key.clone(), 5000)); // again at t=5s within window => duplicate
    }

    #[test]
    fn identical_after_window_is_fresh() {
        let mut c = DedupeCache::new(30_000);
        let key = DedupeKey {
            src: "N0CALL-9".into(),
            kind: "msg".into(),
            id: "01".into(),
        };
        assert!(!c.seen(key.clone(), 1000));
        assert!(!c.seen(key.clone(), 40_000)); // 39s later, window expired => fresh
    }

    #[test]
    fn different_msgid_is_fresh() {
        let mut c = DedupeCache::new(30_000);
        assert!(!c.seen(
            DedupeKey {
                src: "A-1".into(),
                kind: "msg".into(),
                id: "01".into(),
            },
            1000
        ));
        assert!(!c.seen(
            DedupeKey {
                src: "A-1".into(),
                kind: "msg".into(),
                id: "02".into(),
            },
            1100
        ));
    }
}
