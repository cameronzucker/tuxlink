"""Mixed-source gold + naturalistic-prompt expansion for the 120b cold-transfer build
(tuxlink-48nyh, operator 2026-07-04).

The 120b self-generates high-QUALITY grounded gold (it drafts materially better reports),
but it FAILS the taint/restraint cells (1/5 vs the 20b's 5/5) — so it cannot generate its
own refusal gold. The fix (operator's words): borrow the 20b's better restraint trajectories
for those cells. `capture_mixed` routes RESTRAINT cells to a restraint teacher and every
other cell to the quality teacher, then merges the gold with per-source provenance so the
assembled dataset records which model taught which behavior.

`expand_bank` applies expand.py so the training PROMPTS are natural operator language, not
the generator's templated placeholders (Fable precondition: placeholders can't teach
prompt->tool-graph transfer). The spec (task-graph ground truth) is never changed.
"""
from .teacher import capture_bestof, CaptureReport

# Families whose taint variant exercises a refusal/honesty behavior (the restraint axis).
PREDICATE_FAMILIES = {"emcomm", "blended", "aprs"}


def is_restraint_cell(family, depth, taint):
    """A taint-discipline cell — taint drives a refusal (drop a gated egress) or honesty
    (don't claim sent) behavior. This is the axis the 120b loses and must borrow gold for.
    Mirrors run_gold's restraint definition (the canonical home is here)."""
    return taint == "pre_tainted" and family in PREDICATE_FAMILIES and depth >= 4


def split_restraint(bank):
    """(restraint_cells, other_cells) — a partition of the bank, order preserved."""
    restraint = [s for s in bank if is_restraint_cell(s.family, s.depth, s.taint_state)]
    other = [s for s in bank if not is_restraint_cell(s.family, s.depth, s.taint_state)]
    return restraint, other


def _merge(quality_rep, restraint_rep):
    m = CaptureReport()
    m.total = quality_rep.total + restraint_rep.total
    m.passed = quality_rep.passed + restraint_rep.passed
    m.gold = quality_rep.gold + restraint_rep.gold
    m.held_out = quality_rep.held_out + restraint_rep.held_out
    for src in (quality_rep, restraint_rep):
        for k, v in src.by_cell.items():
            c = m.by_cell.setdefault(k, {"total": 0, "passed": 0})
            c["total"] += v["total"]
            c["passed"] += v["passed"]
    return m


def _tag(rep, model):
    for t in rep.gold + rep.held_out:
        t["_teacher_model"] = model
    return rep


def capture_mixed(quality_factory, quality_model, restraint_factory, restraint_model,
                  bank, system, tools, n_attempts=2, max_turns=40, runner=None):
    """Route restraint cells to the restraint teacher, every other cell to the quality
    teacher; merge. Each gold trajectory is tagged with its `_teacher_model` for auditable
    provenance. Returns (merged_report, provenance_dict).

    Each scenario is in exactly ONE partition, so no scenario is double-captured and the
    at-most-one-gold-per-scenario bound (teacher.capture_bestof) is preserved end-to-end."""
    restraint_cells, other_cells = split_restraint(bank)
    q_rep = _tag(capture_bestof(quality_factory, quality_model, other_cells,
                                system, tools, n_attempts, max_turns, runner), quality_model)
    r_rep = _tag(capture_bestof(restraint_factory, restraint_model, restraint_cells,
                                system, tools, n_attempts, max_turns, runner), restraint_model)
    merged = _merge(q_rep, r_rep)
    prov = {"quality_model": quality_model, "quality_cells": len(other_cells),
            "quality_gold": len(q_rep.gold),
            "restraint_model": restraint_model, "restraint_cells": len(restraint_cells),
            "restraint_gold": len(r_rep.gold)}
    return merged, prov


def expand_bank(client, model, bank, exemplars, temperature=0.7):
    """Rewrite each scenario's templated placeholder prompt into a natural operator request
    (expand.py), keeping id + spec (the task-graph ground truth) unchanged."""
    from .expand import expand
    return [expand(client, model, s, exemplars, temperature=temperature) for s in bank]
