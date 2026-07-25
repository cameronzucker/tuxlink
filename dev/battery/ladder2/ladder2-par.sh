#!/usr/bin/env bash
# Ladder-2 driver, PARALLEL variant. Same units, same idempotency, same output
# tree as ladder2.sh -- the only change is that independent (skill, cell) chains
# run concurrently instead of one at a time.
#
# Why this is safe to run over a tree ladder2.sh already populated: a unit with
# score.json is skipped, and a crashed unit (dir, no score.json) is cleaned and
# redone. Stop the serial driver BEFORE starting this one; both would otherwise
# race on the same in-flight unit, and run_cell's rm -rf would delete a bundle
# the other process is still writing.
#
# env: OPENROUTER_API_KEY / ORKEY (Nemotron reviewer curl only)
#      LADDER2_CONC  worker width, default 3
set -uo pipefail

ROOT="$HOME/tuxlink-eig6e-build"
BIN="$ROOT/src-tauri/target/debug/elmer_battery"
SCORE="$ROOT/src-tauri/target/debug/elmer_score"
CORPUS="$ROOT/tests/battery/corpus.json"
OUT="$ROOT/battery-results/ladder2"
REVIEW="$OUT/review.py"
CATALOG="$OUT/catalog.json"
LOG="$OUT/run.log"
MAN="$OUT/manifest.jsonl"
LAT="$OUT/latency.jsonl"          # per-unit wall clock, for the width/latency curve
QMODEL="qwen35-122b-nvfp4"
QEP="https://inference.twin-bramble.ts.net/v1/chat/completions"
TURNCAP=40
TEMP=0.2
CELLS="P1 P2 P3 S1 S2 S3 S4 A1 A2 C1 C2 C3 E1 E2 E3 EU1 EU2 EU3"
SKILLS="base skill"
REVCONDS="off on"
MAXATT=3
CONC="${LADDER2_CONC:-3}"

