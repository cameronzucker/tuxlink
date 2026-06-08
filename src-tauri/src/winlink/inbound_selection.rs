//! Inbound-message selection types and answer mapping.
//!
//! When the CMS proposes a batch of inbound messages, tuxlink surfaces them to
//! the operator so they can pick which ones to download. This module owns the
//! two types that cross the Tauri command boundary for that flow:
//!
//! * [`InboundSelection`] — the operator's answer (which MIDs to accept + what
//!   to do with the rest).
//! * [`PendingProposalDto`] — a sanitised, redacted view of a single proposal
//!   that is safe to hand to the UI layer.
//!
//! The registry/decider that calls `to_answers` lives in a later task; this
//! module is intentionally pure (no I/O, no threads).

use serde::{Deserialize, Serialize};

use crate::winlink::proposal::{Answer, Proposal};

/// What to do with proposals that the operator did NOT explicitly select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnselectedDisposition {
    /// Defer the message — it will be offered again on the next session.
    #[default]
    Hold,
    /// Reject the message — tell the CMS not to offer it again.
    Delete,
}

/// The operator's selection for an inbound proposal batch.
///
/// `selected_mids` lists the message IDs the operator wants to download.
/// `disposition` controls what happens to every proposal whose MID is NOT in
/// that list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundSelection {
    /// MIDs the operator explicitly chose to download.
    pub selected_mids: Vec<String>,
    /// What to do with proposals whose MID is NOT in `selected_mids`.
    pub disposition: UnselectedDisposition,
}

impl InboundSelection {
    /// Map this selection onto a concrete `Answer` for every proposal in the
    /// batch, **in the same order as `proposals`**.
    ///
    /// Invariant: `output.len() == proposals.len()`. MIDs in `selected_mids`
    /// that do not match any proposal are silently ignored — they must not
    /// change the output length or desynchronise the 1:1 mapping.
    pub fn to_answers(&self, proposals: &[Proposal]) -> Vec<Answer> {
        proposals
            .iter()
            .map(|p| {
                if self.selected_mids.iter().any(|mid| mid == &p.mid) {
                    Answer::Accept { resume_offset: 0 }
                } else {
                    match self.disposition {
                        UnselectedDisposition::Hold => Answer::Defer,
                        UnselectedDisposition::Delete => Answer::Reject,
                    }
                }
            })
            .collect()
    }

    /// Accept every proposal in the batch unconditionally.
    ///
    /// Used as the 45-second-timeout fallback: when the operator has not
    /// responded by the time the CMS expects an answer, accept everything so
    /// the session completes rather than stalling.
    pub fn accept_all(proposals: &[Proposal]) -> Vec<Answer> {
        proposals
            .iter()
            .map(|_| Answer::Accept { resume_offset: 0 })
            .collect()
    }
}

/// A redacted, UI-safe view of one inbound proposal.
///
/// MIDs are wire-derived identifiers that may encode callsign fragments or
/// other operator-identifying data. `from_proposal_redacted` applies the
/// project-standard redaction pass before the value reaches the UI layer
/// (B2F-wire pitfall, Codex #8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingProposalDto {
    /// Redacted message ID (safe for UI display / logging).
    pub mid: String,
    /// Uncompressed message size in bytes.
    pub uncompressed_size: usize,
    /// Compressed size in bytes (the amount that actually transfers over the link).
    pub compressed_size: usize,
}

