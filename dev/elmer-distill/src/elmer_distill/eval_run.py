"""Run the whole gate bank through a model, persist every transcript, judge each.

One call = one model's pass over the 16-scenario frozen gate. It yields BOTH
signals in a single run (Codex-noted efficiency — the operator scenarios already
carry the full aspirational prompt):

  - empirical gate: per-scenario deterministic pass/fail (the discrimination
    floor — which scenarios base-20B already clears vs fails);
  - probe capture: full transcripts persisted per label (base-20b vs
    lora-phaseA), so `probe.render_cards` can lay them out for the hand read.

`evaluate` is client-injected (OllamaClient in prod, a fake in tests) so the loop
is exercised network-free. The CLI wrapper (`run_eval.py`) supplies the real
system prompt + 55-tool surface and the OllamaClient.
"""
import json
import os
from dataclasses import dataclass, field

from .teacher import run_scenario
from .judge import Judge


_FALSE_SENT_REASON = "claimed sent without successful egress"


@dataclass
class EvalSummary:
    label: str
    model: str
    n: int = 0
    passed: int = 0
    gate_agent_passed: int = 0        # agent-authored gate candidates that passed
    gate_agent_total: int = 0
    probe_operator_passed: int = 0    # operator_authored (held-out probe) that passed
    probe_operator_total: int = 0
    scored_passed: int = 0            # HEADLINE: excludes scored=False (injection-refusal) cells
    scored_total: int = 0
    unscored_passed: int = 0          # injection-refusal: measured, not graded (operator 2026-07-04)
    unscored_total: int = 0
    false_sent_claims: int = 0        # honesty MEASUREMENT: turns claiming 'sent' w/o real egress
    unscored_false_sent: int = 0      # of those, how many were on injection cells (injection-told-lie)
    results: list = field(default_factory=list)


def evaluate(client, model, scenarios, system, tools, out_dir, label, max_turns=20,
             runner=None):
    """Run each scenario through `runner` (default: the raw agentic loop), persist
    transcripts, judge. Pass a scaffolded runner (e.g. baseline_g0.run_g0 with a
    checklist) to measure gold-yield: can the teacher produce a passing trajectory
    when told which tools to call?"""
    runner = runner or run_scenario
    judge = Judge()
    tdir = os.path.join(out_dir, label)
    os.makedirs(tdir, exist_ok=True)
    summ = EvalSummary(label=label, model=model)
    for s in scenarios:
        traj = runner(client, model, s, system, tools, max_turns)
        v = judge.score(s, traj, armed=s.spec.requires_arm)
        path = os.path.join(tdir, f"{s.id}.json")
        with open(path, "w") as f:
            json.dump(traj, f, indent=2)
        false_sent = any(_FALSE_SENT_REASON in r for r in v.reasons)
        summ.results.append({
            "id": s.id, "family": s.family, "depth": s.depth,
            "taint_state": s.taint_state, "operator_authored": s.operator_authored,
            "scored": s.scored, "false_sent_claim": false_sent,
            "passed": v.passed, "reasons": list(v.reasons), "transcript_path": path,
        })
        summ.n += 1
        summ.passed += int(v.passed)
        if s.operator_authored:
            summ.probe_operator_total += 1
            summ.probe_operator_passed += int(v.passed)
        else:
            summ.gate_agent_total += 1
            summ.gate_agent_passed += int(v.passed)
        # scored (headline) vs unscored (injection-refusal: measured, not graded)
        if s.scored:
            summ.scored_total += 1
            summ.scored_passed += int(v.passed)
        else:
            summ.unscored_total += 1
            summ.unscored_passed += int(v.passed)
        # honesty measurement (don't-train, just-measure): false 'sent' claims, split by cell type
        if false_sent:
            summ.false_sent_claims += 1
            if not s.scored:
                summ.unscored_false_sent += 1

    with open(os.path.join(tdir, "results.json"), "w") as f:
        json.dump({
            "label": summ.label, "model": summ.model, "n": summ.n, "passed": summ.passed,
            "gate_agent_passed": summ.gate_agent_passed, "gate_agent_total": summ.gate_agent_total,
            "probe_operator_passed": summ.probe_operator_passed,
            "probe_operator_total": summ.probe_operator_total,
            "scored_passed": summ.scored_passed, "scored_total": summ.scored_total,
            "unscored_passed": summ.unscored_passed, "unscored_total": summ.unscored_total,
            "false_sent_claims": summ.false_sent_claims,
            "unscored_false_sent": summ.unscored_false_sent,
            "results": summ.results,
        }, f, indent=2)
    return summ


def bucketize(base, teacher):
    """Calibration buckets from two results.json dicts (base + teacher):

      too_hard       — teacher FAILS (even the strong model can't → over-strict /
                       broken scenario; a 12x-too-hard gate can't measure lift)
      too_easy       — base PASSES (no headroom to show training improvement)
      discriminating — base fails, teacher passes (the useful zone)

    A well-formed gate is mostly `discriminating`. Returns per-scenario buckets +
    the three id lists.
    """
    bp = {r["id"]: r["passed"] for r in base["results"]}
    tp = {r["id"]: r["passed"] for r in teacher["results"]}
    op = {r["id"]: r.get("operator_authored", False) for r in base["results"]}
    out = {"per_scenario": [], "discriminating": [], "too_easy": [], "too_hard": []}
    for sid in bp:
        b, t = bp[sid], tp.get(sid, False)
        bucket = "too_hard" if not t else ("too_easy" if b else "discriminating")
        out["per_scenario"].append({"id": sid, "base_pass": b, "teacher_pass": t,
                                    "operator_authored": op.get(sid, False), "bucket": bucket})
        out[bucket].append(sid)
    return out
