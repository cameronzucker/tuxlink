//! Peer data model (spec §1/§2). `peers.json` on-disk shapes.
//!
//! Serde policy (mirrors `contacts/store.rs`): additive tolerance via
//! `#[serde(default)]`, NO `deny_unknown_fields`, every enum carries a
//! `#[serde(other)] Unknown` catch-all so a future variant does NOT fail the
//! whole roster load — the row and the file both keep loading [R4-5].
//!
//! **Known alpha limitation:** `#[serde(other)] Unknown` is forward-compat
//! for LOAD only. It does NOT preserve the original unrecognized value —
//! `Unknown` re-serializes as the literal string `"unknown"` on the next
//! flush, so a value written by a newer binary and then round-tripped
//! through an older one is silently downgraded to `unknown`, not restored.
//! This is accepted for alpha; do not read "quarantines" as "preserves."

use serde::{Deserialize, Serialize};

/// On-disk schema version. Bumped only on a non-additive shape change.
pub const SCHEMA_VERSION: u32 = 1;

/// Soft cap on auto-created records; over-cap eviction is LRU among
/// `RecordSource::Auto` records only [R2-S6][R1-C9].
pub const AUTO_PEER_CAP: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityKind {
    Individual,
    /// Tactical calls have no standard structure; a Tactical peer's dedup
    /// anchor is its FULL presented string, never base-normalized [R4-6].
    Tactical,
    Club,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RecordSource {
    /// Created by the recorder. Evictable under the cap.
    #[default]
    Auto,
    /// Operator-added. Never evicted.
    Manual,
    /// Operator-pinned. Never evicted.
    OperatorPinned,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    Incoming,
    Outgoing,
    Manual,
    Aprs,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GridSource {
    Contact,
    Aprs,
    Manual,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelTransport {
    Packet,
    Ardop,
    VaraHf,
    VaraFm,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Incoming,
    Outgoing,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    /// Operator-entered or operator-promoted. The ONLY agent-dialable
    /// provenance (spec §4 I1). Monotonic: never downgraded [R4-4][R2-S8].
    Operator,
    /// Learned because a station connected to us. Agent-non-dialable,
    /// never auto-promoted, badged "unverified claimed identity".
    #[default]
    ObservedIncoming,
    #[serde(other)]
    Unknown,
}

/// Bandwidth observed on a CONNECTED line. Internally tagged so the one
/// data-carrying variant coexists with `#[serde(other)]` (the officially
/// supported form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ChannelBandwidth {
    Hz { hz: u32 },
    Wide,
    Narrow,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerGrid {
    pub value: String,
    pub source: GridSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AttemptCounts {
    #[serde(default)]
    pub ok: u32,
    #[serde(default)]
    pub fail: u32,
}

/// One RF reachability observation row (spec §2). Dedup key:
/// `(transport, target_callsign, via, freq_hz, bandwidth)` [R4-11].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub transport: ChannelTransport,
    /// EXACT SSID'd callsign for the wire (e.g. `N0DAJ-7`). The wire target
    /// of any dial is always this, never `canonical_base` [R3-9].
    pub target_callsign: String,
    /// Digipeater path, max 2 (packet / VARA FM); empty = direct [R3-6].
    #[serde(default)]
    pub via: Vec<String>,
    /// Center frequency, exact Hz (catalog semantics, #1064) — no rounding.
    #[serde(default)]
    pub freq_hz: Option<u64>,
    #[serde(default)]
    pub bandwidth: Option<ChannelBandwidth>,
    /// Most recent direction observed on this channel.
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub counts: AttemptCounts,
    pub last_seen: String,
}

/// One network reachability row (telnet P2P).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Stable id — keyring key component (`p2p-endpoint:<peer_id>:<id>`).
    /// Promotion mutates provenance IN PLACE on this id so the keyring
    /// secret is never orphaned [R5-5].
    pub id: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub provenance: Provenance,
    pub last_seen: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    /// Stable unique id — the primary key (uuid v4; house pattern).
    pub id: String,
    /// Dedup anchor ONLY (spec §1). Never a wire source.
    pub canonical_base: String,
    /// Every exact form observed or dialed, deduped, verbatim.
    #[serde(default)]
    pub presented_callsigns: Vec<String>,
    #[serde(default)]
    pub identity_kind: IdentityKind,
    /// Set when the operator splits; suppresses auto-merge forever [R5-4].
    #[serde(default)]
    pub do_not_merge: bool,
    /// Held-for-manual-association marker: an observation on a split base
    /// that matched no split record's presented callsigns [R5-4].
    #[serde(default)]
    pub conflict: bool,
    #[serde(default)]
    pub source: RecordSource,
    #[serde(default)]
    pub origin: Origin,
    /// One-way link into contacts.json (spec §Cross-store).
    #[serde(default)]
    pub contact_id: Option<String>,
    #[serde(default)]
    pub grid: Option<PeerGrid>,
    /// Operator free-text. NEVER crosses the agent surface (spec §4).
    #[serde(default)]
    pub note: String,
    pub created_at: String,
    #[serde(default)]
    pub last_connected_at: Option<String>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeersFile {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub peers: Vec<Peer>,
}

// Hand-written Default so schema_version is 1, not 0 (contacts M1 pattern).
impl Default for PeersFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            peers: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_enum_variants_quarantine_the_field_not_the_roster() {
        // [R4-5]: a variant written by a future binary deserializes to
        // Unknown; the row and roster survive.
        let j = r#"{"schema_version":1,"peers":[{
            "id":"p1","canonical_base":"W6ABC","presented_callsigns":["W6ABC-7"],
            "identity_kind":"quantum-club","do_not_merge":false,"conflict":false,
            "source":"auto","origin":"time-travel","contact_id":null,"grid":null,
            "note":"","created_at":"2026-07-10T12:00:00-07:00",
            "last_connected_at":null,"channels":[],"endpoints":[]}]}"#;
        let f: PeersFile = serde_json::from_str(j).expect("unknown variants must not fail the load");
        assert_eq!(f.peers[0].identity_kind, IdentityKind::Unknown);
        assert_eq!(f.peers[0].origin, Origin::Unknown);
    }

    #[test]
    fn bandwidth_round_trips_all_kinds() {
        for bw in [
            ChannelBandwidth::Hz { hz: 2300 },
            ChannelBandwidth::Wide,
            ChannelBandwidth::Narrow,
        ] {
            let s = serde_json::to_string(&bw).unwrap();
            assert_eq!(serde_json::from_str::<ChannelBandwidth>(&s).unwrap(), bw);
        }
        // Future kind → Unknown, not a load failure.
        let f: ChannelBandwidth = serde_json::from_str(r#"{"kind":"ultra"}"#).unwrap();
        assert_eq!(f, ChannelBandwidth::Unknown);
    }

    #[test]
    fn enum_wire_tags_are_kebab_case() {
        // Shape pins per the serde rename_all memory: tags only, explicit test.
        assert_eq!(serde_json::to_string(&RecordSource::OperatorPinned).unwrap(), r#""operator-pinned""#);
        assert_eq!(serde_json::to_string(&Provenance::ObservedIncoming).unwrap(), r#""observed-incoming""#);
        assert_eq!(serde_json::to_string(&ChannelTransport::VaraFm).unwrap(), r#""vara-fm""#);
        assert_eq!(serde_json::to_string(&Origin::Incoming).unwrap(), r#""incoming""#);
        assert_eq!(serde_json::to_string(&IdentityKind::Individual).unwrap(), r#""individual""#);
        assert_eq!(serde_json::to_string(&GridSource::Contact).unwrap(), r#""contact""#);
        assert_eq!(serde_json::to_string(&Direction::Outgoing).unwrap(), r#""outgoing""#);
    }

    #[test]
    fn bandwidth_wire_shape_is_pinned_exactly() {
        // The "kind" tag key + field names are the on-disk / TS contract —
        // pin the exact JSON strings, not just self-round-trips.
        assert_eq!(
            serde_json::to_string(&ChannelBandwidth::Hz { hz: 2300 }).unwrap(),
            r#"{"kind":"hz","hz":2300}"#
        );
        assert_eq!(
            serde_json::to_string(&ChannelBandwidth::Wide).unwrap(),
            r#"{"kind":"wide"}"#
        );
        assert_eq!(
            serde_json::to_string(&ChannelBandwidth::Narrow).unwrap(),
            r#"{"kind":"narrow"}"#
        );
        // Bidirectional pin: the exact wire string parses back to the variant.
        assert_eq!(
            serde_json::from_str::<ChannelBandwidth>(r#"{"kind":"hz","hz":2300}"#).unwrap(),
            ChannelBandwidth::Hz { hz: 2300 }
        );
    }

    #[test]
    fn default_file_has_schema_version_1() {
        let f = PeersFile::default();
        assert_eq!(f.schema_version, SCHEMA_VERSION);
        assert_eq!(f.schema_version, 1);
        assert!(f.peers.is_empty());
    }
}
