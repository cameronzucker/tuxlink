# Handoff — harrier-moraine-tanager — smart auth-failure diagnostics shipped (PR #391 open)

> **Date:** 2026-06-04 · **Agent:** `harrier-moraine-tanager` · **Machine:** pandora
> **Arc:** Resumed condor-hemlock-fir's dispatch of tuxlink-7do4 → asked the 7 §6 clarifying questions
> (operator answered) → wrote the design spec + HTML mocks + fixture-provenance doc → 5-round
> adversarial review (R1 general / R2 security+Part 97 / R3 Codex / R4 UX / R5 synthesis,
> ~54 findings) → revised spec end-to-end → wrote 27-task impl plan → executed via
> subagent-driven-development → post-impl Codex round → fixed BLOCKER + 3 MAJORs → opened PR #391.

---

## 0. Next-session critical first action

```
1. Read THIS handoff first.
2. Check whether the operator has merged PR #391
   (https://github.com/cameronzucker/tuxlink/pull/391). If merged:
   - bd-tuxlink-7do4 can be closed.
   - The worktree at worktrees/bd-tuxlink-7do4-smart-auth-diagnostics
     is merged-dead and ready for the ADR 0009 disposal ritual.
3. If NOT merged: leave the worktree + branch in place. Operator may
   request further fixes before merge.
4. If the operator wants to pick up an adjacent task next:
   - bd-tuxlink-y9go (deep B2fEventSink backend.connect threading)
   - bd-tuxlink-pqbt (Tauri-runtime integration test for cms_connect_test)
   - bd-tuxlink-ryh0 (credential_scope on Re-enter-password)
   These were all filed as follow-ups during this session.
5. Run `bd ready` for other work.
```

---

## 1. What shipped (PR #391)

**Title:** `[harrier-moraine-tanager] feat(connect): smart auth-failure diagnostics — distinguish CMS response classes + redact ;PQ/;PR`

**URL:** https://github.com/cameronzucker/tuxlink/pull/391

**Branch:** `bd-tuxlink-7do4/smart-auth-diagnostics`

**Stats:** 30 commits, ~9010 insertions across 34 files. 1045 Rust tests + ~110 React tests, all green pre-merge.

**Closes:** `tuxlink-7do4`.

### Headline outcomes

1. **The feature** — Smart Auth-Failure Diagnostic Banner classifies 6 distinct CMS failure modes (Mode 1 Network unreachable / Mode 2 Client rejected / Mode 3 Password rejected / Mode 4 Callsign rejected / Mode 5 Session dropped / Mode 6 Maintenance) plus an uncategorized fallback, each with contextual recovery affordances inside the Telnet modem dock.

2. **A shipped-bug fix on main** — the existing `telnet.rs::WireTap` was emitting the `;PR` secure-login response token verbatim through `wire_log` into the session log, which feeds the Copy-log clipboard affordance. Combined with ~26.6-bit entropy in the secure-login algorithm (public salt + MD5), the `(;PQ, ;PR)` pair from a single shared log was an offline brute-force oracle for the user's password. The new central `redaction.rs` module scrubs BOTH `;PQ` and `;PR` symmetrically at every sink (case-insensitive regex applied to whole line, position-agnostic — fixed in Codex post-impl BLOCKER #1).

3. **3 follow-up bd issues filed** for items deliberately deferred:

| Follow-up bd ID | What | Why deferred |
|---|---|---|
| `tuxlink-y9go` | Deep B2fEventSink threading through `backend.connect` | Live `cms_connect` Start path currently collapses Mode 5 + Mode 1 → Mode 1. `cms_connect_test` (the "Check this password works" affordance) gets full event stream because it bypasses backend.connect. Deep threading touches ARDOP/VARA/packet code that all pass None. |
| `tuxlink-pqbt` | Tauri-runtime integration test for `cms_connect_test` | Unit-level coverage via `session::run_exchange_with_events` tests; full Tauri AppHandle integration smoke is follow-up. |
| `tuxlink-ryh0` | `credential_scope` check on Re-enter-password affordance | Aux-callsign auth failure would overwrite primary callsign password. Primary-callsign case (dominant) ships correctly. Codex post-impl MAJOR #6. |

### CI watchout (mentioned in PR body)

- New `regex = "1"` dependency in `src-tauri/Cargo.toml`.
- `redaction.rs` uses `std::sync::LazyLock` (stable since Rust 1.80). `Cargo.toml` says `rust-version = "1.75"`. Compiles clean on the Pi but if CI is pinned to exactly 1.75 this would fail. Swap to `once_cell::sync::Lazy` (already transitive dep) if needed.

### Operator smoke plan (in PR body)

The PR body includes a 7-point smoke checklist:
1. Mode 1 TLS-wrong-port → expect TLS-specific copy.
2. Mode 1 DNS → expect DNS-specific copy.
3. Mode 3 password → expect Re-enter / Test creds / Reset on winlink.org.
4. Mode 0 happy path (cms-z with correct creds) → expect NO banner.
5. Mode 2 prod-CMS (server.winlink.org) → expect "tuxlink not on allowlist" copy + Switch-to-cms-z affordance.
6. Dismiss → retry counter increments on re-failure.
7. Copy log → verify NO `;PR:` / `;PQ:` token in clipboard.

---

## 2. Process arc — for transferable-skill capture

This session exercised the full `build-robust-features` pipeline + `subagent-driven-development` flow at significant scale. Key observations Cameron may want to internalize:

### What worked

1. **Pre-flighting all 7 §6 questions** as a single batched message let the operator answer once + me run autonomously for ~10 hours. Per `feedback_no_atomic_decisions_to_operator`: questions were shape-decisions only; design-detail decisions (parser API, event schema, etc.) got defaults + adrev validation.

