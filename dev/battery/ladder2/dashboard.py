#!/usr/bin/env python3
"""Ladder-2 progress dashboard. Read-only; scans the run tree per request.
Serves on the tailnet: http://r2-poe.twin-bramble.ts.net:8899/
"""
import html, json, os, glob, re, subprocess, datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
# LADDER2_ROOT lets this be render-tested against an rsync mirror off-box.
ROOT=os.path.expanduser(os.environ.get("LADDER2_ROOT","~/tuxlink-eig6e-build/battery-results/ladder2"))
PORT=int(os.environ.get("LADDER2_PORT","8899"))
# The corpus is what each rung actually ASKS. Looked up in a few places so this
# works both on R2 (alongside the build tree) and from a local mirror workdir.
CORPUS_CANDIDATES=[
    os.environ.get("LADDER2_CORPUS",""),
    os.path.join(ROOT,os.pardir,os.pardir,"tests","battery","corpus.json"),
    os.path.join(ROOT,"corpus.json"),
    os.path.join(os.path.dirname(os.path.abspath(__file__)),"corpus.json"),
]
CELLS=["P1","P2","P3","S1","S2","S3","S4","A1","A2","C1","C2","C3","E1","E2","E3","EU1","EU2","EU3"]
# The skill arm carries an extra review condition (Codex review-skill.md as the
# reviewer's system prompt). Listed here or its bundles render nowhere.
# rev_on columns RETIRED (tuxlink-jaer0): reasoning ON hurts the Nemotron
# reviewer (28% vs 39% pass) — settled; the matrix no longer renders it.
COLS=[("base","none"),("base","rev_off"),
      ("skill","none"),("skill","rev_off"),("skill","rev_skill")]
def ph(cond): return "build" if cond=="none" else cond
_corpus_cache={"path":None,"mtime":None,"cells":{}}
def corpus():
    """{cell_id: {title, prompt, predicates}}. Cached on mtime; never fatal --
    a missing/!unreadable corpus just means no hover cards, not a dead page."""
    for p in CORPUS_CANDIDATES:
        if not p or not os.path.exists(p): continue
        try: m=os.path.getmtime(p)
        except OSError: continue
        if _corpus_cache["path"]==p and _corpus_cache["mtime"]==m:
            return _corpus_cache["cells"]
        try:
            raw=json.load(open(p))
            items=raw.get("prompts") if isinstance(raw,dict) else raw
            cells={c["id"]:c for c in (items or []) if isinstance(c,dict) and "id" in c}
        except Exception:
            continue
        _corpus_cache.update({"path":p,"mtime":m,"cells":cells})
        return cells
    return {}
def rung_header(cell):
    """Row label; hovering it reveals what that rung asks + how it is graded."""
    c=corpus().get(cell)
    if not c:
        return f'<th class="rung">{html.escape(cell)}</th>'
    title=html.escape(c.get("title") or "")
    prompt=html.escape(c.get("prompt") or "(no prompt)")
    preds=c.get("predicates") or []
    plist="".join(f"<li>{html.escape(str(x))}</li>" for x in preds)
    return (f'<th class="rung">{html.escape(cell)}<span class="hint">&#9432;</span>'
            f'<div class="card"><div class="ct">{html.escape(cell)}'
            f'{" &mdash; "+title if title else ""}</div>'
            f'<div class="cl">PROMPT</div><pre class="cp">{prompt}</pre>'
            f'<div class="cl">PREDICATES ({len(preds)})</div><ol class="cq">{plist}</ol>'
            f'</div></th>')
def verdicts():
    v={}; p=os.path.join(ROOT,"judgments.jsonl")
    if os.path.exists(p):
        for l in open(p):
            try: r=json.loads(l); v[r["id"]]=r.get("overall")
            except: pass
    return v
def attempts(skill,cell,cond):
    """The attempt-N run directories, and ONLY those.

    A condition dir also holds `ledger.json` (a file) and, for rev_* conditions,
    `meta-N/` dirs carrying the reviewer's critique inputs. Neither is a run.
    Enumerating them unfiltered rendered a phantom badge for each: no score.json
    means read_att falls through to outcome="run", which the legend states as
    "ran & scored, awaiting judge". So every condition showed one spurious blue
    badge and every rev_ condition showed four, implying a judging backlog that
    did not exist."""
    d=os.path.join(ROOT,skill,cell,ph(cond))
    if not os.path.isdir(d): return []
    return sorted(a for a in os.listdir(d)
                  if a.startswith("attempt-") and os.path.isdir(os.path.join(d,a)))
