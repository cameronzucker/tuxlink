# In-situ IR ladder instrument — pre-registered spec (tuxlink-che1k)

**Committed BEFORE the first run.** Operator directive 2026-08-18 ~03:15:
pressure-test the IR candidates in situ overnight; the anti-drift guardrail
is testing against this spec, never iteration beyond it. Any deviation is
recorded in an errata section of the results, loudly. Results are
INSTRUMENT-SHAKEDOWN DATA for the operator's morning review — explicitly
NOT the gated spike's A/B (its baseline comparison and GO/NO-GO stay behind
the operator's uncleaned gates).

## Environment (the shipped shape, as close as the product permits untouched)

- **Server:** `tuxlink-mcp-testserver` (release build, this Pi) on a private
  socket. Serves the FULL production router — 95 tools, the real
  `EgressGuard` (disarmed, untainted; `TUXLINK_TEST_ARM`/`TAINT` unset), no
  scenario (mock read ports; the routines port is the canned mock — noted
  as residue R2 below).
- **Driver:** `d3zwe` (release build, this Pi) with the UNMODIFIED
  production `ELMER_SYSTEM_PROMPT` (d3zwe passes no override by
  construction), full tool snapshot from the socket, `--json`,
  `D3ZWE_TURN_TIMEOUT_SECS=300`, `--allow-remote --endpoint
  https://inference.twin-bramble.ts.net/v1/chat/completions --model
  thinkingmachines/Inkling-Small-NVFP4`. No sampling parameters anywhere
  (serving default is the only sampling; operator ruling).
- **Capture:** per run, verbatim: d3zwe stderr (the full tool-call echo
  trace) and stdout JSON outcome, archived as files. The emitted IR
  artifact is the run's final text; any tool calls made are the trace.
- **Nothing in the product changes.** No new tools, no router edits, no
  bench usage.

## Declared assumptions (operator may overrule any in the morning)

1. **Instruction placement = user message.** The candidate sheet + ask
   travel as the d3zwe prompt. Rationale: no IR intake tool exists on the
   product surface and the product stays untouched tonight; tool-description
   placement is therefore unbuildable in situ. This is the conservative
   approximation; placement remains open decision #3 in ALTERNATIVES.md.
2. **Emission channel = final text.** With no IR tool, the artifact is the
   model's final message. The 95 real tools remain visible and callable —
   which converts the shipped surface's gravitational pull into a measured
   variable (see defection, below) instead of a controlled-away parameter.
3. **Egress disarmed.** Any connect attempt is denied server-side by the
   real guard and shows in the trace as a denial — recorded, not failure.

## Surfaces (frozen as of this commit; no redesign overnight)

- **A** — the ratified five-construct one-pager, verbatim, including its
  status header (literalism documented; open decision #4).
- **D** — `SHEET-D-template-slots.md` (committed beside this spec).
- **B** — `SHEET-B-text-dsl.md` (committed beside this spec).

## Cells (5 per surface × 3 surfaces × 2 samples = 30 runs, plus 3 controls)

- **N1 fresh-author, plain:** the wa-gateways ask (20m cadence, 06:00-09:00
  window, 40m→80m, success and failure logging, honest failed end).
- **N2 fresh-author, inexpressible pressure:** the nv-gateways ask with
  "retry twice" and "send a beacon transmission" embedded (the F2 ask,
  verbatim from probe v3).
- **E1 edit-restatement:** the AS-EDIT-ROUTINE three-part edit, retargeted:
  the prompt carries the CURRENT routine in the candidate surface plus the
  verbatim ladder edit request ("every 15 minutes instead of 30, add 80
  meters as a fallback band after 40, record which band succeeded, not just
  which station"); expected output is the whole re-stated artifact.
- **E2 edit-restatement, subtractive:** carries a current routine with a
  window and a failure log; ask removes the window and changes the failure
  reason — tests whole-restatement without additive bias.
- **C1 correction-from-refusal:** the flawed artifact + named positioned
  refusal (probe v3's C1 flaw per surface), corrected whole re-emission.
- **CTRL (3 runs, no sheet):** the N1 ask alone under the production prompt
  — what the model does natively (expected: routines_* tool usage; canned
  mock responses are residue R2). Baseline for the defection measure only;
  NOT graded as an IR cell.

## Pre-registered grading (mechanical first, eyeball second, both reported)

Per run: (a) **artifact assertions** — surface-specific shape-semantic
checks fixed at commit time: A: parse + required keys + gating nesting +
recursive key-allowlist + (N2) composed-retry detection by connect-count
>1; D: parse + template id + slot-set ⊆ registry + values (bands order,
schedule, window presence/absence per ask); B: line-grammar conformance +
indentation gating + absence of invented line forms; all E cells
additionally assert the UNCHANGED fields survived restatement verbatim-
equivalent. (b) **trace assertions** — count of mutating routine-tool calls
(`routines_save`, `routines_step_*`, etc.): expected 0 under an IR sheet;
any occurrence is recorded as DEFECTION-TO-NATIVE-SURFACE with the call
args preserved (a first-class finding, not a failure to hide). (c)
**outcome kind** from d3zwe JSON. Raw emissions verbatim in the results
document, per the standing discipline: the grader has been wrong twice
tonight; the eyeball row is part of the instrument.

## Known residues (stated now, not discovered later)

- R1: `MockStatus` read ports differ from a live station's data; asks avoid
  depending on status reads.
- R2: the routines port is a canned mock — native-surface calls (controls,
  defections) get mock responses; their ARGS are still real emissions and
  are graded as such.
- R3: d3zwe has no structured transcript; stderr echo is the trace. If echo
  truncation is observed on long args, it goes to errata and the run count
  stands.
- R4: single ask-set; no distractor templates for D yet (Codex's
  recommendation belongs to the spike arms, operator decision #2).

## Abort conditions (stop and write, never improvise)

Serving pre-flight fails; testserver or d3zwe fail to build on the Pi in
bounded time; the socket handshake fails; more than 20% of runs end
`provider_error`. On abort: results-so-far + errata + handoff, untouched
product, morning review proceeds on partial data.
