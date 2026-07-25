#!/usr/bin/env python3
"""Follow-up pass: re-run conditions the JUDGE failed but the deterministic
scorer passed, so they never earned a determinism re-run.

Why this exists. The driver gates retries on det_fail (outcome != completed OR
!routine_saved OR !validates_green), which runs in-loop. The Sonnet judge runs
decoupled and asynchronous, so it cannot gate anything. A routine that saves
green and is then judged FAIL therefore rests on ONE observation. That is the
green-but-incomplete class -- 78% of green bundles in the 2026-07-25 run -- i.e.
exactly the findings that matter, all at n=1. The runbook always planned this
follow-up pass; it had simply never been run.

Sequencing is the whole point of scripting it: the target list must be computed
AFTER the in-flight run completes AND after the judge has caught up, or the 19
conditions re-run today would be selected on stale verdicts.

Runs on the PI (needs the judge store, which lives here) and drives R2 over ssh.
Requires ORKEY / OPENROUTER_API_KEY in env for the relaunched driver's reviewer.

usage: followup_judge_fails.py [--dry-run]
"""
import json, os, subprocess, sys, time, re

R2 = "r2-poe"
R2DIR = "~/tuxlink-eig6e-build/battery-results/ladder2"
STORE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ladder2-judgments.jsonl")
DRY = "--dry-run" in sys.argv
LOG = os.path.join(os.path.dirname(os.path.abspath(__file__)), "followup.log")


def log(m):
    line = "[%s] %s" % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), m)
    print(line, flush=True)
    with open(LOG, "a") as f:
        f.write(line + "\n")


def r2(cmd, timeout=180):
    return subprocess.run(["ssh", R2, cmd], capture_output=True, text=True, timeout=timeout).stdout


def run_complete():
    """COMPLETE only counts after the LAST START (run.log is append-only)."""
    rl = r2("cat %s/run.log" % R2DIR)
    tail = re.split(r"LADDER2(?:-PAR)? START", rl)[-1]
    return ("LADDER2 COMPLETE" in tail) or ("LADDER2-PAR COMPLETE" in tail)


def judge_caught_up():
    scored = int(r2("find %s -name score.json -not -path '*_contaminated*' -not -path '*_followup*' | wc -l" % R2DIR).strip() or 0)
    judged = int(r2("wc -l < %s/judgments.jsonl" % R2DIR).strip() or 0)
    log("  scored=%d judged=%d" % (scored, judged))
    return scored > 0 and judged >= scored


def main():
    log("follow-up orchestrator start (dry_run=%s)" % DRY)

    while not run_complete():
        log("waiting: ladder run still in flight")
        time.sleep(120)
    log("ladder run COMPLETE")

    for _ in range(120):                      # up to ~2h for the judge to drain
        if judge_caught_up():
            break
        log("waiting: judge still catching up")
        time.sleep(60)
    log("judge caught up")

    # ---- select targets from FRESH verdicts -------------------------------
    verd = {}
    for l in r2("cat %s/judgments.jsonl" % R2DIR).splitlines():
        try:
            r = json.loads(l); verd[r["id"]] = r.get("overall")
        except Exception:
            pass
    listing = r2("cd %s && find . -mindepth 4 -maxdepth 4 -type d -name 'attempt-*' "
                 "-not -path '*_contaminated*' -not -path '*_followup*'" % R2DIR)
    conds = {}
    for p in listing.split():
        parts = p.strip("./").split("/")
        if len(parts) != 4:
            continue
        sk, cell, ph, att = parts
        cond = "none" if ph == "build" else ph
        conds.setdefault((sk, cell, cond), []).append(att)
    targets = []
    for k, atts in sorted(conds.items()):
        if len(atts) != 1:
            continue                          # already has determinism re-runs
        bid = "%s/%s/%s/%s" % (k[0], k[1], k[2], atts[0])
        v = verd.get(bid)
        if v and v != "PASS":
            targets.append(k)
    log("targets: %d single-attempt conditions the judge did not PASS" % len(targets))
    for t in targets:
        log("   %s/%s/%s" % t)
    if DRY:
        log("dry run; stopping before any mutation"); return
    if not targets:
        log("nothing to do"); return

    # ---- archive, retarget, drop stale verdicts, relaunch -----------------
    ts = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    arch = "%s/_followup-archive-%s" % (R2DIR, ts)
    lines = "\n".join("%s %s %s" % t for t in targets)
    r2("mkdir -p %s" % arch)
    for sk, cell, cond in targets:
        ph = "build" if cond == "none" else cond
        r2("mkdir -p %s/%s/%s && mv %s/%s/%s/%s %s/%s/%s/%s"
           % (arch, sk, cell, R2DIR, sk, cell, ph, arch, sk, cell, ph))
    log("archived %d condition dirs to %s" % (len(targets), arch))

    # dashboard reads this for the purple re-run rings
    r2("cat > %s/_rerun_targets.txt <<'EOF'\n%s\nEOF" % (R2DIR, lines))

    # drop those ids from BOTH stores or the judge skips the fresh bundles
    tset = {t for t in targets}
    kept = []
    for l in open(STORE):
        try:
            r = json.loads(l)
            if tuple(r["id"].split("/")[:3]) in tset:
                continue
        except Exception:
            pass
        kept.append(l.rstrip("\n"))
    with open(STORE, "w") as f:
        f.write("\n".join(kept) + "\n")
    subprocess.run(["scp", "-q", STORE, "%s:%s/judgments.jsonl" % (R2, R2DIR)], timeout=120)
    log("dropped stale verdicts; store now %d rows" % len(kept))

    key = os.environ.get("ORKEY") or os.environ.get("OPENROUTER_API_KEY") or ""
    if not key:
        log("NO KEY in env -- archived and retargeted, but NOT relaunching."); return
    launch = ("export ORKEY=%s OPENROUTER_API_KEY=%s LADDER2_CONC=8 "
              "LADDER2_TURN_TIMEOUT_SECS=1800 TUXLINK_MAX_RUN_SECS=7200; "
              "cd ~/tuxlink-eig6e-build && setsid nohup bash "
              "battery-results/ladder2/ladder2-par.sh >> battery-results/ladder2/nohup-par.log 2>&1 </dev/null &"
              % (key, key))
    subprocess.run(["ssh", R2, launch], timeout=60)
    log("relaunched driver for the follow-up pass")


if __name__ == "__main__":
    main()
