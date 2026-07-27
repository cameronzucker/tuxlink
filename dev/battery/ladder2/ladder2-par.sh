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
#      LADDER2_TURN_TIMEOUT_SECS  response deadline, default 3600
#      TUXLINK_MAX_RUN_SECS       whole-run deadline; exported to the binary,
#                                 which falls back to its built-in 1800s default
#
# BOTH deadlines are WALL-CLOCK. Running wide slows each individual bundle even
# though aggregate throughput rises, so at width>3 they must be raised or long
# cells are truncated and recorded as `needs_operator`, which reads as a
# capability failure rather than the throughput artifact it is. On 2026-07-25
# this cost 50 of 220 bundles at width 8 against the stock 1800s/600s.
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
# rev_on is RETIRED (operator decision 2026-07-27, tuxlink-jaer0): reasoning ON
# only hurts the Nemotron reviewer — rev_off beat rev_on 39% vs 28% pass in the
# clean skill arm, and Nemotron is vetted capable with reasoning OFF. This was
# settled earlier but only applied via per-run env overrides, so every fresh
# launch silently re-tested a settled question. Do not re-add "on" here; a
# deliberate re-test is a new experiment with its own bd issue.
REVCONDS="off"
# The skill arm may carry an extra review condition: `skill` uses the
# Codex-authored review-skill.md as the reviewer's system prompt (reasoning
# OFF). Set to "off skill" to add it; the driver skips conditions that already
# have a score.json, so adding it to a populated tree runs ONLY the new column.
REVCONDS_SKILL="${LADDER2_REVCONDS_SKILL:-off}"
SKILLFILE="$OUT/review-skill.md"
MAXATT=3
CONC="${LADDER2_CONC:-3}"
# 3600s, not 1800: the enriched authoring surface (14 grounding tools + 5
# validator advisories, PRs #1261/#1262) legitimately deepened iteration on
# branch-heavy cells — natural runtime moved to ~1800s+ (base/S4 completed at
# 1903s/22 turns; P3 base truncated 3/3 at the 1800s wall). A tighter budget
# censors exactly the cells the experiment cares about; churn cost is measured
# as duration instead. Evidence: tuxlink-3cal1 notes (2026-07-27).
TURNTO="${LADDER2_TURN_TIMEOUT_SECS:-3600}"

