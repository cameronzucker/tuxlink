#!/usr/bin/env python3
"""Chain the rev_skill column behind the in-flight follow-up pass.

Sequencing is the point. review.py is invoked BY the running driver, so deploying
the updated reviewer mid-run would change the instrument underneath an in-flight
experiment. This waits for completion and for the judge to drain, THEN deploys,
THEN launches.

The rev_skill column is purely additive: it creates skill/<cell>/rev_skill/ and
touches nothing existing. The driver skips any condition that already has a
score.json, so relaunching over a populated tree runs only the new column.

Runs on the PI, drives R2 over ssh. Needs ORKEY / OPENROUTER_API_KEY for the
reviewer calls.
"""
import os, re, subprocess, sys, time

R2 = "r2-poe"
R2DIR = "~/tuxlink-eig6e-build/battery-results/ladder2"
HERE = os.path.dirname(os.path.abspath(__file__))
SRC = "/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-kz4rg-lift-ladder-iter/dev/battery/ladder2"
LOG = os.path.join(HERE, "revskill.log")
DRY = "--dry-run" in sys.argv


def log(m):
    line = "[%s] %s" % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), m)
    print(line, flush=True)
    with open(LOG, "a") as f:
        f.write(line + "\n")


def r2(cmd, timeout=180):
    return subprocess.run(["ssh", R2, cmd], capture_output=True, text=True, timeout=timeout).stdout


def run_complete():
    rl = r2("cat %s/run.log" % R2DIR)
    tail = re.split(r"LADDER2(?:-PAR)? START", rl)[-1]
    return ("LADDER2 COMPLETE" in tail) or ("LADDER2-PAR COMPLETE" in tail)


def judge_caught_up():
    scored = int(r2("find %s -name score.json -not -path '*_contaminated*' -not -path '*_followup*' | wc -l" % R2DIR).strip() or 0)
    judged = int(r2("wc -l < %s/judgments.jsonl" % R2DIR).strip() or 0)
    log("  scored=%d judged=%d" % (scored, judged))
    return scored > 0 and judged >= scored


def main():
    log("rev_skill orchestrator start (dry_run=%s)" % DRY)

    while not run_complete():
        log("waiting: follow-up pass still in flight")
        time.sleep(120)
    log("follow-up pass COMPLETE")

    for _ in range(120):
        if judge_caught_up():
            break
        log("waiting: judge still catching up")
        time.sleep(60)
    log("judge caught up")

    if DRY:
        log("dry run; would deploy + launch here"); return

    # ---- deploy the updated reviewer ONLY now that nothing is running -----
    for f in ("review.py", "review-skill.md", "ladder2-par.sh", "dashboard.py"):
        subprocess.run(["scp", "-q", os.path.join(SRC, f), "%s:%s/%s" % (R2, R2DIR, f)], timeout=120)
    r2("chmod +x %s/ladder2-par.sh" % R2DIR)
    log("deployed review.py, review-skill.md, ladder2-par.sh, dashboard.py")

    # restart the dashboard so the new rev_skill column appears
    r2("PID=$(ps -eo pid,args --no-headers | awk '$2==\"python3\" && /dashboard.py/ {print $1}'); "
       "[ -n \"$PID\" ] && kill $PID; sleep 1; cd ~/tuxlink-eig6e-build && "
       "setsid nohup python3 battery-results/ladder2/dashboard.py </dev/null "
       ">> battery-results/ladder2/dashboard.log 2>&1 & sleep 2")
    log("dashboard restarted with the rev_skill column")

    key = os.environ.get("ORKEY") or os.environ.get("OPENROUTER_API_KEY") or ""
    if not key:
        log("NO KEY in env -- deployed but NOT launching."); return
    launch = ("export ORKEY=%s OPENROUTER_API_KEY=%s LADDER2_CONC=8 "
              "LADDER2_TURN_TIMEOUT_SECS=1800 TUXLINK_MAX_RUN_SECS=7200 "
              "LADDER2_REVCONDS_SKILL='off on skill'; "
              "cd ~/tuxlink-eig6e-build && setsid nohup bash "
              "battery-results/ladder2/ladder2-par.sh >> battery-results/ladder2/nohup-par.log 2>&1 </dev/null &"
              % (key, key))
    subprocess.run(["ssh", R2, launch], timeout=60)
    log("launched rev_skill column (skill arm only, 18 conditions)")


if __name__ == "__main__":
    main()
