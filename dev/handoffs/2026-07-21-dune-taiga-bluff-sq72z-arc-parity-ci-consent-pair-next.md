# Session handoff — dune-taiga-bluff (2026-07-20 → 2026-07-21)

Continuation of marten-vetch-esker's night handoff. Ends at operator
direction with the consent pair as the next build. Marathon session; the
state below is complete and pushed.

## Merged this session (all CI-green both arches, SHA-verified)

- **#1219 (tuxlink-sq72z, CLOSED):** ONE parse-if-string rule at
  `map_edit_op` for every verb composite param + `arg_shape` transcript
  marker. Wire origin verified as MODEL EMISSION before building. Ledger
  pair 12 (situation-level dual review).
- **#1225 (fg0em phase 1; ISSUE STILL OPEN — see remainder):** the
  radio.connect step editor rebuilt on real selection surfaces (stations
  chips + finder/favorites picker, band chips, Runs-on transport line with
  PACKET precedence), `bands` allowed-vocabulary (BandList-scoped
  case-insensitive validation), 11 adrev findings fixed (pair 13 — incl.
  the workdir methodology incident, see Gotchas).
- **#1226 (tuxlink-hq3e2, CLOSED):** honest object/array schemas on the
  five composite params; COR-3 validator learned the validation-time half
  of the one rule (ROOT-excluded per adrev); kind-exact coercion +
  schema↔table drift test; kind-precise markers
  (`string-to-object`/`string-to-array`); ledger two-families correction.
- **#1228 (tuxlink-ybf9f, CLOSED): ADR 0027 parity CI is LIVE.** 335
  commands classified in `docs/parity/parity-manifest.json`; enforcement =
  `src-tauri/src/parity_check.rs` (6 invariants incl. terminal-shadow +
  tool_budget=92) + `src/parityManifest.test.ts` (invoke + registry-literal
  scan, reviewed allowlist). EVERY new command must land classified; every
  new tool debits the budget; operator-authority mappings are CI failures.

## The load-bearing results

- **Exam definitively passed post-#1219**: two fresh runs (Qwen 3.5 122b +
  GLM-5.2, provenance-verified converged build) one-shot to `done`, zero
  nudges, verbs used throughout (NOT whole-doc bailout — the preference
  experiment answered), ~86% of composite calls still string-coerced and
  absorbed silently. Only rejection: a placement miss, self-healed (the
  predicted pseudo-union wall; parked). Evidence base = TWO families (the
  122b IS Qwen 3.5 — see memory; ledger pair 12 carries the marked
  correction).
- **Spark 256k live**: vLLM `--max-model-len 262144` (native max;
  hybrid-linear = ~24KB KV/token, ~6GB per 256k seq). Rollback container
  `vllm-q122-128k-bak` + `~/vllm-q122-inspect-backup.json` on the Spark
  (`ssh inference.twin-bramble.ts.net`). Elmer meter confirmed 256k after
  first turn (meter shows last-turn value by design).
- **Conditional branching**: engine already ships full
  `Branch{on,op,value,then,else}`; gap is teaching (agent) + a skeleton
  inspector missing op/value editing (human) → tuxlink-6epl8 scope
  EXTENDED to both surfaces, promoted, queued after the consent pair.
- **Elmer test prompts compiled**: all 17 logged runs' prompts verbatim at
  `dev/scratch/elmer-test-prompts.md` (main checkout, gitignored; operator
  may want it tracked as the 6zkb6 seed corpus).

## NEXT SESSION: the consent pair (operator-approved designs, mocks approved)

1. **tuxlink-32aew**: revoke-armed-consent button. Backend: UI-only revoke
   command(s) mirroring the two acknowledge writers (commands.rs:1159-1225
   region), clears the ack, keeps Automatic, NEVER on MCP (now also
   CI-enforced: classify `operator-authority` in the parity manifest or
   parity_check fails). Frontend: Revoke button on the VALID ack state in
   SettingsTab (~line 547-560 — currently button-less) for BOTH ack blocks.
   Approved mock: panel A of
   `worktrees/bd-tuxlink-fg0em-fg0em-designer-radio-entry/dev/scratch/consent-trio/mock-consent-surfaces.png`.
