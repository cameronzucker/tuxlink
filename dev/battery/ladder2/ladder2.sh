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
REVCONDS="off"        # rev_on RETIRED (tuxlink-jaer0): reasoning ON hurts the reviewer (28% vs 39%); implicit 'none' baseline (= the build)
MAXATT=3              # 1 base attempt + up to 2 determinism re-runs on deterministic fail

mkdir -p "$OUT"
# Per-run, unlike the append-only run.log/manifest: a stale list from a previous
# run would make this run look failed (or mask which units failed).
rm -f "$OUT/_failed_units.txt"
log(){ echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" | tee -a "$LOG"; }
man(){ echo "$1" >> "$MAN"; }

# A unit that produced no score.json FAILED. Record it loudly and durably:
# do_cell / the serial loop log per-unit success but nothing on failure, and the
# parallel driver logs "CHAIN DONE" whether or not its units succeeded — so a
# silently-empty run reads as a completed one. 2026-07-26: eight cells collided
# on Xvfb displays, produced nothing, and the log showed only CHAIN DONE for
# each; the loss was invisible for twenty minutes. A file, not a variable,
# because parallel units run in subshells.
FAILURES="$OUT/_failed_units.txt"
fail_unit(){ log "UNIT FAILED $1"; echo "$1" >> "$FAILURES"; }

cell_prompt(){ python3 -c '
import json,sys
raw=json.load(open(sys.argv[1]))
cells={c["id"]:c for c in (raw if isinstance(raw,list) else (raw.get("cells") or raw.get("prompts") or []))}
sys.stdout.write(cells[sys.argv[2]]["prompt"])' "$CORPUS" "$1"; }

# path to a saved def in a build bundle (first .json in routines/), or empty
# The routines/ dir also holds the ENABLED sidecar (enabled.json), which is NOT a
# routine definition. `head -1` is alphabetical, so the sidecar wins whenever the
# routine name sorts after "e" — the revise phase is then preseeded with a non-def
# and elmer_battery exits with "has no string `routine` field", losing the bundle
# while the chain still logs success. Pick the first file that actually carries a
# string `routine` field. (2026-07-26: base/P1 lost its whole revise arm this way;
# the reviewer also got the non-def and produced a 140-byte critique vs ~7000.)
built_def(){ python3 -c '
import glob, json, os, sys
for f in sorted(glob.glob(os.path.join(sys.argv[1], "routines", "*.json"))):
    try:
        d = json.load(open(f))
    except Exception:
        continue
    if isinstance(d, dict) and isinstance(d.get("routine"), str):
        print(f); break
' "$1" 2>/dev/null; }

# 1 if the bundle is a DETERMINISTIC fail (outcome!=completed OR !saved OR !green).
#
# EXCEPT when the scorer already ruled the cell n/a. elmer_score sets
# deterministic.verdict="n/a" for a cell whose corpus entry carries
# no_routine_expected (EU3, pure troubleshooting): saving no routine is the
# CORRECT outcome there, so routine_saved/validates_green are both false by
# design. Reading only those two flags made EU3 fail every attempt and burn the
# full re-run budget on every run, forever — 12 build bundles against 4 for a
# healthy cell. Honour the verdict the scorer already computed.
det_fail(){ python3 -c '
import json,sys,os
b=sys.argv[1]
try:
    s=json.load(open(os.path.join(b,"score.json")))
    o=json.load(open(os.path.join(b,"outcome.json")))
    d=s.get("deterministic") or {}
    if d.get("verdict") == "n/a":
        print(0)
    else:
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
  if [ -f "$out/score.json" ]; then return 0; fi
  fail_unit "$out"; return 1
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

# Exit non-zero if ANY unit failed, so a caller (or a human skimming the exit
# status) cannot mistake a partially-empty run for a complete one.
if [ -s "$FAILURES" ]; then
  log "RUN INCOMPLETE — $(wc -l < "$FAILURES") unit(s) failed; see $FAILURES"
  exit 1
fi

