# Routines edit-verb authoring surface (P2 of the authoring rework)

> **Status:** DRAFT for design adrev (operator-directed, 2026-07-20). Not
> implementation-authorized until the adrev round's findings are dispositioned
> and the operator has seen the result. Phase 1 (registry self-description +
> save-time param validation, tuxlink-3nvvl, PR #1188) is the substrate this
> design assumes.

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

Five new MCP tools, mirroring the designer's own edit vocabulary. Every verb
operates on a NAMED SAVED ROUTINE, validates the result with the full
`validate()` (params lints included, per P1), persists on success-or-warning
exactly like `routines_save` (spec §10: errors block enable, never save), and
returns `{routine, findings, blocked, step?}` with findings for the touched
step listed first.

| Verb | Args | Effect |
|---|---|---|
| `routines_step_add` | `routine`, `track`, `step` (object), `after_step_id?` | Insert an action/control step; appended to the track when `after_step_id` is absent |
| `routines_step_update` | `routine`, `step_id`, `patch` (object) | Shallow-merge onto the step: `params` replaces wholesale, `action`/`on_radio_busy`/`timeout_s`/control payload fields individually |
| `routines_step_remove` | `routine`, `step_id` | Remove the step; branch arms referencing it become validation findings, not silent repairs |
| `routines_trigger_set` | `routine`, `triggers` (array) | Replace the trigger list |
| `routines_meta_set` | `routine`, `{transmit_mode?, on_interrupted?, rename?}` | Mutate envelope fields; `transmit_mode: automatic` still requires the designer ack (C3) — the verb saves the mode and reports the unacked state as today |

All step/trigger/meta payloads are JSON OBJECTS — no stringified JSON
anywhere in the new surface (ADR 0025's tool-shape lesson).

A syntax or shape mistake now costs ONE verb call, and the error names one
step's one field via the P1 param lints ("step s8, param message, …") instead
of a column offset into the whole document.

### D2. Whole-document save stays, and takes an object

`routines_save` remains for bootstrap (the `definition_template` path),
import/export, and the designer's save. It gains `def` (JSON object) as the
preferred parameter; `def_json` (string) stays accepted for one release with
a deprecation note in the tool description, then goes. New-surface docs and
the system-prompt carve-out teach ONLY the object form and the verbs.

### D3. Bootstrap flow the catalog teaches

The catalog's `definition_template` gains a companion note: "save the
template under your routine's name, then build it with routines_step_add /
routines_step_update." The expected agent flow becomes: save minimal valid
skeleton (1 call) → add steps one at a time, each validated in isolation
(N calls) → set trigger → report. Each call is small enough that the 122b
context and syntax budget hold; a failure names the exact fragment.

### D4. No server-side draft state

Verbs edit the SAVED routine directly. A routine with warning findings is
already a legal saved state (spec §10); an enabled routine that an edit
takes from clean to ERROR findings behaves exactly as a whole-document
re-save does today (verified in `save_routine`, commands.rs: "Save even
with errors — errors block enable/run, never save"): it stays saved and
enabled, and the error blocks the RUN at fire time. The verb's response
carries `blocked: true` so the author is told at edit time, not at 2am.
Consent digests: edits flow through the same save path, so C3 digest
invalidation of transmit/write acks Just Works — an edited automatic routine
reverts to unacked exactly as a re-saved one does.

Rejected: session-scoped draft objects (`routines_draft_begin/commit`).
A draft lifecycle adds abandoned-draft GC, a second source of truth, and a
"which draft am I editing" failure mode for small models — for no benefit
the saved-with-findings state does not already provide.

Rejected: RFC 6902 JSON-patch. Patch paths (`/tracks/0/steps/3/params`) are
one more syntax to teach and get wrong; the verbs carry the same power in
the domain's own nouns.

### D5. Designer parity

The React designer keeps its client-side defDraft (its UX needs local undo
and canvas selection). Parity is at the CONCEPT level: the same verb set,
the same fragment-level validation story, powered by the same P1 registry
metadata. The designer's StepInspector typed fields (tuxlink-w3a85) and the
agent's verbs are two skins over one contract.

## Acceptance (phase 3 of the rework)

1. Re-run the 122b no-nudge exam prompt on the new surface: expected
   template-save → step adds → trigger set, zero whole-document resends.
2. Re-run the GLM battery: the s8-brace-typo class of failure must be
   impossible (no hand-built document), and total docs round-trips should
   drop (the verbs' tool descriptions carry the envelope knowledge).
3. `hourly-cms-vara-20m`'s nested-stations defect (P1's REF_TYPE_MISMATCH)
   must be reproducible as: `routines_step_update` returns the finding for
   that step alone.

## Out of scope

- The missing perception actions (predict/FT-8/rank) — P4, designed with the
  Overwatch epic.
- The designer UI half (w3a85 / P5) — builds on this plus P1.
- Trigger/control teaching in the catalog (tuxlink-6epl8) — folds into the
  verbs' own tool descriptions plus the template note (D3).
