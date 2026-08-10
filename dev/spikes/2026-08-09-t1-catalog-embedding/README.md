# T1 catalog-embedding spike (Phase 1, tuxlink-efk3k)

First evidence run for the five-classifier architecture's **T1 tier** — the
mandatory CPU-encoder floor. Answers three questions from the kickoff plan
(`dev/plans/2026-08-09-elmer-classifier-architecture-kickoff.md`):

1. Can a 22–33M-param sentence encoder running on the Pi 5's CPU retrieve the
   right catalog item from a plain-language ask? (top-1 / top-5 / section hit
   rates over the real 1,477-item `winlink-queries.txt`)
2. What are the latency and RAM envelopes on the target hardware? (per-query
   encode ms, catalog precompute time, process peak RSS)
3. Do cosine margins separate "answer confidently" from "genuinely close —
   ask the operator"? (margin distributions by labeled expected behavior, plus
   no-match top-1 similarity for the reject threshold)

## Method

- Corpus: the shipped catalog verbatim (BOM-stripped, `SECTION|ID|desc|size`).
- Queries: `queries.jsonl`, 44 hand-authored asks labeled with real catalog
  IDs (verified by grep before labeling). Classes: unambiguous item targets,
  ambiguous asks (section-level or no-single-truth), a locality-context
  variant pair (`local-wx` vs `local-wx-ctx`), and out-of-catalog probes.
  Authored fresh for this spike — no tuxlink-bench corpus content.
- Three item-text templates (`desc`, `sec_desc`, `full`) to measure how much
  of the record should be embedded.
- One model per process run (honest peak-RSS attribution):

```sh
python3 -m venv venv && venv/bin/pip install -r requirements.txt
for m in sentence-transformers/all-MiniLM-L6-v2 BAAI/bge-small-en-v1.5 \
         intfloat/e5-small-v2 thenlper/gte-small; do
  venv/bin/python spike.py run --model "$m"
done
venv/bin/python spike.py report   # writes results/REPORT.md
```

Results land in `results/*.json` (per-query records included) and
`results/REPORT.md` (summary table). Findings feed the Phase-2 ADR; the
production implementation choice (ort/candle vs. this Python harness) is an
ADR concern, not this spike's.
