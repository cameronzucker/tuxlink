//! Scoped one-shot outbox approval + digest-gated flush (Task 6, tuxlink-13v2l).
//!
//! The operator confirms a **snapshot** of the outbox by receiving an
//! [`OutboxApproval`] token.  Before the flush, [`verify_approval`] re-reads
//! the live outbox (via [`crate::mcp_ports::MonolithOutboxReadPort`]), recomputes
//! the digest, and returns `Ok(())` only when the live set is byte-for-byte
//! identical to what the operator approved.  Any addition, removal, or edit
//! of a staged record between approval and flush → [`ApprovalError::DigestMismatch`]
//! → fail closed.
//!
//! **Digest algorithm:** SHA-256 over `serde_json::to_vec` of the records sorted
//! by `mid`, rendered as lowercase hex.  Canonical JSON via serde_json (no extra
//! normalisation step needed — the struct derives `Serialize` deterministically).

use tuxlink_mcp_core::ports::StagedRecordDto;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A one-shot outbox approval token issued by [`compute_approval`].
///
/// The token binds:
/// - `digest` — SHA-256 of the exact staged set the operator reviewed.
/// - `session_epoch` — the session counter at approval time; a new session
///   (rearm/restart) changes the epoch and invalidates the token.
/// - `expires_unix` — wall-clock Unix timestamp after which the token is
///   rejected (TTL guard against stale approvals).
#[derive(Debug, Clone)]
pub struct OutboxApproval {
    /// Opaque identifier for logging/correlation.
    pub approval_id: String,
    /// `hex(SHA-256(canonical JSON of sorted records))`.
    pub digest: String,
    /// Session epoch at the time of approval.
    pub session_epoch: u64,
    /// Unix timestamp (seconds) after which the token expires.
    pub expires_unix: u64,
}

/// Reasons [`verify_approval`] can reject an approval.
#[derive(Debug, PartialEq, Eq)]
pub enum ApprovalError {
    /// The live outbox digest differs from the approved digest — records were
    /// added, removed, or modified after the approval was issued.
    DigestMismatch,
    /// The session epoch has changed (rearm / new session) since the approval.
    EpochMismatch,
    /// The approval TTL has elapsed.
    Expired,
}

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/// Compute the digest of `records` (sorted by `mid`) using SHA-256.
fn digest_records(records: &[StagedRecordDto]) -> String {
    use sha2::{Digest, Sha256};

    // Sort by mid so record order in the caller's slice never affects the digest.
    let mut sorted: Vec<&StagedRecordDto> = records.iter().collect();
    sorted.sort_by(|a, b| a.mid.cmp(&b.mid));

    // Canonical JSON: serde_json serialises struct fields in declaration order
    // (deterministic for a fixed StagedRecordDto layout).
    let canonical =
        serde_json::to_vec(&sorted).expect("StagedRecordDto is always serialisable");

    // `Sha256::digest` is the sha2 0.11 one-shot static API.
    let hash = Sha256::digest(&canonical);
    hex_lower(hash.as_slice())
}

/// Lowercase-hex encode a byte slice.
fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Issue an [`OutboxApproval`] for the given records.
///
/// - `records` — the staged set the operator has just reviewed.
/// - `session_epoch` — current session counter (arm counter, monotonic).
/// - `now` — current Unix timestamp in seconds (caller supplies for testability).
/// - `ttl` — token lifetime in seconds.
pub fn compute_approval(
    records: &[StagedRecordDto],
    session_epoch: u64,
    now: u64,
    ttl: u64,
) -> OutboxApproval {
    OutboxApproval {
        approval_id: uuid_v4(),
        digest: digest_records(records),
        session_epoch,
        expires_unix: now.saturating_add(ttl),
    }
}

