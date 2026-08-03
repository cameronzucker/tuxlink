#!/usr/bin/env python3
"""Battery cross-model comparison artifact generator (tuxlink-bssvw).

Reads the fingerprint-joined run files in data/ and emits ONE self-contained
HTML file (battery-comparison.html): every run's data embedded, zero external
fetches, opens anywhere. The per-attempt badge grammar mirrors the live run
dashboard (dev-side dashboard.py) so the artifact reads the same way:

    letter + colour   judge verdict (P / ~ / F)
    background        verdict tint, OVERRIDDEN orange when the harness outcome
                      was needs_operator (a truncation is not a verdict)
    border            raw harness outcome
    subscript         deterministic harness check (sg / s / x)
    strikethrough     harness-invalid (excluded from all rates)

2026-08-03 additions: cell-scenario hover on row headers (mirrors the live
dashboard), run-selection checkboxes for side-by-side comparison, and the
capability-bucket rollup (Task Rabbit / Assistant / Collaborator / Elmer).

Add a run: drop its *_joined.json in data/, add a RUNS entry, re-run this
script, commit both. Runs are columns; corpus cells are rows.
"""

import json
import os
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "data")
OUT = os.path.join(HERE, "battery-comparison.html")

# Cell scenarios (id -> {title, prompt}) for row-header hover, mirroring the
# live dashboard. Source: the frozen judge corpus (data/corpus_scenarios.json).
try:
    with open(os.path.join(DATA, "corpus_scenarios.json")) as _f:
        SCENARIOS = json.load(_f)
except OSError:
    SCENARIOS = {}

# Capability buckets (operator taxonomy 2026-08-03). Cell-letter scheme:
# P/S = explicit stepwise builds (Task Rabbit), A = Assistant,
# C = Collaborator, E + EU = Elmer / end-user realism. Edit here to re-bucket.
BUCKETS = [
    ("Task Rabbit",  ["P1", "P2", "P3", "S1", "S2", "S3", "S4"]),
    ("Assistant",    ["A1", "A2"]),
    ("Collaborator", ["C1", "C2", "C3"]),
    ("Elmer / E-U",  ["E1", "E2", "E3", "EU1", "EU2", "EU3"]),
]

