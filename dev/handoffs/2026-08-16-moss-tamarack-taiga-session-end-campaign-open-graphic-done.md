# Handoff — moss-tamarack-taiga, 2026-08-16 SESSION END

FINAL anchor for this session (supersedes anchor #5, same day, whose
in-flight list is now stale). The session ended by operator direction, not
at a natural stopping point, so read this fully before acting.

## THE CANONICAL STATE IS NOT THIS FILE

`docs/campaigns/2026-08-surface-repair-ledger.md` is the surface-repair
campaign's state. The session-start briefing prints its path and open-row
count. Read the LEDGER for what to do next; read this handoff only for
context the ledger cannot carry. Evidence behind every row lives in
`dev/bug-hunts/2026-08-13-zqo-ladder-regression-read.md` plus its
2026-08-14 CORRECTION appendix.

## Shipped this session (all merged on green, all Codex-reviewed)

| PR | What |
|---|---|
| #1350 | Spend-policy hook: metered model APIs deny-by-default. ACTIVE for every session started after that merge. Plan-billed codex/`claude -p` or local endpoints only; loud audited override. |
| #1351 | The zqo ladder regression read: all 49 non-green cells classified, judge-stringency split vs v26, deep-dive on four operator-flagged cells, P1-P17 product + F1-F10 bench findings. |
| #1352 | Surface-repair campaign scaffolding: the ledger, the first known-defect tripwire, briefing-hook surfacing, closure protocol. |
| #1353 | Ledger row 1: stale `local.log` interpolation prose corrected to match the executor. |
| #1354 | Ledger rows 4 + 9: folder refs case-fold and classify as caller input (parse before backend resolution, bare teaching detail); UNKNOWN_PARAM becomes a repairable advisory. |
| #1355 | Anchor #5 (superseded by this file). |

Ledger rows CLOSED: 1, 4, 9. Umbrella issue for the mechanical rows:
`tuxlink-4280b` (its notes carry the remaining-row plan).

## Next work, in order

1. **Wire-shape batch** (ledger rows 6 + 7): `packet_config_get` zero
   sentinels read as absence; `mufdayByHour` is an unlabeled 0-1 fraction
   that both a model and the judge misread as MHz.
2. **Row 3**: user-guide doc 17 contradicts doc 16 on ARDOP-vs-VARA
   weak-signal robustness. Doc 16 is right.
3. **Row 8**: `find_stations intent:recommend` requires `goal` and the
   description does not say so. REMEMBER the tool-surface corpus regen gate
   (`TUXLINK_REGEN_TOOL_SURFACE=1` on R2, scp the artifact back).
4. **Row 5** (its own PR): precondition failures wear the internal-error
   code. Different boundary from row 4: raw String errors through the
   egress layer, not `UiError`.
5. **Row 10**: `vara_status.reachable` TTL cache disagreed with a
   same-session connect refusal.
6. **Row 2 fix**: the fork was SETTLED by the operator-approved comparison
   test (retained zqo fixture teardown reports): ARDOP action tools were
   TRUTHFUL, a real secure-login B2F session moved one message, sessions
   are transient, and `modem_get_status` is amnesiac afterward while
   `selected` misleads by sticking on vara-hf. VARA half: no false-success
   proven. Fix = last-session summary on the observation surface + the
   `selected` affordance + VARA open/status coherence unit tests.

Every such PR cites its ledger row and closes it IN-FILE; a flipped
tripwire gets RENAMED out of the `known_defect_` namespace so `grep
known_defect` always equals the open set. Append PR #1354's number to the
rows 4/9 Closed entries in the next ledger-touching PR.

## The Elmer architecture graphic: DONE, machine-local

`dev/scratch/elmer-architecture-redesign.html` (gitignored, so it lives on
this Pi only). Open with `file:///home/administrator/Code/tuxlink/dev/scratch/elmer-architecture-redesign.html`.
Conceptual brief: original design vs redesign, five classifiers, one
deterministic core, three lanes, three node-flow worked examples, both
themes. OPERATOR-RATIFIED DESIGN RULES are embedded as the file's top
comment: honor them on any revision.