/// Verify that `approval` still covers the `live_records` set.
///
/// Checks in order:
/// 1. Epoch — immediate reject if the session has been rearmed.
/// 2. Expiry — reject if the wall clock has passed `expires_unix`.
/// 3. Digest — recompute and compare; any delta is a mismatch.
pub fn verify_approval(
    approval: &OutboxApproval,
    live_records: &[StagedRecordDto],
    session_epoch: u64,
    now: u64,
) -> Result<(), ApprovalError> {
    if approval.session_epoch != session_epoch {
        return Err(ApprovalError::EpochMismatch);
    }
    if now >= approval.expires_unix {
        return Err(ApprovalError::Expired);
    }
    let live_digest = digest_records(live_records);
    if live_digest != approval.digest {
        return Err(ApprovalError::DigestMismatch);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// UUID helper (avoids pulling the uuid crate into this module's public API).
// ---------------------------------------------------------------------------

fn uuid_v4() -> String {
    use uuid::Uuid;
    Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal [`StagedRecordDto`] for test fixtures.
    fn staged(mid: &str, to: &str, subject: &str, body: &str) -> StagedRecordDto {
        StagedRecordDto {
            mid: mid.into(),
            to: vec![to.into()],
            cc: vec![],
            subject: subject.into(),
            body: body.into(),
        }
    }

    // --- verify_approval --- -------------------------------------------------

    #[test]
    fn verify_denies_when_a_record_is_added_after_approval() {
        let now = 1000;
        let a = staged("A", "eoc", "status", "body");
        let appr = compute_approval(std::slice::from_ref(&a), 7, now, 120);
        assert!(
            matches!(
                verify_approval(&appr, &[a, staged("B", "attacker", "x", "y")], 7, now + 5),
                Err(ApprovalError::DigestMismatch)
            ),
            "adding a record after approval must yield DigestMismatch"
        );
    }

    #[test]
    fn verify_denies_on_epoch_change_or_expiry() {
        let now = 1000;
        let r = staged("A", "eoc", "s", "b");
        let appr = compute_approval(std::slice::from_ref(&r), 7, now, 120);

        // Epoch changed (rearm).
        assert!(
            matches!(
                verify_approval(&appr, std::slice::from_ref(&r), 8, now + 5),
                Err(ApprovalError::EpochMismatch)
            ),
            "changed epoch must yield EpochMismatch"
        );

        // TTL expired (now + 200 > expires_unix = 1000 + 120 = 1120).
        assert!(
            matches!(
                verify_approval(&appr, &[r], 7, now + 200),
                Err(ApprovalError::Expired)
            ),
            "past-TTL must yield Expired"
        );
    }

    #[test]
    fn verify_ok_for_exact_unchanged_set() {
        let now = 1000;
        let r = staged("A", "eoc", "s", "b");
        let appr = compute_approval(std::slice::from_ref(&r), 7, now, 120);
        assert!(
            verify_approval(&appr, &[r], 7, now + 5).is_ok(),
            "exact unchanged set must verify Ok"
        );
    }

    // --- digest stability --- ------------------------------------------------

    #[test]
    fn digest_is_order_independent() {
        // Records presented in different order produce the same digest.
        let a = staged("A", "eoc", "subj", "body");
        let b = staged("B", "net", "ping", "check");
        let d1 = {
            let appr = compute_approval(&[a.clone(), b.clone()], 1, 0, 60);
            appr.digest
        };
        let d2 = {
            let appr = compute_approval(&[b, a], 1, 0, 60);
            appr.digest
        };
        assert_eq!(d1, d2, "digest must be order-independent (sort by mid)");
    }

    #[test]
    fn digest_changes_when_body_modified() {
        let now = 0;
        let original = staged("A", "eoc", "subj", "original body");
        let modified = staged("A", "eoc", "subj", "MODIFIED body");
        let appr = compute_approval(&[original], 1, now, 120);
        assert!(
            matches!(
                verify_approval(&appr, &[modified], 1, now + 5),
                Err(ApprovalError::DigestMismatch)
            ),
            "a body edit must trigger DigestMismatch"
        );
    }

    #[test]
    fn empty_set_approves_and_verifies() {
        let now = 500;
        let appr = compute_approval(&[], 3, now, 60);
        assert!(
            verify_approval(&appr, &[], 3, now + 10).is_ok(),
            "empty set should approve and verify"
        );
    }
}
