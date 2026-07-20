# Handoff — 2026-07-19 (sycamore-basil-savanna): transcript instrument shipped, then the authoring surface it exposed, fixed end to end

One session, eight PRs, driven by live model transcripts the whole way. The
gzbpo instrument landed early and every subsequent fix was diagnosed from a
real captured run and validated by the next one.

## Shipped (all merged to main unless noted)

1. **#1172 `tuxlink-gzbpo` (CLOSED)** — Elmer transcript instrument: redacted
   JSONL sink (writer thread, 32 MB queue budget, 256 MB retention sweep over
   inactive sessions), `elmer_transcript_export` + Logging-window button.
   Live-proven same evening. Files: `<app_data_dir>/elmer-transcripts/` on the
   app host (R2 for current testing).
2. **#1179 `tuxlink-mbh5z` (CLOSED, P0)** — `rebuild_index` deleted `docs_fts`
   and never repopulated: every docs query returned ok-empty until app
   restart. Root cause of runs 1–2's "docs are empty" behavior.
3. **#1180 `tuxlink-dngvs` (CLOSED)** — `routines_actions_list` authoring
   catalog (paste-ready JSON-object `example_params`, trigger kinds with
   examples), `UNKNOWN_ACTION` enumerates the valid action set, 11 missing
   `example_params` authored, spec §13 amended to the 11-tool family.
4. **#1182 + #1183 `tuxlink-591dw` (CLOSED)** — system-prompt routine-authoring
   carve-out from docs-first (operator-diagnosed: the prompt was sending
   models docs-circling for schemas the docs don't carry), agent-boundary
   remedy suffixes on the misread finding codes (incl. the real ack
   mechanism), `radio.connect` contract description, user-guide trigger-JSON
   section.
5. **#1184 `tuxlink-2h16p` (fix 2 of 2)** — CI vitest `--retry=2`; the
   deterministic stabilization (fix 1) stays open on the issue.
6. **#1185 `tuxlink-rt4ey`** — `definition_template` in the catalog: run 5
   showed the model mirroring the catalog's own response shape as the routine
   schema (14-save envelope loop); the template teaches the envelope where the
   model looks first. At handoff-commit time: CI rerunning past the known
   arm64 `packet_answer_p2p_intent` flake (diff-independent). **Merge on
   green; close rt4ey; dispose its worktree per ADR 0009** if this session
   did not finish that.
7. **#1186 `tuxlink-pal78` (this branch)** — ADR 0023 clause 5: GPT-5.6
   shadow-adrev assessment protocol + tracked ledger
   (`dev/gpt56-assessment-ledger.md`, pair 1 recorded: 5.6-sol via OpenRouter,
   clean, unmatched-commit caveat) + working invocation recipe in CLAUDE.md.
   Same merge-on-green instruction.

## The evidence arc (six transcripts, the eval-corpus seed)

`r2-poe:~/.local/share/com.tuxlink.app/elmer-transcripts/` (+ local copies in
the session scratchpad, non-durable). Run 1: 122b wedged on a 56 KB
`find_stations` dump (that tool-shape gap is `tuxlink-to358` #8). Run 2:
Coder Next silently downgraded schedule→manual against empty docs. Run 3:
docs fixed, action enumeration fired in anger, worked. Run 4: 122b's first
clean authored routine ever; over-read `ATTENDED_UNDER_SCHEDULE` (a Warning)
into a downgrade. Run 5: prompt routing worked, catalog mis-taught the
envelope (rt4ey), recovered to the best artifact yet: `hourly-vara-check`,
automatic + `every 1h / align hour`, blocked ONLY on the operator's designer
acknowledgment — which is correct-by-design, and the C3 digest binding held
against the model's attempts to reason around it.

## Overwatch (`tuxlink-nsfo8`) readiness — the question this pivot served

The two pivot gates are effectively cleared: the instrument exists and is
live-proven; the shipped model authors valid routines when the surface
carries the information (run 4 clean save; run 5 reached the exact requested
automatic-hourly definition). **The MVP tier (operator-chosen stations) is
implementable now.** Remaining pre-Session-1 items: (a) the one-run no-nudge
exam on a post-#1185 converged build (cheap; do it first); (b) iizmk's live
half (operator flows + attended write-park click + ADR 0024 accept/reject);
(c) `tuxlink-to358` gates only the agent-discovers-stations tier. Overwatch
Session 1 scope per the epic: capture-transcribe action over the
wwv_offair/data.rs spine, `tux_rig::Mode::Am`, arbiter long-hold,
ObservationStore.

## State at handoff

- **Branches/PRs:** #1185 + #1186 finishing CI (merge on green — standing
  grant, bare `gh pr merge`). All other session branches merged and deleted.
- **Worktrees:** `bd-rt4ey-catalog-definition-template` and
  `bd-pal78-adr0023-shadow-adrev` alive pending those merges; dispose both
  per ADR 0009 (only gitignored content is dev/adversarial transcripts,
  already copied to the main repo's `dev/adversarial/`). All six other
  session worktrees already disposed clean.
- **Main checkout:** untouched (operator state, `bd-tuxlink-ant8s` branch,
  dirty — left alone all session).
- **R2 clone** (`~/Code/tuxlink`): detached at `5a194d3b` with rt4ey's files
  rsynced on top — scratch state; next session should `git checkout -- <the
  named files>` or fetch+checkout fresh before using it.
- **Codex config:** `~/.codex/config.toml` gained the OpenRouter provider
  block (backup at `config.toml.bak-20260719`); key stays in the OS keyring.
- **bd:** gzbpo/mbh5z/dngvs/591dw/rt4ey(pending merge)/closed; pal78 open
  (assessment program, ~10 pairs needed); 2h16p open (fix 1); 93lzx open
  (transcript outcome line); to358 open (surface audit); 6zkb6 (eval harness
  — tonight's transcripts are its seed corpus, discussed with operator);
  0mudm open (trained routing, the durable fix).

## Pending operator decisions

- Designer acknowledgment click on `hourly-vara-check` (makes it live hourly).
- iizmk F2 live half + ADR 0024 accept/reject.
- Hardware: endpoint covered (Framework 13/96GB bought); one more Spark +
  ~$60 QSFP DAC (no switch at 2 nodes) recommended under the DRAM-crisis
  window; Spark #3 + MikroTik deferred until the new job's inbox shape shows.