# Ordered run manifest: newest knowledge about each arm lives here, not in the
# data files. `caveats` render under the column header; keep them short.
RUNS = [
    {
        "file": "control1_joined.json",
        "key": "control1",
        "label": "Qwen3.5 122B",
        "sub": "control1-base &middot; temp 0.2 &middot; ctx 262k",
        "generation": "bc9bc648",
        "date": "2026-07-30",
        "caveats": "C2/EU2 harness-invalid (aymi7, pre-fix generation)",
    },
    {
        "file": "laguna1_joined.json",
        "key": "laguna1",
        "label": "Laguna-S 2.1",
        "sub": "laguna1-t07 &middot; temp 0.7 &middot; ctx 262k",
        "generation": "bc9bc648",
        "date": "2026-07-30",
        "caveats": "21% turn-cap exhaustion (fast churn); HF revision 07614121 (256K build, since superseded upstream)",
    },
    {
        "file": "mistral1_joined.json",
        "key": "mistral1",
        "label": "Mistral Small 4 119B",
        "sub": "mistral1-t015 &middot; temp 0.15 &middot; ctx 32k (host cap)",
        "generation": "bc9bc648+idfix",
        "date": "2026-07-31",
        "caveats": "42% context-censored by the 32k full-KV host ceiling",
    },
    {
        "file": "control2_joined.json",
        "key": "control2",
        "label": "Qwen3.5 122B (gen 2)",
        "sub": "control2-base &middot; temp 0.2 &middot; ctx 262k",
        "generation": "40fd9b7e",
        "date": "2026-07-31",
        "caveats": "bridge control on the aymi7+grc1j+idfix generation; first valid C2/EU2",
    },
    {
        "file": "inkling1_joined.json",
        "key": "inkling1",
        "label": "Inkling-Small NVFP4 276B-A12B",
        "sub": "inkling1 &middot; temp 0.2 &middot; ctx 262k &middot; TP2 dual-Spark &middot; conc 1",
        "generation": "40fd9b7e",
        "date": "2026-08-02",
        "caveats": "serial run (sconv kernel multi-request fault, dev/runbooks/inkling-dual-spark); ELMER_MAX_TOKENS=3000; 7 invalid_action = null-args tic (bohfp)",
    },
    {
        "file": "q235_joined.json",
        "key": "q235",
        "label": "Qwen3 235B-A22B 2507 NVFP4",
        "sub": "q235 &middot; temp 0.2 &middot; ctx 262k &middot; TP2 dual-Spark &middot; conc 8",
        "generation": "40fd9b7e",
        "date": "2026-08-02",
        "caveats": "size-class control for inkling1; 20 cancelled (turn/wall churn); EU2#2 harness-invalid egress-guard assert (lmrd4); first fully wall-metered arm (0.824 kWh, ~347 W)",
    },
    {
        "file": "mistral2_joined.json",
        "key": "mistral2",
        "label": "Mistral Small 4 119B (262k rerun)",
        "sub": "mistral2 &middot; temp 0.15 &middot; ctx 262k &middot; TP2 dual-Spark &middot; conc 8",
        "generation": "40fd9b7e",
        "date": "2026-08-02",
        "caveats": "mistral1 rerun without the 32k ceiling — uncensored ctx does not rescue it; 11 provider_error = strict tekken-encoder 400s (5uwnj), judged FAIL; 0.618 kWh, ~308 W",
    },
    {
        "file": "gptoss1_joined.json",
        "key": "gptoss1",
        "label": "gpt-oss-120b MXFP4",
        "sub": "gptoss1-t10 &middot; temp 1.0 &middot; ctx 131k &middot; dual-solo &middot; conc 4",
        "generation": "40fd9b7e",
        "date": "2026-08-02",
        "caveats": "cross-vendor arm; widest PARTIAL band in the set; 21 cancelled = temp-1.0 reasoning churn (6 on P1 alone); 2 egress-assert harness-invalid (lmrd4); 1.244 kWh, ~258 W",
    },
    {
        "file": "dsv4_joined.json",
        "key": "dsv4",
        "label": "DeepSeek-V4-Flash-0731 NVFP4 304B",
        "sub": "dsv4 &middot; temp 1.0 top_p 0.95 &middot; ctx 262k &middot; TP2 dual-Spark &middot; conc 8",
        "generation": "40fd9b7e",
        "date": "2026-08-03",
        "caveats": "first frontier-competitive arm (community MJPansa quant); record 82 PARTIALs + fewest FAILs; first to crack C1/C2 graveyard; 6 egress-assert kills (lmrd4 worst rate, S4 n=7); 0.886 kWh, ~303 W",
    },
    {
        "file": "laguna2_joined.json",
        "key": "laguna2",
        "label": "Laguna-S 2.1 (Aug build rerun)",
        "sub": "laguna2 &middot; temp 0.7 &middot; ctx 131k + fp8 KV &middot; dual-solo &middot; conc 16",
        "generation": "40fd9b7e",
        "date": "2026-08-03",
        "caveats": "OPERATOR-TERMINATED at 84/180 (Sparks reassigned to tuxlink-bench) — partial, uneven cell coverage; CURRENT shipping Laguna (Aug-1 1M promotion, +28GB, rev f8fdfcdc) at 131k+fp8 vs laguna1 retired 256K build; churn persists (14 cancelled); see bd tuxlink-jwdsa",
    },
    {
        "file": "opus1_joined.json",
        "key": "opus1",
        "label": "Claude Opus 5 (ceiling check)",
        "sub": "opus1 &middot; subscription shim &middot; prompt-encoded tools &middot; conc 2",
        "generation": "40fd9b7e",
        "date": "2026-08-03",
        "caveats": "NOT interface-comparable (prompt-encoded tool bridge; no temp control); instrument-validation arm; 30 slots xvfb-collision-killed (A1/S3/S4 wiped); 49 null-args invalid_action = suspected shim/SSE infra, non-signal",
    },
]

CELLS = ["P1", "P2", "P3", "S1", "S2", "S3", "S4", "A1", "A2",
         "C1", "C2", "C3", "E1", "E2", "E3", "EU1", "EU2", "EU3"]

VCOLOR = {"PASS": "#1a7f37", "PARTIAL": "#9a6700", "FAIL": "#cf222e", "": "#388bfd"}
OCOLOR = {"completed": "#1a7f37", "needs_operator": "#9a6700", "cancelled": "#cf222e",
          "invalid_action": "#cf222e", "provider_error": "#cf222e",
          "tool_denied": "#8250df", "truncated": "#cf222e",
          "unit_failed": "#8250df", "": "#ccc"}
OPERATOR_BG = "#e3742f"
VTXT = {"PASS": "P", "PARTIAL": "~", "FAIL": "F", "": "?"}



import html as _html

