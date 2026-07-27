#!/usr/bin/env python3
"""Autonomous Sonnet-5 judge daemon (plan-based via `claude -p`). Polls R2 for
scored-but-unjudged ladder2 bundles, grades each against its predicates, writes the
verdict to judgments.jsonl, and pushes it to R2 so the dashboard updates. No main
agent-loop involvement. Detached; survives the interactive session ending.
"""
import fcntl, glob, hashlib, json, os, re, subprocess, time
HERE=os.path.dirname(os.path.abspath(__file__))
LOCK=os.path.join(HERE,"judge_daemon.lock")   # single-instance guard
LADDER=os.path.join(HERE,"ladder2")
STORE=os.path.join(HERE,"ladder2-judgments.jsonl")
CORPUS=os.path.join(HERE,"corpus.json")
LOG=os.path.join(HERE,"judge_daemon.log")
RAWDIR=os.path.join(HERE,"judge_failures")   # raw stdout/stderr of every failed judge
R2="r2-poe"; R2DIR="~/tuxlink-eig6e-build/battery-results/ladder2"
RUBRIC=("You are a rigorous Sonnet-5 predicate judge for a Tuxlink Routine-authoring battery. "
 "You are given ONE built routine as JSON: {id, cell, skill, cond, prompt (user ask), predicates (rubric), "
 "saved_def (routine JSON, may be null), outcome, deterministic (harness saved/green flags), final_text}. "
 "cond=none is the raw build; rev_off/rev_on are after a Nemotron review + a qwen revise. "
 "Judge the saved_def against the PREDICATES and the prompt's real requirements, NOT the saved/green flags "
 "(green routines routinely drop requirements). Inspect structurally: triggers (schedule vs manual), steps, "
 "actions, control-flow reachability. Check for: recurrence dropped to manual; a required send/compose/receive "
 "leg missing or unreachable past a success 'end'; a vaguely-related action substituted; an action wrongly "
 "claimed unavailable that IS in the catalog (radio.aprs_send, radio.connect, radio.listen, data.find_stations, "
 "local.compose, local.compose_catalog_request, local.log, local.notify, data.spacewx_swpc/wwv, data.read, "
 "rig.apply_preset, rig.tune_atu, branch/end/delay). HARNESS CONTEXT you must account for: the model under test "
 "runs behind an authoring-only tool allowlist; calls to run/egress/config-write tools are refused with a teaching "
 "message that describes the session as authoring/design-only. A final_text that references a design-only session "
 "or mode is QUOTING that harness policy, not fabricating a restriction — do not score the mode reference itself "
 "as confabulation. Judge the give-up on its merits instead: authoring a routine WAS available, so failing to save "
 "one (or to honestly name a missing catalog action) is still a failure. "
 "If outcome!='completed' or saved_def is null, judge what "
 "exists (usually FAIL unless a predicate says no routine is expected, e.g. pure-troubleshooting EU3). For rev_* "
 "also check the revise didn't INTRODUCE problems (fabricated change-description in final_text vs the actual def; "
 "orphaned duplicate routine). Output ONLY a JSON object, no prose, no code fence: "
 '{"id":<id>,"overall":"PASS"|"PARTIAL"|"FAIL","per_predicate":[{"predicate":<short>,"verdict":"PASS"|"FAIL","why":<one line>}],"note":<one line>}')

def log(m):
    with open(LOG,"a") as f: f.write(f"[{int(time.time())}] {m}\n")

