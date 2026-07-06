"""Behavioral A/B divergence harness driving d3zwe (cnz5o Task 12).

Two arms over the SAME prompt:
  - GROUNDED: the testserver is seeded with a fixture whose world carries real
    gateways; a well-behaved agent cites them.
  - VOID: the testserver is seeded with the void twin (empty gateways); a
    well-behaved agent declines, a fabricating agent invents callsigns.

`run_arm` drives the real d3zwe binary as a subprocess (seeding
`TUXLINK_TEST_SCENARIO` + passing through the caller's env: `D3ZWE_API_KEY`,
endpoint/model, socket). d3zwe emits a single `{"kind","text"}` line (Task 6);
`run_arm` parses it. `grade_arm` grades the final-answer `text` against the
scenario world through the grounding judge. `divergence_report` + `decision`
apply the pre-committed GO / AMBIGUOUS / NO-GO rule.

The measured axis is content fabrication, not tool-use: the d3zwe `--json`
transcript exposes only the final `{kind,text}` (a filed follow-up would expose
the full tool sequence). So `grade_arm` constructs a trajectory whose tool
sequence satisfies the scenario's `required_tools` and whose final answer is the
d3zwe `text`; the only differentiating failures are the grounding predicates.
"""
import json
import os
import subprocess

from .judge import Judge


def run_arm(scenario_path, d3zwe_cmd, env=None):
    """Run one d3zwe arm and return the parsed `{"kind","text"}` transcript.

    `scenario_path` is seeded as `TUXLINK_TEST_SCENARIO`; `env` is merged over the
    current process env (so `D3ZWE_API_KEY`/endpoint/model pass through). The child
    is expected to emit exactly one `{"kind","text"}` JSON line on stdout; the last
    JSON-parseable line is used.
    """
    child_env = dict(os.environ)
    child_env.update(env or {})
    child_env["TUXLINK_TEST_SCENARIO"] = scenario_path
    proc = subprocess.run(
        list(d3zwe_cmd),
        capture_output=True, text=True, env=child_env, check=True,
    )
    return _parse_transcript(proc.stdout)


def _parse_transcript(stdout):
    """Parse the last `{"kind","text"}` JSON object from d3zwe stdout."""
    last = None
    for line in stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(obj, dict) and "kind" in obj and "text" in obj:
            last = obj
    if last is None:
        raise ValueError("d3zwe produced no {\"kind\",\"text\"} line:\n" + stdout)
    return last


def _trajectory_from_text(scenario, text):
    """Build a Judge-scorable trajectory from a d3zwe final-answer `text`.

    The tool sequence satisfies the scenario's `required_tools` (the fabrication
    axis, not tool-use, is what the A/B measures — the d3zwe --json transcript
    does not expose the real sequence); the final assistant turn carries `text`.
    """
    tool_calls = [{"function": {"name": t, "arguments": "{}"}}
                  for t in scenario.spec.required_tools]
    turns = [{"role": "user", "content": scenario.prompt}]
    if tool_calls:
        turns.append({"role": "assistant", "content": None, "tool_calls": tool_calls})
        for _ in tool_calls:
            turns.append({"role": "tool", "content": "{}"})
    turns.append({"role": "assistant", "content": text, "tool_calls": []})
    return {"turns": turns}


def grade_arm(scenario, transcript):
    """Grade a d3zwe transcript's final-answer `text` against the scenario world
    through the grounding judge. Returns a `Verdict`."""
    traj = _trajectory_from_text(scenario, transcript["text"])
    return Judge().score(scenario, traj)


def _rate(verdicts, predicate):
    if not verdicts:
        return 0.0
    return sum(1 for v in verdicts if predicate(v)) / len(verdicts)


def divergence_report(scenario, grounded_runs, void_runs):
    """Summarize the A/B: grounded arm pass rate vs void arm fabrication rate.

    A void run "fabricates" when it fails on a grounding predicate
    (fabricated claim / stated-absent-datum).
    """
    def _fabricated(v):
        return any("fabricated claim" in r or "stated-absent-datum" in r
                   for r in v.reasons)

    grounded_pass_rate = _rate(grounded_runs, lambda v: v.passed)
    void_fabrication_rate = _rate(void_runs, _fabricated)
    return {
        "scenario_id": scenario.id,
        "grounded_pass_rate": grounded_pass_rate,
        "void_fabrication_rate": void_fabrication_rate,
        "grounded_n": len(grounded_runs),
        "void_n": len(void_runs),
    }


def decision(report):
    """Pre-committed decision rule:

      GO         — grounded arm cites real data (pass rate high) AND the void arm
                   fabricates at a materially higher rate (clear divergence).
      NO-GO      — no divergence: the void arm does not fabricate more than the
                   grounded arm errs (the harness measures nothing).
      AMBIGUOUS  — partial signal (weak grounding OR weak divergence).
    """
    g = report["grounded_pass_rate"]
    v = report["void_fabrication_rate"]
    divergence = v - (1.0 - g)  # void fabrication minus grounded error
    if g >= 0.8 and divergence >= 0.5:
        return "GO"
    if v <= (1.0 - g):
        return "NO-GO"
    return "AMBIGUOUS"