def read_att(skill,cell,cond,a,V):
    b=os.path.join(ROOT,skill,cell,ph(cond),a)
    outcome=""; det=""
    try: outcome=json.load(open(os.path.join(b,"outcome.json"))).get("outcome","")
    except: outcome="run" if os.path.isdir(b) and not os.path.exists(os.path.join(b,"score.json")) else ""
    try:
        s=json.load(open(os.path.join(b,"score.json"))).get("deterministic") or {}
        det="sg" if (s.get("routine_saved") and s.get("validates_green")) else ("s" if s.get("routine_saved") else "x")
    except: det=""
    verd=V.get(f"{skill}/{cell}/{cond}/{a}","")
    return outcome,det,verd
def driver_state():
    """RUNNING / COMPLETE / STOPPED, for EITHER driver.

    The serial ladder2.sh and the parallel ladder2-par.sh produce the same tree,
    so the dashboard must recognise both; matching only the serial script's exact
    argv made a live parallel run read as STOPPED."""
    try:
        rl=open(os.path.join(ROOT,"run.log")).read()
        # run.log is append-only across runs, so a COMPLETE from a PREVIOUS run
        # is still in the file. Only the segment after the last START counts.
        tail=re.split(r"LADDER2(?:-PAR)? START",rl)[-1]
        if "LADDER2 COMPLETE" in tail or "LADDER2-PAR COMPLETE" in tail: return "COMPLETE"
    except: rl=""
    try:
        out=subprocess.run(["ps","-eo","args"],capture_output=True,text=True,timeout=5).stdout
        if not re.search(r"ladder2(-par)?\.sh", out): return "STOPPED"
        # Live worker count: how many cells are actually executing right now.
        # Anchored to line start on purpose -- `ps -eo args` shows TWO lines per
        # cell (the `/bin/sh /usr/bin/xvfb-run ...` wrapper repeats the full
        # binary path and argv), so an unanchored match reports double the width.
        n=len(re.findall(r"^\S*/target/debug/elmer_battery --corpus", out, re.M))
        return "RUNNING" if n<=1 else f"RUNNING &times;{n}"
    except: return "?"
def logtail(n=14):
    try: return "".join(open(os.path.join(ROOT,"run.log")).readlines()[-n:])
    except: return "(no run.log)"
VCOLOR={"PASS":"#1a7f37","PARTIAL":"#9a6700","FAIL":"#cf222e","":"#388bfd"}
OCOLOR={"completed":"#1a7f37","needs_operator":"#9a6700","cancelled":"#cf222e","invalid_action":"#cf222e","run":"#0969da","":"#ccc"}
RERUN_BORDER="#a371f7"   # purple: this bundle is in the re-run scope
PLAIN_BORDER="#30363d"
OPERATOR_BG="#e3742f"    # orange background when the run needed an operator
_rerun_cache={"mtime":None,"set":set()}
def rerun_targets():
    """{(arm, cell, cond)} pulled aside for a re-run, from _rerun_targets.txt.

    Read from the run tree rather than hardcoded, so the purple scope outline is
    always whatever was actually re-run. Absent file means no re-run in play and
    nothing gets outlined."""
    p=os.path.join(ROOT,"_rerun_targets.txt")
    if not os.path.exists(p):
        _rerun_cache.update({"mtime":None,"set":set()}); return _rerun_cache["set"]
    try: m=os.path.getmtime(p)
    except OSError: return _rerun_cache["set"]
    if _rerun_cache["mtime"]!=m:
        s=set()
        for l in open(p):
            q=l.split()
            if len(q)==3: s.add(tuple(q))
        _rerun_cache.update({"mtime":m,"set":s})
    return _rerun_cache["set"]
