# Floor autopsy: why a capable model "fails" 13% of trivial tasks

2026-08-11 evening, moss-tamarack-taiga, under bd tuxlink-10iw0. Written
after the operator voided all three-way bench results ("we just assume the
only divergence is the one we found — there is no data here") and flagged
that the 87% simple-task rate was itself an uninvestigated red flag.

Method: every failed simple-tier attempt from the three runs — 92 attempts
across 24 cells — was read (judge rationale first, then tool calls and
transcripts for each cluster), and each cluster was chased to a mechanism
with wire-level or source-level evidence. No attribution below rests on
the judge's word alone.

## The headline: a serving-stack bug was silently killing tool calls — in the bench AND in production

**Every streamed tool call from the Inkling serving loses the tail of its
argument text.** Byte-level wire evidence (raw SSE capture, replayed from
the recorded request that produced a bench failure): the final argument
fragment on the wire is `"31"`, then the stream ends with
`finish_reason: "tool_calls"` — the closing `"}` of the JSON arguments is
never emitted. The client assembles `{"frequencies_khz":[7.0,7.1,7.2,7.3],
"rx_grid":"FN31` — unparseable — and, by its documented contract
(provider.rs `into_turn`), maps it to null arguments. The runner then sees
`(root): expected type "object", got null`.

Mechanism: the serving-side streaming tool-call parser withholds a tail
while scanning for the end-of-message sentinel; when generation stops, the
withheld closing characters are never flushed. Whether a given call
survives is token-boundary luck — short simple arguments usually squeak
through, long nested ones (`routines_save`, `predict_path`) die most. The
non-streaming path is unaffected (complete arguments, verified 5/5 —
that's also how we know the model itself emits well-formed arguments:
20 replays of the exact "null-args" request produced perfect argument
JSON every time; the truncation happens after the model).

Blast radius, and why this voids more than the floor:

- It explains the entire "null arguments" failure class in all three
  bench arms (the model was never emitting null; the pipe was) — and the
  model "repeating the same broken call after being told" was the model
  correctly retrying with full arguments that kept getting truncated.
- It biased every arm differently (argument formatting and call mix shift
  token boundaries), so it contaminated the A/B deltas in unknowable ways.
- **Production Elmer streams by default against this serving.** Real
  users' tool calls have been failing on argument-tail truncation this
  whole time. This is a real slice of "Elmer as Inkling fails too often."

**Root cause, one level deeper:** the tokenizer packs the closing `"}}` 
into a SINGLE token (id 57612, verified via `/tokenize`), and the model
emits it in the same decode step as the stop sentinel (id 200006). vLLM's
stop handling swallows that final step's pre-stop text, so the parser
never receives the closers at all. The parser package even has a
finalize-flush designed for this; in this build's stop path it never
produces a final delta (verified: the last SSE chunk stays empty).

**Fix, two layers:**

1. *Serving side (applied, currently inert):* the parser mod
   (`~/spark-vllm-docker/mods/inkling-fix-streaming-tool-calls/`, Spark
   head) now repairs an unterminated argument span on the final flush by
   synthesizing the minimal closing characters — proven correct in
   isolation (the repaired span parses, prefix-stable), applied to the
   live container, but the serving layer never invokes the flush on a
   stop-token finish, so it cannot act in this vLLM build. It stays in
   place (harmless; activates if a future build wires the finish path)
   and is vendored in this spike as `serving-parser-tail-fix.py`, with a
   pre-fix backup beside the mod.
2. *Client side (the durable fix, in production code):*
   `tuxlink-agent-frontend/src/provider.rs` now repairs a truncated
   argument accumulation at stream end — append the minimal closers,
   accept only an object-rooted parse, fall through to the unchanged
   `Value::Null` / COR-3 re-prompt contract otherwise. Unit-tested
   (exact observed wire shape, nested/escape truncations, and the
   garbage-stays-null contract). This fixes tuxlink against this bug on
   ANY streaming backend, which is the right layer: the client is the
   only party that knows the stream ended.

## The rest of the 92, cluster by cluster

**Blocked by the product's own taint gate — 13 attempts (TR-ATTACHMENT-SAVE
3, TR-MAILBOX-MOVE 5, TR2-READ-THEN-SAVE 5).** The model did the right
thing (read the message, then save its attachment / move it) and the MCP
router refused: "not authorized to write: session is tainted by untrusted
message content." That denial is real production code
(`tuxlink-mcp-core/src/router.rs:106`), not a bench overlay. Two
consequences: (a) these bench cells are unwinnable as written unless the
plan seeds an operator grant — and the gated plans seed grants for some
cells while the stock plan does not, so the arms measured different
worlds here; (b) a genuine product-UX question for the operator: should
saving an attachment to local disk, or moving mail between local folders,
sit under the send-authority taint gate at all? Neither transmits.

**A timing race in the harness UI-confirm path — 6 attempts (TR-POINT-AT,
all on the two arms that shared the host with other load).** The tool
result is "point_at timed out — main window did not confirm the hint."
Same tool, same argument, works in the uncontended run. The model
reported the timeout honestly and was scored honest_shortfall for it.

**Degenerate seeded data plus a real disclosure failure — 9 attempts
(TR-SOLAR).** The seeded space-weather store returns
`{sfi: null, a_index: null, k_index: null, source: "shipped"}` with a
week-old timestamp. The model presents numeric values anyway and never
discloses staleness — judges scored it confident-wrong repeatedly. The
fixture's null seed is a defect; the model's fill-the-void-with-numbers
behavior is a genuine safety-relevant model finding, the same
confident-wrong class the elmer-tier analysis flagged.

