"""Council best-of-N + union coverage (tuxlink-6zkb6).

Deterministic-judge ensembling: generate diversely, filter mechanically. Verifies
best-of-N counts passes, the union covers a scenario if ANY model yields gold, and
gold is captured for training.
"""
from elmer_distill import council
from elmer_distill.scenario import Scenario, SuccessSpec


def _scn(sid):
    return Scenario(id=sid, family=sid.split("-")[0], depth=2, taint_state="clean",
                    prompt=f"p {sid}", spec=SuccessSpec(required_tools=[], ordering=[], staged=[]))


class _Client:
    """Emits a final turn; PASS iff this client's model is in `winners` (empty spec
    passes trivially on a clean final turn)."""
    def __init__(self, model, winners):
        self._win = model in winners
        self.temperature = 0
        self.seed = None

    def chat(self, model, messages, tools, temperature=None):
        # a clean final turn passes an empty spec; a stalled one (tool loop) would not
        return {"message": {"content": "done" if self._win else "", "thinking": "",
                            "tool_calls": [] if self._win else [{"function": {"name": "noop", "arguments": {}}}]}}


def _make_client_factory(model, winners):
    return lambda temperature, seed: _Client(model, winners)


def test_best_of_n_counts_and_gold():
    # a winning model passes every attempt -> n_pass == n, gold captured
    n_pass, gold = council.best_of_n(_make_client_factory("m", {"m"}), "m", _scn("s1"),
                                     "SYS", [], n=3, max_turns=3)
    assert n_pass == 3 and gold is not None
    # a losing model never passes
    n_pass2, gold2 = council.best_of_n(_make_client_factory("m", set()), "m", _scn("s1"),
                                       "SYS", [], n=3, max_turns=3)
    assert n_pass2 == 0 and gold2 is None


def test_council_union_covers_if_any_model_passes():
    scns = [_scn("s1"), _scn("s2")]
    # model A wins s1 only; model B wins s2 only -> union covers both
    winners = {"A": {"s1-only": True}}  # placeholder; use per-model client below

    def make_client(temperature, seed):
        # returns a client whose pass depends on the CURRENT model via closure trick:
        # council passes the model name into best_of_n->run_g0; but our _Client keys on
        # its own model. Simplest: model A always wins, model B always loses.
        return _Client(make_client.model, {"A"})

    # drive run_council manually per model to bind the model into the client
    from elmer_distill.judge import Judge
    rep = council.CouncilReport(n=2, models=["A", "B"])
    for s in scns:
        rep.per_scenario[s.id] = {}
        for m in ["A", "B"]:
            np_, gold = council.best_of_n(_make_client_factory(m, {"A"}), m, s, "SYS", [], n=2, max_turns=3)
            rep.per_scenario[s.id][m] = np_
            if gold is not None and s.id not in rep.gold:
                rep.gold[s.id] = {"model": m, "traj": gold}
    # A covers everything; union = both scenarios
    assert set(rep.covered()) == {"s1", "s2"}
    assert rep.uncovered() == []
    assert all(g["model"] == "A" for g in rep.gold.values())
