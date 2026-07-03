"""G0 — the prompt-only null gate (no training).

Runs the base student (gpt-oss:20b) with three cheap interventions and scores
the result with the Judge. If G0 clears the pre-registered bar, the stall is a
prompting problem, not a weights problem, and we ship the scaffold instead of
fine-tuning (Codex adrev A/G):

  1. few-shot Harmony exemplars prepended (correct long-horizon demonstrations)
  2. a task checklist injected into the system message
  3. a verifier loop: when the model emits a final answer, re-check required
     tools + staged items via the Judge; if unmet and re-prompts remain, inject
     a corrective user turn and continue.
"""
import json

from .simulator import StatefulSimulator
from .judge import Judge


def _as_dict(args):
    if isinstance(args, str):
        try:
            return json.loads(args)
        except Exception:
            return {}
    return args or {}


def _checklist(scenario):
    lines = [f"- call {t}" for t in scenario.spec.required_tools]
    for item in scenario.spec.staged:
        lines.append(f"- stage a {item.tool} containing {item.must_contain}"
                     + (f" addressed to {item.to}" if item.to else ""))
    return "\n".join(lines)


def _unmet(judge, scenario, traj):
    """Requirements not yet satisfied (required tools + staged only; not order/stall)."""
    verdict = judge.score(scenario, traj, armed=scenario.spec.requires_arm)
    return [r for r in verdict.reasons
            if r.startswith("missing required tool") or r.startswith("unstaged")]


def run_g0(client, model, scenario, system, tools, exemplars, max_reprompts=2, max_turns=40):
    sys = system + "\n\nTASK CHECKLIST (complete ALL before you finish):\n" + _checklist(scenario)
    messages = [{"role": "system", "content": sys}]
    messages.extend(exemplars)   # few-shot Harmony exemplars as prior messages
    messages.append({"role": "user", "content": scenario.prompt})
    turns = [{"role": "user", "content": scenario.prompt}]

    sim = StatefulSimulator(armed=scenario.spec.requires_arm)
    if scenario.taint_state == "pre_tainted":
        sim.tainted = True

    judge = Judge()
    reprompts = 0

    for _ in range(max_turns):
        d = client.chat(model, messages, tools)   # client governs sampling (best-of-N: temp>0 + seed)
        msg = d.get("message", {}) or {}
        thinking = msg.get("thinking") or ""
        content = msg.get("content") or ""
        tool_calls = msg.get("tool_calls") or []

        messages.append({
            "role": "assistant",
            "content": content,
            **({"tool_calls": tool_calls} if tool_calls else {}),
            **({"thinking": thinking} if thinking else {}),
        })
        turns.append({"role": "assistant", "thinking": thinking,
                      "content": content, "tool_calls": tool_calls})

        if tool_calls:
            for tc in tool_calls:
                name = tc["function"]["name"]
                args = _as_dict(tc["function"].get("arguments"))
                result = sim.apply(name, args)
                messages.append({"role": "tool", "tool_name": name, "content": json.dumps(result)})
                turns.append({"role": "tool", "tool_name": name, "content": json.dumps(result)})
            continue

        # model emitted a final — verify the checklist
        unmet = _unmet(judge, scenario, {"turns": turns})
        if unmet and reprompts < max_reprompts:
            reprompts += 1
            corrective = ("You have not yet: " + "; ".join(unmet)
                          + ". Continue and complete these before finishing.")
            messages.append({"role": "user", "content": corrective})
            turns.append({"role": "user", "content": corrective})
            continue
        break

    return {"scenario_id": scenario.id, "turns": turns}
