# Freeze pack v1 — Task-0 fixtures for the template+slots spike (tuxlink-3gaz7)

**Status: DRAFT pending operator ratification (writing-plans Task 0).**
Once ratified, these files are byte-frozen: implementation consumes them
verbatim (goldens include these bytes; the runner sends these asks; the
grader compares against these expectations). Any post-ratification change
is a v2 pack that goes back through the operator.

Governing design: `docs/superpowers/specs/2026-08-19-template-slots-intake-design.md`
(operator-approved 2026-08-22). Plan: `docs/superpowers/plans/2026-08-22-template-slots-spike.md`.

## Contents

| File | What it freezes |
|---|---|
| `tool-description.md` | The exact MCP `description` text for `routine_template_compile` — the candidate's entire instruction artifact (worked call per template + one worked refusal) |
| `input-schema.json` | The tool's input JSON Schema (envelope + leaf-type law) |
| `result-envelopes.md` | Every result and error envelope, field by field, with exemplar JSON |
| `completion-copy.md` | The completion-copy table: exact ASCII copy per reachable state, `agent_terminal`, permitted next action, prohibited claims |
| `lowerings/*.json` | The three canonical lowerings: one concrete exemplar per template + the slot-holed skeleton the grader instantiates |
| `matrix-v1.json` | The hashed matrix appendix: all 27 runs — ask text + sha256, seeds, expected template/slots/disposition/trace, prohibited content, unchanged-field lists |
| `MANIFEST.sha256` | sha256 of every file above; the ratification pins these digests |

## Engine facts these fixtures are grounded on (verified in source 2026-08-22)

- `tuxlink-routines/src/scheduler.rs::every_seconds` accepts ONLY `Ns`/`Nm`/`Nh`.
  There is no day unit. It happily parses zero and negative values — the
  scheduler then silently never fires — which is why the compiler enforces
  positivity and bounds (design §2).
- `scheduler.rs::parse_window` validates `HH:MM-HH:MM` (hours 0-23, minutes
  0-59); `window_contains` supports overnight windows (start > end wraps
  midnight). Malformed windows fail closed.
- The registered band vocabulary is the 10-label HF table in
  `src-tauri/src/mcp_ports.rs::BANDS`: 160m 80m 60m 40m 30m 20m 17m 15m 12m 10m.
- `DefinitionStore` (`src-tauri/src/routines/store.rs`) computes revisions as
  sha256 of stored bytes (`revision_of`); save returns the revision.
- Serde wire shapes for `RoutineDef`/`Trigger`/`Step` are those of
  `tuxlink-routines/src/types.rs` (branch `then`/`else` are step-id lists;
  ends are control steps; `align: None` does not serialize).

## Decision points frozen here that the design left to fixture authoring

Reviewer: these are the calls to accept or override at ratification. Each is
marked `[D#]` where it lands in the pack.

- **D1 — nullable-slot omission = null.** `every` and `window` (where a
  template declares them nullable) may be omitted; omission means null. All
  other declared slots must be present (`SLOT_MISSING` refusal). Rationale:
  omission is the natural emission for "not asked for"; forcing explicit
  nulls manufactures refusals the leniency-by-normalization rule exists to
  avoid.
- **D2 — RS-env cell mechanics.** You cannot command a model to emit a bad
  envelope, so the two RS-env asks embed trap-shaped source material:
  RS-env-a embeds a config "export" whose JSON is malformed (single quotes),
  so a verbatim relay fails the one-parse absorption boundary and draws the
  hard error; RS-env-b embeds a window as a `{start, end}` object. Passing
  traces are EITHER `[error, corrected call]` OR `[clean compile]` (the model
  normalized the trap away unprompted — trap-avoided is a pass; recovery
  data simply does not accrue from that run). Only identical-repeat after
  the error is a failure. Note: a well-formed stringified `slots` value is
  ABSORBED by the existing one-parse boundary (design §1) and is telemetry,
  not an error — hence the malformed-string construction for the error cell.
- **D3 — no day unit.** Duration aliases cover seconds/minutes/hours only
  (matching the engine). "2 days" REFUSES (`SLOT_NOT_A_DURATION` names the
  accepted units); the 30-day bound applies to the computed seconds of
  s/m/h expressions (e.g. `"720h"` = 30d passes; `"721h"` refuses).
- **D4 — windows.** Overnight windows (start > end) are ACCEPTED
  (engine-verified). Equal endpoints refuse (`WINDOW_ENDPOINTS_EQUAL`,
  per design — no full-day shorthand in v0).
- **D5 — S1 vs S2 definitions.** S1 = asks whose correct template is the
  PRIMARY but whose surface vocabulary pulls toward a distractor
  ("broadcast", "housekeeping", "log entry"). S2 = asks whose correct
  template is a DISTRACTOR but whose vocabulary pulls toward the primary
  ("gateways", "frequency", schedule-with-window shapes). Confusion is
  probed in both directions; pairs reported per design §4.
- **D6 — free-text grading via skeleton instantiation.** Structured slots
  (`every`, `window`, `stations`, `bands`) are gated EXACT-match on
  normalized values. Free-text slots (`name`, `success_log`, `failure_log`,
  `fail_reason`, `message`, `note`) are gated by per-cell PREDICATES
  (pattern, required tokens, prohibited content). The grader instantiates
  the frozen skeleton lowering with the run's accepted slot values and
  requires structural equality with the compiler's emitted def — grader
  independence per design §4 (the skeleton is authored here, not derived
  from compiler code).
- **D7 — behavior_summary is compiler-rendered.** No existing plain-language
  routine renderer exists in the tree (searched 2026-08-22; the old SPEC's
  "existing renderer" assumption was wrong). The compiler renders the
  summary deterministically from template + normalized slots using the
  frozen sentence forms in `result-envelopes.md`, echoing free-text slots
  verbatim (the smuggling-visibility channel).
- **D8 — routine name rule.** Schema-side: `^[a-z0-9]+(-[a-z0-9]+)*$`,
  1..48 chars (`NAME_INVALID` refusal). Build Task 1 verifies this against
  the store's own name validation when the authoring service is extracted;
  if the store is stricter, the compiler adopts the store's rule and this
  pack takes a v1.1 amendment through the operator (a narrowing, flagged,
  never silent).
- **D9 — the token language.** Exactly two tokens exist, `$station` and
  `$band`, and only in the primary template: ALLOWED in `success_log`
  (compiler rewrites to `$s1.station`/`$s1.band`); REFUSED in `failure_log`
  and `fail_reason` (`SLOT_TOKEN_UNAVAILABLE` — a failed connect exposes no
  station/band); any OTHER `$word` in those three slots refuses
  (`SLOT_TOKEN_UNKNOWN` — silent passthrough of `$stations` to runtime is
  worse than a refusal). Distractor free-text slots (`message`, `note`)
  have NO token language; `$` is literal there.
- **D10 — RS-saved permitted traces.** `[compile save:true]` (one call) or
  `[compile, recompile save:true]` (two calls) both pass; the second is the
  cautious compile-first path the tool description permits. More than two
  intake calls = excess per the design's budget.

## Ratification protocol

Operator reviews this pack (the seven files). Ratification = a written OK
recorded in bd tuxlink-3gaz7 (`bd update tuxlink-3gaz7 --notes ...` by the
session, quoting the operator's words). After ratification: implementation
starts at plan Task 1; every task that consumes a frozen artifact cites the
file + MANIFEST digest it consumed.
