# Skeleton instantiation rules (v1)

The independent grader (design §4, grader independence) instantiates a
skeleton with a run's accepted slot values and requires STRUCTURAL EQUALITY
(serde-value equality, not string equality) with the compiler's emitted
RoutineDef. The skeletons are authored here, by hand, from the design's §2
lowering text — never derived from compiler code. Any disagreement between
an instantiated skeleton and the compiler's output on a matrix run is
INSTRUMENT_INVALID: quarantine, repair, rerun.

## Hole rules

| Hole | Source | Transform |
|---|---|---|
| `{{name}}` | normalized `name` | none |
| `{{stations}}` | normalized `stations` | none |
| `{{bands}}` | normalized `bands` | replaced as a JSON ARRAY value (the hole string is a stand-in) |
| `{{success_log_rewritten}}` | normalized `success_log` | `$station` -> `$s1.station`, `$band` -> `$s1.band` (exact token replacement, D9) |
| `{{failure_log}}` | normalized `failure_log` | none (tokens are refused pre-lowering, D9) |
| `{{fail_reason}}` | normalized `fail_reason` | none |
| `{{message}}` | normalized `message` | none (no token language) |
| `{{note}}` | normalized `note` | none (no token language) |
| `{{triggers}}` | normalized `every` + `window` | replaced as a JSON ARRAY per the trigger forms below |

## Frozen trigger forms

- `every` non-null, `window` non-null:
  `[{ "type": "schedule", "every": "<every>", "window": "<window>", "if_missed": "skip" }]`
- `every` non-null, `window` null/omitted:
  `[{ "type": "schedule", "every": "<every>", "if_missed": "skip" }]`
  (`window: None` and `align: None` do not serialize — types.rs skip rules)
- `every` null/omitted, `window` null/omitted (primary template only):
  `[{ "type": "manual" }]`
- `every` null + `window` non-null: NOT INSTANTIABLE — the compiler refuses
  (`WINDOW_WITHOUT_SCHEDULE`); no skeleton form exists on purpose.

## Two-terminal-ends invariant (round-5 F1)

The primary skeleton's branch arms end in DIFFERENT end steps: `then`
reaches `e1` (`failed: false`), `else` reaches `e2` (`failed: true` +
`reason`). Golden execution tests (plan Task 5) prove `connected: true`
reaches only e1 and `connected: false` only e2 — the engine jumps to an
arm's first id and continues linearly, so a shared end cannot terminate
both paths with different outcomes.