2. **tuxlink-9jkiu**: consent refusals visually distinct + remedy stated
   (panels B/C of the same mock) PLUS the operator-approved scope
   extension: **refused fires become run-history rows** ("why the thing not
   go" must be answerable from history — Laserfiche lesson). Root cause
   mapped: consent and validation refusals both flatten to opaque
   `UiError::Rejected` → same `.refusal-strip`
   (RoutinesDashboard.tsx:342-354) and `.refusal-note` (:412-414); consent
   origin is `RoutineStartError::Unacknowledged*` (session.rs:861-885) →
   mapping at commands.rs:389-409. Fix = tagged consent variant across the
   wire + distinct strip + refused-run journal records. Full investigation
   in this handoff's session transcript + issue notes.
   NOTE: the new commands/changes must satisfy the parity gate (classify in
   the manifest; the consent-refusal finding path is the `finding:` form).

## Open / carried

- **tuxlink-fg0em (OPEN)**: wire-walk verdict = phase 1 did NOT satisfy the
  operator's primary flow (radio-SETUP parity from the designer: audio
  in/out + everything the modem pane configures; stations/favorites are
  explicitly calling-interface, not setup). Remainder = extract per-mode
  setup forms from Ardop/VaraRadioPanel into shared components + actionable
  Runs-on line, PLUS the parity addendum (transport state as a
  validate_def FINDING — one capability tree). **GATED on operator answer:
  does "sound device in/out" reach into VARA's own audio (VARA.ini
  provisioning) or stop at native app config?** Mock gate applies.
- **tuxlink-6epl8 (promoted, extended)**: after consent pair.
- **tuxlink-to358**: 192 enumerated pending entries in the parity manifest
  are now its worklist; favorites-exposure note added.
- **#1224 = MISFILED PR, deliberately OPEN as labeled draft** (head is the
  operator's ant8s branch; closing would dead-classify it under ADR 0017).
  Operator disposes at their leisure.
- Parked (operator sequencing): P0s 4u43s/kw873, bzxwp re-scope, vara-fm
  catalog fix, placement discriminator, bd mound audit (50 open + ~50
  zombie in_progress), battery decision (deferred per dual adrev + exam
  evidence), tuxlink-prdto (designer live-update P2), jt9 ETXTBSY flake
  (filed), 131k local-tile input cap (unfiled, operator's call).
- Operator inputs pending: VARA-audio boundary (above); optional promotion
  of the prompts compilation to a tracked file.

## Worktree + environment state (ADR 0009 enumeration)

- **ALIVE: `worktrees/bd-tuxlink-fg0em-fg0em-designer-radio-entry`** —
  claimed by OPEN tuxlink-fg0em. Branch bd-tuxlink-fg0em/... is
  MERGED-DEAD (#1225): the remainder needs a FOLLOW-UP branch (ADR 0017)
  created in this worktree. Gitignored content that must survive any
  disposal: `dev/scratch/fg0em/*.png` (approved mocks + baseline),
  `dev/scratch/consent-trio/mock-consent-surfaces.{html,png}` (the consent
  pair's APPROVED mock), node_modules.
- Disposed this session (inventoried clean): sq72z, hq3e2, ybf9f worktrees
  (Pi) + all R2 test worktrees (`~/*-test`); this handoff worktree disposes
  after its own push.
- Main checkout: operator's branch bd-tuxlink-ant8s @ 81fd0a2a, untouched.
- Adrev transcripts (main checkout `dev/adversarial/`, local-only):
  routines-situation pair, fg0em pair, hq3e2, ybf9f (2026-07-20/21).
- `bd dolt push` printed usage/remote hints — dolt remote sync may not be
  configured; bd JSONL state is committed via the repo as usual.

## Gotchas worth carrying (all memorialized)

- **cwd resets bit FOUR tools this session**: cargo/git (known), plus
  `gh pr create` (misfiled #1224 — always pass `--head` + verify
  `.headRefName`), plus Codex adrev workdir (pair-13 baseline-skew
  artifacts — verify branch in the SAME invocation). Memory updated
  (worktree-git-mechanics).
- ADR 0026 ended the GPT-5.6 shadow program mid-session: single GPT-5.5
  rounds now; ledger frozen (16 pairs) + one marked correction.
- The night session's handoff lived only on origin/main (not the stale
  main checkout) — read handoffs via `git show origin/main:...` when the
  checkout is old.

Agent: dune-taiga-bluff
