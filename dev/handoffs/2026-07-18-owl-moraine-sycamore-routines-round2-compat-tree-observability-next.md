# Handoff — Routines round 2: design approved, proofs executed; NEXT = compat tree + observability + implementation

- **Agent:** owl-moraine-sycamore (session spanned 2026-07-17 → 18; third handoff of the arc)
- **Open work item:** bd **tuxlink-iizmk** (IN_PROGRESS — carries the full next-session plan in its notes; read it with `bd show tuxlink-iizmk`)
- **Operator-directed break:** context full; fresh session picks up.

## READ FIRST — the three commitments for next session

1. **Compat tree (operator-approved method).** Every distillation scenario
   (`dev/elmer-distill/src/elmer_distill/scenariogen.py` — families
   radio_debug / emcomm / helpdesk / blended, with SuccessSpecs) must be
   actionable BOTH by the agent (50 tools, `dev/elmer-distill/reference/tools.json`)
   and by a human through Routines (16 actions today). Decompose each scenario
   cell into steps → map each step to (routines action | agent tool | MISSING)
   → coverage matrix + ranked missing-action list = the functional
   requirements, replacing operator guesswork (his work context is
   deliberately firewalled — see the design-tension exchange; requirements
   come from the vetted scenario corpus, taste comes from his critique loop
   on rendered mocks). Dual actionability is ADR-shaped — draft it.
2. **OBSERVABILITY DECREE** (operator, verbatim repo-deletion strength — see
   memory `routines-observability-decree`): full visibility into what
   did/didn't run, why/why not, each activity's outputs, end state — History
   UI + backend logs. History has NEVER been validated against a real run.
   Wire-walk it against an executed routine; per-step post-`$`-resolution
   params, outputs, verbatim failure causes, branch decisions, skipped-with-
   reason, terminal state.
3. **Design implementation** against the approved mock v2 ("big improvement"):
   human-scale ramp (13px floor, mono = data only), settings as a grid under
   the canvas, header fact-chips (TX · schedule · enabled) as jump targets
   answering the displacement question, 340px palette, home rows with
   Run / History / menu. Artifacts (main repo, gitignored):
   `dev/scratch/routines-ui-mocks/humanscale-routines.html`,
   `dev/scratch/iizmk-humanscale-mock.png` + `-v2.png`.

## Composability ground truth (executed, not asserted)

Branch `bd-tuxlink-iizmk/composability-proof` @ 7d5a2c76 (pushed, no PR yet):
5 engine tests, all green on R2. R1 "morning-mail-run" (validate→branch-apply→
listen→branch-bail→connect→compose with `$s6.gateway` vars) and R3
"gateway-continuity" (`data.read` output feeds `radio.connect`'s stations
array) EXECUTE correctly through the real engine. R2 "propagation-gated band
plan" is NEGATIVE-proofed: nested `$s1.indices.k_index` unresolvable (VarPath
= one flat key) and branch-on-number is a hard error (strictly boolean).
Closing those two links flips the tests positive — that's the engine half of
round 2. Verified action matrix + engine semantics are in the session log and
bd notes; `local.compose`'s template+`vars` is the sanctioned outputs-to-text
path.

## Also landed this arc

- PR #1154 merged: save accepts human names (slugifies live), dashboard ⋯ menu
  renders in-window, Import/Export routine labels, real delete target.
- PR #1152 merged: packet observation-sink flake serialized (tuxlink-8vt7b
  closed) — which unblocked the release train: **0.94.0 merged** (PR #1146).
- Earlier same session: tour fix (#1145), Routines round-1 (#1148), Contacts
  restoration (#1151, four issues closed).

## Environment / worktree state

- Pi: no in-flight worktrees (all disposed per ADR 0009; artifacts preserved
  to main-repo `dev/scratch/`).
- **R2** (`ssh r2-poe`): worktree `~/Code/tuxlink/worktrees/bd-tuxlink-iizmk-routines-round2`
  detached @ 7d5a2c76 with a warm cargo cache — REUSE it for engine work.
  **All Rust compiles on R2, never the Pi** (operator re-decreed; even leaf
  crates — the old tuxlink-ft8 carve-out is dead, memory updated). Engine
  crate compiles+tests there in ~10 s.
- Orphan Vite (pid 213856, another session's) still holds Pi port 1420; use
  `pnpm exec vite --port 1430`.
- Background Bash tasks may not inherit the shell cwd — `cd` inside the
  command and check the `RUN v… <path>` header (bit this session twice).
