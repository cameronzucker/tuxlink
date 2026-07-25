# qwen-3.5-122b failure-mode analysis (mechanism, code-grounded)

Author: tanager-owl-cardinal. Source run: `qwen-instrumented-1` (qwen-3.5-122b-nvfp4,
local Spark vLLM, 18 cells x {base, skill} = 36 observations), binary origin/main
`0ae53b5e` plus this session's harness patches. Corpus `tests/battery/corpus.json`.

## What this is, and what it is NOT

This is a **mechanism** analysis: for each failure, *what the model actually did* and
*what the payload we fed it actually contained*, grounded in the transcripts AND the
source that produces those payloads (file:line). It is not a rate/reliability study
and draws no skill-vs-base directional conclusion (those need repeats; single
observations are only weakly informative). It records mechanism, evidence, and where
the mechanism lives. It does **not** prescribe fixes; candidate levers are marked OPEN.

Two epistemic corrections that shaped this pass:
- "The model ignores `applied:false`" is **wrong** and struck. A model cannot ignore
  input. The correct question is: *what does the payload we return produce?* Every
  finding below is stated as "what we feed back, and the behavior it produces."
- "Turn-capped / stopped during the run" is an **outcome, not a mechanism**. Each
  turn-cap below is resolved to the specific loop that consumed the turns.

## Instrumentation defect found first (H1 harness)

The battery bundles captured **zero** reasoning deltas for qwen (`deltas.jsonl` holds
only pre-tool-call assistant narration). qwen *does* emit reasoning when driven through
Elmer manually (operator-confirmed), so this is a **capture gap**, not a model property.
A direct SSE probe of the Spark `/v1/chat/completions` endpoint returned content-only
deltas (`{role, content}`) for the request shapes tried; the reasoning channel the
operator sees manually was not reproduced by the battery's request path. The provider
parser handles `reasoning` / `reasoning_content` / `thinking` (unit-tested,
`provider.rs:776-789`), so the miss is upstream of it (request shape or endpoint config).
Consequence: mechanism below is derived from trajectory + narration + saved def + final
text + the response payloads, not from the model's stated reasoning. That was sufficient
for these modes but the "why" is inferred from behavior + payload, not self-report.

---

## Mode A: branch-warning non-termination loop (the dominant hard failure)

**Cells (this run): skill/S1, skill/E1, base/E2** (all three turn-capped at 40).

### What actually happens (payload-grounded)

1. The model saves a **valid** routine on the first `routines_save`. Disposition
   `state:"valid"`. The save returns warnings; `ARM_FALLTHROUGH_LEAK` is present in all
   three (raw payload, skill/S1 save):
   > `ARM_FALLTHROUGH_LEAK` (severity **warning**): branch "s3": the "else" path falls
   > through into "s5"... insert an **end control** after the "else" arm's steps; if the
   > "then" arm is deliberately a shared tail, ignore this.
2. The model tries to clear that warning with `routines_step_update`, permuting **which
   step-ids are in the then/else arm lists** (`else:[s4,s7]` -> `else:[s4,s6,s7]` ->
   `then:[s5,s7]` ...). It **never inserts an `end` control** (verified across all three
   cells: 0 edits contain an end control). The actual remedy is structural (add an end
   node); rearranging arm membership cannot clear it.
3. After every edit the response **still lists `step_findings:[ARM_FALLTHROUGH_LEAK]`**
   (the warning never resolves because the fix was never applied), alongside
   `disposition:{state:"valid", remedies:[], agent_terminal:false}`.
4. The model keeps editing until turn 40:
   - skill/S1, skill/E1: it lands on a patch it already applied; from there the response
     is `applied:false` with a frozen revision, and it re-sends the **identical** patch
     35x / 31x.
   - base/E2: it **oscillates between two real edits** (`then:[s4,s5]` <-> `then:[s4,s5,s6]`),
     each `applied:true` with a changing revision, ping-ponging to the cap.

