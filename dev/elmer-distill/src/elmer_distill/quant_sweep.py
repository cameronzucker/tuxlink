"""Quant-sweep comparison logic — pick the smallest quantization that holds the
gate score, for distribution (tuxlink-6zkb6).

The tuned model is served at several quantizations; the frozen gate is run on each.
Distribution ships the SMALLEST quant that does not cost a gate scenario (gpt-oss is
MXFP4-native, so ~4-bit is near home base — but the deterministic gate decides, not
the intuition). This module is pure + tested; the pod-only orchestration (ollama
quantize + serve + eval) lives in run_quant_sweep.py.
"""

# Small -> large. Distribution prefers the smallest quant that ties the best gate
# score, so a lower rank wins ties.
QUANT_RANK = {
    "Q2_K": 0, "Q3_K_M": 1, "Q4_K_S": 2, "Q4_K_M": 3, "Q5_K_S": 4, "Q5_K_M": 5,
    "Q6_K": 6, "Q8_0": 7, "F16": 8, "BF16": 9, "F32": 10,
}


def _rank(quant):
    return QUANT_RANK.get(quant.upper(), 99)


def recommend(rows):
    """Smallest-size quant whose gate score ties the best observed gate score.
    Ignores rows that failed to build/serve (`ok` is False). None if none succeeded."""
    ok = [r for r in rows if r.get("ok")]
    if not ok:
        return None
    best = max(r["gate_passed"] for r in ok)
    tied = [r for r in ok if r["gate_passed"] == best]
    return min(tied, key=lambda r: _rank(r["quant"]))["quant"]


def _gb(n):
    return f"{n / 1e9:.1f}G" if n else "?"


def _delta(passed, base_gate):
    if base_gate is None:
        return ""
    d = passed - base_gate
    return f"{d:+d}"


def sweep_report(rows, base_gate=None):
    """Render the quant/quality table + the distribution recommendation.

    rows: list of {quant, gate_passed, gate_total, probe_passed, probe_total,
                   size_bytes, ok}. base_gate: the base model's gate score, for a
    Δ column."""
    rec = recommend(rows)
    lines = []
    header = f"  {'quant':<9} {'gate':>7} {'Δbase':>6} {'probe':>7} {'size':>7}  note"
    lines.append(header)
    lines.append("  " + "-" * (len(header) - 2))
    for r in sorted(rows, key=lambda x: _rank(x["quant"])):
        q = r["quant"]
        if not r.get("ok"):
            lines.append(f"  {q:<9} {'FAILED':>7}")
            continue
        gate = f"{r['gate_passed']}/{r['gate_total']}"
        probe = f"{r['probe_passed']}/{r['probe_total']}"
        note = "<= ship this" if q == rec else ""
        lines.append(f"  {q:<9} {gate:>7} {_delta(r['gate_passed'], base_gate):>6} "
                     f"{probe:>7} {_gb(r.get('size_bytes', 0)):>7}  {note}")
    if rec:
        lines.append("")
        lines.append(f"  RECOMMEND for distribution: {rec} "
                     f"(smallest quant holding the best gate score)")
    else:
        lines.append("")
        lines.append("  RECOMMEND: none — every quant failed to build/serve")
    return "\n".join(lines)
