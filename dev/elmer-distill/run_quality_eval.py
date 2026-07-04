#!/usr/bin/env python3
"""Pairwise QUALITY eval: is the 120b's drafted report actually better than the 20b's,
beyond mechanical predicate-pass? (tuxlink-48nyh, operator's 2026-07-04 point.)

Two phases so generation (pod/ollama, both models) and judging (OpenRouter, per-token)
run where the access is:

  # on the pod (both models loaded):
  python3 run_quality_eval.py --phase generate --out eval-runs/quality
  # anywhere with the key ($ELMER_TEACHER_API_KEY):
  python3 run_quality_eval.py --phase judge --out eval-runs/quality \
      --api-base https://openrouter.ai/api/v1 --judge-model deepseek/deepseek-r1

`generate` runs 20b-scaffold + 120b-scaffold on each gate scenario (predicate-surfaced
checklist) and saves the drafted reports. `judge` blind-pairwise-judges them, randomizing
A/B per scenario, and writes a win-rate summary + an anonymized sample for the operator
spot-read (the human anchor for the LLM judge).

JUDGE CHOICE (do not default to a weak/cheap judge for fidelity): for a ONE-OFF read, the
highest-fidelity judge is the most capable model on hand — the operator + the in-loop
frontier assistant — since neither is a candidate (no self-grading bias). This scriptable
`judge` phase exists ONLY for what a human/in-loop judge can't do: a REPEATABLE, headless
instrument to score candidate teachers, re-score post-training students, and run n times
unattended. When used that way, set --judge-model to the MOST capable model available
(a frontier model via OpenRouter — anthropic/claude-*, openai/gpt-*, google/gemini-*),
and CALIBRATE it against the human + in-loop verdicts on the first sample before trusting
it to scale. Cheap open judges (deepseek/*) are a scale/cost option only after calibration.
"""
import argparse
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill.scenario import Scenario                 # noqa: E402
from elmer_distill.ollama_client import OllamaClient        # noqa: E402
from elmer_distill.api_client import APIClient              # noqa: E402
from elmer_distill.baseline_g0 import run_g0                # noqa: E402
from elmer_distill.judge import Judge                       # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402
from elmer_distill.quality_judge import extract_report, judge_pair, combined_summary  # noqa: E402


def _generate(a, scns):
    tools = load_tools()
    judge = Judge()
    reports = {}
    for scn in scns:
        row = {"task": scn.prompt, "scored": scn.scored}
        for tag, model in (("20b", a.student_model), ("120b", a.teacher_model)):
            client = OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx, temperature=a.temperature)
            traj = run_g0(client, model, scn, SYSTEM_PROMPT, tools, exemplars=[],
                          max_reprompts=a.max_reprompts, max_turns=a.max_turns)
            row[f"report_{tag}"] = extract_report(traj)
            # fold the MECHANICAL gate in on the same trajectory, so one run yields both
            # signals and the judge phase can flag mechanical-parity-but-quality-gap cells.
            row[f"pass_{tag}"] = judge.score(scn, traj, armed=scn.spec.requires_arm).passed
            print(f"  [generate {tag:<4} {scn.id:<34}] {len(row[f'report_{tag}'])} chars "
                  f"mech={'PASS' if row[f'pass_{tag}'] else 'fail'}", flush=True)
        reports[scn.id] = row
    path = os.path.join(a.out, "reports.json")
    os.makedirs(a.out, exist_ok=True)
    with open(path, "w") as f:
        json.dump(reports, f, indent=2)
    print(f"[generate] {len(reports)} paired reports -> {path}")


def _judge(a, reports):
    client = APIClient(base_url=a.api_base, max_tokens=a.judge_max_tokens, temperature=0)
    wins = {"120b": 0, "20b": 0, "tie": 0}
    rows, sample = [], []
    for i, (sid, row) in enumerate(sorted(reports.items())):
        v = judge_pair(client, a.judge_model, row["task"],
                       row.get("report_20b", ""), row.get("report_120b", ""), seed=i)
        wins[v["winner"]] += 1
        rows.append({"scenario": sid, **v})
        print(f"  [judge {sid:<34}] winner={v['winner']:<5} ({v['order']})", flush=True)
        sample.append(f"### {sid}\n**A:** {(row['report_20b'] if v['order']=='20b-first' else row['report_120b'])[:600]}\n\n"
                      f"**B:** {(row['report_120b'] if v['order']=='20b-first' else row['report_20b'])[:600]}\n\n"
                      f"_judge: {v['reason'][:300]}_\n")
    n = len(rows)
    combined = combined_summary(reports, rows)   # first-class: mechanical gate + quality together
    summary = {"judge_model": a.judge_model, "n": n, "wins": wins,
               "win_rate_120b": round(wins["120b"] / n, 2) if n else 0.0,
               "mechanical": combined["mechanical"],
               "parity_artifact": combined["parity_artifact"],
               "quality_confirms_mechanical": combined["quality_confirms_mechanical"],
               "per_scenario": rows}
    with open(os.path.join(a.out, "quality-summary.json"), "w") as f:
        json.dump(summary, f, indent=2)
    # anonymized sample for the operator spot-read (A/B, models hidden)
    with open(os.path.join(a.out, "spot-read.md"), "w") as f:
        f.write("# Quality spot-read — A vs B blind (which is the better operator report?)\n\n"
                + "\n".join(sample))
    mech = combined["mechanical"]
    print(f"\n=== MECHANICAL gate: 20b {mech['20b_pass']}/{n}  120b {mech['120b_pass']}/{n} ===")
    print(f"=== QUALITY: 120b {wins['120b']} / 20b {wins['20b']} / tie {wins['tie']}  "
          f"(120b win-rate {summary['win_rate_120b']:.0%}, judge={a.judge_model}) ===")
    if combined["parity_artifact"]:
        print(f"=== PARITY ARTIFACT ({len(combined['parity_artifact'])}): both pass mechanically, "
              f"120b wins on quality -> {', '.join(combined['parity_artifact'])} ===")
    print(f"summary -> {a.out}/quality-summary.json ; blind sample -> {a.out}/spot-read.md")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--phase", choices=["generate", "judge", "all"], default="all")
    ap.add_argument("--student-model", default="gpt-oss:20b")
    ap.add_argument("--teacher-model", default="gpt-oss:120b")
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--temperature", type=float, default=0.0)   # canonical (greedy) report per model
    ap.add_argument("--max-turns", type=int, default=40)
    ap.add_argument("--max-reprompts", type=int, default=2)
    ap.add_argument("--api-base", default="https://openrouter.ai/api/v1")
    ap.add_argument("--judge-model", default="anthropic/claude-opus-4.8",
                    help="MOST capable model available (frontier via OpenRouter). For a one-off "
                         "read prefer the in-loop assistant + human anchor directly; this scriptable "
                         "path is for headless SCALE (candidate teachers, post-train re-eval, n-repeats) "
                         "and must be calibrated to the human/in-loop verdicts before it is trusted.")
    ap.add_argument("--judge-max-tokens", type=int, default=16000)
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "quality"))
    a = ap.parse_args()

    if a.phase in ("generate", "all"):
        scns = [Scenario.from_json(json.load(open(p)))
                for p in sorted(glob.glob(os.path.join(a.candidates, "*.json")))]
        _generate(a, scns)
    if a.phase in ("judge", "all"):
        reports = json.load(open(os.path.join(a.out, "reports.json")))
        _judge(a, reports)


if __name__ == "__main__":
    main()
