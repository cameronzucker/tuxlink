# Choosing a Distillation Teacher for a Self-Hostable Radio Agent

*Notes from Tuxlink's Elmer distillation work, 2026-07-05.*

Tuxlink ships an in-application AI assistant, **Elmer**, that helps a licensed
operator run a Winlink and amateur-radio station: find gateways, predict HF
paths, compose and stage traffic, read station health. The aim for Elmer is a
model that is **capable, grounded, honest, and self-hostable** — one that runs on
commodity or 128 GB-class hardware without shipping the operator's data to a
hosted API.

The path to that model is distillation: take a strong teacher model, have it
solve realistic station tasks, and train a smaller student on the results. This
note records how the teacher was selected, and why the most useful result was
not the teacher at all.

## The evaluation

Teacher selection ran against a **discriminating gate**: sixteen hard,
deliberately un-memorizable agentic scenarios drawn from real emergency-comms
and station workflows (rank the closest VARA gateways and stage a report,
diagnose a failing ARDOP link, refuse a prompt-injected inbox message, build a
24-hour rotating contact plan). The station directory in each scenario is
synthesized per-run, so a model cannot recall an answer; grounded tool use is
the only way through.

Candidates ran through the identical battery via OpenRouter:

- **Qwen3.5-122B-A10B** — the intended student.
- **Qwen3.5-397B-A17B** and **Qwen3-235B-A22B** — teacher candidates.
- **gpt-oss-120B** — a control, and the model an earlier distillation attempt
  had stalled on.

## Finding 1: the family choice was correct

On the full 55-tool surface, the Qwen student cleared roughly three times as
many scenarios as the gpt-oss control. That settled an open question: the
in-family Qwen direction has real headroom where the previous track did not.

## Finding 2: a binary gate cannot pick a teacher

The binary pass/fail score **saturated**. On the same battery, the 122B, the
235B, and the 397B all clustered together — a metric that cannot separate a
122-billion-parameter model from a 397-billion-parameter one has no resolution
left to rank teachers.

The reason is structural. A pass/fail gate is a **floor test** ("did the
trajectory clear every strict predicate?"), not a **quality meter**. It collapses
"completed the task and missed one formatting check" and "stalled in a confused
loop" into the same `FAIL`. For shipping a student against a fixed bar the floor
test is correct. For comparing teachers it is the wrong instrument.

## Finding 3: the gap is multi-turn completion

Reading the transcripts directly told the real story. Where the binary gate saw
a tie, the teachers were **completing multi-step tasks that the student stalled
on** — driving a VARA connection and staging a plan end to end, versus looping
on an ambiguous tool result and giving up.

This matches the published benchmark picture. On single-call function-calling
(BFCL-style), the student and the teachers score nearly the same. The separation
appears on **multi-turn agentic** benchmarks (the TAU2 / Tool-Agent-User family),
which is exactly the axis Elmer's work lives on and exactly the axis a binary
gate is blind to.

## Finding 4: the teacher is the one that fits on the desk

Graded quality scoring put the 235B and the 397B in a statistical tie. That
turns the decision on the other axes, and there the 235B wins cleanly:

- **Self-hostable.** Total parameters set the memory floor. A 235B model at
  4-bit is roughly 117 GB and fits a 128 GB machine with room for context; a
  397B model is roughly 199 GB and does not fit at all. The 235B is the largest
  teacher that self-hosts, which unlocks unlimited local data generation instead
  of perpetual API rent.
- **Open weights**, and it activates more parameters per token (22B) than the
  397B (17B).

An equally-good teacher that also runs locally is not a tie; it is the choice.

## Finding 5: honesty, and a check on the easy conclusion

An early read suggested the student was *more honest* than its teachers: it
never falsely claimed a message was transmitted, while both teachers occasionally
narrated a staged draft or a mock connection as a completed send.

That conclusion did not survive scrutiny. On the hardest send scenario the
student's clean record was not honesty — it **stalled before reaching the send**,
and a task never attempted cannot be falsely claimed. On the routine send tasks
it *did* complete, all three models were comparably honest. The corrected reading
is sharper: false-sent is a **hard-chain, completion-coupled** failure. Teaching
a student to complete more chains therefore *increases* its exposure to that
failure, which is why generated training data must be filtered to reject
false-sent and fabricated-tool-data trajectories rather than copied wholesale.

## Finding 6: the environment was the problem, not the models

The most useful result concerned none of the models. Across every candidate, the
dominant failure mode was the same, and it traced to the **evaluation
environment**: several simulated tools returned empty `{ok: true}` stubs instead
of realistic data. Faced with that void, weaker models looped and stalled;
stronger models fabricated plausible values (invented solar indices, grids,
reliability percentages) to fill the gap.

A large share of the apparent capability gap, and nearly all of the fabrication,
was the simulator not returning real data — not the models being incapable. This
points at the next piece of architecture: rather than maintain a separate
simulator that must be kept in parity with the real application, make the real
application serve as the evaluation environment, with scenario-driven state
injected at its tool boundary. Parity then holds by construction, and the same
scenario format serves training, per-build regression testing, and end-to-end
reproduction of field bug reports.

## Method notes

The transferable lessons had little to do with radios:

- **Measure, do not argue.** A design debate about tool-surface size and teacher
  strength was settled in an afternoon by running both against a gate that
  already existed, for the price of some API calls rather than a week of
  speculation.
- **Quant and qual together.** The binary metric and the read-the-transcript
  judgment disagreed, and the disagreement was the signal — it located the exact
  axis the cheap metric could not see.
- **Adversarially verify, and check the confound.** "The student is more honest"
  and "the sim has no haversine" both looked true and both needed a second look;
  "did it just not get that far?" was the question that corrected the honesty
  reading.

## Status

Elmer and its distillation pipeline are early and under active development.
Tuxlink is in alpha and looking for testers. The teacher is selected
(235B → 122B); the next work is making the evaluation environment return real
data so the generated training gold teaches grounded, honest behavior rather than
fabrication into the gaps.
