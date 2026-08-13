# Spike IR — the whole contract on one page

**Status: DISPOSABLE draft for the spike (tuxlink-s3h20). Syntax is not frozen.
This page is the operator's judgment artifact: if it stops fitting on one page,
that is a finding.**

The model never edits steps, ids, or wiring. It states the whole routine it
wants, small enough to re-emit entirely on every edit — "change X" means
"here is the routine again, with X different." A deterministic compiler we own
expands this into the real `RoutineDef`; the existing validator and executor
run the result unchanged.

## The five constructs

```json
{
  "routine": "nearest-40m-dial",
  "every": "15m",
  "window": "07:00-08:00",
  "do": [
    { "connect": { "stations": "@station-set:or-gateways", "bands": ["40m", "80m"] },
      "on_success": [
        { "log": "Connected on $band to $station" }
      ],
      "on_failure": [
        { "log": "No gateway reached on any band" },
        { "end": { "failed": true, "reason": "no gateway" } }
      ]
    }
  ]
}
```

1. **`every` / `window`** — the schedule, as written. Omit `every` for a
   manual-only routine. (`if_missed` defaults to skip; sayable if needed.)
2. **`connect`** — stations + bands, in fallback order.
3. **`on_success` / `on_failure`** — blocks that CONTAIN steps. This is the
   whole point: structure is nesting, never a jump. No ids exist anywhere.
4. **`log`** — a text line; `$band`, `$station`, `$gateway` name the
   preceding connect's outputs, by their catalog names.
5. **`end`** — stop here; optionally failed, with a reason.

## The rules (all of them)

- **Blocks contain steps.** Nothing references anything by id. Gotos are
  unexpressable in this language.
- **Whole-routine emission, always.** Edits are re-statements. Placement
  semantics, step ids, and revision surgery do not exist at this layer
  (the save layer keeps CAS/revision underneath, unchanged).
- **Lenient in syntax, strict in meaning.** The compiler absorbs harmless
  spelling (a bare string where a one-key object is obvious) but NEVER
  invents fields, identifiers, permissions, or control flow. Anything it
  does not recognize is a named, positioned refusal — never a guess.
- **Every compile echoes.** The compiler returns the plain-language readback
  of what it built (the renderer already exists). Silent interpretation is
  prohibited; the echo is the interpretation.
- **Consent is untouched.** Nothing in this language can express transmit
  mode, acknowledgments, or authority. Those remain operator acts in the
  layers that own them.

## What the spike deliberately leaves out

Branch-on-value comparisons, delay, retry, call/sub-routines, forms/compose,
multi-track. Each is a later construct IF the premise survives. The premise
under test: **a small model that failed step-surgery can state one of these
correctly.**
