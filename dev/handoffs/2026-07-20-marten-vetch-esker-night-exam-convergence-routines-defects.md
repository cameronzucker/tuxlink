# Night-session handoff — marten-vetch-esker (2026-07-20, second handoff)

Continuation of this session's evening handoff
(2026-07-20-marten-vetch-esker-evening-cms-runbook-elmer-popout-shipped.md).
Ends at operator direction with the exam-convergence design recorded and a
fresh session taking the build. Context was full; state is on disk.

## Merged since the evening handoff (all CI-green on head SHA)

- **#1213** (tuxlink-w68mb, CLOSED): dock header is ONE row in every state;
  the Tac Map ↗ Pop out moved onto the map surface (top-left cluster with
  Weather SITREP); popped pathway = compact "Map ↗" + accessible ⇤ glyph
  (spec §5 AMD-3); tab group scrolls at the 300px floor. Ledger pair 9.
  Operator eyeball routes: ?view=header-tacmap-popped&w=300, ?view=map-popout&w=240.
- **#1215** (tuxlink-y6whc, CLOSED): popped-window title bars drag again —
  pointer-events:none on .pop-title (it blanketed the drag region; all four
  pop surfaces affected). Ledger pair 10 (clean-bill both rounds).
- **#1216** (tuxlink-ixasg, CLOSED): favorites are per-CHANNEL —
  favoriteKey = mode|GATEWAY|freq canonicalized through the extracted
  freqStringToCanonicalMhz (kHz records and MHz dials collide correctly;
  dual-adrev consensus P1); freq-less legacy records are documented no-row
  orphans. SI channel-chip spacing 3→7px / 6→12px. Ledger pair 11.

## The 122b exam: definitive read (this is the load-bearing state)

- 12:12 transcript (1784574426051-0) = PRE-fix binary (the deleted
  "do not stringify" rejection appears 7x) — NOT signal, per provenance.
- 19:01 transcript (1784598978430-0, R2
  ~/.local/share/com.tuxlink.app/elmer-transcripts/) = provenance VERIFIED
  live (/proc cwd → converge worktree @ 9f6f2261). VERDICT: **pass on
  capability, fail on process.** Stringified def saved FIRST TRY (#1205
  works); reasoning correct end to end; model self-recovered from the verb
  wall by whole-document re-save; final artifact correct, blocked only on
  AUTO_TX_UNACKED (correct-by-design). But ~20 rejection loops + 3 nudges at
  ONE wall: step_update.patch / meta_set.patch reject the stringified-object
  shape routines_save now accepts.

## The convergence design (operator decision — build this, in order)

1. **tuxlink-sq72z (P1, worktree exists CLEAN:
   worktrees/bd-tuxlink-sq72z-verb-string-acceptance):** ONE parse-if-string
   rule at the MCP argument-decode boundary (object-typed param arriving as
   a string of valid JSON → one parse, then normal validation) + per-call
   transcript telemetry (arg-shape: string-coerced). NOT per-tool patches.
   First check wire origin (model emission vs OpenAI-compat double-decode).
   #1205's per-tool acceptance becomes the redundant special case.
2. **tuxlink-6zkb6 extension:** routine-design batteries, Tuxlink AS the
   harness through OpenRouter across model families; subjective diagnostic
   judge emits {tuxlink-design-defect | model-family-trend | ambiguous} +
   why per transcript. Seed corpus = the 7 R2 elmer-transcripts. Decides
   design-change vs model-training. Note: 6zkb6 already has Stage-1
   discriminating-eval work on branch bd-tuxlink-6zkb6/discriminating-eval —
   read its notes + the 2026-07-02 handoff before extending.
3. **Distill + LoRA track** (0mudm trained routing, 7raoe harness+distill,
   c5ckf Spark A/B): teacher traces = frontier runs over the SAME batteries;
   batteries = the regression gate; string-coercion rate → 0 is the metric.
4. The no-nudge exam re-runs after sq72z lands (operator runs; verify
   provenance before reading, as always).

## NEW routines material defects (operator live use, all P1, filed tonight)

- **tuxlink-fg0em**: designer radio entry is bare free-text fields,
  disconnected from how radios are configured in tuxlink — investigate what
  it feeds, then rebuild on real selection surfaces (mode + dial pickers
  backed by favorites/finder).
- **tuxlink-32aew**: no clean revocation of automatic-routine transmit
  consent — today only via changing start mode (a side effect). Needs a
  first-class revoke-armed-consent button (ADR 0024 semantics).
- **tuxlink-9jkiu**: missing armed send consent surfaces as an unexplained
  generic error, indistinguishable from a routine design defect — needs its
  own clearly-worded, visually distinct surfacing with the remedy stated.

## Also open / carried

- Dependabot: **#1199** (react-virtuoso patch) fully green after the
  tuxlink-rd1rx flake re-run — mergeable on the operator's word. **#1196**
  (syn 2→3) is a broken dependabot lockfile + pointless dev-dep major;
  recommendation stands: `@dependabot ignore this major version` + close.
- tuxlink-k6rn5 CMS onboarding: run book on main; awaiting Rob (delete
  path) + operator hardware setup; tuxlink-fhr4g (TEST-prefix carve-out)
  blocks Test 1.
- SI pop-out: a PARALLEL agent is shipping it — hands off.
- tuxlink-eltpm (OutboxApprovalDialog orphan) unchanged.

## Worktree + environment state

- ALIVE + CLAIMED: worktrees/bd-tuxlink-sq72z-verb-string-acceptance
  (clean, unused — next session builds sq72z there).
- Pre-existing worktrees from other sessions untouched.
- Disposed this session: mfssz, k6rn5-staging, w68mb, y6whc, ixasg, and
  both handoff worktrees (all inventoried clean per ADR 0009).
- Main checkout: operator's branch bd-tuxlink-ant8s @ 81fd0a2a, untouched.
- R2: converge worktree @ 9f6f2261 (post-#1213), app was running from it
  for the exam; transcripts under ~/.local/share/com.tuxlink.app/elmer-transcripts/;
  /tmp/read_transcript.py on r2 pretty-prints them.
- Adrev transcripts (local, main-checkout dev/adversarial/): mfssz, w68mb,
  y6whc, ixasg pairs. Ledger pairs 8-11 on main.

## Session mechanics worth carrying

- Provenance discipline paid twice tonight: the 12:12 "poor results" run
  was a pre-fix binary; the fresh ls-returned-nothing scare on the shadow's
  harness citation was a reset-cwd artifact — verify against git, not shell
  state.
- The stringify failure class only became visible as ONE class by reading
  across runs; per-run fixes were correct but strategy-blind. The operator's
  zoom-out ("what's the actual issue?") is the pattern to internalize:
  after two same-shaped fixes, stop and look for the class.