def rung_th(cell):
    """Row label; hovering reveals what the rung asks + how it is graded —
    markup and classes mirror the live run dashboard exactly."""
    c = SCENARIOS.get(cell)
    if not c:
        return f'<th class="rung">{_html.escape(cell)}</th>'
    title = _html.escape(c.get("title") or "")
    prompt = _html.escape(c.get("prompt") or "(no prompt)")
    preds = c.get("predicates") or []
    plist = "".join(f"<li>{_html.escape(str(x))}</li>" for x in preds)
    return (f'<th class="rung">{_html.escape(cell)}<span class="hint">&#9432;</span>'
            f'<div class="card"><div class="ct">{_html.escape(cell)}'
            f'{" &mdash; " + title if title else ""}</div>'
            f'<div class="cl">PROMPT</div><pre class="cp">{prompt}</pre>'
            f'<div class="cl">PREDICATES ({len(preds)})</div><ol class="cq">{plist}</ol>'
            f'</div></th>')

def det_code(det):
    """Compact deterministic-check code, mirroring the dashboard subscript."""
    if not det:
        return "?"
    s = "s" if det.get("routine_saved") else ""
    g = "g" if det.get("validates_green") else ""
    return (s + g) or "x"


def badge(row):
    verd = row.get("overall") or ""
    outcome = row.get("outcome") or ""
    invalid = bool(row.get("harness_invalid"))
    vc = VCOLOR.get(verd, "#388bfd")
    oc = OCOLOR.get(outcome, "#888")
    bg = (OPERATOR_BG if outcome == "needs_operator" else vc) + "33"
    deco = "text-decoration:line-through;opacity:.55;" if invalid else ""
    det = det_code(row.get("det"))
    tip = f"attempt-{row.get('attempt')}: {outcome} | det={det} | judge={verd or 'unjudged'}"
    if invalid:
        tip += f" | HARNESS-INVALID ({row['harness_invalid']})"
    return (f'<span title="{tip}" style="display:inline-block;min-width:2.2em;margin:1px;'
            f'padding:1px 4px;border-radius:4px;border:1px solid {oc};background:{bg};{deco}'
            f'color:{vc};font-weight:600">{VTXT[verd]}'
            f'<sub style="color:{oc};font-weight:400">{det}</sub></span>')


def load_runs():
    out = []
    for spec in RUNS:
        path = os.path.join(DATA, spec["file"])
        if not os.path.exists(path):
            continue
        with open(path) as f:
            d = json.load(f)
        rows = d["rows"]
        by_cell = {}
        for r in rows:
            by_cell.setdefault(r["cell"], []).append(r)
        for v in by_cell.values():
            v.sort(key=lambda r: r["attempt"])
        valid = [r for r in rows if not r.get("harness_invalid")]
        censored = [r for r in valid if r.get("outcome") == "needs_operator"]
        nc = [r for r in valid if r.get("outcome") != "needs_operator"]
        t = Counter(r.get("overall") for r in valid)
        buckets = {}
        for bname, bcells in BUCKETS:
            bv = [r for r in valid if r["cell"] in bcells]
            bt = Counter(r.get("overall") for r in bv)
            n = len(bv)
            buckets[bname] = {
                "n": n,
                "strict": (100.0 * bt.get("PASS", 0) / n) if n else None,
                "lenient": (100.0 * (bt.get("PASS", 0) + 0.5 * bt.get("PARTIAL", 0)) / n) if n else None,
            }
        out.append({
            **spec, "by_cell": by_cell, "buckets": buckets,
            "stats": {
                "bundles": len(rows),
                "valid": len(valid),
                "invalid": len(rows) - len(valid),
                "pass": t.get("PASS", 0), "partial": t.get("PARTIAL", 0),
                "fail": t.get("FAIL", 0),
                "rate": (100.0 * t.get("PASS", 0) / len(valid)) if valid else 0.0,
                "lenient": (100.0 * (t.get("PASS", 0) + 0.5 * t.get("PARTIAL", 0)) / len(valid)) if valid else 0.0,
                "censored": len(censored),
                "nc_rate": (100.0 * sum(1 for r in nc if r.get("overall") == "PASS")
                            / len(nc)) if nc else 0.0,
                "outcomes": dict(Counter(r.get("outcome") for r in rows)),
            },
        })
    return out


def bar(pct, color, w=160):
    return (f'<div style="background:#21262d;border-radius:3px;height:10px;width:{w}px">'
            f'<div style="background:{color};border-radius:3px;height:10px;'
            f'width:{max(1, round(pct * w / 100))}px"></div></div>')


