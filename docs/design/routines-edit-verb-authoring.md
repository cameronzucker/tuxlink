# Routines edit-verb authoring surface (P2 of the authoring rework)

> **Status:** ADREV-AMENDED, implementation-authorized (2026-07-20). The
> design adrev ran as a matched pair (GPT-5.5 authoritative + GPT-5.6 shadow,
> ledger pair 4): both models independently judged the verb model
> "fundamentally sound" and confined findings to amendments, satisfying the
> operator's convergence gate ("no significant disagreement"). All nine 5.5
> findings and the convergent 5.6 findings are folded in below; the
> amendment provenance is marked [A#] per 5.5's numbering. Phase 1 (registry
> self-description + save-time param validation, tuxlink-3nvvl, PR #1188) is
> the substrate this design assumes.

## Problem

Routine authoring is whole-document-or-nothing: `routines_save` takes the
entire definition and either replaces the routine or rejects. Live transcripts
(2026-07-19) show what that costs each audience:

- The shipped 122b model, asked for a two-band fallback routine, made one
  brace-balancing typo in a 1300-character `def_json` string and resent the
  IDENTICAL payload six times; the parse error's only pointer was
  "column 1226," which a model cannot count to inside an escaped string
  (tuxlink-bzxwp). One syntax slip cost the whole document.
- GLM-5.2 authored a 19-step routine first-try, but only after four
  docs_search calls plus reading four saved routines to reverse-engineer the
  envelope and control shapes — the authoring surface itself taught it
  nothing about editing granularity.
- A human in the designer edits step-by-step (defDraft.ts
  `insertStep`/`updateStep`/`removeStep`), then saves. The agent has no
  equivalent: its only verb is "replace everything."

The operator's framing: "What if agents are able to just edit certain
portions rather than hand-cutting the whole document?" This spec is that,
made concrete.

## Design

### D1. Verbs, not documents

New MCP tools mirroring the designer's edit vocabulary. Every verb operates
on a NAMED SAVED ROUTINE, validates the result with the full `validate()`
(params lints included, per P1), persists per the failure taxonomy (D6), and
returns `{routine, revision, applied, step?, step_findings, routine_findings,
blocked}` — findings for the touched step are a SEPARATE field, not a sort
promise the validator does not make [A9].

| Verb | Args | Effect |
|---|---|---|
| `routines_step_add` | `routine`, `track`, `step` (object, `id` optional), `after_step_id?` OR `branch?: {step_id, arm}` | Insert an action/control step. Appended to the track when no placement is given; a track whose LAST step is an `end` control appends BEFORE that end (adrev round 3: a trailing end is a terminator, not a position — appending after it builds unreachable steps, and the definition_template bootstrap would hit exactly that). `branch` placement inserts into a branch arm atomically — storage position AND arm membership in one operation, matching the designer's `insertStepIntoBranchArm` [A3]. When `step.id` is absent the server assigns the next free id (`s<max+1>`) and returns it [5.6#8]. |
| `routines_step_update` | `routine`, `step_id`, `patch` (object) | Shallow-merge onto the step: `params` replaces wholesale; `action`/`on_radio_busy`/`timeout_s`/control payload fields individually. Changing the step's `id` or its action↔control kind through `patch` is rejected — that is a remove+add, stated in the tool description [5.6#6]. |
| `routines_step_remove` | `routine`, `step_id` | Remove the step AND scrub branch/retry references to it, exactly as the designer's load-bearing scrub does; the response lists what was scrubbed. Dangling-refs-as-findings is rejected: freed ids plus later reuse silently attach unrelated steps to old arms [A4]. |
| `routines_step_move` | `routine`, `step_id`, `after_step_id?` OR `branch?: {step_id, arm}` OR `track?` | Reposition a step without the remove/re-add dance that transits broken-ref states [A6]. |
| `routines_track_add` | `routine`, `track` (name) | Add an empty parallel track (designer parity: `addTrack`) [A6]. |
| `routines_track_remove` | `routine`, `track` (name) | Remove a track and its steps, with the same reference scrub as step removal [A6]. |
| `routines_trigger_set` | `routine`, `triggers` (array) | Replace the trigger list |
| `routines_meta_set` | `routine`, `{transmit_mode?, on_interrupted?, inputs?}` | Mutate envelope fields; `transmit_mode: automatic` still requires the designer ack (C3) — the verb saves the mode and reports the unacked state as today. `rename` is NOT here [A5]. |
| `routines_rename` | `routine`, `new_name` | Dedicated transactional rename: definition file, body name, enabled-state sidecar, scheduler anchor, and `control:"call"` references in OTHER routines all migrate in one operation; the response lists rewritten callers. Rename is identity surgery, not a metadata patch [A5]. |

All step/trigger/meta payloads are JSON OBJECTS — no stringified JSON
anywhere in the new surface (ADR 0025's tool-shape lesson).

A syntax or shape mistake now costs ONE verb call, and the error names one
step's one field via the P1 param lints ("step s8, param message, …") instead
of a column offset into the whole document.

**The scrub is the id-safety invariant** [A4]: both reviewers asked for ONE
consistent rule from the disjunction "scrub atomically, reject-while-
referenced, or never-reuse ids." Scrub-on-remove is the chosen arm — exact
designer parity (defDraft.ts calls its scrub load-bearing for precisely this
hazard), and with no dangling reference left behind, a recycled id has
nothing to misbind to. Server-assigned ids are `s<max+1>` over the current
definition (the designer's `nextStepId`, verbatim), which needs no schema
field; a persisted never-reuse counter was rejected as storage-format
pollution for a hazard the scrub already closes.

### D2. Whole-document save stays, and takes an object

`routines_save` remains for bootstrap (the `definition_template` path),
import/export, and the designer's save. It gains `def` (JSON object) as the
preferred parameter. **Exactly one** of `def` / `def_json` must be present:
both or neither is a tool error. A string supplied as `def` that parses as a
JSON OBJECT is accepted as the definition; a string that does not still
errors, steering to `def_json`. *(Amends A7's original never-auto-parse
rule, 2026-07-20: exam transcript 1784569467900-0 showed a 122b model
emitting `def` stringified and resending the identical payload nine times
against the strict rejection — it cannot perceive the object-vs-string
difference in its own emission, so the theoretical double-encoding
ambiguity A7 guarded was outweighed by an observed hard loop. Parsing a
well-formed stringified object is deterministic and semantically identical
to `def_json`; the trap A7 actually feared — both params present, or silent
misparse of malformed JSON — remains rejected.)* `def_json` carries a
deprecation note for one release, then goes. New-surface docs and the system-prompt
carve-out teach ONLY the object form and the verbs.

### D3. Bootstrap flow the catalog teaches

The catalog's `definition_template` gains a companion note: "save the
template under your routine's name, then build it with routines_step_add /
routines_step_update." The expected agent flow becomes: save minimal valid
skeleton (1 call) → add steps one at a time, each validated in isolation
(N calls) → set trigger → report. Each call is small enough that the 122b
context and syntax budget hold; a failure names the exact fragment. New
routines are born DISABLED, so this whole flow happens on a routine that
cannot fire (see D5).

### D4. No server-side draft state

Verbs edit the SAVED routine directly. A routine with warning findings is
already a legal saved state (spec §10). Consent digests: edits flow through
the same save path, so C3 digest invalidation of transmit/write acks Just
Works — an edited automatic routine reverts to unacked exactly as a re-saved
one does.

Rejected: session-scoped draft objects (`routines_draft_begin/commit`).
A draft lifecycle adds abandoned-draft GC, a second source of truth, and a
"which draft am I editing" failure mode for small models — for no benefit
the saved-with-findings state does not already provide.

Rejected: RFC 6902 JSON-patch. Patch paths (`/tracks/0/steps/3/params`) are
one more syntax to teach and get wrong; the verbs carry the same power in
the domain's own nouns.

Rejected: an atomic multi-op batch verb (`routines_edit` with an ops array)
as the PRIMARY surface. Both reviewers offered batch-or-disable as the
atomicity fix; the disable guard (D5) was chosen because per-op tools with
flat schemas are what small models handle (the MCP-surface lesson: nested
op-array schemas mislead them), and because D5 makes mid-sequence states
harmless rather than requiring every author to plan a transaction.

### D5. Enabled-routine guard — the atomicity answer [A1, A3]

Edit verbs REJECT a currently-enabled routine with a structured
precondition error: `ROUTINE_ENABLED` — "disable with
routines_set_enabled(false), edit, re-enable." No mutation occurs.

This closes both reviewer P1 scenarios in one move: a scheduler can never
fire a mid-edit-sequence state (the routine is disabled for the whole
sequence), and re-enabling is the natural commit point — it re-runs
validation (errors block enable, unchanged), re-anchors the schedule, and
the C3 digest re-ack applies for automatic routines. The authoring flow
(D3) is unaffected because new routines start disabled. `routines_save`
(whole-document) keeps its current enabled-routine behavior — it is a
single atomic replacement, which is exactly the property the verbs lack.

### D6. Per-verb failure taxonomy [A8]

Three mutually exclusive outcomes, stated in every tool description:

1. **Malformed input** — the payload fails schema/serde (bad `step` shape,
   `then` as an object, unknown discriminator): tool error naming the field,
   NO mutation. The P1 param lints do not apply — there is no step yet.
2. **Precondition failure** — unknown routine/track/step id, revision
   conflict (D7), enabled guard (D5): structured error with a stable code,
   NO mutation.
3. **Applied** — the edit deserialized and preconditions passed: the result
   is SAVED (even with error findings — errors block enable/run, never
   save, unchanged from §10), and the response carries `applied: true`,
   the new `revision`, `step_findings` (touched step), `routine_findings`
   (everything else), and `blocked`. Tool descriptions instruct agents to
   resolve touched-step findings before reporting completion [5.6#9].

This resolves the D1-vs-D4 wording contradiction 5.5 flagged: "persists on
success-or-warning" meant outcome 3's save-with-findings, and now says so.

### D7. Revision check — lost-update protection [A2]

`routines_get` / `routines_list` gain a `revision` field (content digest of
the canonical definition JSON). Every edit verb and `routines_save` accept
optional `expected_revision`; a mismatch is a precondition failure
(`REVISION_CONFLICT`, includes the current revision) with no mutation. The
designer ALWAYS sends the revision it loaded — its whole-document save is
the clobber-prone writer in the lost-update scenario (agent adds a step,
operator's stale draft save deletes it). Agent single-verb flows may omit
it; the store serializes verb read-modify-write under a lock, so verbs
cannot interleave with each other or with saves.

### D8. Designer parity

The React designer keeps its client-side defDraft (its UX needs local undo
and canvas selection). Parity is at the CONCEPT level: the same verb set,
the same fragment-level validation story, the same scrub-on-remove
invariant, powered by the same P1 registry metadata. The designer's
StepInspector typed fields (tuxlink-w3a85) and the agent's verbs are two
skins over one contract.

## Acceptance (phase 3 of the rework)

1. Re-run the 122b no-nudge exam prompt on the new surface: expected
   template-save → step adds → trigger set, zero whole-document resends.
2. Re-run the GLM battery: the s8-brace-typo class of failure must be
   impossible (no hand-built document), and total docs round-trips should
   drop (the verbs' tool descriptions carry the envelope knowledge).
3. `hourly-cms-vara-20m`'s nested-stations defect (P1's REF_TYPE_MISMATCH)
   must be reproducible as: `routines_step_update` returns the finding for
   that step alone in `step_findings`.
4. [A1] An enabled routine rejects `routines_step_update` with
   `ROUTINE_ENABLED` and mutates nothing.
5. [A2] A stale `expected_revision` on `routines_save` rejects with
   `REVISION_CONFLICT` and the agent's earlier verb edit survives.
6. [A4] Removing a branch-referenced step scrubs the arm and reports it;
   a later step that recycles the freed id carries no phantom arm
   membership (the scrub, not id hygiene, is the invariant).
7. [A5] Renaming an enabled routine with a caller: single call, the new
   name is enabled, the old is gone, the caller's `call` step points at
   the new name.

## Out of scope

- The missing perception actions (predict/FT-8/rank) — P4, designed with the
  Overwatch epic.
- The designer UI half (w3a85 / P5) — builds on this plus P1. (The designer
  ADOPTING `expected_revision` on save is IN scope — it is the protection's
  point.)
- Trigger/control teaching in the catalog (tuxlink-6epl8) — folds into the
  verbs' own tool descriptions plus the template note (D3).
