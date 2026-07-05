# Choosing a Distillation Teacher for a Self-Hostable Radio Agent

*Notes from Tuxlink's Elmer distillation work, 2026-07-05. A full account of one
afternoon's teacher-selection experiment, including the wrong turns.*

Tuxlink ships an in-application AI assistant, **Elmer**, that helps a licensed
operator run a Winlink and amateur-radio station: find gateways, predict HF
paths, compose and stage traffic, read station health. The aim for Elmer is a
model that is **capable, grounded, honest, and self-hostable** — one that runs on
commodity or 128 GB-class hardware without shipping the operator's data to a
hosted API.

The path to that model is distillation: take a strong teacher model, have it
solve realistic station tasks, and train a smaller student on the results. This
note records how the teacher was selected, the two experiments that produced
non-obvious answers, and why the most useful result was not the teacher at all.

## The evaluation

Teacher selection ran against a **discriminating gate**: sixteen hard,
deliberately un-memorizable agentic scenarios drawn from real emergency-comms
and station workflows — rank the closest VARA gateways and stage a report,
diagnose a failing ARDOP link, refuse a prompt-injected inbox message, build a
24-hour rotating contact plan, drive a VARA peer-to-peer session and confirm it.
The station directory in each scenario is synthesized per run, so a model cannot
recall an answer from training; grounded tool use is the only way through.

Candidates ran through the identical battery via OpenRouter:

- **Qwen3.5-122B-A10B** — the intended student.
- **Qwen3.5-397B-A17B** and **Qwen3-235B-A22B** — teacher candidates.
- **gpt-oss-120B** — a control, and the model an earlier distillation attempt had
  stalled on.

Every model saw the same 55-tool surface and the same system prompt. Scoring used
two instruments deliberately: the gate's **binary pass/fail**, and a **quality
rubric** applied to the full transcripts (completion, grounding, tool-use,
honesty; 0-2 each).

## Question 1: which model family?

On the binary gate, the Qwen student cleared **6 of 14** scored scenarios; the
gpt-oss control cleared **2 of 14**. The difference showed up mechanically in the
transcripts: across the battery the Qwen model issued **257 tool calls**, the
gpt-oss model **104**. The gpt-oss failure mode was *under-engagement* — it
reached for tools less, and answered from assumption more.

That settled a standing question. An earlier gpt-oss distillation track had
plateaued because the teacher was no better than the base at this task. The Qwen
family does not share that ceiling, so the family direction was confirmed before
any teacher was chosen.

## Question 2: does a smaller tool surface help a small model?

Before comparing teachers, a tempting optimization was tested: the Elmer tool
surface is large (dozens of tools, several thousand tokens of schema on every
call), and a smaller model might select better from a shorter menu. The idea —
progressive tool disclosure, revealing tools on demand rather than all at once —
was worth a measurement before any engineering.

The measurement was cheap: run the student against the battery twice, once with
all 55 tools and once with only the 24 the scenarios actually require. The result
argued against the idea. Pruning the surface **did not help the student and
appeared to hurt it** (6 of 14 fell to 2 of 14), and the failures were genuine —
the needed tools were present and the model simply called them less (its tool
calls dropped from 257 to 159 with the shorter menu). The control model moved by
one scenario, within noise.

The reasoning that explained it is worth keeping: progressive disclosure is
**obviously correct for a frontier model and quietly inverts for a small one.**
Disclosure asks the model to notice it needs a capability it cannot see, resist
answering without it, and go find it — a meta-cognitive step. That step is
precisely the one small models are worst at. The optimization that helps the
strong model *removes* the tool the weak model would otherwise have stumbled into.
Progressive disclosure was shelved, and the tool surface left intact.

## Question 3: which teacher? The binary gate could not say

The teacher comparison broke the binary gate. On the same battery the 122B, the
235B, and the 397B **all clustered at 5-6 of 14**. A metric that cannot separate a
122-billion-parameter model from a 397-billion-parameter one has no resolution
left to rank teachers.

The reason is structural. A pass/fail gate is a **floor test** — did the
trajectory clear every strict predicate? — not a **quality meter**. It collapses
"completed the task and missed one formatting check" and "stalled in a confused
loop" into the same `FAIL`. For shipping a student against a fixed bar the floor
test is correct. For comparing teachers it is the wrong instrument.

Reading the transcripts directly produced the ranking the gate could not. Three
scenarios show the difference concretely.

**Drive a VARA peer-to-peer session and confirm a plan** (`warc-vara`). All three
models scored `FAIL`. The 122B student, after 26 tool calls, wrote *"I
successfully connected to N0RNG via VARA in P2P mode. Now I need to send the
staged message. However, I don't see a direct tool to send a staged message via
an established connection..."* — and stalled there, never finishing. The 235B
teacher completed the whole mission (synthesized the plan, drove the connection,
staged, reported) and its transcript actually **passed** the gate. The 397B also
completed it and missed one predicate. Same binary `FAIL` on the student and the
397B; opposite trajectories.

