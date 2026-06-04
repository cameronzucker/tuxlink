# Handoff — condor-hemlock-fir — research arc + smart-auth-diagnostics dispatch

> **Date:** 2026-06-04 · **Agent:** `condor-hemlock-fir` · **Machine:** pandora
>
> **Arc:** Resumed gorge-ridge-bog's docs handoff (tuxlink-yzn6 Wikipedia gaps
> + Gmail-group research) → Mermaid CSS fix → Mermaid parse fix → 4,105-thread
> corpus deep-scrape → durable evidence archive → smart-auth-diagnostics issue
> dispatched for fresh-session implementation.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. Read PR #390's research note + archive README:
   - dev/research/2026-06-04-winlink-group-pain-points.md
   - dev/research/winlink-group-corpus-2026-06-04/README.md
3. Read bd issue tuxlink-7do4 (smart auth-failure diagnostics) IN FULL.
4. Ask the operator the §6 clarifying questions BEFORE writing any code.
   The operator will be at work and answering asynchronously; pre-flight
   ALL questions in one batch so they can reply once and let you run.
5. Once answers are in hand, work autonomously for ~10 hours. Stack
   commits on a single follow-up branch off main: bd-tuxlink-7do4/<slug>.
```

---

## 1. Session arc (compressed)

1. **Resumed gorge-ridge-bog's handoff** (PR #361 merged before session
   start; PR #347/#351/#352/#354/#357 already merged).
2. **PR #364 (MERGED)** — 4 Wikipedia-derived docs gaps: Hybrid Network
   (topic 05), VARA FM (16), PACTOR-not-supported (17), OMV + no-encryption
   (26). Closed Part 1 of tuxlink-yzn6.
3. **PR #370 (MERGED)** — Account lifecycle + keyring credentials (topics
   02 + 27). Operator pruning during session removed SHARES/MARS + version-
   archive sections, expanded password addition. Closed Part 2 of
   tuxlink-yzn6.
4. **PR #383 (MERGED)** — Mermaid CSS theming fix. Root cause: Mermaid v11
   embeds an ID-scoped `<style>` block inside the rendered SVG that beats
   class-only overrides; `!important` required on all theming rules. Also
   corrected dead `.edgePath path` selector → `.flowchart-link` and added
   HTML-element targeting for `.labelBkg` div edge label backgrounds.
   Closed tuxlink-8a84.
5. **PR #387 (MERGED)** — Topic 06 sequence diagrams parse fix. Both
   ```mermaid blocks in `06-the-b2f-protocol.md` were rendering as
   exploding-bomb "Syntax error" blocks (PR #383's CSS theming made
   them visible — they were rendering black-on-dark-blue before).
   Mermaid's sequence parser treats `;` as a line separator inside
   message/note bodies. Rephrased to remove `;`. Closed tuxlink-dls2.
6. **PR #390 (OPEN AT HANDOFF)** — Two-part research deliverable:
   (a) updated synthesis note based on 4,105-thread corpus instead of
   the 25-thread Pass-1 sample; (b) durable evidence archive at
   `dev/research/winlink-group-corpus-2026-06-04/` with redacted corpus
   (11 MB JSONL), scrape script, redactor, theme-quantification
   script, README, and `themes.tsv`. Closes tuxlink-n3h6.
7. **Smart-auth-diagnostics issue filed** (tuxlink-7do4, P2 task) per
   operator strategy direction. This handoff dispatches it for fresh-
   session implementation.

---

## 2. Branch state at handoff

