#!/usr/bin/env bash
# Ladder-2 driver: qwen build + Nemotron adversarial review + qwen revise.
# Autonomous, detached, idempotent, resumable. Does NOT depend on any agent loop.
# Re-launching resumes: a unit with score.json is done and skipped; a crashed
# unit (dir, no score.json) is cleaned and redone.
#
# env: OPENROUTER_API_KEY (for the Nemotron reviewer curl only).
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
QMODEL="qwen35-122b-nvfp4"
QEP="https://inference.twin-bramble.ts.net/v1/chat/completions"
TURNCAP=40
TEMP=0.2
CELLS="P1 P2 P3 S1 S2 S3 S4 A1 A2 C1 C2 C3 E1 E2 E3 EU1 EU2 EU3"
SKILLS="base skill"
REVCONDS="off on"     # Nemotron reasoning on/off; plus implicit 'none' baseline (= the build)
MAXATT=3              # 1 base attempt + up to 2 determinism re-runs on deterministic fail

mkdir -p "$OUT"
log(){ echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" | tee -a "$LOG"; }
man(){ echo "$1" >> "$MAN"; }

cell_prompt(){ python3 -c '
import json,sys
raw=json.load(open(sys.argv[1]))
cells={c["id"]:c for c in (raw if isinstance(raw,list) else (raw.get("cells") or raw.get("prompts") or []))}
sys.stdout.write(cells[sys.argv[2]]["prompt"])' "$CORPUS" "$1"; }

# path to a saved def in a build bundle (first .json in routines/), or empty
built_def(){ ls "$1"/routines/*.json 2>/dev/null | head -1; }

# 1 if the bundle is a DETERMINISTIC fail (outcome!=completed OR !saved OR !green)
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

# run one build/revise bundle; score it. args: out_dir, skill, cell, [preseed_def prompt_file]
run_cell(){
  local out="$1" skill="$2" cell="$3" preseed="${4:-}" promptfile="${5:-}"
  if [ -f "$out/score.json" ]; then return 0; fi   # idempotent skip
  rm -rf "$out"; mkdir -p "$out"
  local extra=()
  [ -n "$preseed" ] && extra+=(--preseed-def "$preseed")
  [ -n "$promptfile" ] && extra+=(--prompt-override-file "$promptfile")
  xvfb-run -a "$BIN" --corpus "$CORPUS" --model "$QMODEL" --endpoint "$QEP" \
      --prompt "$cell" --arm "$skill" --turn-cap "$TURNCAP" --temperature "$TEMP" \
      --out "$out" "${extra[@]}" >> "$out/harness.log" 2>&1
  "$SCORE" --root "$out" --corpus "$CORPUS" >/dev/null 2>&1
  [ -f "$out/score.json" ]
}

log "LADDER2 START cells=[$CELLS] skills=[$SKILLS] revconds=[none $REVCONDS] maxatt=$MAXATT"
for skill in $SKILLS; do
for cell in $CELLS; do
  # ---- BUILD (shared; 'none' condition == build/attempt-1) ----
  bdir="$OUT/$skill/$cell/build"
  run_cell "$bdir/attempt-1" "$skill" "$cell" && log "build $skill/$cell #1: $(cat "$bdir/attempt-1/outcome.json" 2>/dev/null | python3 -c 'import json,sys;print(json.load(sys.stdin)["outcome"])' 2>/dev/null)"
  man "{\"phase\":\"build\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"attempt\":1,\"det_fail\":$(det_fail "$bdir/attempt-1")}"
  if [ "$(det_fail "$bdir/attempt-1")" = "1" ]; then
    for a in 2 3; do
      run_cell "$bdir/attempt-$a" "$skill" "$cell" && log "build $skill/$cell #$a (determinism re-run)"
      man "{\"phase\":\"build\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"attempt\":$a,\"det_fail\":$(det_fail "$bdir/attempt-$a")}"
    done
  fi
  # ---- REVIEW + REVISE conditions (on the shared build attempt-1) ----
  def="$(built_def "$bdir/attempt-1")"
  for cond in $REVCONDS; do
    rdir="$OUT/$skill/$cell/rev_$cond"
    for a in 1 2 3; do
      adir="$rdir/attempt-$a"      # the revise bundle (run_cell rm -rf's this)
      mdir="$rdir/meta-$a"         # review inputs live HERE, a sibling run_cell never wipes
      if [ -f "$adir/score.json" ]; then continue; fi
      # attempt>1 only if attempt-1 was a determinism fail
      if [ "$a" -gt 1 ] && { [ ! -f "$rdir/attempt-1/score.json" ] || [ "$(det_fail "$rdir/attempt-1")" != "1" ]; }; then break; fi
      mkdir -p "$mdir"
      # review (fresh critique per attempt); reasoning capture -> critique.meta
      cp_prompt="$mdir/user_prompt.txt"; cell_prompt "$cell" > "$cp_prompt"
      OPENROUTER_API_KEY="$ORKEY" python3 "$REVIEW" "$cp_prompt" "${def:-/nonexistent}" "$cond" "$CATALOG" \
          > "$mdir/critique.txt" 2> "$mdir/critique.meta"
      # revise prompt = original + critique (in the meta dir, so run_cell's rm -rf can't delete it)
      { cat "$cp_prompt"; echo; echo "---"; echo "A reviewer critiqued your routine. Address every point, then re-save:"; echo; cat "$mdir/critique.txt"; } > "$mdir/revise_prompt.txt"
      run_cell "$adir" "$skill" "$cell" "${def:-}" "$mdir/revise_prompt.txt" && log "revise $skill/$cell/rev_$cond #$a"
      man "{\"phase\":\"revise\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"cond\":\"$cond\",\"attempt\":$a,\"det_fail\":$(det_fail "$adir")}"
    done
  done
done
done
log "LADDER2 COMPLETE"
