//! Contact reachability model — the on-disk shapes for a contact's observed
//! RF channels, network endpoints, grid, and tier (spec §AMENDMENT pts. 1-2:
//! docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md).
//!
//! These types moved VERBATIM from the deleted `peers/model.rs` (operator
//! pivot 2026-07-10/11: a peer IS a contact; the separate `peers.json` entity
//! died). The exact-SSID wire-target rule and every serde `other` fallback
//! carry over unchanged; what died with the peer entity is the identity-merge
//! machinery (`canonical_base`-as-merge-key, `presented_callsigns`,
//! `identity_kind`, `do_not_merge`, conflict records, `RecordSource`).
//!
//! Serde policy (mirrors `contacts/store.rs`): additive tolerance via
//! `#[serde(default)]`, NO `deny_unknown_fields`, every unit-only enum carries
//! a `#[serde(other)] Unknown` catch-all so a future variant does NOT fail the
//! whole address book load — the row and the file both keep loading [R4-5].
//!
//! **Known alpha limitation:** `#[serde(other)] Unknown` is forward-compat
//! for LOAD only. It does NOT preserve the original unrecognized value —
//! `Unknown` re-serializes as the literal string `"unknown"` on the next
//! flush, so a value written by a newer binary and then round-tripped
//! through an older one is silently downgraded to `unknown`, not restored.
//! This is accepted for alpha; do not read "quarantines" as "preserves."

use serde::{Deserialize, Serialize};

/// Soft cap on auto-created (`ContactTier::Unconfirmed`) records; over-cap
/// eviction is LRU by last-seen among unconfirmed records only [R2-S6][R1-C9].
/// `Confirmed` contacts are never auto-created and never evicted (spec
/// §AMENDMENT pt. 8).
pub const AUTO_CONTACT_CAP: usize = 1000;

/// The contact tier (spec §AMENDMENT pt. 1). `Confirmed` = the operator added
/// or confirmed this entry into the address book — the curated tier, exactly
/// the pre-pivot Contact semantics. `Unconfirmed` = auto-created from a P2P
/// session observation or a manual dial; never silently pollutes the curated
/// tier. "Confirmed" claims CURATION, not identity authentication (pt. 3):
/// anyone can transmit any callsign.
///
/// `#[serde(default)]` on the Contact field = `Confirmed`, so every v1 record
/// (written before this field existed) loads as confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ContactTier {
    #[default]
    Confirmed,
    Unconfirmed,
    #[serde(other)]
    Unknown,
}

/// How this contact record came to exist, in plain language: incoming
/// (they dialed us) / outgoing (we dialed them) / added (operator-entered).
/// Feeds the UI "Heard vs dialed" distinction.
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

/// Where a contact's grid value came from. The pivot dropped the peer-model
/// `Contact` variant (a contact sourcing its grid "from a contact" is
/// meaningless now that the grid lives ON the contact).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GridSource {
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
    /// Operator-entered. A stored password attaches to, and is only ever
    /// used with, an operator-entered endpoint (spec [R2-S7], AMENDMENT
    /// pt. 3). Monotonic: an inbound observation never creates or mutates
    /// an Operator endpoint [R4-4][R2-S8].
    Operator,
    /// Learned because a station connected to us. The claimed back-dial
    /// address of a spoofable callsign — never password-bearing.
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

/// A contact's grid + where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactGrid {
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
    /// of any dial is always this exact presented form [R3-9].
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
    /// Timestamp of the most recent attempt on this channel, OK or FAIL (bumps
    /// on every observation). Recency for the map/rail must NOT derive from
    /// this — a failed dial bumps it too. Use [`Channel::last_ok`] for any
    /// "reached / heard" truth-claim.
    pub last_seen: String,
    /// Timestamp of the most recent SUCCESSFUL (`Classified::Ok`) attempt on
    /// this channel; `None` until one completes. This is the only honest
    /// source for a "reached / heard" label — a failed attempt never sets it
    /// and never clears a prior success (T-F Part 0). `#[serde(default)]` so
    /// every pre-T-F record loads with `None`.
    #[serde(default)]
    pub last_ok: Option<String>,
}

