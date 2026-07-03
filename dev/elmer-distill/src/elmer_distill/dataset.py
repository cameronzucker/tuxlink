"""Dataset assembler — gold trajectories -> Harmony training JSONL.

Each row: {"text": <harmony training text>, "loss_spans": [[start,end], ...]}
with loss masked to assistant-generated content only (Codex adrev — mandatory
for agentic SFT). Reports P95 rendered length, which sets Phase-A
`max_seq_length` (spec §5.4): truncating long multi-tool traces would erase the
exact turn-3+ behavior we are training.
"""
import glob
import json
import os
from dataclasses import dataclass, field

from . import harmony


class ContaminationError(RuntimeError):
    """Gold (training) data would contain a held-out gate/probe scenario.

    The frozen gate is the TEST set; training data must come from the generator
    pool. Letting a gate prompt into gold turns the before/after probe into a
    memorization measurement (tuxlink-6zkb6). This is a hard stop, not a filter:
    the dataset is refused rather than silently de-duped, so a contaminated
    pipeline fails loudly instead of shipping a poisoned run.
    """


@dataclass
class DatasetStats:
    n: int
    p95_chars: int
    max_chars: int
    family_counts: dict = field(default_factory=dict)


def _family_of(scenario_id):
    return (scenario_id or "").split("-")[0]


def holdout_ids_from_dir(candidates_dir):
    """Canonical held-out set: every scenario id in the frozen gate candidates
    dir (operator AND agent authored). Pass to `assemble` as the contamination
    guard so callers can't forget an individual scenario."""
    ids = set()
    for p in glob.glob(os.path.join(candidates_dir, "*.json")):
        with open(p) as f:
            ids.add(json.load(f)["id"])
    return frozenset(ids)


def assemble(gold, system, out_path, holdout_ids=frozenset()):
    holdout_ids = frozenset(holdout_ids)
    if holdout_ids:
        leaked = sorted({(t.get("scenario_id") or "") for t in gold} & holdout_ids)
        if leaked:
            raise ContaminationError(
                "refusing to build training data containing held-out gate/probe "
                f"scenario(s): {leaked}. Gold must come from the generator pool, "
                "never the frozen gate (tuxlink-6zkb6 contamination guard).")

    lengths = []
    family_counts = {}
    with open(out_path, "w") as f:
        for traj in gold:
            text = harmony.render_trajectory(system, traj)
            spans = harmony.assistant_loss_spans(text)
            f.write(json.dumps({"text": text, "loss_spans": [[s, e] for (s, e) in spans]}) + "\n")
            lengths.append(len(text))
            fam = _family_of(traj.get("scenario_id"))
            family_counts[fam] = family_counts.get(fam, 0) + 1

    n = len(lengths)
    p95 = sorted(lengths)[int(0.95 * (n - 1))] if n else 0
    return DatasetStats(n=n, p95_chars=p95, max_chars=max(lengths) if lengths else 0,
                        family_counts=family_counts)
