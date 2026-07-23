# Session handoff — shoal-vetch-basin (2026-07-23, late)

The find_stations agent-native redesign is **built, merged to `main`, and the
overflow is fixed and empirically verified.** A Codex drift review then found
that the spec's strongest "by construction" *guarantees* are weaker than
advertised (though **no live overflow**). Per operator: **fix that drift FIRST,
confirm it, THEN start the lift test.**

## THE ONE THING TO DO NEXT

**Read and execute `bd show tuxlink-eig6e`** — a detailed, per-item fix-spec
(file:line, the gap, the exact fix, acceptance test, order). Fix items 1→4 (5 if
time), verifying each on R2 (`~/.cargo/bin/cargo` 1.96 — never `cargo` on the dev
Pi). When 1–4 are green on both arches, **then** run the lift.

Do NOT re-derive the design or re-open the overflow question — that's done. This
is a fidelity pass: make the promised invariants actually type/test-enforced.

## State (all on `main` @ `9cae799e`)

- **MCP `find_stations` redesign — MERGED** (PR #1249): intent-tagged single tool
  (recommend/explore/lookup/aggregate/export), bounded `StationQueryEngine`,
  `FindStationsParams` object-root schema wrapper, `de_stringy_or_native` leniency
  (qwen sends typed/nested args as JSON strings), `SnapshotStore` managed state.
- **Routines `data.find_stations` bound — MERGED** (PR #1251): omitted `limit` →
  default 8, clamped to hard-max 16, truncation disclosed. (Codex: this bounds
  station COUNT, not bytes, and doesn't use the engine — that's drift item 4.)
- **`elmer_battery` on `main`** has the `+Skill` arm (from #1248, already on main)
  AND a `--list-arms` preflight guard. So the lift is runnable from `main`.

## Empirical proof the overflow is fixed (so the test WILL run)

A guarded base+skill smoke (C2, qwen-3.5-122b, R2) completed on **both arms**: a
broad recommend over the real ~1,334-station directory returned a **~2.6 KB
`ranked-subset`** (5 of 1,334, coverage disclosed) — vs the old ~631 KB dump.
Zero deserialize errors, zero panics, arm preflight passed. The provenance SHA was
recorded. Results dir on R2: `battery-results/lift-smoke-1/`.

## The drift to fix first — `tuxlink-eig6e` (P1), in brief

1. `StationResult`/coverage fields are `pub` → "unrepresentable by construction"
   is really engine-convention. Wrap variant payloads in private-field structs +
   checked constructors.
2. **(most load-bearing)** `<32 KB` isn't actually guaranteed: `CappedString`
   allows control chars (JSON-escape up to 6×) and the property test fills with
   `"X"`; no runtime size postcondition; test measures the DTO not the wrapped MCP
   block. Strip control chars, add a runtime `Contract` byte-check, account for
   wrapping.
3. `recommend` coverage is inexact under `exclude_candidate_ids` (`evaluated`
   reports full eligible). Compute from the post-exclusion set.
4. Routines step is station-count-bounded, not byte-bounded, and not same-engine.
   Add a row/vector cap (minimal) or route through the engine (spec-faithful).
5. (P3) Wire the tool `outputSchema`.

Codex **validated as sound**: the object-root wrapper and the lenient deserializer
(still rejects over-cap). Full transcript (local, gitignored):
`dev/adversarial/2026-07-23-find-stations-drift-review-codex.md`.

## How to run the lift once drift is green (no bespoke tooling needed)

From a **clean checkout of `main`** on R2 (`git fetch && git checkout <main-sha>`
— clean, not rsync, so provenance is honest), build `elmer_battery` + `elmer_score`
with `~/.cargo/bin/cargo`, preflight arms with `elmer_battery --list-arms` (must
list `base | matched-control | skill`), then run `base` and `skill` over the
corpus prompts against the qwen endpoint (`https://inference.twin-bramble.ts.net`,
model `qwen35-122b-nvfp4`, keyless `OPENROUTER_API_KEY=local-vllm-nokey`). Record
the binary SHA. Keep both arms on the SAME binary; do NOT reuse the smoke cells.

## Open follow-ups (not blockers)

- `tuxlink-8zq7u` (P1): Elmer needs a mailbox/content classifier for
  prompt-injection defense (Outlook+Copilot style). Separate epic.
- `tuxlink-a4zzo`: routines same-engine adoption (subsumed by drift item 4).

## Branch/worktree notes

- `main` @ `9cae799e` is the source of truth. The m0n38 and a4zzo branches are
  merged/dead.
- Main checkout is another session's (`bd-tuxlink-ant8s/...`); leased. Do write
  work in a worktree off `main`.
