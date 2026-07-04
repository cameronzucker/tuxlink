"""G1 teacher-capture runner.

Drives a model (the gpt-oss:120b teacher) through the agentic loop over the
scenario bank, feeding tool results from the StatefulSimulator, and scores each
resulting trajectory with the Judge. `capture` reports **gold yield per coverage
cell** (family x depth x taint) — the G1 pilot answer to "does correct data
exist where the target behavior lives?" (Codex adrev D).

The client is injected (OllamaClient in production, a fake in tests) so the
agentic loop is exercised network-free.
"""
import json
from dataclasses import dataclass, field

from .simulator import StatefulSimulator
from .judge import Judge


def _as_dict(args):
    if isinstance(args, str):
        try:
            return json.loads(args)
        except Exception:
            return {}
    return args or {}


def run_scenario(client, model, scenario, system, tools, max_turns=20):
    """Run one scenario through the agentic loop; return a trajectory dict."""
    messages = [{"role": "system", "content": system},
                {"role": "user", "content": scenario.prompt}]
    turns = [{"role": "user", "content": scenario.prompt}]

    sim = StatefulSimulator(armed=scenario.spec.requires_arm)
    if scenario.taint_state == "pre_tainted":
        sim.tainted = True

    for _ in range(max_turns):
        d = client.chat(model, messages, tools)   # sampling governed by the client (best-of-N uses temp>0 + seed)
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

        if not tool_calls:
            break

        for tc in tool_calls:
            fn = tc.get("function", {})
            name = fn.get("name", "?")
            args = _as_dict(fn.get("arguments"))
            result = sim.apply(name, args)
            messages.append({"role": "tool", "tool_name": name, "content": json.dumps(result)})
            turns.append({"role": "tool", "tool_name": name, "content": json.dumps(result)})

    return {"scenario_id": scenario.id, "turns": turns}


@dataclass
class CaptureReport:
    total: int = 0
    passed: int = 0
    by_cell: dict = field(default_factory=dict)   # (family, depth, taint) -> {total, passed}
    gold: list = field(default_factory=list)       # judge-passing GENERATOR trajectories (training data)
    held_out: list = field(default_factory=list)   # judge-passing operator_authored trajectories — TEST set (probe), never trained

    def yield_rate(self):
        return self.passed / self.total if self.total else 0.0


def _cell(s):
    return (s.family, s.depth, s.taint_state)


def capture(client, model, scenarios, system, tools, max_turns=20):
    """Run the bank through the teacher, keep judge-passing trajectories as gold."""
    rep = CaptureReport()
    judge = Judge()
    for s in scenarios:
        traj = run_scenario(client, model, s, system, tools, max_turns)
        verdict = judge.score(s, traj, armed=s.spec.requires_arm)
        rep.total += 1
        cell = rep.by_cell.setdefault(_cell(s), {"total": 0, "passed": 0})
        cell["total"] += 1
        if verdict.passed:
            rep.passed += 1
            cell["passed"] += 1
            # operator_authored scenarios are the frozen GATE (test set): capture
            # their trajectories for the before/after probe, but keep them out of
            # gold so they never become training data (tuxlink-6zkb6).
            (rep.held_out if s.operator_authored else rep.gold).append(traj)
    return rep


def capture_bestof(client_factory, model, scenarios, system, tools,
                   n_attempts=3, max_turns=20, runner=None):
    """Best-of-N teacher capture (tuxlink-grg1i): retry each scenario up to
    n_attempts with a fresh client per attempt (client_factory(attempt) returns a
    seeded client so sampling varies), keeping the FIRST judge-passing trajectory.

    `runner(client, model, scenario, system, tools, max_turns)` selects the agentic
    loop. Default = the RAW loop (run_scenario). For GOLD generation pass a SCAFFOLDED
    runner (baseline_g0.run_g0 wrapped with a checklist + verifier reprompts): cold,
    even the 120b teacher passes the judge only ~5% of the time, so the raw loop yields
    almost no gold — the checklist scaffold is what produced iter-2's 118 gold. The
    scaffold lives only in the teacher's prompt; the saved trajectory (and assemble's
    SYSTEM_PROMPT render) is clean, so the student never sees the checklist.

    At most one gold per scenario, so gold volume is bounded by the bank and cannot
    runaway-inflate from the restraint oversample."""
    runner = runner or run_scenario
    rep = CaptureReport()
    judge = Judge()
    for s in scenarios:
        rep.total += 1
        cell = rep.by_cell.setdefault(_cell(s), {"total": 0, "passed": 0})
        cell["total"] += 1
        for attempt in range(n_attempts):
            client = client_factory(attempt)
            traj = runner(client, model, s, system, tools, max_turns)
            if judge.score(s, traj, armed=s.spec.requires_arm).passed:
                rep.passed += 1
                cell["passed"] += 1
                (rep.held_out if s.operator_authored else rep.gold).append(traj)
                break
    return rep