def badge(outcome,det,verd,rerun=False):
    """One run. Five independent facts, none of them displacing another:

    letter colour   judge verdict
    background      judge verdict, OVERRIDDEN to orange when the run needed an
                    operator (a truncation is not a verdict about the routine)
    border          raw harness outcome (completed / needs_operator / cancelled
                    / invalid_action / still running)
    subscript       the deterministic harness check (sg / s / x)
    OUTLINE ring    purple when this bundle is in the re-run scope

    The re-run marker rides on `outline` rather than `border` specifically so the
    border can keep carrying run state. `outline` sits outside the box and does
    not participate in layout, so the ring costs no width; the margin is widened
    on ringed badges only, to keep it from touching its neighbour.
    """
    vc=VCOLOR.get(verd,"#388bfd"); oc=OCOLOR.get(outcome,"#888")
    bg=(OPERATOR_BG if outcome=="needs_operator" else vc)+"33"
    ring=(f'outline:2px solid {RERUN_BORDER};outline-offset:1px;' if rerun else "")
    mar="4px 5px" if rerun else "1px"
    vtxt={"PASS":"P","PARTIAL":"~","FAIL":"F","":"?"}[verd]
    tip=f'{outcome} | det={det} | judge={verd or "unjudged"}'+(" | RE-RUN" if rerun else "")
    return (f'<span title="{tip}" style="display:inline-block;min-width:2.4em;margin:{mar};'
            f'padding:1px 4px;border-radius:4px;border:1px solid {oc};background:{bg};{ring}'
            f'color:{vc};font-weight:600">{vtxt}<sub style="color:{oc};font-weight:400">{det or "?"}</sub></span>')
