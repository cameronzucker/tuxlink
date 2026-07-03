"""Qualitative before/after probe (tuxlink-6zkb6).

The deterministic gate scores tool-trajectory + evidence predicates — it is blind
to whether the model UNDERSTOOD the task (antenna reasoning, adaptive re-planning,
doc-grounded correctness). Those legs are marked *flavor (ungraded)* in the gate.
This probe recovers that lost signal WITHOUT corrupting the empirical measure or
the training loop:

  - it runs only the held-out `operator_authored` scenarios (the contamination
    guard keeps them out of gold), so "after" measures generalization, not
    memorization;
  - it persists full transcripts per run label (e.g. `base-20b` vs `lora-phaseA`)
    to disk so the two are diffable;
  - it renders a side-by-side card doc the operator fills BY HAND — no auto score,
    no gate. The deterministic verdict rides along as context only.

Run `run(...)` once per model (before/after), then `render_cards(before, after,
...)` to emit the hand-eval doc.
"""
import json
import os
from dataclasses import dataclass, field

from .teacher import run_scenario
from .judge import Judge


def select(scenarios):
    """The probe set = the held-out operator-authored scenarios."""
    return [s for s in scenarios if s.operator_authored]


def _traj_path(out_dir, label, scenario_id):
    return os.path.join(out_dir, label, f"{scenario_id}.json")


@dataclass
class ProbeResult:
    scenario_id: str
    passed: bool
    reasons: list
    transcript_path: str


@dataclass
class ProbeReport:
    label: str
    results: list = field(default_factory=list)

    def by_id(self):
        return {r.scenario_id: r for r in self.results}


def run(client, model, scenarios, system, tools, out_dir, label, max_turns=20):
    """Run the probe set through `model`, persist each full transcript under
    `{out_dir}/{label}/`, and return per-scenario verdicts (context only)."""
    judge = Judge()
    rep = ProbeReport(label=label)
    os.makedirs(os.path.join(out_dir, label), exist_ok=True)
    for s in select(scenarios):
        traj = run_scenario(client, model, s, system, tools, max_turns)
        verdict = judge.score(s, traj, armed=s.spec.requires_arm)
        path = _traj_path(out_dir, label, s.id)
        with open(path, "w") as f:
            json.dump(traj, f, indent=2)
        rep.results.append(ProbeResult(
            scenario_id=s.id, passed=verdict.passed,
            reasons=list(verdict.reasons), transcript_path=path))
    return rep


def load_rubric(path):
    with open(path) as f:
        d = json.load(f)
    return {k: v for k, v in d.items() if not k.startswith("_")}


def _verdict_str(r):
    if r is None:
        return "—"
    return "PASS" if r.passed else "fail"


def render_cards(before, after, scenarios, rubric, out_path):
    """Emit the hand-eval markdown: one card per probe scenario, base-vs-trained,
    with the ungraded legs to judge and blank base/trained/delta fields."""
    by_id = {s.id: s for s in scenarios}
    b, a = before.by_id(), after.by_id()
    lines = [
        "# Elmer qualitative probe — hand-eval cards (tuxlink-6zkb6)",
        "",
        f"Before run: `{before.label}` · After run: `{after.label}`.",
        "",
        "Read the two transcripts per scenario and fill `base:` / `trained:` / "
        "`delta:` for each leg BY HAND. These legs are what the deterministic gate "
        "cannot see. The empirical verdict shown per card is **context only** — it "
        "does not gate and does not score this probe.",
        "",
    ]
    for s in select(scenarios):
        sid = s.id
        br, ar = b.get(sid), a.get(sid)
        lines += [
            f"## {sid}",
            "",
            f"**Prompt:** {s.prompt}",
            "",
            f"**Empirical (context only):** base `{_verdict_str(br)}` → trained `{_verdict_str(ar)}`",
            f"**Transcripts:** base `{br.transcript_path if br else '—'}` · "
            f"trained `{ar.transcript_path if ar else '—'}`",
            "",
            "**Hand-judge legs:**",
        ]
        for leg in rubric.get(sid, ["(no rubric legs — read the transcript holistically)"]):
            lines += [f"- {leg}", "  - base: ", "  - trained: ", "  - delta: "]
        lines += ["", "---", ""]
    text = "\n".join(lines)
    with open(out_path, "w") as f:
        f.write(text)
    return text
