"""Multi-model teacher council with best-of-N sampling, judge-filtered.

Because we own a DETERMINISTIC judge, ensembling here is "generate diversely,
filter mechanically" — NOT opinion-aggregation (no meta-judge to fool). Each
(model, scenario) gets N scaffolded attempts (attempt 0 greedy, the rest sampled
with a varied seed); any judge-passing trajectory is gold. The UNION across models
is the gold coverage: different models have different blind spots, so more diverse
generators strictly increase gold, and the judge picks winners objectively.

This is the gold-yield engine: a scenario is "coverable" if ANY council member
produces a passing trajectory in N tries. Coverable scenarios can be trained on;
the union gold is the training data (rendered to Harmony downstream).
"""
from dataclasses import dataclass, field

from .judge import Judge
from .baseline_g0 import run_g0


def best_of_n(make_client, model, scenario, system, tools, n, judge=None,
              max_turns=40, max_reprompts=2, temperature=0.7):
    """N scaffolded attempts through one model; return (n_pass, gold_traj_or_None).
    Attempt 0 is greedy (temp 0 — the deterministic scaffold result is the floor);
    the rest sample with varied seed for diversity."""
    judge = judge or Judge()
    n_pass = 0
    gold = None
    for i in range(n):
        client = make_client(temperature=(0.0 if i == 0 else temperature), seed=i)
        try:
            traj = run_g0(client, model, scenario, system, tools, exemplars=[],
                          max_reprompts=max_reprompts, max_turns=max_turns)
        except Exception:
            # a failed attempt (exhausted retries, a malformed response) is just a
            # non-pass — never abort the run over one attempt.
            continue
        if judge.score(scenario, traj, armed=scenario.spec.requires_arm).passed:
            n_pass += 1
            if gold is None:
                gold = traj
    return n_pass, gold


@dataclass
class CouncilReport:
    n: int
    models: list
    per_scenario: dict = field(default_factory=dict)   # sid -> {model: n_pass}
    gold: dict = field(default_factory=dict)            # sid -> {"model":m, "traj":...}

    def covered(self):
        """Scenario ids with >=1 gold from ANY council member (the union)."""
        return [sid for sid, mp in self.per_scenario.items() if any(v > 0 for v in mp.values())]

    def uncovered(self):
        return [sid for sid, mp in self.per_scenario.items() if not any(v > 0 for v in mp.values())]


def run_council(make_client, models, scenarios, system, tools, n=5,
                max_turns=40, max_reprompts=2, temperature=0.7, progress=None):
    """make_client(temperature, seed) -> a client on the (shared) ollama endpoint;
    the model name is passed per attempt. Returns a CouncilReport: per-model
    per-scenario pass counts + the union gold set (first model to yield per
    scenario).

    Loop is MODEL-OUTER, scenario-inner: on a single-GPU pod that can't hold
    multiple 70B models resident, this loads each model ONCE (vs a swap on every
    step). `progress(model, sid, n_pass, gold, rep)` fires after each cell so the
    caller can persist gold incrementally + log — a long run's partial results
    survive an interruption.
    """
    judge = Judge()
    rep = CouncilReport(n=n, models=list(models))
    for s in scenarios:
        rep.per_scenario[s.id] = {}
    for m in models:
        for s in scenarios:
            try:
                n_pass, gold = best_of_n(make_client, m, s, system, tools, n, judge=judge,
                                         max_turns=max_turns, max_reprompts=max_reprompts,
                                         temperature=temperature)
            except Exception:
                n_pass, gold = 0, None   # a whole-cell failure is 0 coverage, not a crash
            rep.per_scenario[s.id][m] = n_pass
            if gold is not None and s.id not in rep.gold:
                rep.gold[s.id] = {"model": m, "traj": gold}
            if progress is not None:
                progress(m, s.id, n_pass, gold, rep)
    return rep
