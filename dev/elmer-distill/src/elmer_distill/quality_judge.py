"""Pairwise blind quality judging (tuxlink-48nyh).

Measures report QUALITY the mechanical gate can't see: given two anonymized drafted
reports for the same task, a strong judge picks the better one. A/B order is randomized
per scenario (position-bias control) and un-shuffled back to the model on parse. The
judge is injectable (APIClient -> OpenRouter in prod, a fake in tests).
"""
import json
import re

STAGING_TOOLS = {"message_send", "send_form", "aprs_send_message"}
_VERDICT = re.compile(r"VERDICT:\s*(A|B|TIE)", re.IGNORECASE)

_JUDGE_SYSTEM = (
    "You are a senior emergency-communications net-control operator evaluating two "
    "AI-drafted deliverables for the SAME tasking. Judge which is higher QUALITY: "
    "coherence, completeness, correct and specific use of the real data provided, and "
    "practical usefulness to a net-control operator under load. Do NOT reward length or "
    "verbosity; reward precision and clarity. The two may look similar — make the finest "
    "distinction you can. End your reply with exactly one line: 'VERDICT: A', 'VERDICT: B', "
    "or 'VERDICT: TIE' (TIE only if genuinely indistinguishable), preceded by one sentence "
    "of reasoning."
)


def extract_report(traj):
    """The drafted deliverable a judge should read: staged message/report bodies plus the
    final synthesis. This is what the model PRODUCED, not the tool-call scaffolding."""
    parts, final = [], ""
    for t in traj.get("turns", []):
        if t.get("role") != "assistant":
            continue
        for tc in (t.get("tool_calls") or []):
            fn = tc.get("function", {}) or {}
            if fn.get("name") in STAGING_TOOLS:
                args = fn.get("arguments") or {}
                if isinstance(args, str):
                    try:
                        args = json.loads(args)
                    except Exception:
                        args = {}
                body = args.get("body") or args.get("text") or ""
                if body:
                    parts.append(f"[{fn['name']}] {body}")
        if t.get("content"):
            final = t["content"]
    if final:
        parts.append(f"[final] {final}")
    return "\n\n".join(parts).strip()


def build_judge_prompt(task, report_a, report_b):
    user = (f"TASKING:\n{task}\n\n"
            f"--- REPORT A ---\n{report_a or '(empty)'}\n\n"
            f"--- REPORT B ---\n{report_b or '(empty)'}\n\n"
            "Which report is higher quality for the operator?")
    return [{"role": "system", "content": _JUDGE_SYSTEM},
            {"role": "user", "content": user}]


def parse_verdict(text):
    m = None
    for m in _VERDICT.finditer(text or ""):
        pass                      # keep the LAST verdict marker
    return m.group(1).upper() if m else None


def combined_summary(reports, judged_rows):
    """Fold quality in as a FIRST-CLASS metric alongside the mechanical gate (operator
    2026-07-04). `reports[sid]` carries per-model mechanical pass (`pass_20b`/`pass_120b`,
    written by the generate phase); `judged_rows` are the pairwise verdicts.

    The decision-bearing output is `parity_artifact` — scenarios BOTH models pass
    mechanically yet the 120b wins on quality. That cell is exactly what the 16-item
    predicate gate was blind to (the warc-vara hollow-plan class); reporting it is the
    whole point of folding quality in, so mechanical parity can never again be read as
    'the 20b is good enough'."""
    # injection-refusal cells are MEASURED, not graded (operator 2026-07-04): no model reliably
    # refuses prompt injection and Tuxlink defends it at the tool-layer guard, so a quality
    # comparison on those cells is out of scope — exclude them from the headline.
    scored_sids = {sid for sid, row in reports.items() if row.get("scored", True)}
    wins = {"120b": 0, "20b": 0, "tie": 0}
    verdict_by_sid = {}
    for r in judged_rows:
        if r["scenario"] not in scored_sids:
            continue
        wins[r["winner"]] += 1
        verdict_by_sid[r["scenario"]] = r["winner"]
    n = sum(wins.values())
    mech = {"20b_pass": 0, "120b_pass": 0}
    parity_artifact, quality_confirms_mechanical = [], []
    for sid, row in reports.items():
        if sid not in scored_sids:
            continue
        p20, p120 = bool(row.get("pass_20b")), bool(row.get("pass_120b"))
        mech["20b_pass"] += int(p20)
        mech["120b_pass"] += int(p120)
        w = verdict_by_sid.get(sid)
        if p20 and p120 and w == "120b":
            parity_artifact.append(sid)        # mechanically tied, quality favors 120b
        elif not p20 and p120 and w == "120b":
            quality_confirms_mechanical.append(sid)
    return {
        "n": n,
        "mechanical": mech,
        "quality": {"wins": wins,
                    "win_rate_120b": round(wins["120b"] / n, 2) if n else 0.0},
        "parity_artifact": sorted(parity_artifact),
        "quality_confirms_mechanical": sorted(quality_confirms_mechanical),
    }


def judge_pair(client, model, task, report_20b, report_120b, seed=0):
    """Blind pairwise judge. seed parity chooses which model is slot A vs B; the winning
    slot is mapped back to its model so position bias cannot favor either systematically."""
    swap = bool(seed % 2)                       # odd seed -> 120b is A
    a, b = (report_120b, report_20b) if swap else (report_20b, report_120b)
    slot_model = {"A": "120b" if swap else "20b", "B": "20b" if swap else "120b"}
    resp = client.chat(model, build_judge_prompt(task, a, b), tools=[])
    text = (resp.get("message", {}) or {}).get("content", "") or ""
    v = parse_verdict(text)
    winner = "tie" if v in (None, "TIE") else slot_model[v]
    return {"winner": winner, "order": "120b-first" if swap else "20b-first",
            "verdict": v, "reason": text.strip()[:500]}
