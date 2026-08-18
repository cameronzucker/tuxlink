#!/bin/bash
# In-situ IR ladder instrument runner (tuxlink-che1k).
#
# Executes the pre-registered 33-run matrix in INSTRUMENT-2026-08-18-insitu.md,
# exactly: 5 cells x 3 surfaces x 2 samples + 3 controls, against the FULL
# production MCP router (tuxlink-mcp-testserver, real EgressGuard disarmed +
# untainted) and the unmodified production ELMER_SYSTEM_PROMPT (d3zwe passes no
# override), Inkling at serving defaults (NO sampling params anywhere — d3zwe
# constructs OpenAiProvider with temperature=None, which omits the field).
#
# Each run gets a FRESH testserver (fresh guard: disarmed, untainted) and a
# fresh d3zwe process (fresh conversation). Captures, verbatim, per run:
#   runs-insitu/prompts/<run>.prompt.txt  — the exact user prompt sent
#   runs-insitu/<run>.out.json            — d3zwe stdout {"kind","text"}
#   runs-insitu/<run>.trace.txt           — d3zwe stderr (the "→ tool" trace)
#   runs-insitu/<run>.server.log          — testserver stderr
#
# No iteration: cells, asks, and surfaces are FIXED by the committed spec.
# Any deviation observed at run time goes to the results errata, loudly.
set -u

WD="$(cd "$(dirname "$0")" && pwd)"        # the spike dir
ROOT="$(cd "$WD/../../.." && pwd)"         # the worktree root
BIN="$ROOT/src-tauri/target/release"
RUNS="$WD/runs-insitu"
# The UDS transport refuses group/world-writable parent dirs (socket-hijack
# hardening), and /run/user/<uid> is mode 770 — bind inside a 700 subdir.
SOCKDIR="/run/user/$(id -u)/tuxlink-ir"
mkdir -p -m 700 "$SOCKDIR"
SOCK="$SOCKDIR/insitu.sock"
EP="https://inference.twin-bramble.ts.net/v1/chat/completions"
MODEL="thinkingmachines/Inkling-Small-NVFP4"

mkdir -p "$RUNS/prompts"

# ---- Serving pre-flight (spec: catalog + 1-token generation) ----------------
curl -sS -m 15 https://inference.twin-bramble.ts.net/v1/models \
  | jq -e --arg m "$MODEL" '.data[].id | select(. == $m)' >/dev/null \
  || { echo "PRE-FLIGHT FAIL: model catalog"; exit 1; }
curl -sS -m 60 "$EP" -H 'Content-Type: application/json' \
  -d "{\"model\":\"$MODEL\",\"messages\":[{\"role\":\"user\",\"content\":\"Say OK\"}],\"max_tokens\":5}" \
  | jq -e '.choices[0]' >/dev/null \
  || { echo "PRE-FLIGHT FAIL: generation"; exit 1; }
echo "pre-flight OK: $MODEL"

[ -x "$BIN/tuxlink-mcp-testserver" ] && [ -x "$BIN/d3zwe" ] \
  || { echo "PRE-FLIGHT FAIL: binaries missing under $BIN"; exit 1; }

# ---- Surfaces (the committed artifacts, verbatim, headers included) ---------
SHEET_A="$(cat "$WD/IR-ONEPAGER.md")"
SHEET_D="$(cat "$WD/SHEET-D-template-slots.md")"
SHEET_B="$(cat "$WD/SHEET-B-text-dsl.md")"

# ---- Asks (fixed by the spec; N1 = the wa-gateways ask, N2 = probe v3's F2
#      verbatim, E1 = the ladder edit request verbatim) -----------------------
ASK_N1="Every 20 minutes between 06:00 and 09:00, try @station-set:wa-gateways on 40m then 80m in that order; on success log which station and band; on failure log that no gateway was reached and end failed with reason 'no gateway'."
ASK_N2="Every 30 minutes between 18:00 and 21:00, try @station-set:nv-gateways on 40m; on success log the station; if it fails, retry twice before giving up, and also send a beacon transmission announcing our position on each attempt."
ASK_E1="Change it: every 15 minutes instead of 30, add 80 meters as a fallback band after 40, and record which band succeeded, not just which station."
ASK_E2="Remove the time window so it can run at any time of day, and change the failure reason to 'all bands exhausted'."