**Troubleshoot a rejected CMS password** (`helpdesk-cms-password`). The student
issued the **same three tool calls 52 times** against empty `{ok:true}`
responses, then gave up with no answer. The 235B reached a clean, useful
troubleshooting answer in **9 calls**; the 397B in 12.

**Fix a modem and send a priority message** (`blended-fix-and-send`). The 397B
thrashed for 34 calls against empty status reads and then handed the task back to
the operator for the message body. The 235B stayed autonomous, produced a
grounded band analysis, and staged in 17 calls.

The quality rubric put numbers on it: **397B 6.06/8, 235B 5.81/8, 122B 5.38/8.**
The two teachers are a statistical tie (scored by separate graders, a quarter of
a point apart). The student trails on **completion and tool-use** — it loops and
stalls where the teachers finish — which is exactly the gap distillation is meant
to close.

## The teacher decision: the one that fits on the desk

With the teachers tied on quality, the decision turned on the other axes, and
there the 235B wins cleanly:

- **Self-hostable.** Total parameters set the memory floor. A 235B model at 4-bit
  is roughly 117 GB and fits a 128 GB machine with room for context; a 397B model
  is roughly 199 GB and does not fit at all. The 235B is the largest teacher that
  self-hosts — which unlocks unlimited **local** data generation instead of
  perpetual API rent, and a fully local training pipeline.
- **Open weights**, and it activates more parameters per token (22B vs 17B).
- **More efficient trajectories.** The 235B consistently reached answers in fewer
  tool calls than the 397B (9 vs 12, 17 vs 34). Concise, correct trajectories are
  better *teaching* material than verbose ones — the student learns to complete
  efficiently rather than to thrash.

An equally-good teacher that also runs on the desk is not a tie; it is the choice.
**235B → 122B** is the selected pairing.

## A larger model that did not help: the thinking variants

Reasoning ("thinking") variants of the candidates were on the table, being both
larger in effective compute and intuitively better suited to multi-step tasks.
They were dropped. A verified survey of the published benchmarks found the
thinking variants beat their instruct counterparts by roughly **one point** on
function-calling — within noise — and the stronger claims of a large
thinking-model advantage did not survive adversarial checking. The thinking runs
are also markedly slower and more expensive to generate. Larger, in this case, was
the wrong tool for the job, and the instruct variants carry the same signal for
less.

## A check on the easy conclusion: honesty

An early read suggested the student was *more honest* than its teachers: it never
falsely claimed a message was transmitted, while both teachers occasionally
narrated a staged draft or a mock connection as a completed send.

That conclusion did not survive scrutiny. On the hardest send scenario the
student's clean record was not honesty — it **stalled before reaching the send**,
and a task never attempted cannot be falsely claimed. On the routine send tasks it
*did* complete, all three models were comparably honest. The corrected reading is
sharper: false-sent is a **hard-chain, completion-coupled** failure. Teaching a
student to complete more chains therefore *increases* its exposure to that
failure, which is why generated training data must be **filtered** to reject
false-sent and fabricated-tool-data trajectories rather than copied wholesale.

## The finding that mattered most: the environment, not the models

The most useful result concerned none of the candidates. Across every model, the
dominant failure mode was the same, and it traced to the **evaluation
environment**: several simulated tools returned empty `{ok: true}` stubs instead
of realistic data. Faced with that void, weaker models looped and stalled
(the 52-call password loop); stronger models fabricated plausible values —
invented solar indices, grid squares, reliability percentages — to fill the gap.

A large share of the apparent capability gap, and nearly all of the fabrication,
was the simulator not returning real data, not the models being incapable. That
points at the next piece of architecture: rather than maintain a separate
simulator that must be kept in parity with the real application, make the real
application *be* the evaluation environment, with scenario-driven state injected
at its tool boundary. Parity then holds by construction, and the same scenario
format serves training, per-build regression testing, and end-to-end reproduction
of field bug reports.

## Method notes

The transferable lessons had little to do with radios:

- **Measure, do not argue.** Two design debates — tool-surface size, and teacher
  strength — were each settled in an afternoon by running against a gate that
  already existed, for the price of some API calls rather than a week of
  speculation.
- **Quant and qual together.** The binary metric and the read-the-transcript
  judgment disagreed, and the disagreement *was* the signal — it located the exact
  axis (multi-turn completion) the cheap metric could not see.
- **Check the confound.** "The student is more honest" looked true and was mostly
  an artifact of the student stalling before it could fail. "Did it just not get
  that far?" was the question that corrected it. The same discipline caught the
  fabrication-versus-capability confound in the environment.

## Status

Elmer and its distillation pipeline are early and under active development.
Tuxlink is in alpha and looking for testers. The teacher is selected (235B → 122B);
the next work is making the evaluation environment return real data so the
generated training set teaches grounded, honest behavior rather than fabrication
into the gaps.
