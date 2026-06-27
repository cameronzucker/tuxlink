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
//! This module also owns the [`SelectionRegistry`] — the Tauri-managed
//! rendezvous between the blocking B2F exchange thread (which pauses a turn to
//! ask the operator which inbound messages to download) and the async Tauri
//! command the UI calls to answer ([`resolve_selection`]). The
//! [`build_selecting_decider`] factory wires that registry, the event emitter,
//! and the per-connect abort flag into a `Fn` decider the exchange loop calls
//! once per inbound proposal batch.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::winlink::b2f_events::AttemptId;
use crate::winlink::proposal::{Answer, PendingMessage, Proposal};
use crate::winlink::session::ExchangeError;

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
    /// Used as the [`SELECTION_TIMEOUT`] fallback: when the operator has not
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
    /// Redacted sender address (tuxlink-9u07u). Sourced from the `;PM:` manifest,
    /// which is the only carrier of sender/subject — the `FC` proposal line has
    /// neither. Empty when the message came from an `FC`-only path (no manifest).
    pub sender: String,
    /// Redacted subject (tuxlink-9u07u). Same `;PM:` source as `sender`; empty
    /// when unavailable.
    pub subject: String,
}

impl PendingProposalDto {
    /// Build from an `FC` proposal line (MID + sizes only). Sender/subject are
    /// blank — the `FC` line does not carry them. Used as the fallback when the
    /// CMS sent no `;PM:` manifest.
    ///
    /// MID is wire-derived; redact before it crosses to the UI (B2F-wire pitfall, Codex #8).
    pub fn from_proposal_redacted(p: &Proposal) -> Self {
        PendingProposalDto {
            mid: crate::winlink::redaction::redact_freeform(&p.mid).into_owned(),
            uncompressed_size: p.size,
            compressed_size: p.compressed_size,
            sender: String::new(),
            subject: String::new(),
        }
    }

    /// Build from a `;PM:` manifest entry (tuxlink-9u07u) — the rich form with
    /// sender + subject. `compressed_size` is unknown from `;PM:` (it appears
    /// only on the later `FC` line) so it is reported as `0`.
    ///
    /// MID, sender, and subject are all wire-derived; each is redacted before it
    /// crosses to the UI (B2F-wire pitfall, Codex #8).
    pub fn from_pending_redacted(pm: &PendingMessage) -> Self {
        PendingProposalDto {
            mid: crate::winlink::redaction::redact_freeform(&pm.mid).into_owned(),
            uncompressed_size: pm.size,
            compressed_size: 0,
            sender: crate::winlink::redaction::redact_freeform(&pm.sender).into_owned(),
            subject: crate::winlink::redaction::redact_freeform(&pm.subject).into_owned(),
        }
    }
}

/// How long the exchange thread waits for the operator to answer a selection
/// prompt before falling back to accept-all (WLE parity).
///
/// 45s is dev-smoke-verified against the 60s CMS socket idle (Task 9): the
/// timeout fires and the accept-all answer goes out with margin before the
/// server would drop the idle socket.
pub const SELECTION_TIMEOUT: Duration = Duration::from_secs(45);

/// One pending selection prompt awaiting the operator's answer.
///
/// The exchange thread parks on `tx`'s receiver; the Tauri command thread
/// finds this slot by `(attempt_id, request_id)` and sends the answer through
/// `tx`. `request_id` disambiguates successive prompts within one attempt so a
/// late answer for an already-resolved/timed-out batch cannot resolve a newer
/// one (the stale-answer race).
pub struct SelectionSlot {
    /// The connect attempt this prompt belongs to.
    pub attempt_id: AttemptId,
    /// Monotonic per-process prompt id (see [`REQUEST_SEQ`] / [`build_selecting_decider`]).
    pub request_id: u64,
    /// Channel the answering thread sends the operator's selection through.
    pub tx: mpsc::Sender<InboundSelection>,
}