def page():
    V=verdicts(); state=driver_state(); RR=rerun_targets()
    scored_ids=set()
    for sc in glob.glob(os.path.join(ROOT,"*/*/*/*/score.json")):
        p=os.path.dirname(sc).split(os.sep); cond="none" if p[-2]=="build" else p[-2]
        scored_ids.add(f"{p[-4]}/{p[-3]}/{cond}/{p[-1]}")
    scored=len(scored_ids)
    awaiting=len([i for i in scored_ids if i not in V])
    total=len(CELLS)*len(COLS)
    done=sum(1 for c in CELLS for (sk,cd) in COLS if any(os.path.exists(os.path.join(ROOT,sk,c,ph(cd),a,"score.json")) for a in attempts(sk,c,cd)))
    from collections import Counter
    tally=Counter(V[i] for i in scored_ids if i in V)
    rows=""
    for c in CELLS:
        tds=""
        for (sk,cd) in COLS:
            atts=attempts(sk,c,cd)
            if not atts: tds+='<td style="text-align:center;color:#21262d" title="not run yet">&middot;</td>'; continue
            rr=(sk,c,cd) in RR
            cell="".join(badge(*read_att(sk,c,cd,a,V),rerun=rr) for a in atts)
            tds+=f'<td style="text-align:center;white-space:nowrap">{cell}</td>'
        rows+=f'<tr>{rung_header(c)}{tds}</tr>'
    CLABEL={"none":"no&nbsp;review<br><span style='color:#8b949e;font-weight:400'>(raw build)</span>",
            "rev_off":"review<br><span style='color:#8b949e;font-weight:400'>reasoning OFF</span>",
            "rev_on":"review<br><span style='color:#8b949e;font-weight:400'>reasoning ON</span>",
            "rev_skill":"review<br><span style='color:#a371f7;font-weight:400'>SKILL prompt</span>"}
    ALABEL={"base":"base<br><span style='color:#8b949e;font-weight:400'>no scaffold</span>",
            "skill":"skill<br><span style='color:#8b949e;font-weight:400'>Build-Carefully</span>"}
    hdr="".join(f'<th>{ALABEL[sk]}<br>&nbsp;<br>{CLABEL[cd]}</th>' for sk,cd in COLS)
    sc="#1a7f37" if state.startswith("RUNNING") else ("#0969da" if state=="COMPLETE" else "#cf222e")
    now=datetime.datetime.now(datetime.timezone.utc).strftime("%H:%M:%SZ")
    return f'''<!doctype html><html><head><meta charset=utf-8>
<noscript><meta http-equiv=refresh content=20></noscript>
<title>Ladder2 {state}</title><style>body{{font:13px system-ui,sans-serif;margin:18px;background:#0d1117;color:#c9d1d9}}
table{{border-collapse:collapse}}td,th{{border:1px solid #30363d;padding:3px 5px}}th{{background:#161b22}}
a{{color:#58a6ff}}sub{{font-size:.7em}}
/* Rung label: hover reveals the prompt that rung actually asks + its predicates. */
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
/* Last rows would push the card off-screen; flip it upward. */
tr:nth-last-child(-n+5) th.rung .card{{top:auto;bottom:0}}</style></head><body>
<h2>Ladder 2 &nbsp;<span style="color:{sc}">{state}</span></h2>
<p>cells done: <b>{done}/{total}</b> conditions &nbsp;|&nbsp; scored bundles (incl. determinism re-runs): <b>{scored}</b>
&nbsp;|&nbsp; judged: <b>{sum(tally.values())}</b> (<span style="color:#1a7f37">P {tally.get("PASS",0)}</span> / <span style="color:#9a6700">~ {tally.get("PARTIAL",0)}</span> / <span style="color:#cf222e">F {tally.get("FAIL",0)}</span>) &nbsp;|&nbsp; <b style="color:#388bfd">awaiting judge: {awaiting}</b> &nbsp;|&nbsp; updated {now} (auto-refresh 20s)</p>
<div style="background:#161b22;border:1px solid #30363d;border-radius:6px;padding:10px 14px;margin:8px 0;max-width:900px">
<b>How to read a cell.</b> Each badge is one run. Multiple badges = determinism re-runs of the same condition.<br>
<b>Big letter = the Sonnet judge's verdict</b> (does the routine actually do what the prompt asked, judged against its predicates, not just "did it save"):
&nbsp;<span style="color:#1a7f37;font-weight:700">P</span> = PASS (meets the requirements) &nbsp;
<span style="color:#9a6700;font-weight:700">~</span> = PARTIAL (some requirements met, some dropped) &nbsp;
<span style="color:#cf222e;font-weight:700">F</span> = FAIL (drops or breaks a requirement) &nbsp;
<span style="color:#388bfd;font-weight:700">?</span> = ran &amp; scored, <b>awaiting judge</b>.<br>
A faint grey <span style="color:#6e7681">&middot;</span> in an otherwise empty cell = <b>not run yet</b> (the run hasn't reached it). Most empty cells are this, not a judging backlog.<br>
<b>Small subscript = the deterministic harness check</b> (mechanical, no judgment):
&nbsp;<b>sg</b> = a routine was saved AND passed validation (no errors) &nbsp;
<b>s</b> = saved but validation had an error &nbsp;
<b>x</b> = nothing was saved &nbsp; <b>?</b> = still running / no data.<br>
<b>Hover any badge</b> for the raw run outcome (completed / cancelled / needs_operator / invalid_action).<br>
<b>Background</b> = the judge verdict, except <span style="background:#e3742f33;padding:0 4px;border-radius:3px">orange</span> which means the run <b>needed an operator</b> (it was truncated, so it is not a verdict about the routine).<br>
<b>Badge border</b> = the raw harness outcome (green completed, amber needed-operator, red cancelled / invalid_action, blue still running).<br>
<b><span style="color:#a371f7">Purple ring</span> = this bundle is in the re-run scope</b> &mdash; the conditions pulled aside and re-run under raised deadlines. The ring sits OUTSIDE the border, so a badge shows its run state and its re-run membership at the same time. Everything without a ring is from the original run.<br>
<span style="color:#8b949e"><b>Columns</b> are builder-arm x review-condition. Within an arm, left-to-right is the experiment: raw build &rarr; after a Nemotron review+revise (reasoning off) &rarr; same with reasoning on. Compare the three to see whether the review helped, hurt, or did nothing.</span>
</div>
<table><tr><th></th>{hdr}</tr>{rows}</table>
<h3>run.log</h3><pre style="background:#161b22;padding:8px;border-radius:6px;overflow:auto;max-height:280px">{logtail()}</pre>
<script>
/* Auto-refresh, but never yank a prompt card out from under you mid-read.
   Replaces the old <meta refresh>, which reloaded unconditionally every 20s. */
var over=false;
document.addEventListener('mouseover',function(e){{if(e.target.closest&&e.target.closest('th.rung'))over=true;}});
document.addEventListener('mouseout', function(e){{if(e.target.closest&&e.target.closest('th.rung'))over=false;}});
setInterval(function(){{if(!over)location.reload();}},20000);
</script>
</body></html>'''
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        try: body=page().encode()
        except Exception as e: body=f"error: {e}".encode()
        self.send_response(200); self.send_header("Content-Type","text/html; charset=utf-8")
        self.send_header("Content-Length",str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self,*a): pass
if __name__=="__main__":
    ThreadingHTTPServer(("0.0.0.0",PORT),H).serve_forever()
