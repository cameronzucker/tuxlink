#!/usr/bin/env python3
"""Autonomous Sonnet-5 judge daemon (plan-based via `claude -p`). Polls R2 for
scored-but-unjudged ladder2 bundles, grades each against its predicates, writes the
verdict to judgments.jsonl, and pushes it to R2 so the dashboard updates. No main
agent-loop involvement. Detached; survives the interactive session ending.
"""
import json, os, subprocess, time, glob
HERE=os.path.dirname(os.path.abspath(__file__))
LADDER=os.path.join(HERE,"ladder2")
STORE=os.path.join(HERE,"ladder2-judgments.jsonl")
CORPUS=os.path.join(HERE,"corpus.json")
LOG=os.path.join(HERE,"judge_daemon.log")
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
 "rig.apply_preset, rig.tune_atu, branch/end/delay). If outcome!='completed' or saved_def is null, judge what "
 "exists (usually FAIL unless a predicate says no routine is expected, e.g. pure-troubleshooting EU3). For rev_* "
 "also check the revise didn't INTRODUCE problems (fabricated change-description in final_text vs the actual def; "
 "orphaned duplicate routine). Output ONLY a JSON object, no prose, no code fence: "
 '{"id":<id>,"overall":"PASS"|"PARTIAL"|"FAIL","per_predicate":[{"predicate":<short>,"verdict":"PASS"|"FAIL","why":<one line>}],"note":<one line>}')

def log(m):
    with open(LOG,"a") as f: f.write(f"[{int(time.time())}] {m}\n")

def rsync_down():
    subprocess.run(["rsync","-a",f"{R2}:{R2DIR}/",LADDER+"/"],timeout=180,
                   stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

def push_store():
    subprocess.run(["scp","-q",STORE,f"{R2}:{R2DIR}/judgments.jsonl"],timeout=60,
                   stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

def judged_ids():
    if not os.path.exists(STORE): return {}
    return {json.loads(l)["id"]:json.loads(l) for l in open(STORE)}

def pkg(bundle,cells):
    p=bundle.split(os.sep); cond="none" if p[-2]=="build" else p[-2]
    bid=f"{p[-4]}/{p[-3]}/{cond}/{p[-1]}"
    sc=json.load(open(os.path.join(bundle,"score.json"))); ji=sc.get("judge_input") or {}
    o=json.load(open(os.path.join(bundle,"outcome.json")))
    return bid, {"id":bid,"cell":p[-3],"skill":p[-4],"cond":cond,
        "prompt":cells[p[-3]]["prompt"],
        "predicates":ji.get("predicates") or cells[p[-3]]["predicates"],
        "outcome":o.get("outcome"),"deterministic":sc.get("deterministic"),
        "saved_def":(ji.get("artifacts") or {}).get("def"),
        "final_text":(o.get("detail") or "")[:1200]}

def judge(one):
    prompt=RUBRIC+"\n\nROUTINE TO JUDGE:\n"+json.dumps(one)
    r=subprocess.run(["claude","-p",prompt,"--model","sonnet"],
                     capture_output=True,text=True,timeout=300)
    out=r.stdout.strip()
    s=out.find("{"); e=out.rfind("}")
    if s<0 or e<0: raise ValueError("no json: "+out[:200])
    return json.loads(out[s:e+1])

def complete():
    try: return "LADDER2 COMPLETE" in open(os.path.join(LADDER,"run.log")).read()
    except: return False

def main():
    log("judge daemon start")
    idle=0
    while True:
        rsync_down()
        try: cells={c["id"]:c for c in json.load(open(CORPUS))["prompts"]}
        except Exception as e: log(f"corpus err {e}"); time.sleep(60); continue
        done=judged_ids()
        todo=[]
        for sc in glob.glob(os.path.join(LADDER,"*","*","*","attempt-*","score.json")):
            b=os.path.dirname(sc)
            try: bid,one=pkg(b,cells)
            except Exception: continue
            if bid not in done: todo.append((bid,one))
        if todo:
            log(f"{len(todo)} unjudged")
            for bid,one in todo:
                try:
                    v=judge(one); v["judge"]="sonnet-5"; v["judged_at"]=int(time.time())
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
