#!/usr/bin/env python3
"""Ladder-2 progress dashboard. Read-only; scans the run tree per request.
Serves on the tailnet: http://r2-poe.twin-bramble.ts.net:8899/
"""
import json, os, glob, subprocess, datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
ROOT=os.path.expanduser("~/tuxlink-eig6e-build/battery-results/ladder2")
PORT=8899
CELLS=["P1","P2","P3","S1","S2","S3","S4","A1","A2","C1","C2","C3","E1","E2","E3","EU1","EU2","EU3"]
COLS=[("base","none"),("base","rev_off"),("base","rev_on"),("skill","none"),("skill","rev_off"),("skill","rev_on")]
def ph(cond): return "build" if cond=="none" else cond
def verdicts():
    v={}; p=os.path.join(ROOT,"judgments.jsonl")
    if os.path.exists(p):
        for l in open(p):
            try: r=json.loads(l); v[r["id"]]=r.get("overall")
            except: pass
    return v
def attempts(skill,cell,cond):
    d=os.path.join(ROOT,skill,cell,ph(cond))
    return sorted([a for a in os.listdir(d)]) if os.path.isdir(d) else []
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
    try:
        rl=open(os.path.join(ROOT,"run.log")).read()
        if "LADDER2 COMPLETE" in rl: return "COMPLETE"
    except: rl=""
    try:
        out=subprocess.run(["ps","-eo","args"],capture_output=True,text=True,timeout=5).stdout
        return "RUNNING" if "bash battery-results/ladder2/ladder2.sh" in out else "STOPPED"
    except: return "?"
def logtail(n=14):
    try: return "".join(open(os.path.join(ROOT,"run.log")).readlines()[-n:])
    except: return "(no run.log)"
VCOLOR={"PASS":"#1a7f37","PARTIAL":"#9a6700","FAIL":"#cf222e","":"#388bfd"}
OCOLOR={"completed":"#1a7f37","needs_operator":"#9a6700","cancelled":"#cf222e","invalid_action":"#cf222e","run":"#0969da","":"#ccc"}
def badge(outcome,det,verd):
    vc=VCOLOR.get(verd,"#388bfd"); oc=OCOLOR.get(outcome,"#888")
    vtxt={"PASS":"P","PARTIAL":"~","FAIL":"F","":"?"}[verd]
    return f'<span title="{outcome} | det={det} | judge={verd or "unjudged"}" style="display:inline-block;min-width:2.4em;margin:1px;padding:1px 4px;border-radius:4px;border:1px solid {oc};background:{vc}22;color:{vc};font-weight:600">{vtxt}<sub style="color:{oc};font-weight:400">{det or "?"}</sub></span>'
def page():
    V=verdicts(); state=driver_state()
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
            cell="".join(badge(*read_att(sk,c,cd,a,V)) for a in atts)
            tds+=f'<td style="text-align:center;white-space:nowrap">{cell}</td>'
        rows+=f'<tr><th style="text-align:left;padding-right:8px">{c}</th>{tds}</tr>'
    CLABEL={"none":"no&nbsp;review<br><span style='color:#8b949e;font-weight:400'>(raw build)</span>",
            "rev_off":"review<br><span style='color:#8b949e;font-weight:400'>reasoning OFF</span>",
            "rev_on":"review<br><span style='color:#8b949e;font-weight:400'>reasoning ON</span>"}
    ALABEL={"base":"base<br><span style='color:#8b949e;font-weight:400'>no scaffold</span>",
            "skill":"skill<br><span style='color:#8b949e;font-weight:400'>Build-Carefully</span>"}
    hdr="".join(f'<th>{ALABEL[sk]}<br>&nbsp;<br>{CLABEL[cd]}</th>' for sk,cd in COLS)
    sc="#1a7f37" if state=="RUNNING" else ("#0969da" if state=="COMPLETE" else "#cf222e")
    now=datetime.datetime.now(datetime.timezone.utc).strftime("%H:%M:%SZ")
    return f'''<!doctype html><html><head><meta charset=utf-8><meta http-equiv=refresh content=20>
<title>Ladder2 {state}</title><style>body{{font:13px system-ui,sans-serif;margin:18px;background:#0d1117;color:#c9d1d9}}
table{{border-collapse:collapse}}td,th{{border:1px solid #30363d;padding:3px 5px}}th{{background:#161b22}}
a{{color:#58a6ff}}sub{{font-size:.7em}}</style></head><body>
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
<span style="color:#8b949e"><b>Columns</b> are builder-arm x review-condition. Within an arm, left-to-right is the experiment: raw build &rarr; after a Nemotron review+revise (reasoning off) &rarr; same with reasoning on. Compare the three to see whether the review helped, hurt, or did nothing.</span>
</div>
<table><tr><th></th>{hdr}</tr>{rows}</table>
<h3>run.log</h3><pre style="background:#161b22;padding:8px;border-radius:6px;overflow:auto;max-height:280px">{logtail()}</pre>
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
