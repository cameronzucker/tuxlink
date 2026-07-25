# qwen failure-mode analysis: Opus vs GPT-5.6-Sol-high disagreement map

Author: tanager-owl-cardinal. Companion to
[2026-07-24-qwen-failure-modes-analysis.md](2026-07-24-qwen-failure-modes-analysis.md)
(my analysis, "Opus" below). GPT-5.6-Sol-high (Codex, agent `sumac-towhee-marten`)
independently analyzed the SAME raw package with read access to the same source, and
was NOT shown my findings. Goal per operator: surface **disagreement**, not
convergence. Every disagreement below is adjudicated against source (file:line), because
both analyses are supposed to be grounded, so "who is right" is checkable, not a vote.

Raw GPT-5.6 report (gitignored, local): `dev/scratch/2026-07-24-gpt56-qwen-failuremodes.md`.

## Where we converge (stated briefly, not the focus)

- **Mode A core (the payload paradox).** Both independently landed on: a warnings-only
  routine returns `state:valid, agent_terminal:false, remedies:[]` while the warning
  stays visible, and m5oia's `applied:false` does not change those signals. Both cite
  `ports.rs:1643` and the `finding_remedy` covered-set at `mcp_ports.rs:4174`. Independent
  confirmation of the central finding.
- EU3 both arms = PASS (predicate is `no_routine_expected`).
- The denied-diagnostic probes are a **battery-harness allowlist** confound, not
  necessarily the product surface (Opus Mode E == GPT-5.6 Mode 7).
- S3/skill's bail is a false catalog-negative; `radio.aprs_send` really is in the
  registry (GPT-5.6 grounds `actions/mod.rs:485`; Opus grounded it via 10 in-run uses).
- Recurrence-dropped-to-manual is a real, repeated green-but-incomplete mode.

## Disagreements, adjudicated

### 1. GPT-5.6 found a mode I missed: log-interpolation doc/runtime contradiction
**GPT-5.6:** the action catalog tells the model embedded refs do NOT interpolate
(`local.rs:698`: "refs embedded inside longer text do NOT interpolate and log as literal
text"), but the executor DOES interpolate them (`executor.rs:~247`: "Everything else
interpolates embedded `$path` tokens in place," citing the battery S1 case verbatim).
**Adjudication: GPT-5.6 correct, verified in source.** A genuine Tuxlink documentation
defect. Opus missed it entirely. Impact is latent here (it did not cause a headline
failure) but it teaches the model stale behavior and can induce unnecessary edits or
mislead log-predicate grading. **Adopt as a new finding (Tuxlink doc defect).**

### 2. GPT-5.6 corrected my Mode G (stringified `/def`)
**Opus:** base/S3 failed because qwen stringified `/def`; "whether the DTO should parse a
stringified def is OPEN."
**GPT-5.6:** that tolerance ALREADY exists. `arg_shape.rs` implements a one-parse-if-string
rule (tuxlink-sq72z) and `routines_save.def` was taught it (#1205). So mere stringification
cannot explain the "expected object, got string" reject; the inner string was likely
malformed/incomplete (unprovable here: the recorded call args clip at 326 chars).
**Adjudication: GPT-5.6 correct, verified in `arg_shape.rs`.** My "OPEN" was wrong; the
product is not missing the leniency. **Correct Mode G:** the locus is the model emitting a
*malformed* stringified object, not a missing tolerance. (Caveat both share: neither can
see the full inner string to prove the exact defect.)

### 3. GPT-5.6 sharpened Mode A's mechanism AND attribution (the scaffold guard)
**Opus:** the loop persists because the payload gives no positive terminal signal;
attribution primarily the Tuxlink payload contract.
**GPT-5.6:** the skill scaffold ALREADY contains the anti-loop rule. `provider.rs` item 7:
"Make at most one changed repair attempt for each distinct finding. Never repeat an
identical **rejected** tool call." But a no-op edit returns a **success** object with
`applied:false`, which is not a "rejected" call, so the scaffold's guard never fires.
**Adjudication: GPT-5.6 correct, verified in `provider.rs` item 7.** This is more specific
than my framing and it moves the attribution: in the **skill** arm (S1, E1) the model was
explicitly taught not to loop and did anyway, and the payload's success-framing of a no-op
is exactly what dodges the scaffold's guard. Nuance both must carry: **base/E2 has no
scaffold**, so there the payload contract is the whole story; the teaching-execution angle
applies only to the skill-arm loops. **Sharpen Mode A:** the actionable seam is that a
no-op is delivered as success, not as a rejection, so neither the scaffold guard nor a
"stop" signal engages.

### 4. Grading: P1/base — GPT-5.6 MISS vs Opus PASS
**Adjudication: GPT-5.6 correct.** The `data.find_stations` step params are
`{modes:[vara-hf]}` with **no `bands` and no `limit`**; `40m` appears only on the later
`radio.connect`. The prompt asks for the nearest FIVE 40m gateways. The routine finds up
to 8 all-band vara-hf stations (defaults per `find_stations.rs`) and connects on 40m; the
"limit 5" is dropped and the find is not band-constrained. My clean PASS was too lenient;
this is at most PARTIAL. I graded on the presence of a find/connect/log/schedule shape
without inspecting the find params.

### 5. Grading: A2/base — GPT-5.6 "unreachable compose" vs Opus "send dropped"
**Adjudication: GPT-5.6 correct and sharper.** Step order:
`find -> connect -> branch(on connected: then=[end], else=[log]) -> end(s4) -> log ->
compose(s7)`. On a SUCCESSFUL connect the branch jumps to the success `end` (s4), so
`compose` (s7) is **unreachable on the success path**. The model built the send step and
then made it topologically dead. That is a dataflow/reachability failure (GPT-5.6 Mode 3),
distinct from and worse than my "no explicit send" (Mode C). **Adopt the sharper mechanism.**

## Where Opus had the edge

- **base/E2 identical-loop.** GPT-5.6 could only hedge ("an identical-patch loop for E2
  specifically is not proven") because the raw package I built **clipped RESULT records at
  ~1200 chars and truncated later edit args**. Opus read the full 38-call trace and
  confirmed E2 is a two-state oscillation (`applied:true` throughout), which is why m5oia
  is silent there. So on E2 my evidence is stronger, but only because GPT-5.6 was starved
  by MY package construction. **This is a self-inflicted limitation to fix before any
  re-run of this comparison: hand the full untruncated payloads and all edit args.**

## Net

The central Mode A payload paradox is independently confirmed by both models. On the
specifics, GPT-5.6 was more rigorous in five verifiable places (one missed mode, one
corrected attribution of mine, one corrected root-cause of mine, two too-lenient gradings
of mine), all because it read source I did not (the skill scaffold, the executor,
`arg_shape.rs`) and checked step reachability rather than action presence. Opus was more
complete only on base/E2, and only because the package I fed GPT-5.6 was clipped. The
disagreements are adjudicated in GPT-5.6's favor above with source citations; the
corrections are folded back into the main analysis doc's banner.
