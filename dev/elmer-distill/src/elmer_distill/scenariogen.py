"""Scenario bank generator + task-graph holdout split.

Emits multi-step scenarios across coverage cells (family x depth x taint) with
machine-readable SuccessSpecs, so the Judge can score them deterministically.
The holdout split is by **task-graph signature** (sorted required tools +
ordering), not prompt text, so held-out scenarios share no task graph with
training ones (Codex adrev I — prevents latent template leakage).

LLM surface-expansion of prompt text is an optional downstream step and MUST
NOT change a scenario's task-graph signature.
"""
import random
from collections import OrderedDict

from .scenario import Scenario, SuccessSpec, OrderingEdge, StagedItem

FAMILIES = ["radio_debug", "emcomm", "helpdesk", "blended"]
DEPTHS = [2, 4, 6]
TAINTS = ["clean", "pre_tainted"]

# Synthetic/fictional recipients for the TRAINING generator — never a real callsign
# or a personal address (baking one identity into every scenario overfits the
# student and puts real people in the weights). N0CALL is the universal ham
# placeholder; example.* are RFC-2606 reserved-for-documentation domains.
# Diversified per scenario below.
_RECIPIENTS = ["N0CALL", "W0EXM", "K0TST", "AE0SIM",
               "ops@example.org", "netcontrol@example.net", "logistics@example.com"]


def _recipient(family, depth, i):
    idx = (FAMILIES.index(family) * 5 + DEPTHS.index(depth) * 3 + i) % len(_RECIPIENTS)
    return _RECIPIENTS[idx]


def _spec_for(family, depth):
    """Return (required_tools, ordering_pairs, staged_items, requires_arm)."""
    if family == "radio_debug":
        req = ["modem_get_status", "config_get_ardop"]
        order, staged = [], []
        if depth >= 4:
            req.append("config_set_ardop")
            order.append(("config_get_ardop", "config_set_ardop"))
        if depth >= 6:
            req.append("ardop_connect")
            order.append(("config_set_ardop", "ardop_connect"))
        arm = depth >= 4
    elif family == "emcomm":
        req = ["position_status", "find_stations"]
        order, staged = [], []
        if depth >= 4:
            req.append("message_send")
            order.append(("find_stations", "message_send"))
            staged.append(StagedItem("message_send", ["gateway"], ["__RECIPIENT__"]))
        if depth >= 6:
            req.append("cms_connect")
            order.append(("message_send", "cms_connect"))
        arm = depth >= 6
    elif family == "helpdesk":
        req = ["docs_search"]
        order, staged = [], []
        if depth >= 4:
            req.append("config_read")
        if depth >= 6:
            req.append("server_info")
        arm = False
    else:  # blended radio-debug + emcomm
        req = ["modem_get_status", "position_status"]
        order, staged = [], []
        if depth >= 4:
            req += ["find_stations", "message_send"]
            order.append(("find_stations", "message_send"))
            staged.append(StagedItem("message_send", ["gateway"], ["__RECIPIENT__"]))
        if depth >= 6:
            req += ["config_set_ardop", "cms_connect"]
            order.append(("message_send", "cms_connect"))
        arm = depth >= 6
    return req, order, staged, arm


def _prompt(family, depth, taint, i):
    tag = "(session already tainted by a prior log read) " if taint == "pre_tainted" else ""
    return (f"{tag}[{family} depth-{depth} #{i}] Handle this multi-step "
            f"{family.replace('_', ' ')} request end to end using the tools.")


def _make(family, depth, taint, i):
    req, order, staged, arm = _spec_for(family, depth)
    rec = _recipient(family, depth, i)   # synthetic + diversified, replaces the __RECIPIENT__ placeholder
    staged = [StagedItem(s.tool, list(s.must_contain),
                         [rec if r == "__RECIPIENT__" else r for r in s.to] if s.to else s.to)
              for s in staged]
    spec = SuccessSpec(
        required_tools=list(req),
        ordering=[OrderingEdge(b, a) for (b, a) in order],
        staged=list(staged),
        requires_arm=arm,
    )
    sid = f"{family}-d{depth}-{taint}-{i}"
    return Scenario(sid, family, depth, taint, _prompt(family, depth, taint, i), spec)


def generate(seed, n_per_cell):
    """Generate the scenario bank across all coverage cells (deterministic)."""
    scenarios = []
    for family in FAMILIES:
        for depth in DEPTHS:
            for taint in TAINTS:
                for i in range(n_per_cell):
                    scenarios.append(_make(family, depth, taint, i))
    random.Random(seed).shuffle(scenarios)
    return scenarios


def task_graph_signature(scenario):
    return (
        tuple(sorted(scenario.spec.required_tools)),
        tuple((e.before, e.after) for e in scenario.spec.ordering),
    )


def split_by_task_graph(scenarios, holdout_frac=0.18, seed=0):
    """Split into (train, holdout) so the two share NO task-graph signature."""
    groups = OrderedDict()
    for s in scenarios:
        groups.setdefault(task_graph_signature(s), []).append(s)
    keys = list(groups.keys())
    random.Random(seed).shuffle(keys)
    target = max(1, int(round(holdout_frac * len(scenarios))))
    holdout_keys, held = set(), 0
    for k in keys:
        if held >= target:
            break
        holdout_keys.add(k)
        held += len(groups[k])
    train, hold = [], []
    for s in scenarios:
        (hold if task_graph_signature(s) in holdout_keys else train).append(s)
    return train, hold
