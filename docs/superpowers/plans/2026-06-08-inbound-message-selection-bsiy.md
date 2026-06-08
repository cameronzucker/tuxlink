# Inbound Message Selection (tuxlink-bsiy) — Implementation Plan v2 (adrev-hardened)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use `- [ ]` checkboxes. BEFORE each task: read `.claude/skills/test-driven-development/` + `docs/pitfalls/testing-pitfalls.md`; follow TDD (failing test → impl → green). After each logical group: ≥3 review rounds.

**Goal:** Let the operator review pending inbound messages on a CMS connect and choose which to download — mirroring Winlink Express's "Review Pending Messages" dialog — instead of unconditionally downloading all.

**Architecture (v2, post-adversarial review):** The B2F engine decides accept/reject/defer via a closure at `winlink/session/mod.rs:725`. v1 replaces the CMS-path closure with a selecting one that emits the proposal list and **blocks the `spawn_blocking` thread** awaiting the operator's choice (45 s timeout → accept-all, mirroring WLE). Three structural decisions forced by the cross-provider adrev:
1. **The selection registry lives in Tauri managed state** (`SelectionRegistry`), NOT on `NativeBackend` — because `cms_resolve_inbound_selection` only has `Arc<dyn WinlinkBackend>` and cannot reach a concrete field (Codex #1). Both the command and `native_connect` clone the same `Arc<SelectionRegistry>`.
2. **`attempt_id` + an `Arc<dyn B2fEventSink>` are threaded through `connect` → `native_connect`** so the emitted event carries the same `attempt_id` the frontend stale-filter expects (Codex #2). Today `cms_connect` mints `AttemptId` only in the error arm; mint it at the start instead.
3. **The selecting decide seam returns `Result<Vec<Answer>, ExchangeError>`** so abort propagates cancellation BEFORE writing `FS` (Codex #3 / Claude). Abort ≠ accept-all.

Registry is keyed by `(AttemptId, request_id)`; the resolve command matches both and `take()`s the sender (idempotent; defeats stale-answer races across the per-batch prompts — `decide` is called once per ≤5-msg batch). Gated behind an **opt-in preference, default OFF** (= today's auto-download-all, WLE parity). No B2F protocol change — `Answer`/`answer_line`/`parse_answers` already speak Accept/Reject/Defer.

**Parity ground truth:** `~/Code/library-of-hamexandria/winlink-re/.../ReviewPendingMessages.cs` + `B2Protocol.cs`. WLE secure path columns: MID / uncompressed / compressed size (no sender/subject — unavailable pre-download). All rows pre-checked; Hold-vs-Delete radio (default Hold); 45 s timer → accept-all; opt-in preference.

**Scope:** CMS/Telnet dial only (`native_connect`). ARDOP/VARA/packet/P2P decide-sites stay accept-all (hardware-deferred, RADIO-1). Resume-on-receive out of scope. `ask_again`/"stop asking this session" **deferred to v2** (removed here — was incompletely specified and leaked across connects: Codex #6).

---

## Pitfalls to honor (read before coding)
- **B2F-wire redaction** (`docs/pitfalls/implementation-pitfalls.md` B2F-wire entry): wire-derived strings crossing to the UI route through `crate::winlink::redaction::redact_freeform`. The proposal MID is wire-derived → redact it before emission (Codex #8).
- **serde-lockdown** (`b2f_events.rs:183`): new event variant must be added to the test's variant enumeration; it checks field *names*, not values — so redaction (above) guards values.
- **Production-mount-path test** (memory: test the production mount path): the frontend panel test mounts the real App provider stack, not just a scaffold.
- **Scoped-vitest-misses-contract-tests** (memory): the verify gate runs `clippy --all-targets` + full `vitest`.
- **1:1 invariant** (`session/mod.rs:726`): `answers.len() == proposals.len()`; an empty `Vec` is NOT reject-all (it's `AnswerCountMismatch`). Every path preserves this.

---

## Task 1: Selection types + answer mapping (pure)

**Files:** Create `src-tauri/src/winlink/inbound_selection.rs`; modify `src-tauri/src/winlink/mod.rs`. Test: inline.

- [ ] **Step 1 — failing tests** (note corrected names per Codex #10):

```rust
#[test] fn selected_accept_unselected_hold_defers() { /* A,C selected, B Hold -> Accept,Defer,Accept */ }
#[test] fn unselected_delete_rejects() { /* unselected + Delete -> Reject */ }
#[test] fn unknown_mids_are_ignored_without_breaking_one_to_one() {
    // selecting a MID not in the batch must not change len or desync the mapping
    let proposals = vec![prop("A"), prop("B")];
    let sel = InboundSelection { selected_mids: vec!["A".into(), "ZZZ".into()], disposition: Hold };
    let a = sel.to_answers(&proposals);
    assert_eq!(a.len(), 2);
    assert!(matches!(a[0], Answer::Accept{..})); assert!(matches!(a[1], Answer::Defer));
}
#[test] fn empty_selection_hold_defers_all() { /* selected=[] , Hold -> all Defer */ }
#[test] fn empty_selection_delete_rejects_all() { /* selected=[], Delete -> all Reject */ }
```

- [ ] **Step 2 — run, confirm fail** (`cargo test --manifest-path src-tauri/Cargo.toml --lib inbound_selection`).
- [ ] **Step 3 — implement** `UnselectedDisposition::{Hold,Delete}` (Default=Hold), `InboundSelection { selected_mids: Vec<String>, disposition }`, `to_answers(&[Proposal]) -> Vec<Answer>` (Accept if selected; else Hold→Defer / Delete→Reject; exactly one per proposal in order), `accept_all(&[Proposal]) -> Vec<Answer>` (timeout fallback), and `PendingProposalDto { mid, uncompressed_size, compressed_size }` with a **redacting** constructor:

```rust
impl PendingProposalDto {
    /// MID is wire-derived; redact before it crosses to the UI (B2F-wire pitfall, Codex #8).
    pub fn from_proposal_redacted(p: &Proposal) -> Self {
        PendingProposalDto {
            mid: crate::winlink::redaction::redact_freeform(&p.mid),
            uncompressed_size: p.size,
            compressed_size: p.compressed_size,
        }
    }
}
```

- [ ] **Step 4 — run, confirm pass.**
- [ ] **Step 5 — commit** `feat(winlink): inbound-selection types + redacting proposal DTO (tuxlink-bsiy)`.

---

## Task 2: `B2fEvent::InboundProposalsOffered` + redaction/lockdown tests

**Files:** modify `src-tauri/src/winlink/b2f_events.rs` (enum + lockdown test ~`:183`).

- [ ] **Step 1 — failing tests:** (a) variant serializes with `attempt_id`, `request_id`, redacted proposals; (b) **a proposal MID containing the canonical secure-login token `;PR:72768415` does NOT appear in the serialized event** (Codex #8); (c) the lockdown test includes the new variant.
- [ ] **Step 2 — run, confirm fail.**
- [ ] **Step 3 — add the variant** `InboundProposalsOffered { attempt_id: AttemptId, request_id: u64, proposals: Vec<PendingProposalDto> }` and a doc comment: `// SAFETY-CRITICAL: keep the serde-lockdown test (b2f_events.rs:183) in sync; proposal strings MUST be redacted by the producer (PendingProposalDto::from_proposal_redacted).`
- [ ] **Step 4 — explicitly add the variant to the lockdown test's variant vector**; confirm pass.
- [ ] **Step 5 — commit** `feat(winlink): InboundProposalsOffered event + redaction/lockdown tests (tuxlink-bsiy)`.

---

## Task 3: `SelectionRegistry` (Tauri managed state) + selecting decider returning `Result`

**Files:** modify `inbound_selection.rs`. Test: inline (drive the decider with a pre-loaded registry + a separate resolver thread; and a timeout test with an injected small timeout).

Design: `SelectionRegistry = Arc<Mutex<Option<SelectionSlot>>>` where `SelectionSlot { attempt_id, request_id, tx: mpsc::Sender<InboundSelection> }`. The decider is built per-connect (closures capture the registry clone, the emit fn, the attempt_id, the abort flag). It returns `Result<Vec<Answer>, ExchangeError>` so abort cancels cleanly.

```rust
pub const SELECTION_TIMEOUT: Duration = Duration::from_secs(45); // WLE parity; dev-smoke-verified vs 60s socket idle (Task 9)
pub struct SelectionSlot { pub attempt_id: AttemptId, pub request_id: u64, pub tx: mpsc::Sender<InboundSelection> }
pub type SelectionRegistry = Arc<Mutex<Option<SelectionSlot>>>;
static REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

/// `aborting` is the SAME AtomicBool native_connect already threads for socket abort.
pub fn build_selecting_decider<E>(reg: SelectionRegistry, attempt_id: AttemptId, emit: E, aborting: Arc<AtomicBool>)
  -> impl Fn(&[Proposal]) -> Result<Vec<Answer>, ExchangeError>
where E: Fn(u64, &[PendingProposalDto]) + Send + Sync + 'static {
  move |proposals| {
    if proposals.is_empty() { return Ok(Vec::new()); } // receive_turn pre-gates empties; defensive.
    if aborting.load(Ordering::SeqCst) { return Err(ExchangeError::Cancelled); }
    let request_id = REQUEST_SEQ.fetch_add(1, Ordering::SeqCst);
    let dtos: Vec<_> = proposals.iter().map(PendingProposalDto::from_proposal_redacted).collect();
    let (tx, rx) = mpsc::channel();
    *reg.lock().unwrap() = Some(SelectionSlot { attempt_id, request_id, tx });
    emit(request_id, &dtos);
    let r = rx.recv_timeout(SELECTION_TIMEOUT);
    // de-register this slot iff it is still ours (resolve may have take()n it already)
    { let mut g = reg.lock().unwrap(); if matches!(&*g, Some(s) if s.request_id == request_id) { *g = None; } }
    if aborting.load(Ordering::SeqCst) { return Err(ExchangeError::Cancelled); } // abort raced the answer
    match r {
      Ok(sel) => Ok(sel.to_answers(proposals)),
      Err(_) => Ok(InboundSelection::accept_all(proposals)), // 45s timeout -> WLE accept-all
    }
  }
}
```

- [ ] **Step 1 — failing tests:** (a) operator answer maps correctly; (b) **timeout → accept-all** (inject small timeout); (c) **abort during prompt: set `aborting=true` + drop the registry slot → decider returns `Err(Cancelled)`, NOT accept-all** (Codex #3); (d) **stale-answer regression: register req=7, time it out, register req=8, submit an answer for req=7 → it does NOT resolve req=8** (Codex #5); (e) double-submit for the same req is a no-op after the first `take()`.
- [ ] **Step 2 — run, confirm fail.**  - [ ] **Step 3 — implement** as above; comment why `Fn`+interior-mutability is correct and why empty-batch is defensive.  - [ ] **Step 4 — pass.**  - [ ] **Step 5 — commit** `feat(winlink): SelectionRegistry + abort-aware selecting decider (tuxlink-bsiy)`.

---

## Task 4: `receive_turn` honors decider cancellation; thread sink+attempt_id+registry into `native_connect`

**Files:** `src-tauri/src/winlink/session/mod.rs` (decide seam type + the call at `:725`), `src-tauri/src/winlink/telnet.rs` (forward the closure type), `src-tauri/src/winlink_backend.rs` (`native_connect` ~`:2051`, the CMS decide-site `:2156`, `connect` ~`:1161`, `abort` `:1274`), `src-tauri/src/ui_commands.rs` (`cms_connect` ~`:2060`).

Minimal-blast-radius approach to the `Result` seam: add a parallel `receive_turn_selecting` (or generalize `decide`'s bound to `Fn(&[Proposal]) -> Result<Vec<Answer>, ExchangeError>` and adapt the existing accept-all callers with `Ok(...)`). Prefer generalizing the bound + wrapping the other 6 sites' closures in `Ok(...)` (small, uniform, keeps one engine). On `Err(e)` from `decide`, `receive_turn` returns `Err(e)` **before** `write_bytes(answer_line)` — so abort sends no `FS` and stores no messages.

- [ ] **Step 1 — failing tests:** (a) a decider returning `Err(Cancelled)` makes `receive_turn` return `Err` and write NO answer line (assert via a mock writer that captured bytes contain no `FS`); (b) the existing accept-all callers still behave identically (regression); (c) **abort during prompt → `native_connect` unwinds to `Cancelled`, Inbox unchanged** (backend-level).
- [ ] **Step 2 — run, confirm fail.**
- [ ] **Step 3 — implement:**
  - Generalize `decide` bound to return `Result<Vec<Answer>, ExchangeError>`; wrap the 6 RF/P2P accept-all closures as `|p| Ok(p.iter().map(|_| Answer::Accept{resume_offset:0}).collect())` (verbatim behavior).
  - In `cms_connect` (`ui_commands.rs:2060`): mint `let attempt_id = AttemptId::fresh();` at the start; build the `TauriEventSink`; pass `attempt_id` + `Arc<dyn B2fEventSink>` + the managed `Arc<SelectionRegistry>` into `backend.connect(...)` (extend the `connect`/`native_connect` signatures, or stash the sink+registry+attempt on the call path). Use the SAME `attempt_id` for the result classification.
  - In `native_connect`: **iff** the `review_inbound_before_download` preference is on, build the selecting decider (`build_selecting_decider(registry.clone(), attempt_id, emit_via_sink, aborting.clone())`); **else** keep the existing accept-all closure (wrapped in `Ok`) verbatim. `emit` calls the event sink (already `Send+Sync`, already crosses `spawn_blocking` like `ProgressSink`).
  - In `abort()` (`:1274`): after the socket shutdown, also `*registry.lock().unwrap() = None;` (drop the sender → `recv_timeout` wakes `Disconnected`; combined with the `aborting` flag the decider returns `Err(Cancelled)`).
- [ ] **Step 4 — pass** (`cargo test ... --lib winlink_backend session`).
- [ ] **Step 5 — commit** `feat(winlink): Result-returning decide seam + CMS selecting-connect wiring + abort (tuxlink-bsiy)`.

---

## Task 5: `cms_resolve_inbound_selection` command (managed-state registry)

**Files:** `src-tauri/src/ui_commands.rs` (command near `cms_abort` `:2199`; register in `invoke_handler`); the managed-state setup (where Tauri state is `.manage(...)`d).

- [ ] **Step 1 — failing tests:** (a) resolving with matching `(attempt_id, request_id)` sends + `take()`s the slot; (b) mismatched attempt_id OR request_id → `Ok(())` no-op; (c) double-resolve → second is a no-op (slot already taken); (d) unknown/empty registry → `Ok(())`.
- [ ] **Step 2 — run, confirm fail.**
- [ ] **Step 3 — implement** (registry is `tauri::State<Arc<SelectionRegistry>>`, NOT a backend field — Codex #1):

```rust
#[tauri::command]
pub async fn cms_resolve_inbound_selection(
  attempt_id: u64, request_id: u64, selection: InboundSelection,
  registry: tauri::State<'_, Arc<SelectionRegistry>>,
) -> Result<(), UiError> {
  let mut g = registry.lock().unwrap();
  if let Some(slot) = g.as_ref() {
    if slot.attempt_id.as_u64() == attempt_id && slot.request_id == request_id {
      let slot = g.take().unwrap();              // idempotent: a 2nd resolve finds None
      let _ = slot.tx.send(selection);           // ignore: receiver may have timed out
    }
  }
  Ok(()) // unknown/stale -> silent no-op (frontend also guards via AttemptId stale-filter)
}
```

- [ ] **Step 4 — pass.**  - [ ] **Step 5 — commit** `feat(commands): cms_resolve_inbound_selection via managed-state registry (tuxlink-bsiy)`.

---

## Task 6: Preference (opt-in, default OFF) with live-config refresh

**Files:** the config DTO (`src-tauri/src/config.rs:16-53` per Codex #9) + the setter command pattern (`ui_commands.rs:5068-5104`); the Preferences UI component.

- [ ] **Step 1 — failing tests:** (a) config round-trips with `review_inbound_before_download: bool` `#[serde(default)]` = false; absent field → false (back-compat); (b) the setter **persists AND calls `state.current().set_config(cfg)`** so the next connect sees it without restart (mirror the existing live-config-refresh test).
- [ ] **Step 2 — run, confirm fail.**
- [ ] **Step 3 — implement** the field + a setter following the existing persist-then-`set_config` pattern (do NOT just persist — Codex #9); add the inline Preferences checkbox "Review pending messages before downloading" (WLE label).
- [ ] **Step 4 — pass.**  - [ ] **Step 5 — commit** `feat: review-inbound-before-download preference w/ live refresh, default off (tuxlink-bsiy)`.

---

## Task 7: Frontend — types, hook, inline panel, AppShell mount

**Files:** `src/connections/sessionTypes.ts`, new `src/connections/useInboundSelection.ts` + `InboundSelectionPanel.tsx`(+`.css`), modify `src/shell/AppShell.tsx`, the Preferences UI.

Design notes from adrev: the **backend `recv_timeout` is the single source of truth**; the frontend 45 s countdown is **cosmetic** and on reaching 0 auto-submits the current checkbox state (the backend's own timeout also covers it). No heartbeat. The `AttemptId` stale-filter drops events for a superseded connect (log a `console.warn`); the backend then times out to accept-all — documented, with a test.

- [ ] **Step 1 — failing tests** (Vitest): (a) panel renders one row per proposal (MID + `formatSize`), all rows **pre-checked**; Select All / Deselect All; Hold/Delete radio (default Hold); footer "Download {n} Checked"; submit payload `{ attempt_id, request_id, selection: { selected_mids, disposition } }`. (b) **Production-mount test**: mount inside the real App provider stack (QueryClient etc.), fire a mock `b2f-event` `InboundProposalsOffered`, assert the panel appears and submit invokes `cms_resolve_inbound_selection` with the right payload (memory: production-mount-path). (c) hook stale-filter: an event with a non-current `attempt_id` does NOT render the panel.
- [ ] **Step 2 — run, confirm fail** (`pnpm -C <worktree> vitest run InboundSelection`).
- [ ] **Step 3 — implement** the hook (`listen<B2fEvent>('b2f-event')` → on `InboundProposalsOffered` guarded by the `AttemptId` stale-filter from `useAuthDiagnostic.ts:193`; `submit` → `invoke('cms_resolve_inbound_selection', { attemptId, requestId, selection })`; cosmetic countdown auto-submits at 0) and the panel (model on `CatalogBuilderPanel.tsx`: `onClose`, `Set<mid>` initialized to ALL, toggle/select-all/deselect-all, reuse `formatSize` from `MessageList.tsx:68`, Hold/Delete `<input type=radio>`, inline overlay `position:fixed` + backdrop, ESC = onClose). NOT a separate window.
- [ ] **Step 4 — pass; mount in AppShell** (lazy import + state + overlay mount per CatalogBuilder `:57-63,:270-275,:1069-1085`); add the TS mirror types.
- [ ] **Step 5 — commit** per step. `feat(ui): inline pending-message selection panel (tuxlink-bsiy)`.

---

## Task 8: Verify gate (before any push)
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` — re-run to exit 0.
- [ ] full `pnpm vitest run` + contract tests (`menuModel.test.ts` EXPECTED_IDS if a Preferences menu entry was added).
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --lib` (inbound_selection, b2f_events, session, winlink_backend, ui_commands, config).
- [ ] `pgrep -f vitest` empty afterward (reap zombies).

## Task 9: Operator dev-smoke (browser, cms-z) — MANDATORY gates from adrev
- [ ] Launch `pnpm tauri dev` in the worktree; enable the preference; connect to `cms-z.winlink.org` (authorized dev path, no callsign TX).
- [ ] **Hold/Delete semantics (Codex #7, blocker-gated):** deselect one msg with **Hold** → reconnect → verify it is **re-offered** (server kept it) AND not in Inbox. Deselect one with **Delete** → reconnect → verify it is **gone** and never reached Inbox. If `=`/`-` don't behave as Hold/Delete, switch the operator path to emit WLE's `H`/`N` (extend `answer_line`) before ship.
- [ ] **Socket-idle (Codex #4):** trigger the prompt, wait the full 45 s without answering → verify accept-all completes cleanly (no broken-pipe). If cms-z drops the idle connection, surface the graceful "connection lost during selection" error and reconsider the timeout.
- [ ] **Abort:** abort during the prompt → clean Cancelled, **no** messages downloaded.
- [ ] **Preference OFF:** no prompt; downloads all (parity default, zero behavior change).
- [ ] **Multi-batch:** queue >5 pending → confirm prompt-per-batch behavior matches expectation.

---

## Self-review (3+ rounds) + adrev coverage
- Codex #1 → Task 5 managed-state registry ✓. Codex #2 → Task 4 attempt_id+sink threading ✓. Codex #3 → Task 3/4 `Result` seam + abort test ✓. Codex #4 → Task 9 socket-idle gate ✓. Codex #5/Claude → Task 3/5 `(attempt_id, request_id)` + take() + stale test ✓. Codex #6 → ask_again removed ✓. Codex #7 → Task 9 Hold/Delete gate + H/N fallback ✓. Codex #8 → Task 1/2 redaction + token-leak test ✓. Codex #9 → Task 6 live-refresh ✓. Codex #10 → Task 1 renamed + empty-selection tests ✓. Claude preference-OFF no-op → Task 4 regression test ✓. Claude production-mount → Task 7b ✓. Claude countdown-sync → Task 7 backend-authoritative design ✓.
- **Refuted (no action):** "abort can't unblock — closure holds local tx" — `tx` is moved into the registry slot; dropping the slot drops the only sender. The real abort risk (accept-all semantics) is handled by the `Result`/`aborting` path.
- **Type consistency:** `SelectionSlot { attempt_id, request_id, tx }`, `SelectionRegistry = Arc<Mutex<Option<SelectionSlot>>>`, `InboundSelection { selected_mids, disposition }`, `UnselectedDisposition::{Hold,Delete}`, `PendingProposalDto { mid, uncompressed_size, compressed_size }`, event `InboundProposalsOffered { attempt_id, request_id, proposals }`, command `cms_resolve_inbound_selection(attempt_id, request_id, selection)` — consistent Rust↔TS.
