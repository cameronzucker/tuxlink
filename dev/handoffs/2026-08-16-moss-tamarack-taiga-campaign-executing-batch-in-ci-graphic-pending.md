# Handoff — moss-tamarack-taiga, 2026-08-16 (campaign executing; batch in CI; graphic pending)

COMPACTION ANCHOR #5. Predecessor:
`2026-08-13-moss-tamarack-taiga-ladder-forensics-ir-slice-and-spend-policy.md`
(anchor #4). Same session continues across compaction: KEEP the moniker
moss-tamarack-taiga. Worktree: `worktrees/bd-tuxlink-efk3k-classifier-arch`
on branch `agent-moss-tamarack-taiga/handoff-5` at write time. The cwd
RESETS to the operator's main checkout constantly: standalone `cd` into the
worktree, VERIFY with pwd+branch, then act. One git write per Bash call.

## The single canonical state document

`docs/campaigns/2026-08-surface-repair-ledger.md` — the surface-repair
campaign ledger IS the state; the session-start briefing prints it. Do not
reconstruct campaign state from this handoff; read the ledger. Closed so
far: rows 1 (stale interpolation doc, PR #1353), 4 + 9 (error-hygiene
batch, PR #1354). Full evidence behind every row:
`dev/bug-hunts/2026-08-13-zqo-ladder-regression-read.md` + its 2026-08-14
CORRECTION appendix (the interpolation finding was a stale doc; the
executor has interpolated embedded refs since v0.98; AS-EDIT-ROUTINE
exonerated; spike baseline re-selected to AS-CHECKIN-CLEAN /
COLLAB-NET-CLEAR).

## Shipped this arc (all merged on green)

#1350 spend-policy hook (metered APIs deny-by-default; the hook is ACTIVE
for any session started after its merge — plan-billed codex/claude -p or
local endpoints only). #1351 the zqo regression read. #1352 the campaign
ledger + first tripwire + briefing surfacing. #1353 ledger row 1.

## IN FLIGHT at compaction (finish these first)

1. **PR #1354 (error-hygiene batch, rows 4+9) is riding CI** — a
   background watcher was attached; on green, `gh pr merge 1354 --merge
   --delete-branch` (merge-on-green is a standing operator grant), then
   from the worktree: `git switch --detach origin/main`, delete the local
   branch, and append the PR number to the ledger's Closed entries for
   rows 4/9 in the NEXT ledger-touching PR (they cite "error-hygiene batch
   PR" without the number).
2. **Elmer architecture graphic subagent is running** (background,
   harness-tracked; completion notification will arrive). Deliverable:
   `<session-scratchpad>/elmer-architecture-redesign.html` — REVIEW the
   file, then publish with the Artifact tool (favicon "🛡️", title comes
   from its <title> tag) and give the operator the URL. Brief it followed:
   old-design vs redesign security architecture, deterministic boundary
   layers as strata, tactical-dark primary + field-manual light themes,
   status chips SHIPPED/IN BUILD/DESIGNED/DEFERRED, honest deferred note,
   no em-dashes in body copy.
3. **Handoff PR (this file) — merge on green.**

## Then: remaining mechanical ledger rows (umbrella bd tuxlink-4280b)

In batch order: wire-shape batch (rows 6 zero-sentinels + 7 mufday label);
row 3 doc-17 reconciliation (doc 16 is right); row 8 find_stations goal
doc (REMEMBER the tool-surface corpus regen gate on R2:
TUXLINK_REGEN_TOOL_SURFACE=1, scp artifact back); row 5 typed-precondition
error classes (its own PR: raw String egress boundary, NOT UiError); row 2
fix (last-session summary on modem_get_status + the `selected` affordance
+ VARA open/status coherence unit tests — the fork was SETTLED by the
operator-approved comparison test from retained fixture evidence: action
tools truthful, sessions transient, status amnesiac). Every such PR cites
its ledger row and closes it in-file; flipped tripwires get RENAMED out of
the known_defect_ namespace.

## Waiting on the operator (surfaced as an actionable list 2026-08-14; he
answered item 2 = the comparison test, done)

1. IR one-pager read (`dev/spikes/2026-08-13-ir-compiler-slice/
   IR-ONEPAGER.md`) — the ONLY remaining spike gate; baseline re-selected.
2. The bench relay paste (block provided in-conversation; also ledger row
   21 + read doc §BENCH).
3. Four design calls with my defaults: outbox taint (recommend: keep
   taint, add transmit outcome summaries), clean bit (recommend: add it),
   parity grants tx-grid + preset-create (recommend: grant both, preset
   under writes-config consent), engine default (recommend: keep + add the
   linear ungated-transmit lint).
4. Older queue: sideload ratification, readback wording (branch
   bd-tuxlink-k2h9l/readback-eval parked), openrouter block removal in
   ~/.codex/config.toml.

## Post-campaign work (unchanged from anchor #4)

Inkling A/B per `dev/scratch/inkling-ab-smoke-plan.md` (OPEN empirical
question: does the Spark serve the responses wire API — codex rejects
wire_api "chat" now; fallbacks written in the plan). IR spike on the
operator's one-pager approval. Weekend-epic remainder: request-classifier
wiring + in-repo measurement, content triage pre-quarantine.

## Standing rules (each cost a correction once — do not relearn)

Plan-billed/local model transports ONLY (hook enforces; key ≠ authority).
Attribution discipline: happy path written out first; spec/fixture/doc are
suspects — VERIFY AGAINST CODE (the campaign's founding lesson). Evidence
records get corrections APPENDED, never rewritten. bd dup-search BEFORE
creating issues. Operator tree = untouchable (a chained git switch nearly
bit it again 2026-08-14; git refused). Ask-don't-guess is DESIRED model
behavior in diagnosis domains (operator ruling). rx_grid == own grid is
REQUIRED NVIS functionality. Codex round on every code PR; stdin prompt
via file; findings verified against source before acting.
