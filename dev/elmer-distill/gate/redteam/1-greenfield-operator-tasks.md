# Greenfield operator-authored hard tasks — FILL THIS FIRST

> **Anti-anchoring rule (Codex A/G + wire-walk gate).** Author these *before* you
> open `2-candidates-redteam.md`. If you read my 6 drafts first, your "own" tasks
> get anchored to my framing and stop being an independent signal. These are
> tagged `operator_authored: true` and are **never** selected by teacher-success
> during calibration — they exist specifically to surface the teacher's blind
> spots, which teacher-selected scenarios cannot.

## What I need from you

Your own genuinely-hard tasks from real operating experience — the ones where a
mediocre assistant would confidently do the wrong thing. Two flavors, both wanted:

1. **Emcomm / command-post** — activation, net control, logistics, multi-artifact
   under load, taint/refusal situations.
2. **Everyday Winlink-support frustration** — the help-desk stuff that actually
   wastes your time: config gotchas, "it says X but the real cause is Y", docs
   that don't match reality, CMS/gateway auth, form workflows.

Don't worry about JSON, tool names, or predicates — write them in plain English
below. I convert each to a schema-conformant `Scenario` (mapping the grading to
the real tool surface + evidence-bound predicates) and bring the mapping back to
you for a sanity check before it enters the bank.

## How many

As many as you have real conviction about. Even 4-8 strong ones materially change
the bank. I'll blend yours + the surviving drafts + synthesized variants to reach
the ~40 Stage-1 target, but **yours are the anchor set** — I fill *around* them,
never dilute them.

## Template — copy the block per task

```
### G-task <N>: <short title>

WHO/WHEN: <the operator role + situation; e.g. "net control, hour 6 of a Helene activation">

WHAT I ASK THE ASSISTANT (verbatim, as I'd actually type it):
<the prompt>

WHAT A GOOD ANSWER DOES:
<the artifact(s) / actions that count as success>

WHAT A PLAUSIBLE-BUT-WRONG ASSISTANT DOES (the trap):
<the confident wrong behavior a weak model falls into — this is the discriminating signal>

HARD BECAUSE:
<one line: why this separates a real command-post assistant from a fluent bluffer>

REALISM SOURCE (optional):
<real incident / WUG post / your own bench experience>
```

---

<!-- BEGIN YOUR TASKS BELOW THIS LINE -->

### G-task 1: <short title>

WHO/WHEN:

WHAT I ASK THE ASSISTANT (verbatim):

WHAT A GOOD ANSWER DOES:

WHAT A PLAUSIBLE-BUT-WRONG ASSISTANT DOES (the trap):

HARD BECAUSE:

REALISM SOURCE (optional):
