#!/usr/bin/env bash
# External spend guard for the qwen3.7-max spot check.
#
# The harness's own --cell-ceiling-usd cannot protect a PARALLEL run: it meters
# ACCOUNT-WIDE credits, so every concurrent cell observes the sum of all of them
# and they mass-cancel together (2026-07-25 analysis, bd tuxlink-l264r). The spot
# check therefore runs with that ceiling raised out of the way, and this watchdog
# is the real limit: it polls the same credits endpoint from OUTSIDE the run and
# kills every spot process if the delta crosses the budget.
#
# Reasoning-native models bill thinking tokens as OUTPUT, so a blowup here looks
# like cost rather than time, which is exactly what a wall-clock deadline misses.
#
# env: OPENROUTER_API_KEY, SPOT_BUDGET_USD (default 10)
set -uo pipefail

BUDGET="${SPOT_BUDGET_USD:-10}"
OUT="${SPOT_OUT:-$HOME/tuxlink-eig6e-build/battery-results/spot-37max}"
LOG="$OUT/cost.log"
PIDS="$OUT/pids"
mkdir -p "$OUT"

# True iff any recorded spot PID is still alive.
alive(){ [ -s "$PIDS" ] || return 1
         while read -r p; do [ -n "$p" ] && kill -0 "$p" 2>/dev/null && return 0; done < "$PIDS"
         return 1; }

usage(){ python3 -c '
import os,json,urllib.request
k=os.environ["OPENROUTER_API_KEY"]
r=urllib.request.Request("https://openrouter.ai/api/v1/credits",headers={"Authorization":"Bearer "+k})
d=json.load(urllib.request.urlopen(r,timeout=20))["data"]
print("%.6f"%float(d.get("total_usage") or 0))'; }

BASE="$(usage)" || { echo "cannot read credits; refusing to run unguarded" >&2; exit 1; }
echo "[$(date -u +%H:%M:%SZ)] baseline=\$$BASE budget=\$$BUDGET" >> "$LOG"

while true; do
  sleep 45
  # stop when no spot processes remain
  if ! alive; then
    NOW="$(usage)"; D=$(python3 -c "print('%.4f'%($NOW-$BASE))")
    echo "[$(date -u +%H:%M:%SZ)] spot processes gone; final spend=\$$D" >> "$LOG"
    exit 0
  fi
  NOW="$(usage)" || continue
  D=$(python3 -c "print('%.4f'%($NOW-$BASE))")
  echo "[$(date -u +%H:%M:%SZ)] spend=\$$D / \$$BUDGET" >> "$LOG"
  OVER=$(python3 -c "print(1 if $NOW-$BASE >= $BUDGET else 0)")
  if [ "$OVER" = "1" ]; then
    echo "[$(date -u +%H:%M:%SZ)] BUDGET EXCEEDED (\$$D >= \$$BUDGET) -- killing spot check" >> "$LOG"
    # Kill ONLY the recorded PIDs and their groups. Pattern-based killing is
    # unsafe here: a shell whose own argv contains the pattern self-matches, and
    # a mis-scoped pkill on this box would take out the ladder re-run.
    while read -r pid; do
      [ -n "$pid" ] && kill -TERM -"$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null
    done < "$PIDS"
    exit 2
  fi
done
