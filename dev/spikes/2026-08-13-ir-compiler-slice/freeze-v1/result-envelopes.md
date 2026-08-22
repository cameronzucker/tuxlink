# Frozen result and error envelopes — `routine_template_compile` (v1)

Every reachable result shape, field by field. Implementation serializes
exactly these field names; goldens assert them byte-for-byte. Design
authority: SS1 (result algebra), round-5 F2/F6, round-4 F1.

## The result object (returned on every non-envelope-error call)

| Field | Type | Present when | Meaning |
|---|---|---|---|
| `lowering` | `"ok"` \| `"failed"` | always | Did template+slots lower to a RoutineDef |
| `persistence` | `"not_saved"` \| `{"saved":{"revision":string}}` \| `{"save_refused":{"reason":string}}` | always | What happened to storage. Never anything else on `lowering:"failed"` than `"not_saved"` |
| `draft_validation` | `"valid"` \| `"advisory"` \| `"blocked"` \| `"n/a"` | always | Real-validator verdict on the lowered def; `"n/a"` exactly when `lowering:"failed"` |
| `submitted_slots` | object | always | The slot values as received (post one-parse absorption), verbatim |
| `normalized_slots` | object | `lowering:"ok"` only | Post-normalization values (`"2 hours"` -> `"2h"`); absent on refusal |
| `behavior_summary` | string | `lowering:"ok"` only | Deterministic compiler-rendered readback; echoes every free-text slot verbatim inside double quotes (the smuggling-visibility channel) |
| `findings` | array | always (empty allowed) | Compile refusal findings, validator findings, and save findings — ONE uniform name (design SS1 normalizes the findings/routine_findings drift) |
| `disposition` | object | only when a save was attempted (`save:true`) | The verbatim production `AuthoringDispositionDto`: `state` (`"valid"` \| `"invalid-agent-repairable"` \| `"saved-needs-operator"`), `agent_terminal`, `remedies[]`, `blocked_by[]`, `acceptable_warnings[]`, `advisories[]`, `completion?` |
| `completion` | string | always | The exact copy row for this state from `completion-copy.md` |

Null-noise discipline: absent fields are ABSENT, never null.

## The finding object

| Field | Type | Present when | Rule |
|---|---|---|---|
| `code` | string | always | From the frozen code table below, or a real validator code riding through |
| `slot` | string | the finding is slot-located | Names the slot verbatim — position IS the slot (design SS1) |
| `value` | string | an offending value exists | The offending value verbatim |
| `rule` | string | always | The law, stated ("bands order is meaning; repeats are not allowed") |
| `remedy` | string | blocking AND agent-fixable ONLY | Op-shaped ("call routine_template_compile again with slots.every set to a single duration"). Never on warnings (ADR 0025 amendment), never on operator-only or environmental findings |
| `fault` | string | environmental classes only | Explicit attribution: "environment - not your call's fault" |

Co-firing findings carry equal-strength anchors and cross-reference each
other by code, or are merged (design SS1).

## Frozen compile-refusal codes (`lowering:"failed"`, nothing mutated)

| Code | Fires when | Remedy present |
|---|---|---|
| `TEMPLATE_UNKNOWN` | template id not in the registry; rule text enumerates the three valid ids | yes |
| `SLOT_UNKNOWN` | slot name not declared by this template; rule enumerates the template's slots | yes |
| `SLOT_MISSING` | a required slot absent (D1: `every`/`window` where declared nullable may be omitted = null) | yes |
| `SLOT_NOT_A_DURATION` | full-input consumption failed ("15 minutes then retry"), unknown unit ("2 days" — D3), non-integer, or empty | yes |
| `DURATION_OUT_OF_RANGE` | computed seconds outside 1s..30d (zero, negative, overflow — checked arithmetic) | yes |
| `SLOT_NOT_A_WINDOW` | not `HH:MM-HH:MM` with hours 0-23 / minutes 0-59 | yes |
| `WINDOW_ENDPOINTS_EQUAL` | start == end (D4; overnight start > end is VALID) | yes |
| `WINDOW_WITHOUT_SCHEDULE` | window non-null while every is null (unrepresentable on Manual) | yes |
| `BAND_UNKNOWN` | label not in {160m 80m 60m 40m 30m 20m 17m 15m 12m 10m} after alias normalization | yes |
| `BAND_DUPLICATE` | same canonical band twice (closes retry-laundering, round-5 F5) | yes |
| `BANDS_EMPTY` | empty bands list (empty selects packet dialing; `$band` would be unresolvable) | yes |
| `NAME_INVALID` | name fails `^[a-z0-9]+(-[a-z0-9]+)*$` or length 1..48 (D8) | yes |
| `SLOT_TOKEN_UNAVAILABLE` | `$station`/`$band` in `failure_log`/`fail_reason` (D9; failed connect exposes only connected:false + last_error) | yes |
| `SLOT_TOKEN_UNKNOWN` | any other `$word` token in the primary's three free-text slots (D9) | yes |

Validator findings (e.g. unresolved station set) are NOT compile refusals:
they arrive with `lowering:"ok"`, `draft_validation:"blocked"|"advisory"`,
riding the same `findings` array with their existing production codes —
one refusal grammar end to end (design §2, post-compile validation).

## Frozen save-path codes

