# Compaction anchor — spruce-birch-dune, 2026-08-17

SAME SESSION CONTINUES after this anchor: keep the moniker
**spruce-birch-dune**, do not re-pick. This is a mid-session compaction
anchor (operator-directed), not a session end.

## Headline state

**AMENDED before this anchor's own merge: #1362 MERGED on green (head
b4e20cb2 verified), tuxlink-wovan CLOSED, the wovan worktree disposed per
ritual (row-2 transcript preserved to the main checkout's
dev/adversarial/), its branch deleted. The campaign's MECHANICAL PHASE IS
COMPLETE — all ten fix-now rows closed and merged; the ledger's Fix-now
section is empty on main. Live-thread steps 1-2 below are DONE; only step
3 (this anchor's merge + worktree re-park) remains. `bd dolt push` is a
documented no-op (no Dolt remote configured; issues are local +
repo-versioned).**

Original snapshot: rows 1,3,4,5,6,7,8,9,10 of
`docs/campaigns/2026-08-surface-repair-ledger.md` closed AND merged;
row 2's PR #1362 in its final CI run with the in-file ledger closure
aboard.

## LIVE THREADS — first actions after compaction

1. **PR #1362**: a Monitor (task bth1sb6e2) watches its final run. On
   green: verify headSha == b4e20cb2 via `gh pr view 1362 --json
   headRefOid`, then `gh pr merge 1362 --merge --delete-branch`, then
   `bd close tuxlink-wovan`. On FAIL: the branch lives in worktree
   `worktrees/bd-tuxlink-wovan-modem-last-session` — fix forward there.
2. After the merge: dispose `worktrees/bd-tuxlink-wovan-modem-last-session`
   per the ADR 0009 ritual — FIRST copy its gitignored
   `dev/adversarial/2026-08-16-row2-last-session-codex.md` to the MAIN
   checkout's `dev/adversarial/` (the other six session transcripts are
   already consolidated there), inventory, then cd to the main repo,
   rm -rf, `git worktree prune`, and `git branch -d
   bd-tuxlink-wovan/modem-last-session`.
3. This anchor's own PR: merge on green (docs gate), then park THIS
   worktree (`worktrees/bd-tuxlink-efk3k-classifier-arch`) back detached
   on origin/main and `git branch -d` the anchor branch.
4. Then the session continues per the operator's direction — the Weekend
   Epic's next stages are 4 (classifier wiring + in-repo measurement;
   GATED on the threshold-recal debt below) and 5 (content triage);
   stage 6 (IR spike) is operator-gated. Do NOT start these unprompted if
   the operator has redirected; ask nothing, but check the conversation
   for direction after compaction.

## Shipped this session (all merged on green, all Codex-reviewed)

| PR | Rows | What |
|---|---|---|
| #1356 | — | Prior session's handoff (merged at session start; prior worktree parked). |
| #1357 | 6+7 | Wire-shape batch: packet_config_get unset → explicit nulls (sentinels were manufactured at the mcp-core mapping); mufday → mufday_fraction_by_hour / mufdayFractionByHour on both wires. Codex: impossible dry-run packet state; dev/ render-harness rename gap. |
| #1358 | 3 | Docs 17 AND 14 reconciled to 15/16 (VARA more robust at low SNR). Codex: fifth occurrence in doc 14; unsourced rapid-QSB comparative softened to non-comparative. |
| #1361 | 8 | find_stations recommend→goal disclosure + R2 corpus regen (one row changed). Codex: doc-block placement. |
| #1359 | 5 | EgressPortError::Precondition + marker classifier; Transport marker-gates (Codex); carrier deliberately internal_error (anti-guess-loop precedent) — class lives in the message. |
| #1360 | 10 | VARA open outcomes overwrite the reachability TTL cache. Codex (2 passes): cmd-stage-only false writes via connect_staged; probe generation guard; success half proven through the real open seam (readiness gate fails open). |
| #1362 | 2 | IN CI. last_session outbound-dial memory (ARDOP/VARA/packet chokepoints + panic path) + selected.note on-wire caveat (corpus-neutral) + VARA open/status coherence test (open ≠ dial recorded). Codex: Elmer harness manages the store; packet coverage; JoinError folds into the record. Inbound-listener half NARROWED with grounds (inbound surfaces via contact observations; recording would let an inbound call overwrite the agent's own dial memory). |

bd: umbrella **tuxlink-4280b CLOSED**. **tuxlink-wovan** closes on #1362's
merge.

## Debts and lessons that must survive compaction

1. **Threshold-recal debt (ADR 0030)** — row 8's corpus regen means the
   `tuxlink-tools` classify-thresholds entry (0.582/0.008) was calibrated
   against the pre-regen corpus. Classifier not live yet → no bad verdicts
   today, but recalibration GATES Weekend Epic stage 4. Recorded on
   (closed) tuxlink-4280b notes + the ledger row-8 Closed entry + memory.
2. **cwd RESETS bit twice this session**: two verification passes silently
   ran in the operator's stale July main checkout — one wrongly "refuted"
   a correct Codex finding (NoActiveIdentity), one misread the ARDOP
   exchange shape. Absolute paths for Edits were safe; RELATIVE-path shell
   reads are the hazard — prefix each with an explicit cd.
3. **Codex line numbers can be wrong while the finding is right** — verify
   the claim by searching for the construct, never by checking the cited
   line.
4. **The 8 old stashes in the shared repo are operator-era history**
   (May–June) — repo-global, not session-scoped, deliberately untouched.
5. **Weekend Epic = the frame for status answers** (operator correction
   this session; memory `project_weekend_epic_shape.md`). Stages 1-3 done
   once #1362 lands; 4-5 next; 6 operator-gated.

## Environment right now

- Worktrees: `bd-tuxlink-wovan-modem-last-session` (row-2 branch, PR
  #1362 in CI); `bd-tuxlink-efk3k-classifier-arch` (THIS anchor's branch;
  re-park detached after the anchor PR merges). The two 4280b worktrees
  were disposed per ritual (transcripts preserved first).
- Adrev transcripts: main checkout `dev/adversarial/2026-08-16-*` (six
  files; row-2's still in the wovan worktree until its disposal).
- Merged-dead local branches deleted (rows 3/5/8/10 + the
  moss-tamarack-taiga handoff branches, all safe `-d`).
- Operator's main checkout untouched (his bd-tuxlink-ant8s WIP intact).
- R2 clean (corpus-regen temp worktree disposed right after the scp).
- bd `dolt push` NOT yet run this session — run it whenever the next
  natural git-push moment happens.

## Waiting on the operator (do not chase)

IR one-pager read (gates the spike; baseline re-selected to
AS-CHECKIN-CLEAN / COLLAB-NET-CLEAR) · bench relay of F1-F10 (ledger row
21) · four design calls (ledger rows 16-19) · sideload ratification ·
readback wording (bd-tuxlink-k2h9l/readback-eval parked) · stale
`[model_providers.openrouter]` block in ~/.codex/config.toml.

## Standing rules (each cost a correction once; carry verbatim)

- Plan-billed or local model transports ONLY (hook enforces).
- Verify against CODE, not docs — and in the RIGHT TREE.
- Operator's main checkout untouchable; cwd resets constantly; standalone
  cd + pwd+branch check before any git write; one git write per call.
- Codex adrev on every code PR; ground findings against source; narrow
  overreach WITH grounds.
- bd dup-search before creating; never infer bd IDs.
- The LEDGER is the campaign's canonical state, not this anchor.