def bucket_color(pct):
    if pct is None:
        return "#484f58"
    if pct >= 60:
        return "#1a7f37"
    if pct >= 30:
        return "#9a6700"
    return "#cf222e"


def page(runs):
    # Run-selector control bar (side-by-side comparison).
    sel = "".join(
        f'<label style="margin-right:14px;white-space:nowrap"><input type="checkbox" '
        f'checked data-toggle="{r["key"]}" onchange="tg(this)"> {r["label"]}</label>'
        for r in runs)
    selector = (f'<div style="position:sticky;top:0;background:#0d1117ee;padding:8px 4px;'
                f'border-bottom:1px solid #30363d;z-index:5"><b>Compare:</b> {sel} '
                f'<a href="#" onclick="allr(true);return false">all</a> / '
                f'<a href="#" onclick="allr(false);return false">none</a>'
                f'<span class=sub style="margin-left:10px">untick runs to put any two side by side</span></div>')

    # Topline summary table.
    srows = ""
    for r in runs:
        s = r["stats"]
        srows += (
            f'<tr data-run="{r["key"]}"><td><b>{r["label"]}</b><br><span class=sub>{r["sub"]}</span></td>'
            f'<td>{r["generation"]}</td>'
            f'<td style="text-align:right">{s["valid"]}/{s["bundles"]}</td>'
            f'<td style="text-align:right"><b>{s["rate"]:.1f}%</b><br>'
            f'{bar(s["rate"], "#1a7f37")}</td>'
            f'<td style="text-align:right">{s["lenient"]:.1f}%<br>'
            f'{bar(s["lenient"], "#2da44e")}</td>'
            f'<td style="text-align:right">{s["pass"]} / {s["partial"]} / {s["fail"]}</td>'
            f'<td style="text-align:right">{s["censored"]}</td>'
            f'<td style="text-align:right">{s["invalid"]}</td>'
            f'<td class=sub>{r["caveats"]}</td></tr>')

    # Capability-bucket rollup: rows = buckets, columns = runs.
    bhdr = "".join(f'<th data-run="{r["key"]}">{r["label"]}</th>' for r in runs)
    brows = ""
    for bname, bcells in BUCKETS:
        tds = ""
        for r in runs:
            b = r["buckets"].get(bname, {})
            st, le, n = b.get("strict"), b.get("lenient"), b.get("n", 0)
            if st is None:
                tds += f'<td data-run="{r["key"]}" style="text-align:center;color:#484f58">no data</td>'
            else:
                c = bucket_color(st)
                tds += (f'<td data-run="{r["key"]}" style="text-align:center">'
                        f'<b style="color:{c};font-size:15px">{st:.0f}%</b>'
                        f'<span class=sub> / {le:.0f}%</span><br>{bar(st, c, 90)}'
                        f'<span class=sub>n={n}</span></td>')
        cells_list = " ".join(bcells)
        brows += (f'<tr><th class=rung title="cells: {cells_list}">{bname}</th>{tds}</tr>')

    # Per-cell grid: rows = cells, columns = runs. Row header hovers carry the
    # cell's scenario title + full prompt (live-dashboard parity).
    hdr = "".join(
        f'<th data-run="{r["key"]}">{r["label"]}<br><span class=sub>{r["sub"]}</span></th>' for r in runs)
    rows_html = ""
    for cell in CELLS:
        tds = ""
        for r in runs:
            atts = r["by_cell"].get(cell, [])
            if not atts:
                tds += f'<td data-run="{r["key"]}" style="text-align:center;color:#484f58">&middot;</td>'
                continue
            tds += (f'<td data-run="{r["key"]}" style="text-align:center;white-space:nowrap">'
                    + "".join(badge(a) for a in atts) + "</td>")
        rows_html += f'<tr>{rung_th(cell)}{tds}</tr>' 

    # Outcome-class breakdown per run.
    all_outcomes = sorted({o for r in runs for o in r["stats"]["outcomes"] if o})
    orows = ""
    for o in all_outcomes:
        cells = "".join(
            f'<td data-run="{r["key"]}" style="text-align:right">{r["stats"]["outcomes"].get(o, 0)}</td>'
            for r in runs)
        oc = OCOLOR.get(o, "#888")
        orows += (f'<tr><th class=rung style="color:{oc}">{o}</th>{cells}</tr>')
    ohdr = "".join(f'<th data-run="{r["key"]}">{r["label"]}</th>' for r in runs)

    legend = (
        '<span class=sub>badge grammar (mirrors the live run dashboard): '
        'letter+colour = judge verdict (P pass, ~ partial, F fail) &middot; '
        'background = verdict tint, orange when the harness outcome was '
        'needs_operator &middot; border = raw harness outcome &middot; '
        'subscript = deterministic check (s saved, g validates-green, x neither) '
        '&middot; strikethrough = harness-invalid, excluded from every rate. '
        'Hover any badge for the exact facts; hover a row header for the '
        'cell scenario. Temperatures are per-vendor recommendations; '
        'cross-model comparisons are directional. Opus rides a different '
        'tool interface (see its caveats) — treat as instrument validation, '
        'not a ranking row.</span>')

    js = """<script>
function tg(cb){document.querySelectorAll('[data-run="'+cb.dataset.toggle+'"]')
  .forEach(function(e){e.style.display=cb.checked?'':'none'});}
function allr(on){document.querySelectorAll('[data-toggle]').forEach(function(cb){
  cb.checked=on;tg(cb);});}
</script>"""

    return f'''<!doctype html><html><head><meta charset=utf-8>
<title>Elmer battery: cross-model comparison</title>
<style>
 body{{background:#0d1117;color:#c9d1d9;font:14px/1.5 system-ui,sans-serif;margin:24px}}
 h1{{font-size:20px}} h2{{font-size:16px;margin-top:28px}}
 table{{border-collapse:collapse;margin-top:8px}}
 th,td{{border:1px solid #30363d;padding:5px 9px;vertical-align:top;text-align:left}}
 th{{background:#161b22}} th.rung{{background:#161b22;text-align:left}}
 .sub{{color:#8b949e;font-weight:400;font-size:12px}}
 sub{{font-size:9px}} a{{color:#58a6ff}}
 th.rung{{text-align:left;padding-right:8px;position:relative;cursor:help}}
 th.rung .hint{{color:#6e7681;margin-left:4px;font-weight:400}}
 th.rung:hover .hint{{color:#58a6ff}}
 th.rung .card{{display:none;position:absolute;left:100%;top:0;z-index:50;width:520px;
  max-height:60vh;overflow:auto;background:#161b22;border:1px solid #58a6ff;border-radius:6px;
  padding:10px 12px;box-shadow:0 8px 24px #010409cc;white-space:normal;font-weight:400;text-align:left}}
 th.rung:hover .card{{display:block}}
 th.rung .ct{{color:#58a6ff;font-weight:700;margin-bottom:6px}}
 th.rung .cl{{color:#8b949e;font-size:.78em;letter-spacing:.08em;margin:8px 0 3px}}
 th.rung .cp{{margin:0;padding:8px;background:#0d1117;border:1px solid #30363d;border-radius:4px;
  font:12px ui-monospace,SFMono-Regular,Menlo,monospace;white-space:pre-wrap;word-break:break-word;color:#c9d1d9}}
 th.rung .cq{{margin:0;padding-left:20px}}
 th.rung .cq li{{margin:3px 0;color:#c9d1d9}}
 tr:nth-last-child(-n+5) th.rung .card{{top:auto;bottom:0}}
</style></head><body>
{selector}
<h1>Elmer battery: cross-model comparison</h1>
<div class=sub>18 corpus cells &times; 10 attempts per model, dual-Spark GB10 cluster,
fingerprint-joined judge verdicts (sonnet-5 daemon). Regenerated by
generate_comparison.py; data embedded, no external fetches. The battery
continues as the standalone <b>tuxlink-bench</b> project — this artifact is
the v1-era record (see dev/battery/README.md).</div>
<h2>Capability buckets <span class=sub>strict% / lenient% of valid attempts</span></h2>
<table><tr><th class=rung>bucket</th>{bhdr}</tr>{brows}</table>
<h2>Topline</h2>
<table><tr><th>model / run</th><th>generation</th><th>valid</th>
<th>PASS rate<br><span class=sub>(strict)</span></th>
<th>lenient<br><span class=sub>(P+0.5&middot;PT)</span></th>
<th>P / ~ / F</th><th>ctx-censored</th><th>invalid</th><th>caveats</th></tr>
{srows}</table>
<h2>Per-cell, per-attempt</h2>
{legend}
<table><tr><th class=rung>cell</th>{hdr}</tr>{rows_html}</table>
<h2>Harness outcome classes</h2>
<table><tr><th class=rung>outcome</th>{ohdr}</tr>{orows}</table>
{js}
</body></html>'''


if __name__ == "__main__":
    runs = load_runs()
    html = page(runs)
    with open(OUT, "w") as f:
        f.write(html)
    print(f"wrote {OUT} ({len(html)} bytes, {len(runs)} runs)")