2. **5-round adrev produced ~54 findings**, half of which I'd never have caught self-reviewing. R2's entropy attack analysis (`(;PQ, ;PR)` brute-force) was the single most expensive finding — it inverted a "challenge is safe alone" assumption I'd baked into §6. **R5 synthesis as its own discipline** (writing the dispositions appendix in spec §14) forced me to triage every finding instead of cherry-picking.

3. **Subagent-driven-development at 27-task scale** worked well. Each subagent had bounded context + the plan code; my orchestration cost was ~10 tool uses per task (dispatch + review). Total session time was ~7-8 hours of subagent work + ~2-3 hours of my coordination — well within the operator's 10-hour autonomous window. The implementer-level deviations from the plan (4 sub-components in Task 21, AttemptId Copy semantics in Task 12, regex pattern in Codex fix) were all _improvements_ over my sketches, not regressions.

4. **Post-impl Codex round caught real bugs the spec-stage adrev missed** — particularly the regex-bypass redaction and the F-line-validation precondition. Codex's whole-program view of the diff against `origin/main` is qualitatively different from spec-stage review of design text.

### What I'd do differently

1. **Don't trust the AttemptId minting layer-by-layer.** Task 12 + Task 14 each minted their own AttemptId at the command layer. The inner `run_exchange_with_events` ALSO minted one. The mismatch broke the cms_connect_test success UX (Codex MAJOR #2). I caught this in implementer self-review but rationalized "it's fine for the banner." It wasn't. **Lesson:** correlation IDs need a single source of truth + explicit threading from day 1.

2. **The original Task 12 deep-threading plan was correctly scope-reduced** (live Start can ship with collapsed Mode 1 + Mode 5) — but I should have updated the spec §14 dispositions table to reflect the reduction. The eventual PR body covers it, but the spec is now slightly inconsistent with what shipped. **Lesson:** if the impl scope changes mid-execution, the spec gets a paired update.

3. **The plan's Task 7 ("redaction in listener + p2p paths") returned NO CHANGE** — the implementer's analysis was correct (the listener/p2p paths route through `session::run_exchange_with_role` which doesn't intercept raw bytes the way WireTap does). **Lesson:** plan-stage threat-model analysis should distinguish "byte-level WireTap" from "application-level wire_log" — they're different boundaries.

---

## 3. Branch + worktree state at handoff

| Branch | State |
|---|---|
| `main` | Pre-PR-#391 — no merge yet. |
| `bd-tuxlink-7do4/smart-auth-diagnostics` | THIS branch — PR #391 open. Awaiting operator review + merge. |

**Active worktree:** `worktrees/bd-tuxlink-7do4-smart-auth-diagnostics/` — clean (nothing uncommitted at handoff time). Contains `node_modules/` (gitignored, installed for pre-push lint hook in Task 1's push step) + `src-tauri/target/` (gitignored cargo build artifacts).

**Disposal**: per ADR 0009 — once PR #391 merges, this worktree is merged-dead and goes through the 4-step disposal ritual. The branch lifecycle hooks will deny further commits/pushes to it post-merge per ADR 0017.

**Other worktrees from prior sessions**: 6+ merged-dead worktrees from gorge-ridge-bog + condor-hemlock-fir's handoffs remain undisposed (per condor-hemlock-fir's handoff §5). Not blocking; operator-call territory.

---

## 4. Notable artifacts (read in this order if picking up)

1. **PR #391**: the actual delivered work + full adrev disposition body.
2. **Design spec** at `docs/superpowers/specs/2026-06-04-smart-auth-diagnostics-design.md` — particularly §14 dispositions appendix.
3. **Impl plan** at `docs/superpowers/plans/2026-06-04-smart-auth-diagnostics-plan.md` — 27 tasks executed.
4. **Post-impl Codex adrev** at `dev/adversarial/2026-06-04-smart-auth-diagnostics-postimpl-codex.md` (local-only; gitignored) — the 7 findings + their dispositions.
5. **Pitfalls entry** CRED-1 at `docs/pitfalls/implementation-pitfalls.md` — pairs with RADIO-1 as the credential-safety rule going forward.
6. **The redaction module** at `src-tauri/src/winlink/redaction.rs` — single source of truth for `(;PQ, ;PR)` scrubbing. Every new sink that touches B2F wire bytes MUST route through it.

---

## 5. Things to NOT do

- **DO NOT close `bd-tuxlink-7do4`** until PR #391 merges. The follow-up bd issues (`tuxlink-y9go`, `tuxlink-pqbt`, `tuxlink-ryh0`) are separate and stay open.
- **DO NOT dispose the worktree** at `worktrees/bd-tuxlink-7do4-smart-auth-diagnostics/` until PR #391 merges — the operator may request additional fixes.
- **DO NOT push to `bd-tuxlink-7do4/smart-auth-diagnostics`** after PR merge — the ADR 0017 lifecycle hook will deny it; use a fresh branch for follow-up work.
- **DO NOT silently widen the credential redaction surface to other modules** without a unit test driving the canonical wl2k-go test vector (`23753528` / `FOOBAR` / `72768415`) through the new path. CRED-1 in the pitfalls doc enforces this.

---

## 6. Session totals

- 30 commits on `bd-tuxlink-7do4/smart-auth-diagnostics`.
- 27 plan tasks executed (Phase 1-7) via subagent-driven-development + 4 Codex-finding fix commits.
- ~9010 LOC added (production + tests + docs).
- 1045 Rust unit/integration tests + ~110 React tests, all green.
- 1 BLOCKER (shipped-bug ;PR leak) remediated.
- 6 adversarial-review rounds total (5 pre-plan on spec + 1 post-impl on diff).
- ~54 adrev findings dispositioned.
- 3 follow-up bd issues filed.
- 1 PR opened (PR #391).