/// Tauri-managed rendezvous for the one in-flight selection prompt.
///
/// `Option` because at most one prompt is pending at a time (the exchange is a
/// single sequential turn loop). `Arc<Mutex<…>>` so the blocking exchange
/// thread and the async Tauri command thread can both reach it; a clone is
/// captured by [`build_selecting_decider`]'s closure and another lives in
/// Tauri managed state.
pub type SelectionRegistry = Arc<Mutex<Option<SelectionSlot>>>;

/// Process-monotonic source of `request_id`s. Starts at 1 so 0 can serve as a
/// "no request" sentinel in callers that want one.
static REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

/// The decision the operator made for the whole download, cached after the FIRST
/// proposal batch so later `FC` batches resolve without re-prompting
/// (tuxlink-9u07u). The CMS sends one `;PM:` manifest up front but proposes the
/// download in several small `FC` blocks; WLE reviews once and applies that one
/// choice to every block. This caches the equivalent.
#[derive(Debug, Clone)]
enum CachedDecision {
    /// The operator's explicit selection (which MIDs to download + what to do
    /// with the rest). Applied to each batch by matching MIDs.
    Select(InboundSelection),
    /// The timeout fallback (WLE parity): accept every message in every batch.
    AcceptAll,
}

impl CachedDecision {
    /// Map this cached decision onto a concrete batch of proposals.
    fn answers_for(&self, proposals: &[Proposal]) -> Vec<Answer> {
        match self {
            CachedDecision::Select(sel) => sel.to_answers(proposals),
            CachedDecision::AcceptAll => InboundSelection::accept_all(proposals),
        }
    }
}

/// Build the decider the exchange loop calls once per inbound proposal batch.
///
/// The returned closure registers a [`SelectionSlot`] in `reg`, emits the
/// proposals to the UI via `emit`, then blocks until the operator answers
/// (resolved through [`resolve_selection`]), the operator aborts (`aborting`),
/// or [`SELECTION_TIMEOUT`] elapses (→ WLE accept-all).
///
/// **Why `Fn` + interior mutability (rather than `FnMut`):** the exchange loop
/// holds the decider behind a shared reference and may call it across batches;
/// an `FnMut` would force `&mut` threading through the call site. All mutation
/// the decider performs is on shared, internally-synchronised state — the
/// `Mutex` inside `reg`, the atomic `REQUEST_SEQ`, the atomic `aborting`, and a
/// fresh per-call `mpsc` channel — none of which needs unique closure access.
/// `Fn` is therefore both correct and the looser bound the call site wants.
///
/// `aborting` is the SAME `AtomicBool` `native_connect` already threads for
/// socket abort; reusing it means an operator abort cancels a pending prompt
/// without a second flag to keep in sync.
pub fn build_selecting_decider<E>(
    reg: SelectionRegistry,
    attempt_id: AttemptId,
    emit: E,
    aborting: Arc<AtomicBool>,
) -> impl Fn(&[Proposal], &[PendingMessage]) -> Result<Vec<Answer>, ExchangeError>
where
    E: Fn(u64, &[PendingProposalDto]) + Send + Sync + 'static,
{
    build_selecting_decider_with_timeout(reg, attempt_id, emit, aborting, SELECTION_TIMEOUT)
}