# ---- E1 current routines (30m cadence, 40m only, station-only success log) --
read -r -d '' CUR_E1_A <<'EOF' || true
{"routine":"or-gateway-check","every":"30m","do":[{"connect":{"stations":"@station-set:or-gateways","bands":["40m"]},"on_success":[{"log":"Connected to $station"}],"on_failure":[{"log":"No gateway reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
EOF
read -r -d '' CUR_E1_D <<'EOF' || true
{"template":"scheduled-connect-with-fallback","slots":{"name":"or-gateway-check","every":"30m","window":null,"stations":"@station-set:or-gateways","bands":["40m"],"success_log":"Connected to $station","failure_log":"No gateway reached","fail_reason":"no gateway"}}
EOF
read -r -d '' CUR_E1_B <<'EOF' || true
routine or-gateway-check every 30m
connect @station-set:or-gateways on 40m
  on success:
    log "Connected to $station"
  on failure:
    log "No gateway reached"
    end failed "no gateway"
EOF

# ---- E2 current routines (window present, failure log + reason present) -----
read -r -d '' CUR_E2_A <<'EOF' || true
{"routine":"wa-morning-check","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]},"on_success":[{"log":"Reached $station on $band"}],"on_failure":[{"log":"No gateway was reached"},{"end":{"failed":true,"reason":"no gateway"}}]}]}
EOF
read -r -d '' CUR_E2_D <<'EOF' || true
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-morning-check","every":"20m","window":"06:00-09:00","stations":"@station-set:wa-gateways","bands":["40m","80m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
EOF
read -r -d '' CUR_E2_B <<'EOF' || true
routine wa-morning-check every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
  on success:
    log "Reached $station on $band"
  on failure:
    log "No gateway was reached"
    end failed "no gateway"
EOF

# ---- C1 flawed artifacts + refusals (probe v3, verbatim) --------------------
read -r -d '' FLAWED_A <<'EOF' || true
{"routine":"wa-gateways-dial","every":"20m","window":"06:00-09:00","do":[{"connect":{"stations":"@station-set:wa-gateways","bands":["40m","80m"]}},{"log":"Reached $station on $band"},{"end":{"failed":true,"reason":"no gateway"}}]}
EOF
REFUSAL_A='compile refused: the log at do[1] and the end at do[2] run UNCONDITIONALLY after every connect attempt — the request requires the log only on success, and the failed end only on failure. Nothing is gated.'
read -r -d '' FLAWED_D <<'EOF' || true
{"template":"scheduled-connect-with-fallback","slots":{"name":"wa-dial","every":"20m","window":null,"stations":"@station-set:wa-gateways","bands":["80m","40m"],"success_log":"Reached $station on $band","failure_log":"No gateway was reached","fail_reason":"no gateway"}}
EOF
REFUSAL_D='refused: the request specified 40m FIRST then 80m as fallback (bands order is meaning), and a 06:00-09:00 window which is missing.'
read -r -d '' FLAWED_B <<'EOF' || true
routine wa-gateways-dial every 20m window 06:00-09:00
connect @station-set:wa-gateways on 40m, 80m
log "Reached $station on $band"
end failed "no gateway"
EOF
REFUSAL_B='compile refused: the log and the end lines are unindented — they run unconditionally after every attempt; the request requires the log only on success and the failed end only on failure.'

# ---- One run: fresh server, fresh d3zwe, verbatim capture -------------------
run_cell() { # $1 run-name, $2 prompt
  local name="$1" prompt="$2"
  # Resumable: a run with a non-empty outcome file is complete; partials
  # (killed mid-run) have an empty/absent .out.json and re-run.
  if [ -s "$RUNS/$name.out.json" ] && grep -q '"kind"' "$RUNS/$name.out.json"; then
    echo "[skip] $name already complete"
    return 0
  fi
  printf '%s' "$prompt" > "$RUNS/prompts/$name.prompt.txt"

  rm -f "$SOCK"
  TUXLINK_MCP_SOCK="$SOCK" "$BIN/tuxlink-mcp-testserver" \
    2> "$RUNS/$name.server.log" &
  local srv=$!
  local waited=0
  while [ ! -S "$SOCK" ] && [ "$waited" -lt 50 ]; do sleep 0.2; waited=$((waited+1)); done
  if [ ! -S "$SOCK" ]; then
    echo "SERVER FAIL: socket never appeared for $name" | tee -a "$RUNS/$name.trace.txt"
    kill -INT "$srv" 2>/dev/null; wait "$srv" 2>/dev/null
    return 1
  fi

  # The runner self-bounds at 1800s (whole-run cap); 2000s here is hung-process
  # protection only, above the runner's own bound. Occurrences go to errata.
  D3ZWE_TURN_TIMEOUT_SECS=300 timeout 2000 "$BIN/d3zwe" \
    --socket "$SOCK" --json --allow-remote \
    --endpoint "$EP" --model "$MODEL" \
    --prompt "$prompt" \
    > "$RUNS/$name.out.json" 2> "$RUNS/$name.trace.txt"
  local rc=$?
  echo "d3zwe-exit=$rc" >> "$RUNS/$name.trace.txt"

  kill -INT "$srv" 2>/dev/null
  wait "$srv" 2>/dev/null
  echo "[$(date -u +%H:%M:%SZ)] $name done (exit $rc)"
}

# ---- Prompt composition per cell kind ---------------------------------------
p_fresh()  { printf '%s\n\nRequest: %s' "$1" "$2"; }
p_edit()   { printf '%s\n\nThe current routine:\n%s\n\nEdit request: %s\n\nEmit the whole updated routine, output only the artifact.' "$1" "$2" "$3"; }
p_corr()   { printf '%s\n\nThe request was: %s\n\nYou previously emitted:\n%s\n\nThe compiler refused it:\n%s\n\nEmit the corrected WHOLE routine now, output only the artifact.' "$1" "$2" "$3" "$4"; }

# ---- The matrix (order: surface-major, matching the spec's cell list) -------
for i in 1 2; do
  run_cell "A-N1-$i" "$(p_fresh "$SHEET_A" "$ASK_N1")"
  run_cell "A-N2-$i" "$(p_fresh "$SHEET_A" "$ASK_N2")"
  run_cell "A-E1-$i" "$(p_edit  "$SHEET_A" "$CUR_E1_A" "$ASK_E1")"
  run_cell "A-E2-$i" "$(p_edit  "$SHEET_A" "$CUR_E2_A" "$ASK_E2")"
  run_cell "A-C1-$i" "$(p_corr  "$SHEET_A" "$ASK_N1" "$FLAWED_A" "$REFUSAL_A")"
done
for i in 1 2; do
  run_cell "D-N1-$i" "$(p_fresh "$SHEET_D" "$ASK_N1")"
  run_cell "D-N2-$i" "$(p_fresh "$SHEET_D" "$ASK_N2")"
  run_cell "D-E1-$i" "$(p_edit  "$SHEET_D" "$CUR_E1_D" "$ASK_E1")"
  run_cell "D-E2-$i" "$(p_edit  "$SHEET_D" "$CUR_E2_D" "$ASK_E2")"
  run_cell "D-C1-$i" "$(p_corr  "$SHEET_D" "$ASK_N1" "$FLAWED_D" "$REFUSAL_D")"
done
for i in 1 2; do
  run_cell "B-N1-$i" "$(p_fresh "$SHEET_B" "$ASK_N1")"
  run_cell "B-N2-$i" "$(p_fresh "$SHEET_B" "$ASK_N2")"
  run_cell "B-E1-$i" "$(p_edit  "$SHEET_B" "$CUR_E1_B" "$ASK_E1")"
  run_cell "B-E2-$i" "$(p_edit  "$SHEET_B" "$CUR_E2_B" "$ASK_E2")"
  run_cell "B-C1-$i" "$(p_corr  "$SHEET_B" "$ASK_N1" "$FLAWED_B" "$REFUSAL_B")"
done
# Controls: the N1 ask alone, no sheet — native-surface baseline (residue R2:
# routines port is the canned mock; ARGS are the real emissions).
for i in 1 2 3; do
  run_cell "CTRL-$i" "$ASK_N1"
done

echo "matrix complete: $(ls "$RUNS"/*.out.json 2>/dev/null | wc -l) outcome files in $RUNS"
