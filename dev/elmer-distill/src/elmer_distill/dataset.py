"""Dataset assembler — gold trajectories -> Harmony training JSONL.

Each row: {"text": <harmony training text>, "loss_spans": [[start,end], ...]}
with loss masked to assistant-generated content only (Codex adrev — mandatory
for agentic SFT). Reports P95 rendered length, which sets Phase-A
`max_seq_length` (spec §5.4): truncating long multi-tool traces would erase the
exact turn-3+ behavior we are training.
"""
import json
from dataclasses import dataclass, field

from . import harmony


@dataclass
class DatasetStats:
    n: int
    p95_chars: int
    max_chars: int
    family_counts: dict = field(default_factory=dict)


def _family_of(scenario_id):
    return (scenario_id or "").split("-")[0]


def assemble(gold, system, out_path):
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
