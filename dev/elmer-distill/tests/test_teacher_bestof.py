"""Best-of-N teacher capture (tuxlink-grg1i, iter-3).

The restraint-rebalanced bank oversamples taint/refusal cells. The teacher
(gpt-oss:120b) yields well on them but can still occasionally fire the egress
(the exact failure the student inherits), so a single attempt under-yields and
threatens the 118 volume floor. `capture_bestof` retries a scenario up to
n_attempts (fresh seeded client each attempt) and keeps the FIRST judge-passing
trajectory — at most one gold per scenario, so volume is bounded by the bank and
never explodes. It does NOT change which cells exist: the bank weighting already
declines to over-represent the teacher-limited hard-action cells.
"""
from elmer_distill.scenario import Scenario, SuccessSpec
from elmer_distill.teacher import capture_bestof


def _trivial_scenario():
    spec = SuccessSpec(required_tools=["docs_search"], ordering=[], staged=[],
                       requires_arm=False, predicates=[])
    return Scenario("helpdesk-d2-clean-0", "helpdesk", 2, "clean",
                    "handle it", spec)


class _Client:
    """Emits docs_search + honest final iff `pass_now`, else an empty (failing) turn."""
    def __init__(self, pass_now):
        self.pass_now = pass_now
        self.calls = 0

    def chat(self, model, messages, tools, temperature=0):
        self.calls += 1
        if self.pass_now and self.calls == 1:
            return {"message": {"content": "",
                                "tool_calls": [{"function": {"name": "docs_search", "arguments": {}}}]}}
        return {"message": {"content": "done", "tool_calls": []}}


def test_retries_until_a_passing_trajectory_is_found():
    made = []

    def factory(attempt):
        c = _Client(pass_now=(attempt == 1))   # attempt 0 fails, attempt 1 passes
        made.append(c)
        return c

    rep = capture_bestof(factory, "gpt-oss:120b", [_trivial_scenario()], "SYS", tools=[], n_attempts=3)
    assert rep.passed == 1
    assert len(rep.gold) == 1
    assert len(made) == 2, "should stop retrying once a passer is found"


def test_stops_after_n_attempts_when_never_passing():
    made = []

    def factory(attempt):
        c = _Client(pass_now=False)
        made.append(c)
        return c

    rep = capture_bestof(factory, "gpt-oss:120b", [_trivial_scenario()], "SYS", tools=[], n_attempts=4)
    assert rep.passed == 0
    assert rep.gold == []
    assert len(made) == 4, "exhausts exactly n_attempts"
    assert rep.total == 1 and rep.by_cell[("helpdesk", 2, "clean")]["total"] == 1


def test_uses_injected_scaffolded_runner():
    """Gold-gen must be able to swap the raw agentic loop for a scaffolded runner
    (run_g0) — the raw loop yields ~5% from the 120b (its cold gate score), which
    starved the iter-3 pool. The runner receives (client, model, scenario, system,
    tools, max_turns) and its trajectory is what gets judged."""
    seen = []

    def my_runner(client, model, scenario, system, tools, max_turns):
        seen.append(scenario.id)
        # emit the passing docs_search trajectory regardless of client scripting
        return {"scenario_id": scenario.id, "turns": [
            {"role": "user", "content": "x"},
            {"role": "assistant", "content": None,
             "tool_calls": [{"function": {"name": "docs_search", "arguments": {}}}]},
            {"role": "assistant", "content": "done"}]}

    rep = capture_bestof(lambda a: _Client(pass_now=False), "m", [_trivial_scenario()],
                         "SYS", tools=[], n_attempts=3, runner=my_runner)
    assert seen == ["helpdesk-d2-clean-0"], "injected runner was not used"
    assert rep.passed == 1 and len(rep.gold) == 1


def test_keeps_at_most_one_gold_per_scenario():
    """Even if every attempt would pass, only the first is kept — gold volume is
    bounded by the bank, so the restraint oversample can't runaway-inflate."""
    def factory(attempt):
        return _Client(pass_now=True)

    rep = capture_bestof(factory, "gpt-oss:120b", [_trivial_scenario()], "SYS", tools=[], n_attempts=5)
    assert len(rep.gold) == 1
