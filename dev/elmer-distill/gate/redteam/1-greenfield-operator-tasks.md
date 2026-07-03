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

1. Connect to the UV-Pro over Bluetooth SPP KISS. After 60 minutes, export a weather report from heard valid weather stations and post it to the outbox.

2. What is the station I'm most likely able to reach on 30m with a low-mounted dipole that's more than 500 miles away but less than 2000 miles away? What are the two runners-up stations? Formulate that list, surface it to me here, synthesize it as a report and post it to the outbox. If send authority is armed, send it via Telnet CMS to recipient@domain.com. Finally, if all else is successful, send an all-hands message via APRS that 30m outbound comms are established.

3. Using aggregated APRS weather data, identify any areas where the wind is gusting >25 mph and synthesize that into a report. Then, post it to the outbox and disseminate a character-limit-aware version over APRS.

3. Help me add a tactical identity. I'm not sure what one is or how it works.

4. I'm calling every station I can think of on ARDOP but I'm not getting through to anyone. I have a radio conencted over USB with a digirig. Help me troubleshoot it. Why am I not getting out?

5. Configure the station for Telnet Post Office reachable over the AREDN network, then send a message over tactical chat to all stations indicating that high speed AREDN Post Office functionality is up and ready for use.

6. Synthesize a 24-hour calling plan in 2-hour increments based on VARA-mode gateway stations I'm most likely able to reach with a low-mounted delta loop antenna with some NVIS characteristics on the WARC bands. Then, drive the connected HF radio to test that connectivity plan for the current corresponding 2-hour slot to confirm a propagation baseline against expectations. Keep driving through all stations until you connect with at least one, then synthesize an adjusted connectivity plan and post it to the outbox. If you have armed send authority, send it in a P2P session to N0RNG. Finally, send a confirmation message over Tactical Chat to all stations that an HF comms plan has been established, confirmed, and disseminated over P2P, being aware of character limits.