mkdir -p "$OUT"
# Per-run, unlike the append-only run.log/manifest: a stale list from a previous
# run would make this run look failed (or mask which units failed).
rm -f "$OUT/_failed_units.txt"
# Small appends (<PIPE_BUF) with >> are atomic on Linux, so concurrent workers
# can share these logs without interleaving a line.
log(){ echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" >> "$LOG"; }
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
lat(){ echo "$1" >> "$LAT"; }

cell_prompt(){ python3 -c '
import json,sys
raw=json.load(open(sys.argv[1]))
cells={c["id"]:c for c in (raw if isinstance(raw,list) else (raw.get("cells") or raw.get("prompts") or []))}
sys.stdout.write(cells[sys.argv[2]]["prompt"])' "$CORPUS" "$1"; }

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

# EXCEPT when the scorer already ruled the cell n/a: elmer_score sets
# deterministic.verdict="n/a" for a corpus entry carrying no_routine_expected
# (EU3, pure troubleshooting), where saving no routine is the CORRECT outcome, so
# routine_saved/validates_green are both false by design. Reading only those two
# flags made EU3 fail every attempt and burn the full re-run budget on every run
# — 12 build bundles against 4 for a healthy cell.
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
      --turn-timeout-secs "$TURNTO" \
      --out "$out" "${extra[@]}" >> "$out/harness.log" 2>&1
  t1=$(date +%s)
  "$SCORE" --root "$out" --corpus "$CORPUS" >/dev/null 2>&1
  # Wall clock per unit + the outcome, so the width-vs-latency curve and any
  # timeout inflation (the 1800s max_response_duration is WALL-CLOCK) are
  # measurable rather than inferred.
  lat "{\"unit\":\"$out\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"conc\":$CONC,\"secs\":$((t1-t0)),\"outcome\":\"$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["outcome"])' "$out/outcome.json" 2>/dev/null || echo unknown)\"}"
  if [ -f "$out/score.json" ]; then return 0; fi
  fail_unit "$out"; return 1
}

# One full (skill, cell) chain: shared build (+determinism re-runs), then each
# review condition's review+revise. Identical semantics to ladder2.sh.
do_cell(){
  local skill="$1" cell="$2" disp="$3"
  local bdir="$OUT/$skill/$cell/build"
  # EVERY rung runs 3x unconditionally (operator decision 2026-07-27,
  # tuxlink-x43aa): the det_fail-gated design re-sampled only FAILURES, so a
  # lucky single-attempt green was trusted as stable. Models are
  # non-deterministic; a result is a RATE, and successes get flakiness-tested
  # exactly like failures. Per-attempt score.json skip keeps resume semantics.
  for a in 1 2 3; do
    run_cell "$bdir/attempt-$a" "$skill" "$cell" "$disp" && log "build $skill/$cell #$a: $(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["outcome"])' "$bdir/attempt-$a/outcome.json" 2>/dev/null)"
    man "{\"phase\":\"build\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"attempt\":$a,\"det_fail\":$(det_fail "$bdir/attempt-$a")}"
  done
  local def; def="$(built_def "$bdir/attempt-1")"
  local conds="$REVCONDS"
  [ "$skill" = "skill" ] && conds="$REVCONDS_SKILL"
  for cond in $conds; do
    local rdir="$OUT/$skill/$cell/rev_$cond"
    for a in 1 2 3; do
      local adir="$rdir/attempt-$a" mdir="$rdir/meta-$a"
      if [ -f "$adir/score.json" ]; then continue; fi
      # 3x unconditional (tuxlink-x43aa): no det_fail gate — every rev
      # attempt runs, sampling reviewer+revise stability like the build.
      mkdir -p "$mdir"
      local cp_prompt="$mdir/user_prompt.txt"; cell_prompt "$cell" > "$cp_prompt"
      # The builder's own account + the post-build routine inventory. Both were
      # invisible to the reviewer before 2026-07-26, so it scored 0 on final_text
      # dishonesty (17 bundles) and orphaned duplicates (22 bundles).
      python3 -c 'import json,sys
try: sys.stdout.write((json.load(open(sys.argv[1])).get("detail") or "").strip())
except Exception: pass' "$bdir/attempt-1/outcome.json" > "$mdir/final_text.txt" 2>/dev/null
      ls -1 "$bdir/attempt-1/routines" 2>/dev/null > "$mdir/inventory.txt"
      if [ ! -s "$mdir/inventory.txt" ] && [ -z "${def:-}" ]; then
        # Give-up laundering guard (tuxlink-hwo1b, lnctz base/C1): with no
        # saved routine there is nothing to critique, and the reviewer —
        # handed only the builder's final_text — parrots the give-up prose
        # back verbatim (meta-N/critique.txt was byte-identical to
        # final_text.txt), re-injecting the excuse as authoritative reviewer
        # input. Substitute a fixed authoring redirect instead; every
        # skill-arm C1 revise that received an author-and-save critique
        # recovered.
        printf '%s\n' \
          "No routine was saved. The deliverable is a SAVED routine: call routines_actions_list, translate the request into steps, save with routines_save, and check with routines_validate. If a capability the request needs has no routine action in the catalog, save the closest achievable routine and name the missing action in your final summary." \
          > "$mdir/critique.txt"
        : > "$mdir/critique.meta"
      else
        REVIEW_SKILL_FILE="$([ "$cond" = "skill" ] && echo "$SKILLFILE")" \
        OPENROUTER_API_KEY="$ORKEY" python3 "$REVIEW" "$cp_prompt" "${def:-/nonexistent}" "$cond" "$CATALOG" \
            "$mdir/final_text.txt" "$mdir/inventory.txt" \
            > "$mdir/critique.txt" 2> "$mdir/critique.meta"
      fi
      { cat "$cp_prompt"; echo; echo "---"; echo "A reviewer critiqued your routine. Address every point, then re-save:"; echo; cat "$mdir/critique.txt"; } > "$mdir/revise_prompt.txt"
      run_cell "$adir" "$skill" "$cell" "$disp" "${def:-}" "$mdir/revise_prompt.txt" && log "revise $skill/$cell/rev_$cond #$a"
      man "{\"phase\":\"revise\",\"skill\":\"$skill\",\"cell\":\"$cell\",\"cond\":\"$cond\",\"attempt\":$a,\"det_fail\":$(det_fail "$adir")}"
    done
  done
  # Say which happened. "CHAIN DONE" beside eight empty cells is how the
  # 2026-07-26 Xvfb collision stayed invisible for twenty minutes.
  if grep -q "^$OUT/$skill/$cell/" "$FAILURES" 2>/dev/null; then
    log "CHAIN INCOMPLETE $skill/$cell — $(grep -c "^$OUT/$skill/$cell/" "$FAILURES") unit(s) failed"
  else
    log "CHAIN DONE $skill/$cell"
  fi
}

log "LADDER2-PAR START conc=$CONC turn_timeout=${TURNTO}s max_run=${TUXLINK_MAX_RUN_SECS:-1800(default)}s cells=[$CELLS] skills=[$SKILLS] revconds=[none $REVCONDS / skill-arm: $REVCONDS_SKILL]"
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

# Exit non-zero if ANY unit failed, so a caller (or a human skimming the exit
# status) cannot mistake a partially-empty run for a complete one.
if [ -s "$FAILURES" ]; then
  log "RUN INCOMPLETE — $(wc -l < "$FAILURES") unit(s) failed; see $FAILURES"
  exit 1
fi

