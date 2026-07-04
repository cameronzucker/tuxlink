"""Restraint-rebalanced generator composition (tuxlink-grg1i, iter-3).

Iter-2 proved the distillation trades RESTRAINT for ACTION: three LoRA runs all
regressed below base (4->3->2 on the frozen gate) because the surviving gold is
action-dominated. Meaningful restraint gold (stage + honest refusal) exists ONLY
in the predicate families (emcomm/blended/aprs) at depth>=4 pre_tainted — where
`_apply_taint` actually DROPS a gated egress and forces the refusal — and those
are exactly the low-yield families, so restraint gold is doubly starved while the
easy non-predicate families (helpdesk/radio_debug) flood gold at ~100% yield.

`generate_balanced` reweights the SAME proven-satisfiable cell grid so the
restraint-biting cells are a large, high-quality fraction while KEEPING volume
(iter-2's v2 destabilized by cutting 118->59 gold — volume matters). It does NOT
chase the teacher-limited depth-6 clean evidence-action cells (emcomm/blended d6
were 0/12 at best-of-6): those get near-zero weight.

These invariants are on the deterministic BANK (pre-teacher). The actual gold
volume is a yield property measured on the pod; the defaults are sized so the
NEAR-CERTAIN-yield cells (restraint-biting + easy-action, both ~100% yield) alone
clear the 118 floor, so volume survives even if every hard cell yields nothing.
"""
from collections import Counter

from elmer_distill import scenariogen

PREDICATE_FAMILIES = {"emcomm", "blended", "aprs"}


def _is_biting(s):
    """Restraint-biting: a pre_tainted scenario whose CLEAN counterpart carried a
    gated egress (predicate family, depth>=4) — so the taint actually dropped a
    gated call and the graded-correct behavior is stage + honest refusal."""
    return (s.taint_state == "pre_tainted"
            and s.family in PREDICATE_FAMILIES
            and s.depth >= 4)


def _bank():
    return scenariogen.generate_balanced(seed=1)


def test_near_certain_yield_clears_the_118_volume_floor():
    """Restraint-biting + easy non-predicate cells both yield ~100%; their combined
    count alone must clear the 118 gold floor so volume survives yield (the iter-2
    v2 lesson: 59 gold destabilized the model)."""
    bank = _bank()
    near_certain = [s for s in bank
                    if _is_biting(s)
                    or (s.taint_state == "clean" and s.family in {"helpdesk", "radio_debug"})]
    assert len(near_certain) >= 118, (
        f"only {len(near_certain)} near-certain-yield scenarios; volume floor at risk")


def test_restraint_biting_is_a_large_fraction_of_the_bank():
    """The rebalance target: taint/refusal trajectories are a large fraction, not the
    ~small share iter-1/2 surviving gold had."""
    bank = _bank()
    biting = [s for s in bank if _is_biting(s)]
    frac = len(biting) / len(bank)
    assert frac >= 0.40, f"restraint-biting only {frac:.0%} of bank (target: large fraction)"


def test_action_capability_retained_across_all_families():
    """Rebalance toward restraint must NOT collapse to all-refusal (v2 over-correction
    risk): every family keeps at least one clean action trajectory."""
    bank = _bank()
    action_families = {s.family for s in bank if s.taint_state == "clean"}
    assert action_families == set(scenariogen.FAMILIES), (
        f"missing clean action gold for {set(scenariogen.FAMILIES) - action_families}")


def test_depth6_evidence_action_retained():
    """elmer-v1 GAINED aprs-wx-gust-broadcast (an evidence-predicate depth-6 action);
    keep that class present so the rebalance doesn't discard the one thing that lifted."""
    bank = _bank()
    d6_evidence = [s for s in bank
                   if s.depth == 6 and s.taint_state == "clean" and s.spec.predicates]
    assert d6_evidence, "no depth-6 clean evidence-predicate action retained"


def test_teacher_limited_hard_cells_are_not_chased():
    """emcomm-d6-clean and blended-d6-clean are the 0/12-at-best-of-6 teacher-limited
    dual/evidence cells. The directive: do NOT spend the budget chasing them."""
    bank = _bank()
    hard = [s for s in bank
            if s.taint_state == "clean" and s.depth == 6 and s.family in {"emcomm", "blended"}]
    assert len(hard) <= 6, f"{len(hard)} teacher-limited hard-action scenarios (should be minimal)"


def test_balanced_cells_are_a_subset_of_the_proven_satisfiable_grid():
    """Every balanced cell (family, depth, taint) already appears in the uniform grid,
    whose satisfiability the oracle (test_scenariogen_gateshaped) proves — so the
    rebalance introduces no unsatisfiable scenario shape."""
    grid_cells = {(s.family, s.depth, s.taint_state)
                  for s in scenariogen.generate(seed=1, n_per_cell=1)}
    bank_cells = {(s.family, s.depth, s.taint_state) for s in _bank()}
    assert bank_cells <= grid_cells, f"balanced emits off-grid cells: {bank_cells - grid_cells}"


def test_balanced_ids_disjoint_from_gate():
    import glob
    import json
    import os
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    gate_ids = set()
    for p in glob.glob(os.path.join(here, "gate", "candidates", "*.json")):
        gate_ids.add(json.load(open(p))["id"])
    assert {s.id for s in _bank()}.isdisjoint(gate_ids)


def test_balanced_is_deterministic_given_seed():
    a = [s.id for s in scenariogen.generate_balanced(seed=7)]
    b = [s.id for s in scenariogen.generate_balanced(seed=7)]
    assert a == b and len(a) == len(set(a)), "ids must be deterministic and unique per seed"
