# Handoff — Routines ranks 1-5 + O3/O4 build arc SHIPPED (all 6 PR groups merged)

- **Agent:** alder-oriole-cedar (2026-07-18/19, single long session)
- **Work item:** bd **tuxlink-iizmk** (the round-2 build arc)
- **Model note:** started on Fable 5, switched to Opus 4.8 mid-arc after a Fable usage limit (during B2 chunk 1); no work lost.

## What shipped — 6 PR groups, all merged to main

Design + plan: PR #1164. Then serially:
- **#1166 (A)** tolerant journal reader — unknown future event types become opaque entries instead of corrupting History (had to merge first).
- **#1170 (B)** O3/O4: call_child + end_reached + park_kind journal events; two-phase invoker split (child run ids journaled in all call paths, parent cancellation propagates, children registry-cancellable, F&F can't wedge at Running); End reason threads into run_finished; History call/end/park rows with child navigation.
- **#1173 (C)** writes_config consent class: one parameterized consent-closure walk + canonical SHA-256 digest; write_ack + digest binding on BOTH ack classes (an ack binds iff by/at set AND digest == live closure digest — routine edit / callee edit / digest-less legacy all invalidate it, closing the R1 replay hole); AUTO_WRITE_UNACKED + digest clause on AUTO_TX_UNACKED, MIXED_MODE_STALL_WRITE, ATTENDED_WRITE_UNDER_SCHEDULE, WRITE_VALUE_RUNTIME; single-read start gate (TOCTOU close); child-start root-digest re-verification; acknowledge_write + routines_consent_closure UI-only (pinned absent from MCP router).
- **#1174 (D)** the five action families: rank 1 status reads (modem/backend/app), rank 3 config reads (config + 4 per-modem), rank 2 data.find_stations (callsigns feed radio.connect), rank 4 data.docs_search, rank 5 config.set_ardop (first writes_config action, locked drive-level RMW shared with + de-racing the MCP write path). Every read pinned byte-identical to its MCP tool. Shape-true dry-run + example_params + UNKNOWN_READ_SOURCE lint. **24/24 compat-tree cells now human-actionable.**
- **#1175 (E)** frontend consent+authoring: ConsentGate branches on park kind (write parks never render Part 97 transmit copy, incl. journal launch-recovery); SettingsTab ack panels render VALIDITY (valid/pending/present-but-invalid) + enumerate the closure being signed; palette WRITES badge + example_params seeding + inspector description.
- **#1176 (F)** user-guide routines-actions reference (searchable by the data.docs_search it documents) + compat-tree §3 coverage recount 0->24/24 + wire-walk reachability evidence.

## Verification provenance
All Rust compiled/tested on R2 (ssh r2-poe, rustup 1.96 at ~/.cargo/bin — NOT distro 1.75). Every task captured a behavioral FAIL before implementing. Final counts on the last group's base: leaf tuxlink-routines ~269, monolith ~3600+, MCP 108, full vitest 4631 + typecheck, workspace clippy --all-targets -D warnings clean. Frontend on the Pi.

## Adversarial rigor
Design: 5 rounds (R1+R5 Codex gpt-5.5 per ADR 0023; R2 engine, R3 contract/completeness, R4 authoring-UX) — 30 findings, all folded. Plan: 3 review rounds folded (branch mechanics under ADR 0017; ChildHandle cross-crate constructibility; engine OnceLock invoker mount; F&F oneshot relay; dry_run_shape descriptor hook; per-source dry-run shapes).

## Incidents / flakes this session (all handled)
1. **Wrong-branch commit (recovered).** Mid-C3, cwd silently reset to the main checkout and `git add -A` swept its ~106 pre-existing untracked files into a junk commit on bd-tuxlink-ant8s/ardop-connect-fixes (NO C3 code — that was safe in the worktree). Undone with `git -C <main> reset HEAD~1` (mixed, non-destructive); C3 re-committed correctly. Lesson appended to memory feedback_worktree_git_mechanics: NEVER `git add -A` (stage explicit paths); re-cd after any non-cd command run.
2. **arm64 Rust flakes (re-run).** winlink_backend packet_answer_p2p_intent (group B, filed new) and another arm64 Rust flake (group E). Both: my diff had zero Rust (E) or didn't touch the file (B); re-ran the failed job → green.
3. **ConsentGate.test.tsx flake (tuxlink-2h16p).** reopenSignal-bump test flakes ~1/3 under full-file runs (cross-test async leak in useParkedRuns mount-recovery), NOT arm64-only. Diff-verified pre-existing (E1's parkKind change is purely additive). Root cause + broader repro added to tuxlink-2h16p notes. Left unfixed (scope-separate); handle a CI trip with a re-run.
4. **F1 real regression (fixed).** The routines-actions chapter was ungrouped in src/help/topics.ts → buildTopics throws at import under full vitest (scoped run missed it). Fixed by grouping it in the using-tuxlink section.

## What REMAINS — operator / next-session (F2 live acceptance)
The code is complete and merged. The live half of the wire-walk gate is the operator's:
1. **Converge rebuild** on R2 (`pnpm dev:converged` — restarts the app) to pick up all six groups.
2. **Operator supplies the key user flows greenfield** (per the wire-walk rule — the agent must NOT draft them): author each new action in the live designer, dry-run, run, read History.
3. **Live wire-walk capture:** a probe exercising branch + skip + sync call (child) + End-with-reason, journal captured to a fresh dev/render-harness/real-run-<date>.json (&real=2), History rendered.
4. **Attended config.set_ardop write-park:** needs the operator's own click (a Part 97-style operator act). Validated by vitest + the E4 harness render; the live click is the operator step.
5. **ADR 0024** accept/reject (still Proposed).
6. Then **close tuxlink-iizmk**.

## R2 outbox
The N0RNG observability probe message was DELETED at session start (moved to Trash with app-faithful sidecars + search-index repoint). The operator's own 2026-07-08 self-addressed Elmer diagnostic (ALH7GWOQ47D7) remains in the outbox — left alone; it will forward on the next CMS connection.

## Follow-ups filed
- bd **tuxlink-yt3tc** — general callee-pinning (Call targets resolve live, contra §7 snapshot doctrine; consent half closed by child-start re-verification, general half deferred).
- bd new flake issue for the winlink_backend arm64 P2P observation test.
- tuxlink-2h16p — ConsentGate flake, root-caused + broader repro noted.
