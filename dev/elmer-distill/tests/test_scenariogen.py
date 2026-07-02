from elmer_distill.scenariogen import generate, split_by_task_graph, task_graph_signature


def test_deterministic_and_covers_cells():
    a = generate(seed=1, n_per_cell=2)
    b = generate(seed=1, n_per_cell=2)
    assert [s.id for s in a] == [s.id for s in b]              # deterministic
    fams = {s.family for s in a}
    assert {"radio_debug", "emcomm", "helpdesk", "blended"} <= fams
    assert any(s.depth >= 6 for s in a)                        # deep multi-tool present


def test_holdout_shares_no_task_graph():
    scen = generate(seed=1, n_per_cell=3)
    train, hold = split_by_task_graph(scen, holdout_frac=0.2, seed=0)
    tr = {task_graph_signature(s) for s in train}
    ho = {task_graph_signature(s) for s in hold}
    assert tr.isdisjoint(ho) and len(hold) > 0
    assert len(train) + len(hold) == len(scen)
