"""Gate-shaped generator invariants + a SATISFIABILITY ORACLE (tuxlink-6zkb6, iter-2).

Iteration-1's LoRA got no lift: the generator emitted SIMPLE task-graphs (2-4 tools,
no evidence predicates, inert taint) while the frozen gate is 5-6-tool chains with
evidence predicates + real taint discipline. Training on the easy distribution did
not transfer to the hard test.

The dominant risk when hardening the generator toward the gate shape is emitting an
UNSATISFIABLE scenario — a predicate whose `minimum` exceeds the records the
simulator actually returns, an aprs broadcast that can't fit the char limit, a taint
graph that forces a denied egress. An unsatisfiable scenario yields ZERO gold from
gold-gen and teaches the student nothing (worse: it silently shrinks the pool).

The oracle below is the guard: for every generated scenario it constructs the
trajectory a PERFECT agent would produce — calling each required tool, staging bodies
that cite the simulator's REAL records, honoring taint discipline — and asserts the
Judge passes it. If the generator ever emits a scenario no perfect agent can pass,
this test fails loudly instead of the failure surfacing as a mysterious low gold yield
on a $/hr H200.
"""
from elmer_distill import scenariogen
from elmer_distill.judge import Judge
from elmer_distill.simulator import StatefulSimulator
from elmer_distill.tool_surface import classify, EGRESS_TOOLS, TIER2_WRITE_TOOLS, TAINT_TOOLS

_APRS_MSG_MAX = 67


def _band_records(band):
    return StatefulSimulator()._find_stations({"bands": [band]})["stations"]


def _aprs_records():
    return StatefulSimulator()._aprs_list_stations({})["stations"]


def _staged_body_for(tool, spec):
    """Build a message body that satisfies every predicate bound to `tool` plus the
    staged must_contain, drawing on the simulator's real fixtures."""
    clauses = []
    # staged must_contain (each element: substring, or list = any-of synonym group)
    for item in spec.staged:
        if item.tool != tool:
            continue
        for m in item.must_contain:
            clauses.append(m[0] if isinstance(m, (list, tuple)) else m)
    for chk in spec.predicates:
        if chk.tool != tool:
            continue
        p, params = chk.predicate, chk.params
        if p == "references_real_gateway":
            recs = [r for r in _band_records(params["band"]) if r["band"] == params["band"]]
            for r in recs[: params["minimum"]]:
                clauses.append(f"{r['callsign']} {int(r['freq_khz'])} kHz")
        elif p == "schedule_has_blocks":
            clauses.append(" ".join(f"{h:02d}:00" for h in range(params["n"])))
        elif p == "aprs_positions_cited":
            recs = {r["callsign"].upper(): r for r in _aprs_records()}
            for cs in params["callsigns"]:
                r = recs.get(cs.upper())
                if r:
                    clauses.append(f"{cs} at {r['grid']}")
        elif p == "aprs_gust_alert_cited":
            th = params["threshold"]
            for r in _aprs_records():
                g = r.get("gust_mph")
                if g is not None and float(g) > th:
                    clauses.append(f"{r['callsign']} gust {int(g)} mph")
    return "; ".join(clauses) or "status report"


def _oracle_trajectory(scenario):
    """The trajectory a perfect agent produces for this scenario."""
    spec = scenario.spec
    turns = [{"role": "user", "content": scenario.prompt}]
    for tool in spec.required_tools:
        kind = classify(tool)
        if kind == "staging":
            body = _staged_body_for(tool, spec)
            to = next((it.to for it in spec.staged if it.tool == tool and it.to), None)
            args = {"body": body}
            if to:
                args["to"] = to[0]
        elif tool == "find_stations":
            args = {}
        elif tool == "predict_path":
            args = {"rx_grid": "DM43", "frequencies_khz": [3585]}
        elif tool == "aprs_send_message":
            args = {"text": "OPS: comms up"}          # <= 67 chars
        elif tool in ("ardop_connect",):
            args = {"target": "W7GW"}                  # a reachable station
        else:
            args = {}
        turns.append({"role": "assistant", "content": None,
                      "tool_calls": [{"function": {"name": tool, "arguments": args}}]})
    # a perfect final answer never falsely claims a completed send
    turns.append({"role": "assistant", "content": "Task handled; report staged as requested."})
    return {"scenario_id": scenario.id, "turns": turns}


def _all():
    return scenariogen.generate(seed=1, n_per_cell=2)


def test_every_generated_scenario_is_satisfiable():
    """A perfect agent passes every generated scenario against the real simulator."""
    judge = Judge()
    for s in _all():
        traj = _oracle_trajectory(s)
        v = judge.score(s, traj, armed=s.spec.requires_arm)
        assert v.passed, f"UNSATISFIABLE {s.id}: {v.reasons}"


def test_aprs_family_present():
    fams = {s.family for s in _all()}
    assert "aprs" in fams, fams


def test_depth6_carries_evidence_predicates():
    """The gate's hardness is evidence predicates on 5-6-tool chains; the generator
    must emit that shape, not just tool counts (the iter-1 gap)."""
    d6 = [s for s in _all() if s.depth == 6]
    with_pred = [s for s in d6 if s.spec.predicates]
    assert with_pred, "no depth-6 scenario carries an evidence predicate"
    # covering the real predicate vocabulary the gate uses
    kinds = {p.predicate for s in d6 for p in s.spec.predicates}
    assert "references_real_gateway" in kinds
    assert kinds & {"aprs_positions_cited", "aprs_gust_alert_cited"}


def test_gateshaped_chains_reach_five_tools():
    lengths = [len(s.spec.required_tools) for s in _all()]
    assert max(lengths) >= 5, f"max chain only {max(lengths)} tools (gate is 5-6)"


def test_pretainted_encodes_real_taint_discipline():
    """A pre_tainted scenario must (a) actually taint the session via a taint tool in
    the chain, and (b) NOT require an egress/tier-2 tool — the correct behavior is
    stage + honest refusal, so requiring a gated call would force a judge failure.
    (Iter-1's pre_tainted was an inert prompt tag: no taint tool, so identical to
    clean.)"""
    pretainted = [s for s in _all() if s.taint_state == "pre_tainted"]
    assert pretainted
    gated = EGRESS_TOOLS | TIER2_WRITE_TOOLS
    # at least the egress-bearing families encode a genuine refusal
    refusal = [s for s in pretainted
               if (set(s.spec.required_tools) & TAINT_TOOLS)
               and not (set(s.spec.required_tools) & gated)]
    assert refusal, "no pre_tainted scenario encodes taint (taint-tool present, no gated call required)"
    # and NO pre_tainted scenario simultaneously taints AND requires a gated call
    for s in pretainted:
        if set(s.spec.required_tools) & TAINT_TOOLS:
            assert not (set(s.spec.required_tools) & gated), \
                f"{s.id} taints then requires a gated call (guaranteed judge failure)"


def test_generated_ids_disjoint_from_gate():
    """Contamination-in-spirit: no generated scenario shares an id with a gate
    candidate (the assemble guard holds out by id)."""
    import glob, json, os
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    gate_ids = set()
    for p in glob.glob(os.path.join(here, "gate", "candidates", "*.json")):
        gate_ids.add(json.load(open(p))["id"])
    gen_ids = {s.id for s in _all()}
    assert gen_ids.isdisjoint(gate_ids)
