#!/usr/bin/env python3
"""Grade the qwen3.7-max spot-check bundles with the SAME judge as the ladder.

Deliberately imports judge_daemon rather than reimplementing: the comparison of
interest is qwen3.7-max vs qwen3.5-122b, so the grader must be held constant. A
different rubric, or a human grader, would make the model difference and the
grader difference inseparable. This reuses RUBRIC and judge() verbatim.

Does NOT touch the ladder's store, and does not take the daemon's lock
(acquire_lock is only called from judge_daemon.main).

usage: grade_spot.py <spot_dir> <corpus.json> [out.jsonl]
"""
import importlib.util, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("jd", os.path.join(HERE, "judge_daemon.py"))
jd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(jd)

spot = sys.argv[1]
corpus_path = sys.argv[2]
out_path = sys.argv[3] if len(sys.argv) > 3 else os.path.join(spot, "spot-judgments.jsonl")

cells = {c["id"]: c for c in json.load(open(corpus_path))["prompts"]}
done = set()
if os.path.exists(out_path):
    done = {json.loads(l)["id"] for l in open(out_path)}

for cell in sorted(os.listdir(spot)):
    d = os.path.join(spot, cell)
    if not os.path.isdir(d) or not os.path.exists(os.path.join(d, "score.json")):
        continue
    bid = "spot37max/%s/none/attempt-1" % cell
    if bid in done:
        print("skip (already judged):", bid); continue
    sc = json.load(open(os.path.join(d, "score.json")))
    o = json.load(open(os.path.join(d, "outcome.json")))
    ji = sc.get("judge_input") or {}
    one = {
        "id": bid, "cell": cell, "skill": "base", "cond": "none",
        "prompt": ji.get("prompt") or cells[cell]["prompt"],
        "predicates": ji.get("predicates") or cells[cell]["predicates"],
        "outcome": o.get("outcome"),
        "deterministic": sc.get("deterministic"),
        "saved_def": (ji.get("artifacts") or {}).get("def"),
        "final_text": (o.get("detail") or "")[:1200],
    }
    try:
        v = jd.judge(one)
        v["judge"] = "sonnet-5"
        v["model_under_test"] = "qwen/qwen3.7-max"
        with open(out_path, "a") as f:
            f.write(json.dumps(v) + "\n")
        print("judged %-28s -> %s" % (bid, v.get("overall")))
    except Exception as e:
        print("FAIL %s: %s" % (bid, e))
print("\nwrote:", out_path)