/// One network reachability row (telnet P2P).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Stable id — keyring key component (`p2p-endpoint:<contact_id>:<id>`).
    pub id: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub provenance: Provenance,
    /// Most recent attempt on this endpoint, OK or FAIL. See
    /// [`Channel::last_seen`] — not a reachability truth-source.
    pub last_seen: String,
    /// Most recent SUCCESSFUL attempt on this endpoint; `None` until one
    /// completes. The only honest "reached / heard" source (T-F Part 0).
    #[serde(default)]
    pub last_ok: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_wire_shape_is_pinned_exactly() {
        // The kebab-case tags are the on-disk / TS contract.
        assert_eq!(serde_json::to_string(&ContactTier::Confirmed).unwrap(), r#""confirmed""#);
        assert_eq!(serde_json::to_string(&ContactTier::Unconfirmed).unwrap(), r#""unconfirmed""#);
        assert_eq!(serde_json::to_string(&ContactTier::Unknown).unwrap(), r#""unknown""#);
        // Bidirectional pin + forward-compat: a future tier loads as Unknown.
        assert_eq!(
            serde_json::from_str::<ContactTier>(r#""unconfirmed""#).unwrap(),
            ContactTier::Unconfirmed
        );
        assert_eq!(
            serde_json::from_str::<ContactTier>(r#""platinum""#).unwrap(),
            ContactTier::Unknown
        );
        // The Default IS Confirmed — the v1 migration hinges on this.
        assert_eq!(ContactTier::default(), ContactTier::Confirmed);
    }

    #[test]
    fn enum_wire_tags_are_kebab_case() {
        // Shape pins per the serde rename_all memory: tags only, explicit test.
        assert_eq!(serde_json::to_string(&Provenance::ObservedIncoming).unwrap(), r#""observed-incoming""#);
        assert_eq!(serde_json::to_string(&ChannelTransport::VaraFm).unwrap(), r#""vara-fm""#);
        assert_eq!(serde_json::to_string(&Origin::Incoming).unwrap(), r#""incoming""#);
        assert_eq!(serde_json::to_string(&Direction::Outgoing).unwrap(), r#""outgoing""#);
        assert_eq!(serde_json::to_string(&GridSource::Manual).unwrap(), r#""manual""#);
        assert_eq!(serde_json::to_string(&GridSource::Aprs).unwrap(), r#""aprs""#);
    }

    #[test]
    fn grid_source_dropped_contact_variant_falls_back_to_unknown() {
        // The pivot dropped GridSource::Contact; a stored `"contact"` value
        // (written by the pre-pivot peer store) quarantines to Unknown via
        // `#[serde(other)]` instead of failing the load.
        let g: GridSource = serde_json::from_str(r#""contact""#).unwrap();
        assert_eq!(g, GridSource::Unknown);
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
        assert_eq!(
            serde_json::from_str::<ChannelBandwidth>(r#"{"kind":"hz","hz":2300}"#).unwrap(),
            ChannelBandwidth::Hz { hz: 2300 }
        );
    }

    #[test]
    fn last_ok_defaults_to_none_and_round_trips() {
        // T-F Part 0: `last_ok` is `#[serde(default)]`, so a pre-T-F record
        // (no `last_ok` key) loads as None; and a present value round-trips.
        let ch: Channel = serde_json::from_str(
            r#"{"transport":"vara-hf","target_callsign":"W6ABC-7",
                "last_seen":"2026-07-11T12:00:00-07:00"}"#,
        )
        .expect("a record without last_ok must load");
        assert_eq!(ch.last_ok, None, "absent last_ok defaults to None");

        let ep: Endpoint = serde_json::from_str(
            r#"{"id":"e1","host":"peer.example","port":8772,
                "last_seen":"2026-07-11T12:00:00-07:00"}"#,
        )
        .expect("an endpoint without last_ok must load");
        assert_eq!(ep.last_ok, None);

        // A present value round-trips and serializes as the RFC3339 string.
        let ch = Channel {
            transport: ChannelTransport::VaraHf,
            target_callsign: "W6ABC-7".into(),
            via: vec![],
            freq_hz: Some(7_101_000),
            bandwidth: None,
            direction: Direction::Outgoing,
            counts: AttemptCounts { ok: 1, fail: 0 },
            last_seen: "2026-07-11T12:05:00-07:00".into(),
            last_ok: Some("2026-07-11T12:05:00-07:00".into()),
        };
        let json = serde_json::to_string(&ch).unwrap();
        assert!(json.contains(r#""last_ok":"2026-07-11T12:05:00-07:00""#), "{json}");
        assert_eq!(serde_json::from_str::<Channel>(&json).unwrap(), ch);
    }

    #[test]
    fn unknown_enum_variants_quarantine_the_field_not_the_row() {
        // [R4-5]: a variant written by a future binary deserializes to
        // Unknown; the row survives.
        let ch: Channel = serde_json::from_str(
            r#"{"transport":"quantum-link","target_callsign":"W6ABC-7",
                "direction":"sideways","last_seen":"2026-07-11T12:00:00-07:00"}"#,
        )
        .expect("unknown variants must not fail the load");
        assert_eq!(ch.transport, ChannelTransport::Unknown);
        assert_eq!(ch.direction, Direction::Unknown);
    }
}