def rsync_down():
    """Mirror R2 EXACTLY, including deletions.

    Without --delete the mirror only ever grows, so a bundle archived or removed
    on R2 (e.g. a contaminated condition pulled aside for a re-run) survives
    locally and gets judged again from the stale copy. Observed 2026-07-25: the
    daemon reported 57 unjudged immediately after a re-run launch and began
    re-judging the ARCHIVED bundles rather than the fresh ones. The mirror is
    purely derived from R2, so deleting from it is always safe."""
    subprocess.run(["rsync","-a","--delete",f"{R2}:{R2DIR}/",LADDER+"/"],timeout=180,
                   stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

def push_store():
    subprocess.run(["scp","-q",STORE,f"{R2}:{R2DIR}/judgments.jsonl"],timeout=60,
                   stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

def judged_ids():
    if not os.path.exists(STORE): return {}
    return {json.loads(l)["id"]:json.loads(l) for l in open(STORE)}

def fingerprint(bundle):
    """Content hash of what the judge actually grades.

    The store is keyed on the bundle PATH (<skill>/<cell>/<cond>/attempt-N),
    which is stable only while a run tree is immutable. Rebuild the tree in
    place — a re-run, a resumed run, a driver fix — and identical paths now name
    DIFFERENT artifacts, so the daemon skips every one of them as already-judged
    and the dashboard shows the OLD run's verdicts against the new run's work.
    Observed twice on 2026-07-26; the second time only because the numbers were
    implausible. Hash score.json + outcome.json so a rebuilt bundle re-judges by
    itself instead of relying on whoever rebuilt it remembering to clear the
    store."""
    h=hashlib.sha256()
    for n in ("score.json","outcome.json"):
        p=os.path.join(bundle,n)
        try:
            with open(p,"rb") as f: h.update(f.read())
        except OSError:
            h.update(b"<missing>")
        h.update(b"\0")
    return h.hexdigest()[:16]

def pkg(bundle,cells):
    p=bundle.split(os.sep); cond="none" if p[-2]=="build" else p[-2]
    bid=f"{p[-4]}/{p[-3]}/{cond}/{p[-1]}"
    sc=json.load(open(os.path.join(bundle,"score.json"))); ji=sc.get("judge_input") or {}
    o=json.load(open(os.path.join(bundle,"outcome.json")))
    return bid, {"id":bid,"fp":fingerprint(bundle),
        "cell":p[-3],"skill":p[-4],"cond":cond,
        "prompt":cells[p[-3]]["prompt"],
        "predicates":ji.get("predicates") or cells[p[-3]]["predicates"],
        "outcome":o.get("outcome"),"deterministic":sc.get("deterministic"),
        "saved_def":(ji.get("artifacts") or {}).get("def"),
        "final_text":(o.get("detail") or "")[:1200]}

FENCE=re.compile(r"```[a-zA-Z0-9_-]*\n?|```")
OVERALL={"PASS","PARTIAL","FAIL"}

def extract_verdict(out):
    """Pull the verdict object out of a model reply.

    Tolerates: markdown fences, prose before/after, and multiple JSON objects in
    one reply. Scans every balanced object from each '{' via raw_decode and picks
    the first one that actually looks like a verdict, instead of assuming the
    text between the first '{' and the last '}' is a single value -- that
    assumption is what produced 'Extra data: line 1 column 19 (char 18)'.
    """
    s=FENCE.sub("",out).strip()
    dec=json.JSONDecoder()
    i=0; objs=[]
    while True:
        i=s.find("{",i)
        if i<0: break
        try: obj,end=dec.raw_decode(s,i)
        except ValueError:
            i+=1; continue
        if isinstance(obj,dict): objs.append(obj)
        i=end
    # Prefer the COMPLETE verdict shape. A reply may open with a minimal
    # {"overall":"PASS"} before the real, fuller verdict; taking the first
    # 'overall' we see would silently record the wrong result.
    for want in (lambda o: o.get("overall") in OVERALL and "per_predicate" in o,
                 lambda o: o.get("overall") in OVERALL,
                 lambda o: "per_predicate" in o):
        for o in objs:
            if want(o): return o
    raise ValueError("no verdict object in reply")

def dump_raw(bid,r,err):
    """Persist the raw reply of a failed judge so the flake rate stays measurable.
    Without this the retry loop repairs failures silently and leaves no evidence."""
    try:
        os.makedirs(RAWDIR,exist_ok=True)
        p=os.path.join(RAWDIR,"%s.%d.txt"%(bid.replace("/","_"),int(time.time())))
        with open(p,"w") as f:
            f.write("bid=%s\nerror=%s\nreturncode=%s\n\n=== STDOUT ===\n%s\n\n=== STDERR ===\n%s\n"
                    %(bid,err,getattr(r,"returncode",None),
                      getattr(r,"stdout",""),getattr(r,"stderr","")))
        return p
    except Exception as e: return "raw-dump-failed:%s"%e

def judge(one):
    prompt=RUBRIC+"\n\nROUTINE TO JUDGE:\n"+json.dumps(one)
    # stdin=DEVNULL: without it `claude -p` waits 3s for stdin on every call.
    r=subprocess.run(["claude","-p",prompt,"--model","sonnet"],
                     stdin=subprocess.DEVNULL,capture_output=True,text=True,timeout=300)
    try:
        v=extract_verdict(r.stdout or "")
    except Exception as e:
        raise ValueError("%s [raw: %s]"%(e,dump_raw(one["id"],r,e)))
    if v.get("overall") not in OVERALL:
        raise ValueError("bad overall %r [raw: %s]"%(v.get("overall"),dump_raw(one["id"],r,"bad overall")))
    v["id"]=one["id"]   # never trust the model to echo the id back correctly
    return v

def complete():
    """Either driver's completion marker. ladder2-par.sh writes LADDER2-PAR
    COMPLETE, which does NOT contain the serial marker, so matching only the
    serial one would leave the daemon polling forever after a parallel run."""
    try:
        rl=open(os.path.join(LADDER,"run.log")).read()
        # Append-only across runs: a COMPLETE from a PREVIOUS run persists, so
        # only look after the last START or a re-run exits the daemon instantly.
        tail=re.split(r"LADDER2(?:-PAR)? START",rl)[-1]
        return ("LADDER2 COMPLETE" in tail) or ("LADDER2-PAR COMPLETE" in tail)
    except: return False

def acquire_lock():
    """Refuse to run a second instance. Two daemons share one append-only STORE
    and both scp it to R2, so a duplicate double-judges bundles and races the
    push. flock is released automatically if the process dies, so a hard kill or
    a reboot never leaves a stale lock behind."""
    f=open(LOCK,"w")
    try: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
    except OSError:
        log("another judge daemon holds the lock; exiting")
        raise SystemExit(0)
    f.write(str(os.getpid())); f.flush()
    return f

def main():
    _lock=acquire_lock()   # held for the process lifetime; do not close
    log(f"judge daemon start (pid {os.getpid()})")
    idle=0
    while True:
        rsync_down()
        try: cells={c["id"]:c for c in json.load(open(CORPUS))["prompts"]}
        except Exception as e: log(f"corpus err {e}"); time.sleep(60); continue
        done=judged_ids()
        todo=[]; restale=0
        for sc in glob.glob(os.path.join(LADDER,"*","*","*","attempt-*","score.json")):
            b=os.path.dirname(sc)
            try: bid,one=pkg(b,cells)
            except Exception: continue
            prior=done.get(bid)
            if prior is None:
                todo.append((bid,one))
            elif prior.get("fp") != one["fp"]:
                # Same path, different artifact: the tree was rebuilt under a
                # verdict that no longer describes it. Re-judge rather than show
                # a stale result. A store written before fingerprints existed has
                # no "fp" and lands here too — that costs one re-judge pass on
                # first upgrade, which is the correct trade against silently
                # reporting the previous run's verdicts.
                restale+=1; todo.append((bid,one))
        if todo:
            log(f"{len(todo)} unjudged" + (f" ({restale} stale: bundle changed under an existing verdict)" if restale else ""))
            for bid,one in todo:
                try:
                    v=judge(one); v["judge"]="sonnet-5"; v["judged_at"]=int(time.time())
                    v["fp"]=one["fp"]   # never trust the model to echo it back
                    with open(STORE,"a") as f: f.write(json.dumps(v)+"\n")
                    push_store()
                    log(f"judged {bid} -> {v.get('overall')}")
                except Exception as e:
                    log(f"FAIL judge {bid}: {e}")
            idle=0
        else:
            idle+=1
            if complete() and idle>=2:
                log("all judged + run complete; exiting"); return
        time.sleep(60)

if __name__=="__main__": main()