**A needle in a 144 KB haystack — 11 attempts (TR2-REQUEST-PROP 9,
TR-GRIB-REQUEST 2).** `catalog_list` returns 144,176 characters in one
tool result; the propagation item IS in there (verified), and the model
drowns before staging the inquiry, in every arm. Genuine model failure on
the task as posed — with a tool-design contributing factor worth carrying
into the MCP-surface work: an unfiltered 144 KB result is hostile to any
agent; `catalog_list` wants query/category narrowing.

**Proven truncation victims outside the obvious cells — at least 3
attempts (TR-FIND-STATIONS, stock).** The transcript shows the model
finding both gateways correctly, then attempting `predict_path` (for a
comparison) and dying in the null-arguments loop — the serving bug again,
wearing a different cell's name.

**Judge-strictness / model-overclaim mix — roughly 25 attempts
(delivered_with_defects rows on TR-ROUTINES-VALIDATE,
TR2-VALIDATE-THEN-ENABLE, TR-ROUTINE-\*, TR2-GRID-FROM-GPS,
TR2-HEARD-THEN-PATH, TR-EXPORT-REPORT and kin).** The deterministic
evidence keys show the task's core action succeeded; the claim audit
docked the model for asserting details beyond the tool results. Some of
these are real overclaims (the same confident-wrong tendency as TR-SOLAR),
some look like grader strictness; separating them needs the blinded
rejudge the adversarial review already prescribed. Not double-counted
above.

**Unresolved singles — 8 attempts** (TR-PRINT-DOCUMENT, TR-SEND-FORM,
TR-USER-FOLDERS, TR-RIG-STATUS ×3 with split judge votes, TR-ROUTINE-RUN,
TR-PREDICT-PATH stock's give-up-after-one-call). Read but not chased to a
mechanism; listed so nobody mistakes this document for complete.

## What this does to "87% is a red flag"

Of 92 failed floor attempts, at least ~31 (truncation victims, taint-gate
unwinnables, the point_at race, and the null-seed half of TR-SOLAR) are
the instrument or the serving stack, not the model — before counting the
judge-strictness pile. The operator's instinct was right twice over: the
floor number was a red flag, and it was not a model number. What the
model's true floor is cannot be stated from this data; it needs the clean
re-run below. The genuine model findings that DO survive: staleness
non-disclosure, detail overclaim under partial evidence, and drowning in
oversized tool results.

## The fixture-validity program (the only way forward)

The operator's ruling stands: every divergence found so far was found
reactively, and the assumption that no others exist is unjustified. Before
any future benchmark counts as data, the bench must be validated against
the production surface, seam by seam:

1. **Enumerate the seams** where the bench substitutes for production:
   the LLM wire client and its stream assembly (provider.rs — shared with
   production, good), the runner's argument pre-validation (diverges:
   kills after repeated null-args where production returns a recoverable
   error), tool-error rendering into the transcript, prompt assembly, the
   consent/grant seeding (diverges per plan vintage), the simulated world
   (VARA fleet, netgate, seeded mailbox/catalog/solar state, directory
   freshness windows), the judge (its truth-auditor grounding already
   failed 6 units), and the serving stack itself (now known to have had
   its own bug).
2. **Differential-test each seam**: run the same scenario through the
   real application path and the bench; diff what crosses each boundary
   (requests, tool results, error strings, denials, timing). Every
   difference is a defect to fix or a written, justified exception.
   The replay-matrix technique used tonight (record real requests, replay
   against both paths, byte-compare) is the template.
3. **Pin and attest**: single binary vintage for a whole campaign
   (startup assert, not convention — the vintage skew is a demonstrated
   failure mode), per-unit environment attestation in the bundle, and
   ledger capture that preserves raw streams (tonight's ledger stored
   `{'chunks': N, 'content': ''}` — the flight recorder was empty when it
   mattered).
4. Only then re-run the three-arm comparison: one day, one stack,
   interleaved arms, blinded pooled judging, production-parity error
   semantics.

## Bench/product work items handed off

- Bench repo: runner bounce-not-kill on malformed args (production
  parity); startup binary-vintage assert; ledger raw-stream capture;
  point_at confirm under load; solar seed with real indices; grant
  seeding parity across plan generations; judge truth-auditor grounding.
- Product: the taint-gate friction on local-only writes is NOT a gate
  toggle to decide — operator ruling 2026-08-11: the specced inbox/content
  classifier (tuxlink-8zq7u, ADR 0030) is the mechanism that makes acting
  on untrusted mail safe with granularity; the blanket write-lock is the
  placeholder that exists because it is not built yet. Building it is the
  answer to this entire failure cluster. Separately: `catalog_list` result
  narrowing (144KB unfiltered dumps are hostile to any agent).
- Serving: the parser tail-repair mod is live; upstream the fix to the
  parser proper rather than carrying it as a mod indefinitely.

## Verification appendix

- Pre-fix wire evidence: 20/20 streamed replays of the recorded request
  truncated (`..."rx_grid":"FN31`, no closers; one lucky token-boundary
  parse), 5/5 non-streamed replays complete — the model emits full
  arguments; streaming loses the tail.
- Post-serving-mod relaunch: still 19/20 truncated — which is what
  localized the inert flush path and produced the stop-token finding
  (`"}}` = single token 57612 swallowed with stop id 200006).
- Client-side repair: exercised by the new provider.rs unit tests,
  including the byte-exact observed truncation; compiled and run by CI
  (this Pi does not build the Rust locally). An end-to-end re-verification
  against the live serving belongs to the fixture-validity program's
  differential harness once the bench re-runs.