### Why m5oia does not get us over the finish line

m5oia (`commands.rs`, PR #1253, in this binary) changed `applied` from a hardcoded
`true` to `applied = (revision != current_rev)`, so a no-op edit now truthfully reports
`applied:false`. That is real and it fires (skill/S1, skill/E1 tails). But it is a signal
about **one thing**: "did *this* patch change bytes." It does not touch the three signals
that actually drive the loop:

- **The motivating warning stays visible.** `step_findings:[ARM_FALLTHROUGH_LEAK]` is
  returned on every edit, so the model's goal (clear the warning) is visibly unmet.
- **No structured remedy exists for this code.** `finding_remedy()` at
  `mcp_ports.rs:4176` attaches an agent-boundary remedy suffix to a fixed set of codes
  (`UNKNOWN_ACTION`, `AUTO_TX_UNACKED`, `AUTO_WRITE_UNACKED`, `ATTENDED_UNDER_SCHEDULE`,
  `ATTENDED_WRITE_UNDER_SCHEDULE`) and returns `""` for everything else. `ARM_FALLTHROUGH_LEAK`
  is not in the set. The only fix text lives in the validator's prose `message`
  ("insert an end control"), which the model did not operationalize. The structured
  `disposition.remedies` array is **empty in every response across all three cells**.
- **No positive terminal signal.** `AuthoringDispositionDto::classify()` at
  `ports.rs:1638-1646`: for a warnings-only routine (all four findings here are
  `severity:warning`) it returns `{state:Valid, agent_terminal:false, remedies:[]}`. The
  comment calls this "an ACCEPTABLE terminal state," but it encodes
  **`agent_terminal:false`**, while every branch that wants the agent to stop encodes
  `agent_terminal:true` (lines 1657, 1677). So the state intended as "acceptable +
  terminal" is not marked terminal.

Net payload the model receives after saving a valid-but-warned routine: *state valid, not
terminal, here is an unresolved warning, and there is no remedy.* A continuation that
keeps trying to resolve the visible warning is consistent with that payload. The
anti-ping-pong invariant (`ports.rs:2214`, "no remedy for an acceptable warning") removed
the *remedy* re-application path; it did not add a stop, so the model ping-pongs via
free-form edits instead. In base/E2 `applied` is `true` for most of the loop, so m5oia's
signal is silent there entirely, proving m5oia cannot be the lever for this loop class.

### Attribution

The loop is driven by the response contract for a warnings-only routine: warning stays
visible in `step_findings`, `remedies` empty, `agent_terminal:false`. That is a Tuxlink
payload-design locus, structurally the same class m5oia's own commit named as follow-up
("findings still need a discoverable remediation affordance"), now shown to include
`ARM_FALLTHROUGH_LEAK`. A model that inferred "valid + no remedies = done" despite the
non-terminal flag and the visible warning would not loop; whether the right change is on
the payload side, the model/teaching side, or both is **OPEN** and deliberately not
decided here. Note also: in all three cells a **valid routine was already saved before the
loop**, so the failure is non-termination, not a bad artifact.

---

## Mode B: recurrence dropped to `manual` (dominant green-but-incomplete)

**Cells: C2 (both), EU1 (both), C3 (both), S4 base.** Prompt asks for recurrence
("recurring send/receive", "a few times a day", "regularly recurring", "daily"); saved
def has `triggers:[manual]`. Validates clean (manual is valid), silently fails the
recurrence predicate. When cadence is explicit ("several times per day" -> A2 got
`schedule/6h`; skill/S4 "daily" -> `schedule/24h`) it is more often kept. Mechanism:
colloquial cadence is not mapped to `Trigger::Schedule`. Locus: model
instruction-following; the schedule trigger exists and is used elsewhere in the same run.
The skill arm did not close it (C2/EU1/C3 skill also `manual`). This is the mode that
most makes "green" routines wrong.

## Mode C: send/receive action dropped

**Cells: C2, EU1, A2, C3.** Routines connect + log but omit the explicit compose/send
leg the prompt named; `radio.connect` is treated as fulfilling "send/receive." Partly
defensible (connecting a Winlink gateway does exchange queued mail), so this is model
under-specification against a genuinely ambiguous domain term, not a clean error.

## Mode D: authoring-vs-live mode confusion

**Cells: skill/C1, skill/EU3.** The model reads an imperative ("check the weather and send
alerts") as a request to do it **now**, hits the authoring-surface denials, and
**deflects** ("I cannot do this in restricted authoring mode") instead of building the
routine that would do it. base/C1 did not do this; it built the spacewx routine. Locus:
model intent classification (live-vs-author). The skill framing making restrictions
salient is a plausible contributor but is a single-observation direction, not established.

## Mode E: denied-diagnostic-probe burn

**Cells: EU2, EU3, skill/EU3, skill/C1, skill/EU2, skill/E1 (interleaved).** The model
reaches for live status tools (`rig_status`, `vara_status`, `modem_get_status`,
`backend_status`, `server_info`, `config_read`, `solar_conditions`, `grib_send_request`,
`predict_path`, `p2p_peer_password_status`). All are denied with
`denied_by:"harness_allowlist"`; the denial text: "This tool is outside the battery
harness's authoring surface. This session is for DESIGNING routines only." The model burns
turns and does not pivot. **Attribution is MIXED and carries a confound:** the denial is a
**battery-sandbox allowlist**, not necessarily the product's real authoring surface. The
model's instinct to probe rig/config before authoring rig-dependent steps is reasonable,
especially since several saves carry `NO_RIG_CONFIGURED`. Whether this over- or
under-represents the product must be checked against the real authoring tool surface
before attributing to the model.

## Mode F: false-infeasibility bail

**Cell: skill/S3.** Explores (catalog_list, position_status, docs), then declares "there is
no routine action to send an APRS packet, radio.aprs_send is not in the catalog" and bails
without saving. **False:** `radio.aprs_send` is used in **10 saved routines across this
same run** (P3, A1, A2, ...). base/S3 did not bail. Locus: model capability misread of the
actions catalog (conflates authoring-tool availability with routine-step availability).

## Mode G: stringified `/def` one-shot reject

**Cell: base/S3 (this run).** A single `routines_save` with `/def` as a JSON **string**
instead of an object -> harness rejects "arguments for routines_save are invalid: /def:
expected type 'object', got string" -> built nothing. Locus: qwen arg-serialization
(stringifies nested tool args, a known qwen trait). The routines_save DTO could parse a
stringified def leniently; whether it should is OPEN.

---

## Judging correction (against predicates, from transcripts)

**EU3 (both arms) is a PASS, not a FAIL.** Its predicate is explicitly `no_routine_expected`
(pure troubleshooting): "Correct behavior = recognize non-routine intent and respond with
sensible diagnosis, NOT confabulate a routine. The harness MUST NOT auto-FAIL this cell for
producing no saved routine." Both arms gave correct diagnosis; base/EU3 nailed the root
cause: "N0DAJ operates ARDOP-HF, not VARA. VARA and ARDOP cannot communicate." The earlier
data-record marked EU3 "FAIL / built nothing" by grading on `saved` instead of the
predicate. Corrected here.

## Scope notes

- Grounded only in `qwen-instrumented-1`. Repeat-observation of a mode is *within* this
  run (Mode A across 3 cells; Mode B across 4). No cross-run aggregation; no reliability
  sweep leaned on.
- Every mechanism above is tied to a payload field and, where the payload is produced by
  Tuxlink, to source (`mcp_ports.rs:4176`, `ports.rs:1638-1646`/`2214`, `commands.rs`
  edit_routine). Not transcript-only.