/// Inner factory taking an explicit `timeout`, so tests can exercise the
/// timeout path without waiting [`SELECTION_TIMEOUT`]. The public
/// [`build_selecting_decider`] delegates here with the production constant.
///
/// **Abort has THREE checkpoints**, because an abort sets `aborting=true` and
/// then drops the registry slot, but a slot-drop only wakes `recv_timeout` if a
/// slot was registered when it landed:
///
/// 1. **Pre-register:** abort already happened → cancel without prompting.
/// 2. **Post-register / pre-recv:** abort raced into the gap AFTER the
///    pre-register check but BEFORE we registered (its slot-drop a no-op because
///    nothing was registered yet; our register then re-created the slot). Without
///    this check the decider would park for the full `timeout` with no wake
///    source — a socket shutdown does NOT wake an mpsc `recv`. Re-checking here
///    honors the abort promptly.
/// 3. **Post-recv:** abort landed after we registered — its slot-drop wakes
///    `recv_timeout` (severs the only `tx`), and we re-check the flag to return
///    `Cancelled` rather than accept-all.
fn build_selecting_decider_with_timeout<E>(
    reg: SelectionRegistry,
    attempt_id: AttemptId,
    emit: E,
    aborting: Arc<AtomicBool>,
    timeout: Duration,
) -> impl Fn(&[Proposal], &[PendingMessage]) -> Result<Vec<Answer>, ExchangeError>
where
    E: Fn(u64, &[PendingProposalDto]) + Send + Sync + 'static,
{
    // The operator's decision, cached after the FIRST batch so later `FC` batches
    // resolve without re-prompting (tuxlink-9u07u). The CMS sends one `;PM:`
    // manifest up front, then proposes the download in several small `FC` blocks;
    // this caches the single review choice and applies it to every block — one
    // prompt, not one per block. Interior mutability keeps the decider `Fn` (the
    // exchange loop holds it behind a shared reference across batches).
    let cached: Arc<Mutex<Option<CachedDecision>>> = Arc::new(Mutex::new(None));

    move |proposals, manifest| {
        // Empty-batch defence: `receive_turn` pre-gates empty batches (it never
        // asks the operator about zero messages), but a decider that registered
        // a slot + emitted an empty prompt for a stray empty batch would hang
        // the turn on an answer that the UI has no reason to send. Returning an
        // empty answer vector here keeps the decider correct even if the
        // pre-gate ever regresses.
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        // Later batches: the operator already reviewed the whole manifest on the
        // first batch. Apply that one cached decision and return WITHOUT emitting
        // another prompt — this is the fix for the per-block prompt storm. Clone
        // out of the guard so the lock is released before `answers_for` runs.
        let cached_decision = cached.lock().unwrap().clone();
        if let Some(decision) = cached_decision {
            return Ok(decision.answers_for(proposals));
        }
        // Abort that already happened before we registered: cancel without
        // prompting. Returning Cancelled (not accept-all) means an operator who
        // aborts mid-handshake does not silently download everything.
        if aborting.load(Ordering::SeqCst) {
            return Err(ExchangeError::Cancelled);
        }

        let request_id = REQUEST_SEQ.fetch_add(1, Ordering::SeqCst);
        // Emit the FULL pending-message manifest when the CMS sent one (`;PM:`
        // lines, with sender + subject) so the operator reviews EVERY pending
        // message at once — not just this first `FC` block. Fall back to the
        // `FC` proposals (MID-only) when no manifest arrived.
        let dtos: Vec<PendingProposalDto> = if manifest.is_empty() {
            proposals
                .iter()
                .map(PendingProposalDto::from_proposal_redacted)
                .collect()
        } else {
            manifest
                .iter()
                .map(PendingProposalDto::from_pending_redacted)
                .collect()
        };
        let (tx, rx) = mpsc::channel();
        *reg.lock().unwrap() = Some(SelectionSlot { attempt_id, request_id, tx });
        emit(request_id, &dtos);

        // Abort lost-wake guard: if an abort raced in after our pre-check but before we
        // park (its slot-drop a no-op because we had not registered yet, then our
        // register re-created the slot), recv_timeout would have no wake source and we
        // would block the full timeout. Re-check here so abort is honored promptly.
        if aborting.load(Ordering::SeqCst) {
            let mut g = reg.lock().unwrap();
            if matches!(&*g, Some(s) if s.request_id == request_id) {
                *g = None;
            }
            return Err(ExchangeError::Cancelled);
        }

        let r = rx.recv_timeout(timeout);

        // De-register this slot iff it is still ours: `resolve_selection` may
        // have already `take()`n it (the answer path), in which case a newer
        // prompt could be registered and we must not clobber it.
        {
            let mut g = reg.lock().unwrap();
            if matches!(&*g, Some(s) if s.request_id == request_id) {
                *g = None;
            }
        }

        // An abort may have raced the answer (operator hit abort just as the
        // answer arrived, or just before the timeout). Abort wins: cancel.
        if aborting.load(Ordering::SeqCst) {
            return Err(ExchangeError::Cancelled);
        }

        match r {
            Ok(sel) => {
                // Cache the operator's choice so every later `FC` block applies
                // it without another prompt.
                *cached.lock().unwrap() = Some(CachedDecision::Select(sel.clone()));
                Ok(sel.to_answers(proposals))
            }
            // Timeout OR sender dropped without an abort flag set: WLE parity
            // says accept everything so the session completes rather than
            // stalling. (The abort case returned Cancelled above.) Cache the
            // fallback so later blocks also accept-all rather than each timing
            // out for the full duration.
            Err(_) => {
                *cached.lock().unwrap() = Some(CachedDecision::AcceptAll);
                Ok(InboundSelection::accept_all(proposals))
            }
        }
    }
}