mkdir -p "$OUT"
# Small appends (<PIPE_BUF) with >> are atomic on Linux, so concurrent workers
# can share these logs without interleaving a line.
log(){ echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" >> "$LOG"; }
man(){ echo "$1" >> "$MAN"; }
lat(){ echo "$1" >> "$LAT"; }

cell_prompt(){ python3 -c '
import json,sys
raw=json.load(open(sys.argv[1]))
cells={c["id"]:c for c in (raw if isinstance(raw,list) else (raw.get("cells") or raw.get("prompts") or []))}
sys.stdout.write(cells[sys.argv[2]]["prompt"])' "$CORPUS" "$1"; }

built_def(){ ls "$1"/routines/*.json 2>/dev/null | head -1; }

det_fail(){ python3 -c '
import json,sys,os
b=sys.argv[1]
try:
    s=json.load(open(os.path.join(b,"score.json")))
    o=json.load(open(os.path.join(b,"outcome.json")))
    d=s.get("deterministic") or {}
    fail=(o.get("outcome")!="completed") or (not d.get("routine_saved")) or (not d.get("validates_green"))
    print(1 if fail else 0)
except Exception:
    print(1)' "$1"; }

# run one bundle. args: out_dir, skill, cell, display_num, [preseed_def prompt_file]
# xvfb-run -a auto-picks a display and RACES when workers start together (two
# invocations can select the same number). Each worker gets an explicit -n.
run_cell(){
  local out="$1" skill="$2" cell="$3" disp="$4" preseed="${5:-}" promptfile="${6:-}"
  if [ -f "$out/score.json" ]; then return 0; fi
  rm -rf "$out"; mkdir -p "$out"
  local extra=()
  [ -n "$preseed" ] && extra+=(--preseed-def "$preseed")
  [ -n "$promptfile" ] && extra+=(--prompt-override-file "$promptfile")
  local t0 t1
  t0=$(date +%s)
  xvfb-run -n "$disp" "$BIN" --corpus "$CORPUS" --model "$QMODEL" --endpoint "$QEP" \
      --prompt "$cell" --arm "$skill" --turn-cap "$TURNCAP" --temperature "$TEMP" \
      --out "$out" "${extra[@]}" >> "$out/harness.log" 2>&1
  t1=$(date +%s)
  "$SCORE" --root "$out" --corpus "$CORPUS" >/dev/null 2>&1
  # Wall clock per unit + the outcome, so the width-vs-latency curve and any
  # timeout inflation (the 1800s max_response_duration is WALL-CLOCK) are
  # measurable rather than inferred.
  lat "{\"unit\":\"$out\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"conc\":$CONC,\"secs\":$((t1-t0)),\"outcome\":\"$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["outcome"])' "$out/outcome.json" 2>/dev/null || echo unknown)\"}"
  [ -f "$out/score.json" ]
}

# One full (skill, cell) chain: shared build (+determinism re-runs), then each
# review condition's review+revise. Identical semantics to ladder2.sh.
do_cell(){
  local skill="$1" cell="$2" disp="$3"
  local bdir="$OUT/$skill/$cell/build"
  run_cell "$bdir/attempt-1" "$skill" "$cell" "$disp" && log "build $skill/$cell #1: $(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["outcome"])' "$bdir/attempt-1/outcome.json" 2>/dev/null)"
  man "{\"phase\":\"build\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"attempt\":1,\"det_fail\":$(det_fail "$bdir/attempt-1")}"
  if [ "$(det_fail "$bdir/attempt-1")" = "1" ]; then
    for a in 2 3; do
      run_cell "$bdir/attempt-$a" "$skill" "$cell" "$disp" && log "build $skill/$cell #$a (determinism re-run)"
      man "{\"phase\":\"build\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"attempt\":$a,\"det_fail\":$(det_fail "$bdir/attempt-$a")}"
    done
  fi
  local def; def="$(built_def "$bdir/attempt-1")"
  for cond in $REVCONDS; do
    local rdir="$OUT/$skill/$cell/rev_$cond"
    for a in 1 2 3; do
      local adir="$rdir/attempt-$a" mdir="$rdir/meta-$a"
      if [ -f "$adir/score.json" ]; then continue; fi
      if [ "$a" -gt 1 ] && { [ ! -f "$rdir/attempt-1/score.json" ] || [ "$(det_fail "$rdir/attempt-1")" != "1" ]; }; then break; fi
      mkdir -p "$mdir"
      local cp_prompt="$mdir/user_prompt.txt"; cell_prompt "$cell" > "$cp_prompt"
      OPENROUTER_API_KEY="$ORKEY" python3 "$REVIEW" "$cp_prompt" "${def:-/nonexistent}" "$cond" "$CATALOG" \
          > "$mdir/critique.txt" 2> "$mdir/critique.meta"
      { cat "$cp_prompt"; echo; echo "---"; echo "A reviewer critiqued your routine. Address every point, then re-save:"; echo; cat "$mdir/critique.txt"; } > "$mdir/revise_prompt.txt"
      run_cell "$adir" "$skill" "$cell" "$disp" "${def:-}" "$mdir/revise_prompt.txt" && log "revise $skill/$cell/rev_$cond #$a"
      man "{\"phase\":\"revise\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"cond\":\"$cond\",\"attempt\":$a,\"det_fail\":$(det_fail "$adir")}"
    done
  done
  log "CHAIN DONE $skill/$cell"
}

log "LADDER2-PAR START conc=$CONC cells=[$CELLS] skills=[$SKILLS] revconds=[none $REVCONDS]"
idx=0
running=0
for skill in $SKILLS; do
for cell in $CELLS; do
  idx=$((idx+1))
  # Distinct X display per chain; +200 stays clear of the :99-:109 range the
  # serial runs' xvfb-run -a orphans already occupy.
  do_cell "$skill" "$cell" $((200+idx)) &
  running=$((running+1))
  if [ "$running" -ge "$CONC" ]; then wait -n; running=$((running-1)); fi
done
done
wait
log "LADDER2-PAR COMPLETE"