| Code | State | Notes |
|---|---|---|
| `NAME_EXISTS_CREATE_ONLY` | `save_refused` | CreateOnly collision under the authoring lock; copy = the ask-the-user row; bytes+revision untouched (regression-tested) |
| `STORE_IO_ERROR` | pinned error state | `fault` attribution present; exact copy in the table |
| `AUTHORING_LOCK_UNAVAILABLE` | pinned error state | `fault` attribution present; exact copy in the table |

## Envelope HARD errors (tool error, no result object; nothing processed)

Error text form, frozen:
`[<CODE>] <one sentence naming the offending key or path and the law>. The call was not processed; nothing changed. Resend one corrected call.`

| Code | Fires when |
|---|---|
| `MALFORMED_JSON` | arguments not parseable at all (names the parse location — no key exists) |
| `MISSING_REQUIRED_KEY` | `template` or `slots` absent |
| `UNDECLARED_KEY` | any top-level key besides `template`/`slots`/`save` |
| `SLOTS_NOT_OBJECT` | `slots` not an object AFTER the one-parse absorption boundary (includes a stringified slots value whose embedded JSON is malformed — D2) |
| `SLOT_VALUE_NOT_SCALAR` | nested object or array in a slot (names `slots.<name>`); `bands` excepted |
| `SLOT_WRONG_LEAF_TYPE` | number or boolean leaf (all declared scalar slots are string-or-null in v0; names the path) |
| `BANDS_NOT_STRINGS` | non-string item inside `bands` |

Absorption policy, decided explicitly (design §1): a WELL-FORMED stringified
`slots` value is absorbed by the existing one-parse boundary and is NOT an
error; the raw emission is retained in telemetry and raw shape is graded
separately from post-absorption success.

## Frozen behavior_summary sentence forms

Free-text slot values are echoed verbatim inside double quotes. `{x}` are
normalized-slot holes; `{, between A and B,}` renders only when window is
non-null.

- primary, scheduled: `Every {every}{, between {start} and {end},} attempt stations in '{stations}', trying each station on {bands joined " then "}. On the first successful connect, log: "{success_log}". If every attempt fails, log: "{failure_log}" and end the run failed ({fail_reason}).`
- primary, manual (every null): `On manual run, attempt stations in '{stations}', trying each station on {bands joined " then "}. On the first successful connect, log: "{success_log}". If every attempt fails, log: "{failure_log}" and end the run failed ({fail_reason}).`
- aprs: `Every {every}{, between {start} and {end},} transmit one APRS broadcast: "{message}".`
- log-entry: `Every {every}, write one log line: "{note}".`

## Exemplar envelopes (informative; goldens pin the byte-exact versions)

compiled-valid (the wa-gateway-check worked example):

```json
{
  "lowering": "ok",
  "persistence": "not_saved",
  "draft_validation": "valid",
  "submitted_slots": { "name": "wa-gateway-check", "every": "2h", "window": "08:00-18:00", "stations": "wa-gateways", "bands": ["40m", "80m"], "success_log": "Connected to $station on $band", "failure_log": "All WA gateways unreachable this cycle", "fail_reason": "no gateway reachable" },
  "normalized_slots": { "name": "wa-gateway-check", "every": "2h", "window": "08:00-18:00", "stations": "wa-gateways", "bands": ["40m", "80m"], "success_log": "Connected to $station on $band", "failure_log": "All WA gateways unreachable this cycle", "fail_reason": "no gateway reachable" },
  "behavior_summary": "Every 2h, between 08:00 and 18:00, attempt stations in 'wa-gateways', trying each station on 40m then 80m. On the first successful connect, log: \"Connected to $station on $band\". If every attempt fails, log: \"All WA gateways unreachable this cycle\" and end the run failed (no gateway reachable).",
  "findings": [],
  "completion": "<compiled-valid row from completion-copy.md>"
}
```

refused (the worked refusal):

```json
{
  "lowering": "failed",
  "persistence": "not_saved",
  "draft_validation": "n/a",
  "submitted_slots": { "name": "valley-check", "every": "15 minutes then retry", "window": null, "stations": "wa-gateways", "bands": ["40m"], "success_log": "connected $station $band", "failure_log": "no valley luck", "fail_reason": "unreachable" },
  "findings": [{
    "code": "SLOT_NOT_A_DURATION",
    "slot": "every",
    "value": "15 minutes then retry",
    "rule": "every takes a single duration like '15m' or '2h'; extra words are not part of a duration",
    "remedy": "call routine_template_compile again with slots.every set to just the interval, e.g. \"15m\""
  }],
  "completion": "<refused row from completion-copy.md>"
}
```

saved-valid: as compiled-valid plus `"persistence": {"saved": {"revision": "<sha256-prefix>"}}` and the verbatim `disposition` object (`state: "valid"`, empty lists, production completion sentence).

save_refused: `lowering:"ok"`, `draft_validation:"valid"`, `persistence: {"save_refused": {"reason": "NAME_EXISTS_CREATE_ONLY"}}`, one `NAME_EXISTS_CREATE_ONLY` finding, the ask-the-user completion row.

compiled-blocked (RS-blocked cells): `lowering:"ok"`, `persistence:"not_saved"`, `draft_validation:"blocked"`, findings = the real validator's unresolved-station-set finding riding through, the compiled-blocked completion row.