| Branch | State |
|---|---|
| `main` | Carries merged PRs #364, #370, #383, #387 (this session) + parallel-session merges. |
| `bd-tuxlink-xygm/recover-handoffs` | Operator-state branch; pre-session-start uncommitted handoffs from prior sessions still uncommitted in main checkout (operator's decision per session-start: "Skip cleanup entirely; you'll handle it"). |
| `bd-tuxlink-n3h6/deepened-corpus-synthesis` | **OPEN PR #390** — 2 commits (synthesis update + corpus archive); awaiting operator review + merge. THIS HANDOFF DOC ALSO LIVES ON THIS BRANCH. |
| `bd-tuxlink-yzn6/docs-winlink-coverage-gaps` | Merged-dead (PR #364) |
| `bd-tuxlink-yzn6/docs-account-keyring` | Merged-dead (PR #370) |
| `bd-tuxlink-8a84/fix-mermaid-theming` | Merged-dead (PR #383) |
| `bd-tuxlink-dls2/fix-topic-06-mermaid-parse` | Merged-dead (PR #387) |

---

## 3. PRs from this session

| # | Title | State |
|---|---|---|
| #364 | docs: 4 Winlink coverage gaps (Wikipedia-derived) | MERGED |
| #370 | docs: Winlink account lifecycle + keyring | MERGED |
| #383 | fix(help): Mermaid CSS wins ID-scoped specificity via !important | MERGED |
| #387 | fix(docs): topic 06 sequence diagrams — remove `;` | MERGED |
| #390 | docs(research): deepen Winlink-group synthesis (4,105 threads) + archive | **OPEN — awaiting operator review/merge** |

---

## 4. bd issues touched

| Issue | Status | Notes |
|---|---|---|
| `tuxlink-yzn6` | Closed by PR #370 (both parts done) | Wikipedia gaps + Gmail-group synthesis |
| `tuxlink-tdeg` | Filed + closed same session as weak-evidence | Callsign-validation distinguishing; the 536-thread corpus would now support re-filing as a product issue |
| `tuxlink-8a84` | Closed by PR #383 | Mermaid CSS fix |
| `tuxlink-dls2` | Closed by PR #387 | Topic 06 parse fix |
| `tuxlink-n3h6` | Closed by PR #390 (when merged) | Deepened-corpus synthesis |
| **`tuxlink-7do4`** | **OPEN, P2, READY FOR PICK-UP** | **Smart auth-failure diagnostics — THIS HANDOFF'S DISPATCH TARGET** |
| `tuxlink-drbi` (pre-existing) | Open | Tauri 2.x app_id mismatch — unrelated; available as alt-pick |

---

## 5. Worktree disposal state

All 6 merged-dead worktrees from gorge-ridge-bog's handoff REMAIN undisposed
(operator-call territory; uncommitted content in some worktrees discovered
during inventory at session start makes default disposal risky). Add to that
the 4 merged-dead branches from this session's worktrees. None of these are
blocking for tuxlink-7do4 work.

Active worktree: `worktrees/bd-tuxlink-yzn6-docs-winlink-coverage-gaps/`
(this session's worktree; contains the in-flight dev/research artifacts +
gitignored dev/scratch/winlink-group-research/ corpus + scripts +
deleted-cookies). Tuxlink-7do4 work should create its own fresh worktree
off origin/main; this worktree can be disposed once PR #390 lands.

Gitignored-but-stateful content in this worktree (per ADR 0009 inventory):
- `dev/scratch/winlink-group-research/corpus-deep.jsonl` (raw scrape,
  4,626 records including 521 bot-degraded; NOT committed; superseded by
  the redacted-and-committed `dev/research/winlink-group-corpus-2026-06-04/corpus.jsonl`)
- `dev/scratch/winlink-group-research/corpus-clean.jsonl` (the filter
  output before redaction)
- `dev/scratch/winlink-group-research/scrape-deep.log` (run log)
- `dev/scratch/winlink-group-research/thread-list.png` + `deep-list.png` +
  `probe-*.png` (diagnostic screenshots)
- `dev/scratch/mermaid-render-now.png` + `mermaid-issue-2.png` +
  `mermaid-verify.png` + `mermaid-probe-svg.html` (from PR #383 + PR #387
  diagnosis)

None of the above is load-bearing for future work — the durable artifacts
are committed under `dev/research/winlink-group-corpus-2026-06-04/`. The
operator-exported cookies were deleted at end of scrape.

---

## 6. Anticipated clarifying questions for fresh session to ask UP FRONT

Operator will be at work and answering asynchronously. The fresh session
should pre-flight ALL of these in one message so the operator can reply
once and let the agent run for ~10 hours.

1. **UI placement of the failure-detail state.** Should the per-failure-
   mode diagnostic surface as (a) an inline banner inside the connect
   panel above the session log, (b) a modal that pops on transition to
   the error state, (c) inline within the session log itself as a
   highlighted block, or (d) a sidebar callout adjacent to the connect
   button? Default proposal if no answer: (a) inline banner with the
   recovery action(s) embedded.

2. **Recovery action set.** Beyond the deep-link to winlink.org for
   password reset, which of the following recovery affordances should
   the diagnostic offer? (i) "Re-enter password" inline edit, (ii)
   "Re-run wizard" button, (iii) "Test credentials again" diagnostic
   button without committing to a real CMS connect, (iv) "Copy session
   log" for sharing with help channels. Default proposal: all four
   surfaced contextually per failure mode.

3. **Error-string wording.** Should the diagnostic copy mirror WLE's
   wording (more familiar to migrating operators) or use tuxlink-
   original copy that names the actual failure mode plainly? Default
   proposal: tuxlink-original (matches tuxlink's "be a modern app"
   posture from the 2026-06-04 strategy discussion).

4. **Scope envelope for the 10-hour work session.** Three options:
   (a) Minimum-viable: taxonomy parsing + connect-panel banner with
   password-rejection as the only fully-wired recovery path. Other
   failure modes surface but recovery is generic 'check the session
   log'. ~6 hours.
   (b) Full taxonomy + all 5 recovery paths wired. ~10 hours.
   (c) Full taxonomy + recovery + cms-z integration tests. ~14 hours
   (likely overflows single session).
   Default proposal: (b).

5. **Test reach for the session.** (i) Unit tests for the taxonomy
   parser against Pat-derived response-string fixtures (cheap, fast).
   (ii) Integration tests against cms-z.winlink.org (needs that to be
   reachable and credentials to be available; the password-rejection
   test specifically needs a known-bad-password path). (iii) Both.
   Default proposal: unit + cms-z-integration for the happy path; defer
   cms-z password-rejection integration to a follow-up session that
   operator-coordinates the test creds.

6. **Pat-reference depth.** Per ADR 0016 tuxlink went native-B2F (no
   Pat sidecar). Should we still cite Pat's response-handling code as
   the canonical reference for response-string mapping, or use the B2F
   prose docs only (which `feedback_winlink_re_authoritative_sources`
   flags as unreliable)? Default proposal: Pat-as-reference. The work
   includes a `dev/scratch/<session>/pat-auth-fixtures.md` document
   citing the specific Pat source files + line numbers used as input
   to the taxonomy.

7. **B2F handshake instrumentation.** To map response strings to
   failure modes, we need to capture the actual handshake bytes. Should
   the new code path also add structured logging at the B2F-handshake
   layer (categorized events, timestamps, last-line-received) that the
   connect panel reads from? Default proposal: yes — the structured
   log feeds the diagnostic display.

---

## 7. Reference materials for the fresh session

In the existing tuxlink codebase:
- B2F handler: `src-tauri/src/winlink/...` (audit; not deep-read this session)
- Connect command path: `src-tauri/src/commands.rs` or similar (audit)
- Connect panel UI: `src/connections/...` or `src/shell/AppShell*.tsx` (audit)
- Existing session log component: `src/radio/sections/SessionLogSection.tsx`
- Existing connection state types: `src/connections/sessionTypes.ts`

In the corpus:
- `dev/research/winlink-group-corpus-2026-06-04/corpus.jsonl` — search for
  "auth failed", "password not recognised", "callsign not recognized",
  "Login failed", "connection refused" to gather WLE error-string fixtures.

External references:
- Pat source at https://github.com/la5nta/pat (or use Cameron's local
  clone if available — should be under `external/` per ADR 0011, though
  ADR 0016 went native).
- B2F protocol prose docs (Winlink website, suspect per memory).

Memories the fresh session MUST honor:
- `feedback_no_disk_creds_default` — keyring only; no env vars or config
  files for credentials in any new code path.
- `feedback_no_users_calibration` — only data loss is a real consequence;
  don't propose ceremony for non-data-loss situations.
- `feedback_radio1_governs_tx_not_ui` — Part 97 consent is the Connect
  click, not the UI changes for the error display. Don't escalate this
  work to "RADIO-1 P1" framing.
- `project_cms_rejects_unknown_clients` — tuxlink is NOT on prod CMS
  allowlist; cms-z.winlink.org is the dev target.
- `feedback_winlink_re_authoritative_sources` — Pat / wl2k-go are ground
  truth; prose docs are unreliable.
- `feedback_no_ceremony_spiral_on_small_fixes` — but this IS a multi-day
  product feature, not a small fix. Build-robust-features pipeline IS
  the right discipline here (TDD against the taxonomy + at least one
  Codex adrev round before declaring impl complete).

---

## 8. Out-of-repo state changes

- 23 Google session cookies (operator-exported via Cookie-Editor in
  Firefox laptop, pasted into chat, saved to gitignored path)
  USED for two scrape runs (Pass 1 + Pass 2 deep), then DELETED at end
  of scrape session. No cookies persist on disk.

---

## 9. Session totals

- **4 PRs merged this session** (#364, #370, #383, #387)
- **1 PR open at handoff** (#390 — research synthesis + corpus archive)
- **5 bd issues closed** (tuxlink-yzn6, tuxlink-tdeg, tuxlink-8a84, tuxlink-dls2, tuxlink-n3h6-via-PR-390-merge)
- **1 bd issue filed for next session** (tuxlink-7do4 — smart auth diagnostics)
- **11 MB durable evidence archive** committed (`dev/research/winlink-group-corpus-2026-06-04/`)
- **4,105 thread records** captured + redacted + theme-quantified
- **2 strategic decisions made** with operator: reject view-password/export-credentials (Layer 1 of the strategy menu); accept smart auth diagnostics + Layer 2 deep-link to reset.

---

## 10. Next-session prompt (paste into a fresh session)

```
Resume tuxlink as a fresh autonomous-execution session picking up
condor-hemlock-fir's 2026-06-04 dispatch of tuxlink-7do4 (smart auth-
failure diagnostics in the connect panel).

Handoff doc lives on PR #390's branch (bd-tuxlink-n3h6/deepened-corpus-
synthesis) at dev/handoffs/2026-06-04-condor-hemlock-fir-handoff-smart-
auth-diagnostics.md — READ IT FIRST. The synthesis note and corpus
archive that motivate the work are in the same PR.

Critical gating step before code: ask the operator the §6 clarifying
questions (7 questions, presented as a single batched message). The
operator will be at work and answering async; pre-flight them all
upfront so they can reply once and let you run.

Once answers are in, work autonomously for ~10 hours. Stack commits on
a single follow-up branch off origin/main: bd-tuxlink-7do4/<your-slug>.
This is multi-day product feature work — build-robust-features pipeline
applies (TDD against the taxonomy + at least one Codex adrev round
before declaring impl complete).

Reject any urge to add view-password or credential-export features —
explicitly out of scope per operator decision 2026-06-04. Honor the
no-disk-creds-default memory throughout.
```

---

Agent: condor-hemlock-fir