It went through a full Codex accuracy adrev (27 findings, transcript at
`dev/adversarial/2026-08-16-elmer-arch-graphic-codex.md`, gitignored). ALL
27 applied and verified. The audit record, including which findings were
verified against source and the corrected facts, is
`dev/scratch/elmer-graphic-corrections-pending.md` (banner marks it
APPLIED). Five criticals worth knowing because they are FACTS ABOUT THE
PRODUCT that were stated wrongly and are now right:

1. The routine IR is JSON (top-level `routine`, `every`/`window`, a `do`
   list, `connect`, nested `on_success`/`on_failure`, one-key `log`,
   object `end`). Blocks contain steps; gotos are unexpressable.
2. Compile failures produce a NAMED, POSITIONED REFUSAL of an unrecognized
   construct. Not a docs punt, and a fallthrough-style error is impossible
   in that language.
3. Consent is OUTSIDE the IR entirely.
4. Automations do NOT use an arm grant at enable. An automatic routine
   carries a design-time, operator-only `transmit_ack` recorded in Routine
   Settings, bound to a digest of its transmit closure, invalidated by any
   relevant edit; `routines_enable` succeeds only while it is valid.
   (`docs/superpowers/specs/2026-07-13-routines-design.md` section 4.)
5. Per-datum provenance is BANDWIDTH-BY-TYPE, not origin tracing: real
   provenance cannot be recovered from model-supplied JSON, so each
   parameter is classified by the bandwidth its type permits. Egress stays
   blocked on session taint even when every parameter is clean.
   (`src-tauri/tuxlink-security/src/provenance.rs`.)

Facts 4 and 5 are worth internalizing beyond the graphic: I stated both
wrongly from memory, and both were caught only by reading source.

## Waiting on the operator (nothing here is agent-actionable)

1. **IR one-pager read** (`dev/spikes/2026-08-13-ir-compiler-slice/IR-ONEPAGER.md`)
   — the only remaining gate on the compiler spike. Note the baseline
   moved: AS-EDIT-ROUTINE was exonerated, so the A/B re-baselines on the
   real-gating class (AS-CHECKIN-CLEAN / COLLAB-NET-CLEAR).
2. **Bench relay** of the F1-F10 findings to the bench agents (ledger row
   21 carries the list; the read doc section BENCH carries the full text).
3. **Four design calls** with recommendations: outbox taint scope (keep
   taint, add transmit outcome summaries), the clean bit (add it),
   parity grants for tx-grid override and preset-create (grant both), the
   engine's continue-past-failed-connect default (keep, add the linear
   ungated-transmit lint).
4. Older queue: sideload ratification, readback wording (branch
   `bd-tuxlink-k2h9l/readback-eval` parked), the stale
   `[model_providers.openrouter]` block in `~/.codex/config.toml`.

## Post-campaign work

Inkling A/B per `dev/scratch/inkling-ab-smoke-plan.md`. OPEN empirical
question recorded there: current codex rejects `wire_api = "chat"`, so the
Spark profile was switched to `"responses"` and whether the Spark serving
stack speaks that wire API is unverified; fallbacks are written into the
plan. Then: IR spike on approval, request-classifier wiring plus in-repo
measurement, content triage at pre-quarantine scope.

## Standing rules (each cost a correction; do not relearn)

- Plan-billed or local model transports ONLY. The hook enforces it now. A
  key in the keyring is not authorization.
- **Verify against CODE, not docs.** This campaign's founding lesson, and
  it recurred twice more this session (the stale interpolation prose; my
  own consent and provenance errors).
- Evidence records get corrections APPENDED, never rewritten.
- Attribution discipline: write the concrete happy path first; the spec,
  fixture, doc, and reviewer are all suspects.
- bd dup-search BEFORE creating issues.
- The operator's main checkout is untouchable. cwd RESETS to it constantly:
  standalone `cd` into the worktree, verify with pwd and branch, then act.
  One git write per Bash call.
- Ask-don't-guess is DESIRED model behavior in diagnosis domains.
- `rx_grid == own grid` is REQUIRED NVIS functionality, not a defect.
- Codex adrev on every code PR and on factual documents; verify its
  findings against source before applying, and narrow anything it
  overreaches.

## Environment

Worktree `worktrees/bd-tuxlink-efk3k-classifier-arch` on branch
`agent-moss-tamarack-taiga/handoff-6` at write time; park it detached on
`origin/main` after this merges. No other in-flight branches from this
session; all task branches merged and deleted both sides. Nothing running
on R2: the zqo ladder is complete and its judge drained.
