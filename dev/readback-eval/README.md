# Readback style eval (tuxlink-k2h9l)

Decides the `routines_save` readback wording (mutation-contract slice (b),
tuxlink-fb0hc) on **measured divergence detection**, not taste — operator
direction 2026-08-13.

## The question

A readback exists to make intent-vs-artifact divergence visible. Which of the
three candidate styles — A narrative paragraph, B labeled scannable lines,
C edit-anchored diff — actually gets mismatches **caught** by a reader?

## Method

1. **Corpus**: 12 representative routines built through the real
   `RoutineDef` serde types (real action names and param shapes), each with a
   faithful prose request — the operator's intent.
2. **Mutations**: per routine, applicable injected divergences — recipient,
   schedule interval, time window, consent posture (attended→automatic),
   draft reference, connect params (band), dropped step, weakened retry. For
   style C: the edit-flow divergences (edit not applied; edit plus a smuggled
   consent flip).
3. **Renderers**: the real, artifact-derived renderers in
   `tuxlink-routines/src/readback.rs` (slice (b) substrate).
4. **Judges**: reader models play the operator — given REQUEST + READBACK,
   answer `{matches, differences[]}`. Two tiers (a frontier reader as
   careful-operator proxy, a small model as a stress test), N samples per
   cell for rates.
5. **Metrics**: per style — loose detection (flagged any mismatch), strict
   detection (named the mutated value), false-alarm rate on clean pairs;
   per-mutation-class breakdown.

## Run

```bash
# 1. Generate cases (R2 — cargo lives there):
cargo run -p tuxlink-routines --example readback_eval_gen \
  --manifest-path src-tauri/Cargo.toml > dev/readback-eval/cases.jsonl

# 2. Judge (any box with the OpenRouter key in the keyring):
python3 dev/readback-eval/judge.py dev/readback-eval/cases.jsonl \
  --model <frontier-id> --model <small-id> --samples 3 \
  --out dev/readback-eval/verdicts.jsonl
```

Results land in `RESULTS.md` beside this file; the operator's redline of the
winning style is the final pass before slice (b) builds against it.
