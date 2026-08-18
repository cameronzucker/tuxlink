# Overnight handoff + compaction re-entry gate — basil-redwood-cove, 2026-08-18 ~03:30 AZT

SAME SESSION CONTINUES across a deliberate pre-overnight compaction (operator
call: 65% context + overnight run = compact on committed artifacts, the
anti-drift structure from the 2026-08-17 incident review). Keep the moniker
**basil-redwood-cove**. The operator is ASLEEP; he reviews DATA in the
morning. Nothing below asks him anything before then.

## RE-ENTRY GATE — do ALL of this before any other action post-compaction

1. Read IN FULL: this file; `dev/spikes/2026-08-13-ir-compiler-slice/INSTRUMENT-2026-08-18-insitu.md`
   (the pre-registered spec — the ONLY authority for overnight work) with its
   two SHEET files and `ALTERNATIVES.md`; CLAUDE.md §"Session conduct";
   memory files `feedback_standing_conduct_2026_08_18.md`,
   `project_ci_merge_mechanics.md`, `feedback_git_session_mechanics_cluster.md`,
   `feedback_no_unspecced_self_iteration.md`, `feedback_subagent_tier_default_low.md`.
2. Verify live state matches the ledger below: `gh pr list` + checks;
   `git worktree list` (expect: che1k + izcq0 + qqmys live under worktrees/,
   plus dozens of pre-existing stale ones — DO NOT touch the stale set);
   `bd list --status=in_progress | grep -E "che1k|izcq0|qqmys|wnfkm"`.
3. Serving pre-flight per the spec (catalog + 1-token generation; model id
   `thinkingmachines/Inkling-Small-NVFP4`; NO sampling params anywhere).
4. If ANY divergence from this ledger: stop that thread, record it in the
   results errata, continue only the unaffected work. Never improvise
   around a wall — write it down (operator's explicit instruction).

## THE OVERNIGHT TASK (operator-directed, verbatim intent)

"Assemble a more suitable test ladder instrument to pressure test the IR
candidates in situ while I sleep... The guardrail against another incident
is just using that time for testing and not blind iteration without spec.
I'll review the data in the morning."

Execution = the committed spec, exactly: build `tuxlink-mcp-testserver` +
`d3zwe` release on THIS Pi (small crates, ~2-4 min warm; the full app never
builds here), run the 33-run matrix, grade per the pre-registered
assertions, produce the results document (tables + verbatim emissions +
traces + errata) in the spike dir on the che1k branch, commit + push + PR
with CI watcher. LABEL: instrument-shakedown data, NOT the gated spike.
Surfaces are FROZEN; no IR redesign; no product changes; no bench.

## Morning review queue (surface this to the operator, in order)

1. Overnight in-situ results (the PR from che1k).
2. The two IR rulings: flatness-vs-blessed-unrolling (operational evidence
   generatable in-spike behind a flag); spike arms + environment
   (ALTERNATIVES.md open decisions 1-2; placement + status-header are 3-4).
3. His classifier-paring observation vs the settled recon answer (nothing
   on main narrows the tool list — executor.rs:72-75 documents full-surface
   as invariant; his observed paring needs a source: converged build? a
   branch? a different program?). Bring as a question WITH the evidence.
4. Dependabot ladder progress + the react-group PR (#1310) contents look.
5. IR spike gates unchanged: his one-pager read; ladder-lands + regression
   read. The zqo regression read re-verdicted AS-EDIT-ROUTINE as
   instrument-caused (see recon notes in bd tuxlink-che1k) — feeds gate
   discussion, does not clear it.

## Ledger at handoff time

**PRs:** #1367/#1368/#1369/#1370/#1371 MERGED (skill-routing docs, hook
payload-cwd fix, Session conduct, redirect hardening, test isolation).
OPEN awaiting green→merge: #1372 security sweep (izcq0; on merge: bd close
tuxlink-izcq0, close dependabot #1312+#1313 as superseded with comment,
dispose izcq0 worktree per ADR 0009), #1373 dispatch-skill model-id fix
(wnfkm; on merge: bd close + no worktree... wnfkm worktree EXISTS — dispose
after merge), #1374 IR alternatives evidence package (qqmys; on merge: bd
close + dispose worktree). Watchers were attached to all three pre-
compaction; if their notifications were lost to the restart-boundary,
re-check with `gh pr checks` directly. Merge rule: green on verified
headSha, `gh pr merge N --merge --delete-branch`, never with checks pending.
**Dependabot:** rebase requested on #1269 1271 1272 1273 1274 1275 1276
1277 1278 1309 1310. Merge each on green, risk order (cargo patches + CI
action first, npm minors, dev-majors last); #1310 react group: inspect
contents BEFORE merging. NEVER touch #1224 (operator's misfiled draft),
#1319/#1320 (awaiting his posture ruling), #1323 (release-please).
**bd open (mine):** tuxlink-che1k (this instrument, in_progress),
tuxlink-qqmys, tuxlink-izcq0, tuxlink-wnfkm (each closes on its PR merge).
**Worktrees (mine, live):** bd-tuxlink-che1k-ir-insitu-instrument (the
overnight workspace), bd-tuxlink-izcq0-dependabot-security-sweep,
bd-tuxlink-qqmys-ir-alternatives-package. Dispose each per ADR 0009 ritual
after its PR merges. The dozens of OTHER stale worktrees are a surfaced-
not-authorized cleanup — leave them.
**Main checkout:** on main, tracked-clean; operator's 75 untracked WIP
files + his ant8s stash at stash@{0} — untouchable.
**Standing rules re-affirmed tonight:** merge only on green (docs carve-out
literal); load-bearing changes = full CI + Codex round BEFORE merge;
filing = claiming = executing; one git write per call, cd-first, cwd resets
every turn; subagents get explicit model:"sonnet" (Fable only with stated
justification); plain names in prose, IDs in parens; report times in AZT;
no sampling params to Inkling; bench quarantined for outcome claims.
**Session-end (when the operator wakes / ends it):** bd dolt push noted as
no-op (no remote), final git push all branches done continuously, this
handoff updates with the overnight outcome before the session truly closes.

## Key mechanical facts the overnight work needs (from tonight's recon)

- Testserver: env `TUXLINK_MCP_SOCK=<path>` required; full 95-tool router;
  real guard disarmed by default; routines port is a canned mock (residue
  R2 in the spec); no Tauri needed; SIGINT unlinks socket.
- d3zwe: `--socket <path> --json --allow-remote --endpoint <inkling>
  --model thinkingmachines/Inkling-Small-NVFP4 --prompt <cell>`;
  `D3ZWE_TURN_TIMEOUT_SECS=300`; stderr carries the `→ tool name args`
  trace (capture it); stdout carries the outcome JSON; abort tools fire on
  cancel automatically.
- Build: `cargo build --manifest-path src-tauri/Cargo.toml -p
  tuxlink-mcp-testserver -p d3zwe --release` (Pi-OK, small crates only).
- ELMER_SYSTEM_PROMPT rides automatically (d3zwe passes no override).
