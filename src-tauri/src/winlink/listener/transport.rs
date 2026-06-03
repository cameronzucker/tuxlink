//! `TransportKind` — enumeration of the P2P-capable transports tuxlink supports.
//!
//! The shared listener-arms layer is transport-agnostic: it labels every arm event
//! and every accept-decision with the transport so the forensics log + UI can
//! distinguish Telnet vs Packet vs ARDOP vs VARA-HF vs VARA-FM at glance.
//!
//! Pactor is included for completeness, but per the closure-plan amendment
//! (§1.5) the actual Pactor listener path is operator-decision-gated and not in
//! scope for tuxlink-3o2o; it is here so the data shape doesn't need to change
//! when/if Pactor lands.
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1
//! bd: tuxlink-3o2o

use serde::{Deserialize, Serialize};

/// Each P2P-capable transport that can run a listener.
///
/// Serialized as a lowercase kebab string (`telnet`, `packet`, `ardop`, `vara-hf`,
/// `vara-fm`, `pactor`) so the on-disk JSONL forensics log + the cross-process JSON
/// shape stays human-readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportKind {
    Telnet,
    Packet,
    Ardop,
    VaraHf,
    VaraFm,
    Pactor,
}

impl TransportKind {
    /// Stable kebab-case label used in serialized forms and UI strings.
    pub fn as_str(&self) -> &'static str {
        match self {
            TransportKind::Telnet => "telnet",
            TransportKind::Packet => "packet",
            TransportKind::Ardop => "ardop",
            TransportKind::VaraHf => "vara-hf",
            TransportKind::VaraFm => "vara-fm",
            TransportKind::Pactor => "pactor",
        }
    }
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_labels_match_serde() {
        // Compile-time spot-check that as_str matches serde's kebab encoding.
        for kind in [
            TransportKind::Telnet,
            TransportKind::Packet,
            TransportKind::Ardop,
            TransportKind::VaraHf,
            TransportKind::VaraFm,
            TransportKind::Pactor,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
        }
    }

    #[test]
    fn roundtrips_through_json() {
        let kind = TransportKind::VaraHf;
        let json = serde_json::to_string(&kind).unwrap();
        let back: TransportKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
    }
}