impl PendingProposalDto {
    /// MID is wire-derived; redact before it crosses to the UI (B2F-wire pitfall, Codex #8).
    pub fn from_proposal_redacted(p: &Proposal) -> Self {
        PendingProposalDto {
            mid: crate::winlink::redaction::redact_freeform(&p.mid).into_owned(),
            uncompressed_size: p.size,
            compressed_size: p.compressed_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::proposal::Proposal;

    /// Convenience constructor — builds a minimal valid Proposal for tests.
    fn prop(mid: &str) -> Proposal {
        Proposal {
            code: 'C',
            msg_type: "EM".to_string(),
            mid: mid.to_string(),
            size: 100,
            compressed_size: 50,
        }
    }

    #[test]
    fn selected_accept_unselected_hold_defers() {
        // A and C selected, B not selected → Accept, Defer, Accept
        let proposals = vec![prop("A"), prop("B"), prop("C")];
        let sel = InboundSelection {
            selected_mids: vec!["A".into(), "C".into()],
            disposition: UnselectedDisposition::Hold,
        };
        let answers = sel.to_answers(&proposals);
        assert_eq!(answers.len(), 3);
        assert!(matches!(answers[0], Answer::Accept { resume_offset: 0 }));
        assert!(matches!(answers[1], Answer::Defer));
        assert!(matches!(answers[2], Answer::Accept { resume_offset: 0 }));
    }

    #[test]
    fn unselected_delete_rejects() {
        // B not selected with Delete disposition → Reject
        let proposals = vec![prop("A"), prop("B"), prop("C")];
        let sel = InboundSelection {
            selected_mids: vec!["A".into(), "C".into()],
            disposition: UnselectedDisposition::Delete,
        };
        let answers = sel.to_answers(&proposals);
        assert_eq!(answers.len(), 3);
        assert!(matches!(answers[0], Answer::Accept { resume_offset: 0 }));
        assert!(matches!(answers[1], Answer::Reject));
        assert!(matches!(answers[2], Answer::Accept { resume_offset: 0 }));
    }

    #[test]
    fn unknown_mids_are_ignored_without_breaking_one_to_one() {
        // Selecting a MID not in the batch must not change len or desync the mapping.
        let proposals = vec![prop("A"), prop("B")];
        let sel = InboundSelection {
            selected_mids: vec!["A".into(), "ZZZ".into()],
            disposition: UnselectedDisposition::Hold,
        };
        let answers = sel.to_answers(&proposals);
        assert_eq!(answers.len(), 2);
        assert!(matches!(answers[0], Answer::Accept { .. }));
        assert!(matches!(answers[1], Answer::Defer));
    }

    #[test]
    fn empty_selection_hold_defers_all() {
        let proposals = vec![prop("A"), prop("B"), prop("C")];
        let sel = InboundSelection {
            selected_mids: vec![],
            disposition: UnselectedDisposition::Hold,
        };
        let answers = sel.to_answers(&proposals);
        assert_eq!(answers.len(), 3);
        assert!(answers.iter().all(|a| matches!(a, Answer::Defer)));
    }

    #[test]
    fn empty_selection_delete_rejects_all() {
        let proposals = vec![prop("A"), prop("B"), prop("C")];
        let sel = InboundSelection {
            selected_mids: vec![],
            disposition: UnselectedDisposition::Delete,
        };
        let answers = sel.to_answers(&proposals);
        assert_eq!(answers.len(), 3);
        assert!(answers.iter().all(|a| matches!(a, Answer::Reject)));
    }

    #[test]
    fn accept_all_produces_one_accept_per_proposal() {
        let proposals = vec![prop("X"), prop("Y")];
        let answers = InboundSelection::accept_all(&proposals);
        assert_eq!(answers.len(), 2);
        assert!(answers
            .iter()
            .all(|a| matches!(a, Answer::Accept { resume_offset: 0 })));
    }

    #[test]
    fn pending_proposal_dto_copies_sizes() {
        let p = prop("TJKYEIMMHSRB");
        let dto = PendingProposalDto::from_proposal_redacted(&p);
        assert_eq!(dto.uncompressed_size, 100);
        assert_eq!(dto.compressed_size, 50);
    }

    #[test]
    fn accept_all_on_empty_slice_returns_empty() {
        let answers = InboundSelection::accept_all(&[]);
        assert!(answers.is_empty());
    }

    #[test]
    fn from_proposal_redacted_scrubs_credential_token_in_mid() {
        // A MID carrying a ;PR: response token must be scrubbed before crossing to the UI (Codex #8).
        let p = Proposal {
            code: 'C',
            msg_type: "EM".into(),
            mid: "X ;PR: 72768415".into(),
            size: 100,
            compressed_size: 50,
        };
        let dto = PendingProposalDto::from_proposal_redacted(&p);
        assert!(!dto.mid.contains("72768415"), "credential token leaked into DTO mid: {:?}", dto.mid);
    }
}
