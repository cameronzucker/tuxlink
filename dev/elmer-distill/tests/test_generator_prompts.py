"""Hand-authored generator-cell prompts (tuxlink-6zkb6).

Every (family, depth) cell must have a natural, non-placeholder prompt whose
recipient matches the cell's synthetic assignment, so the surface prompt and the
graded task-graph agree.
"""
from elmer_distill import scenariogen, generator_prompts


def test_every_cell_authored_and_natural():
    for fam in scenariogen.FAMILIES:
        for depth in scenariogen.DEPTHS:
            p = generator_prompts.AUTHORED.get((fam, depth))
            assert p, f"no authored prompt for {fam} d{depth}"
            assert "[" not in p and "Handle this multi-step" not in p  # not a placeholder
            assert len(p) > 30


def test_authored_prompt_matches_cell_recipient():
    # for each staged cell, the authored prompt names the same synthetic recipient
    # scenariogen assigns to i=0 (prompt/graph agreement)
    scns = {(s.family, s.depth): s for s in scenariogen.generate(seed=1, n_per_cell=1)
            if s.taint_state == "clean"}
    for (fam, depth), s in scns.items():
        recips = [r for st in s.spec.staged for r in (st.to or [])]
        p = generator_prompts.AUTHORED[(fam, depth)]
        for r in recips:
            assert r in p, f"{fam} d{depth}: authored prompt missing recipient {r}"


def test_apply_returns_none_for_unknown_cell():
    class _S:
        id, family, depth = "nope-d99-clean-0", "nope", 99
    assert generator_prompts.apply(_S()) is None


def test_apply_only_i0_variant():
    # i>0 variants must be model-authored (different synthetic recipient) — not the
    # i=0 authored prompt, which would desync prompt from the graded recipient.
    class _S0:
        id, family, depth = "emcomm-d4-clean-0", "emcomm", 4
    class _S1:
        id, family, depth = "emcomm-d4-clean-1", "emcomm", 4
    assert generator_prompts.apply(_S0()) is not None
    assert generator_prompts.apply(_S1()) is None