/// Resolve a pending selection. Returns true iff the slot matched and the
/// answer was delivered. Idempotent: a second call with the same key finds the
/// slot already taken and returns false. A mismatched `(attempt_id,
/// request_id)` is a silent no-op (defeats stale-answer races across batches).
///
/// Called by Task 5's `cms_resolve_inbound_selection` Tauri command; the
/// signature is the stable seam between that command and this concurrency core.
pub fn resolve_selection(
    reg: &SelectionRegistry,
    attempt_id: AttemptId,
    request_id: u64,
    selection: InboundSelection,
) -> bool {
    let mut g = reg.lock().unwrap();
    if matches!(&*g, Some(s) if s.attempt_id == attempt_id && s.request_id == request_id) {
        let slot = g.take().expect("just matched Some");
        // Ignore the send result: the decider may have already timed out and
        // dropped its receiver, in which case the answer is simply discarded.
        let _ = slot.tx.send(selection);
        return true;
    }
    false
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

    // ========================================================================
    // SelectionRegistry + selecting decider + resolve_selection (Task 3)
    // ========================================================================
    // AttemptId, ExchangeError, AtomicBool/AtomicU64/Ordering, mpsc/Arc/Mutex,
    // and Duration all come in via `use super::*` (the parent module imports
    // them for the production code above).

    /// A no-op emit closure for tests that do not assert on emission.
    fn noop_emit() -> impl Fn(u64, &[PendingProposalDto]) + Send + Sync + 'static {
        |_req, _dtos| {}
    }

    /// Spin-wait (bounded) until the registry slot is populated, then return its
    /// `(attempt_id, request_id)`. Panics if the slot never appears, so a hung
    /// decider surfaces as a test failure rather than a deadlock.
    fn wait_for_slot(reg: &SelectionRegistry) -> (AttemptId, u64) {
        for _ in 0..200 {
            if let Some(s) = reg.lock().unwrap().as_ref() {
                return (s.attempt_id, s.request_id);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("decider never registered a selection slot");
    }

    // Test (a): the operator's answer flows back through the registry and the
    // decider maps it onto the proposal batch.
    #[test]
    fn decider_returns_operator_answer_mapped_to_proposals() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let attempt_id = AttemptId(11);
        let proposals = vec![prop("A"), prop("B"), prop("C")];

        let result = std::thread::scope(|scope| {
            let decider = build_selecting_decider(
                Arc::clone(&reg),
                attempt_id,
                noop_emit(),
                Arc::clone(&aborting),
            );
            let proposals = &proposals;
            let handle = scope.spawn(move || decider(proposals, &[]));

            let (slot_attempt, slot_req) = wait_for_slot(&reg);
            assert_eq!(slot_attempt, attempt_id);
            let delivered = resolve_selection(
                &reg,
                slot_attempt,
                slot_req,
                InboundSelection {
                    selected_mids: vec!["A".into(), "C".into()],
                    disposition: UnselectedDisposition::Hold,
                },
            );
            assert!(delivered, "resolve_selection should match the live slot");
            handle.join().unwrap()
        });

        let answers = result.expect("decider should return Ok when answered");
        assert_eq!(answers.len(), 3);
        assert!(matches!(answers[0], Answer::Accept { resume_offset: 0 }));
        assert!(matches!(answers[1], Answer::Defer));
        assert!(matches!(answers[2], Answer::Accept { resume_offset: 0 }));
    }

    // Test (a'): the decider emits the request_id + DTOs exactly once.
    #[test]
    fn decider_emits_request_id_and_dtos_once() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let attempt_id = AttemptId(21);
        let proposals = vec![prop("A"), prop("B")];
        let emitted: Arc<Mutex<Vec<(u64, usize)>>> = Arc::new(Mutex::new(Vec::new()));

        std::thread::scope(|scope| {
            let emitted_c = Arc::clone(&emitted);
            let emit = move |req: u64, dtos: &[PendingProposalDto]| {
                emitted_c.lock().unwrap().push((req, dtos.len()));
            };
            let decider =
                build_selecting_decider(Arc::clone(&reg), attempt_id, emit, Arc::clone(&aborting));
            let proposals = &proposals;
            let handle = scope.spawn(move || decider(proposals, &[]));

            let (a, r) = wait_for_slot(&reg);
            resolve_selection(
                &reg,
                a,
                r,
                InboundSelection { selected_mids: vec![], disposition: UnselectedDisposition::Hold },
            );
            handle.join().unwrap().unwrap();
        });

        let log = emitted.lock().unwrap();
        assert_eq!(log.len(), 1, "emit should fire exactly once");
        assert_eq!(log[0].1, 2, "emit should carry one DTO per proposal");
    }

    // Test (b): no operator answer within the (injected, tiny) timeout → accept-all.
    #[test]
    fn decider_times_out_to_accept_all() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let proposals = vec![prop("A"), prop("B")];

        let decider = build_selecting_decider_with_timeout(
            Arc::clone(&reg),
            AttemptId(31),
            noop_emit(),
            Arc::clone(&aborting),
            Duration::from_millis(50),
        );
        let answers = decider(&proposals, &[]).expect("timeout path returns Ok(accept_all)");
        assert_eq!(answers.len(), 2);
        assert!(answers
            .iter()
            .all(|a| matches!(a, Answer::Accept { resume_offset: 0 })));

        // The slot must have been de-registered after the decider returned.
        assert!(reg.lock().unwrap().is_none(), "slot should be cleared after timeout");
    }

    // Test (b'): an empty proposal batch is a defensive no-prompt Ok(empty).
    #[test]
    fn decider_returns_empty_for_empty_batch_without_registering() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let decider =
            build_selecting_decider(Arc::clone(&reg), AttemptId(41), noop_emit(), Arc::clone(&aborting));
        let answers = decider(&[], &[]).expect("empty batch returns Ok(empty)");
        assert!(answers.is_empty());
        assert!(reg.lock().unwrap().is_none(), "empty batch must not register a slot");
    }

    // Test (c): operator aborts while the prompt is pending → Err(Cancelled),
    // NOT accept-all. The abort flag is set AND the slot is dropped (severing
    // the only tx so recv_timeout returns Disconnected promptly).
    #[test]
    fn decider_returns_cancelled_when_aborted_during_prompt() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let proposals = vec![prop("A"), prop("B")];

        let result = std::thread::scope(|scope| {
            let decider = build_selecting_decider(
                Arc::clone(&reg),
                AttemptId(51),
                noop_emit(),
                Arc::clone(&aborting),
            );
            let proposals = &proposals;
            let handle = scope.spawn(move || decider(proposals, &[]));

            wait_for_slot(&reg);
            aborting.store(true, Ordering::SeqCst);
            *reg.lock().unwrap() = None; // drop the only tx → recv_timeout = Disconnected
            handle.join().unwrap()
        });

        assert_eq!(result, Err(ExchangeError::Cancelled));
    }

    // Test (c'): pre-abort — abort flag already set before the decider runs →
    // Err(Cancelled) without ever registering a slot or emitting.
    #[test]
    fn decider_returns_cancelled_when_already_aborting() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(true));
        let proposals = vec![prop("A")];
        let decider =
            build_selecting_decider(Arc::clone(&reg), AttemptId(61), noop_emit(), Arc::clone(&aborting));
        assert_eq!(decider(&proposals, &[]), Err(ExchangeError::Cancelled));
        assert!(reg.lock().unwrap().is_none(), "pre-abort must not register a slot");
    }

    // Test (c''): the lost-wake window. An abort whose `aborting=true` lands
    // exactly in the gap between register/emit and recv (its slot-drop a no-op
    // because nothing was registered when it ran, then our register re-created
    // the slot) must be honored by the post-register/pre-recv check — NOT block
    // the decider until the timeout (a socket shutdown does not wake an mpsc recv).
    #[test]
    fn decider_cancels_if_abort_lands_between_register_and_recv() {
        // Simulate abort's `aborting=true` landing exactly after register/emit (the
        // lost-wake window): the emit closure flips the flag. The decider must return
        // Cancelled via the post-register/pre-recv check, NOT block until the timeout.
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let aborting_in_emit = aborting.clone();
        let emit = move |_req: u64, _dtos: &[PendingProposalDto]| {
            aborting_in_emit.store(true, Ordering::SeqCst);
        };
        // a large timeout: if the guard is missing, this test would hang ~that long
        let decider = build_selecting_decider_with_timeout(
            reg.clone(),
            AttemptId(1),
            emit,
            aborting.clone(),
            Duration::from_secs(30),
        );
        let proposals = vec![prop("A")];
        let start = std::time::Instant::now();
        let r = decider(&proposals, &[]);
        assert_eq!(r, Err(ExchangeError::Cancelled));
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "must not park for the full timeout"
        );
        // slot cleaned up
        assert!(reg.lock().unwrap().is_none());
    }

    // Test (d): stale-answer regression. Register req=7, time it out (slot
    // cleared), register req=8; an answer keyed to req=7 must NOT resolve req=8.
    #[test]
    fn resolve_does_not_cross_batches_on_stale_request_id() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let attempt = AttemptId(70);

        // req=7 is modelled as already timed out: we never register it, leaving
        // no slot for it. Only req=8 is registered as the live batch.
        // Batch 8 is the live one.
        let (tx8, rx8) = mpsc::channel();
        *reg.lock().unwrap() = Some(SelectionSlot { attempt_id: attempt, request_id: 8, tx: tx8 });

        // A late answer for the dead req=7 arrives.
        let crossed = resolve_selection(
            &reg,
            attempt,
            7,
            InboundSelection { selected_mids: vec!["A".into()], disposition: UnselectedDisposition::Hold },
        );
        assert!(!crossed, "a req=7 answer must not resolve the req=8 slot");
        // req=8's channel must NOT have received the stale answer.
        assert!(rx8.try_recv().is_err(), "req=8 slot wrongly received a req=7 answer");
        // The live slot is untouched.
        assert!(reg.lock().unwrap().is_some(), "the live req=8 slot must remain registered");
    }

    // Test (d'): mismatched attempt_id (same request_id) is also a no-op.
    #[test]
    fn resolve_does_not_cross_attempts_on_mismatched_attempt_id() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::channel();
        *reg.lock().unwrap() = Some(SelectionSlot { attempt_id: AttemptId(80), request_id: 5, tx });

        let crossed = resolve_selection(
            &reg,
            AttemptId(81), // different attempt
            5,
            InboundSelection { selected_mids: vec![], disposition: UnselectedDisposition::Hold },
        );
        assert!(!crossed, "a different attempt_id must not resolve the slot");
        assert!(rx.try_recv().is_err());
        assert!(reg.lock().unwrap().is_some());
    }

    // Test (Task 5 wire contract): the Tauri command receives attempt_id as a
    // plain u64 (the JSON number the frontend sends back) and constructs
    // AttemptId(attempt_id) before calling resolve_selection. This test uses
    // u64 numeric literals explicitly to document that wire contract and guard
    // that AttemptId(7u64) round-trips correctly through the match predicate.
    // A full #[tauri::command] State-injection test would need the Tauri test
    // harness; this conversion test + the resolve_selection tests above cover
    // the command's logic end-to-end.
    #[test]
    fn u64_attempt_id_wire_contract_matches_and_rejects_wrong_id() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::channel();
        *reg.lock().unwrap() = Some(SelectionSlot {
            attempt_id: AttemptId(7),
            request_id: 42,
            tx,
        });

        // Correct attempt_id: AttemptId(7u64) — mirrors what the command builds
        // from the frontend's numeric JSON field.
        let sel = InboundSelection {
            selected_mids: vec!["A".into()],
            disposition: UnselectedDisposition::Hold,
        };
        let matched = resolve_selection(&reg, AttemptId(7u64), 42, sel);
        assert!(matched, "AttemptId(7u64) must match the registered AttemptId(7) slot");
        assert!(rx.try_recv().is_ok(), "selection must be delivered through the channel");

        // Wrong attempt_id (8u64): must be a no-op — frontend's stale event guard.
        let (tx2, rx2) = mpsc::channel();
        *reg.lock().unwrap() = Some(SelectionSlot {
            attempt_id: AttemptId(7),
            request_id: 42,
            tx: tx2,
        });
        let missed = resolve_selection(
            &reg,
            AttemptId(8u64),
            42,
            InboundSelection { selected_mids: vec![], disposition: UnselectedDisposition::Hold },
        );
        assert!(!missed, "AttemptId(8u64) must not resolve an AttemptId(7) slot");
        assert!(rx2.try_recv().is_err(), "wrong attempt_id must not deliver anything");
    }

    // Test (e): double-submit is a no-op after the first take(). The first call
    // delivers + clears the slot; the second finds nothing and returns false.
    #[test]
    fn resolve_is_idempotent_double_submit_is_noop() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let attempt = AttemptId(90);
        let (tx, rx) = mpsc::channel();
        *reg.lock().unwrap() = Some(SelectionSlot { attempt_id: attempt, request_id: 3, tx });

        let first = resolve_selection(
            &reg,
            attempt,
            3,
            InboundSelection { selected_mids: vec!["A".into()], disposition: UnselectedDisposition::Hold },
        );
        assert!(first, "first submit should resolve the slot");
        assert!(rx.try_recv().is_ok(), "first submit should deliver the answer");

        let second = resolve_selection(
            &reg,
            attempt,
            3,
            InboundSelection { selected_mids: vec!["B".into()], disposition: UnselectedDisposition::Delete },
        );
        assert!(!second, "second submit for the same key must be a no-op");
        assert!(rx.try_recv().is_err(), "no second answer should be delivered");
        assert!(reg.lock().unwrap().is_none(), "slot stays cleared after the first take");
    }

    // tuxlink-9u07u: once the operator reviews the first batch, every later `FC`
    // batch in the same session applies that one decision WITHOUT re-prompting —
    // the fix for the per-block prompt storm.
    #[test]
    fn second_batch_applies_cached_decision_without_reprompting() {
        use std::sync::atomic::AtomicUsize;

        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let emit_count = Arc::new(AtomicUsize::new(0));
        let ec = Arc::clone(&emit_count);
        let emit = move |_req: u64, _dtos: &[PendingProposalDto]| {
            ec.fetch_add(1, Ordering::SeqCst);
        };
        let decider =
            build_selecting_decider(Arc::clone(&reg), AttemptId(71), emit, Arc::clone(&aborting));

        // Batch 1: A, B. Operator selects only A and Holds the rest.
        let batch1 = vec![prop("A"), prop("B")];
        let answers1 = std::thread::scope(|scope| {
            let d = &decider;
            let b1 = &batch1;
            let handle = scope.spawn(move || d(b1, &[]));
            let (a, r) = wait_for_slot(&reg);
            assert!(resolve_selection(
                &reg,
                a,
                r,
                InboundSelection {
                    selected_mids: vec!["A".into()],
                    disposition: UnselectedDisposition::Hold,
                },
            ));
            handle.join().unwrap()
        })
        .expect("batch 1 resolves to the operator's selection");
        assert!(matches!(answers1[0], Answer::Accept { .. }));
        assert!(matches!(answers1[1], Answer::Defer));
        assert_eq!(emit_count.load(Ordering::SeqCst), 1, "first batch prompts exactly once");

        // Batch 2 (a later FC block): C, A. No new prompt; the cache applies.
        let answers2 = decider(&[prop("C"), prop("A")], &[])
            .expect("second batch resolves from the cached decision");
        assert_eq!(
            emit_count.load(Ordering::SeqCst),
            1,
            "the second batch must NOT emit another prompt"
        );
        assert!(matches!(answers2[0], Answer::Defer), "C was never selected -> held");
        assert!(matches!(answers2[1], Answer::Accept { .. }), "A stays selected across batches");
    }

    // tuxlink-9u07u: the first prompt is built from the WHOLE `;PM:` manifest
    // (with sender + subject), not just the first `FC` block.
    #[test]
    fn first_batch_emits_the_full_manifest_with_sender_and_subject() {
        let reg: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let captured: Arc<Mutex<Vec<PendingProposalDto>>> = Arc::new(Mutex::new(Vec::new()));
        let cap = Arc::clone(&captured);
        let emit = move |_req: u64, dtos: &[PendingProposalDto]| {
            *cap.lock().unwrap() = dtos.to_vec();
        };
        let decider =
            build_selecting_decider(Arc::clone(&reg), AttemptId(72), emit, Arc::clone(&aborting));

        // The FC block proposes only A, but the manifest lists A, B, C.
        let manifest = vec![
            PendingMessage {
                recipient: "N7CPZ".into(),
                mid: "A".into(),
                size: 10,
                sender: "alpha@winlink.org".into(),
                subject: "Alpha".into(),
            },
            PendingMessage {
                recipient: "N7CPZ".into(),
                mid: "B".into(),
                size: 20,
                sender: "bravo@winlink.org".into(),
                subject: "Bravo".into(),
            },
            PendingMessage {
                recipient: "N7CPZ".into(),
                mid: "C".into(),
                size: 30,
                sender: "charlie@winlink.org".into(),
                subject: "Charlie".into(),
            },
        ];
        let batch1 = vec![prop("A")];
        std::thread::scope(|scope| {
            let d = &decider;
            let b1 = &batch1;
            let m = &manifest;
            let handle = scope.spawn(move || d(b1, m));
            let (a, r) = wait_for_slot(&reg);
            assert!(resolve_selection(
                &reg,
                a,
                r,
                InboundSelection { selected_mids: vec![], disposition: UnselectedDisposition::Hold },
            ));
            let _ = handle.join().unwrap();
        });

        let dtos = captured.lock().unwrap().clone();
        assert_eq!(dtos.len(), 3, "the prompt lists the WHOLE manifest, not just the FC block");
        assert!(
            dtos.iter().all(|d| !d.sender.is_empty() && !d.subject.is_empty()),
            "manifest-sourced rows carry sender + subject: {dtos:?}"
        );
    }
}
