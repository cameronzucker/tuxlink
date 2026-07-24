# Session handoff — osprey-fen-peregrine (2026-07-24)

find_stations agent-native quality + the Build-Carefully lift. Long session: shipped
three PRs (all merged to `main` @ **1e07aa15**), added a Spark inference preset, ran
the lift twice (stopped both for correctness), caught + corrected an ADR-0022
deferral of my own, and built the consent-affordance fix whole.

## THE ONE THING TO DO NEXT

**Run the next ladder iteration: rebuild `elmer_battery` from `main` (1e07aa15) and
re-run the base+skill lift** against the corrected product (all three fixes below
are on `main`). Do NOT reuse prior cells; fresh results dir. The behavioral bar:
base's **P3 / S3** now reach an *honest terminal* — a saved routine + a
`saved-needs-operator` / "needs your acknowledgment" stop, or a clean **attended**
routine — instead of looping (S3) or emitting a silently-broken automatic routine
(P3 `AUTO_TX_UNACKED`). Inspect the new `disposition` field on the routines
save/edit/validate tool results to confirm the agent is getting the typed outcome.

## Shipped to main this session

- **#1252 (tuxlink-eig6e + tuxlink-8rpw5)** — find_stations drift items 1-4
  (unrepresentable partial-as-complete, real <32 KB guarantee + runtime backstop,
  exact coverage under exclusions, byte-bound routines step) + the **load-bearing
  band/dial fix**: `recommend` now returns an in-band `selected_connection`
  (band filter constrains connections, not just gateway eligibility).
- **#1253 (tuxlink-m5oia)** — routine edit ops report `applied:false` on a no-op
  patch (was hardcoded `true`), so agents stop looping identical edits.
- **#1254 (tuxlink-kbh4t)** — consent authoring: typed `AuthoringDisposition`
  (valid | invalid-agent-repairable | saved-needs-operator) with revision-bound
  remedies + `agent_terminal`, and the composition-gap closure
  (`CALLEE_CONSENT_UNREACHABLE`: authoring is now a superset of the runtime
  child-start gate). Spec: `docs/superpowers/specs/2026-07-24-routines-consent-authoring-disposition-design.md`;
  plan: `docs/superpowers/plans/2026-07-24-routines-consent-authoring-disposition.md`;
  Codex GPT-5.6-sol consult grounded.

## Spark inference control plane

Added a **non-thinking Nemotron preset** `ns120nt` (served `ns120-nvfp4-nothink`)
to `~/serving/spark-dashboard/profiles.json` on the Spark (`gx10-65aa`, reachable
`ssh inference.twin-bramble.ts.net`, passwordless sudo). Verbatim clone of `ns120`
+ `--default-chat-template-kwargs '{"enable_thinking":false}'` (matched control for
a thinking-vs-non-thinking benchmark). Live in the GUI (dashboard restarted).
`validated:false` — treat first switch as a load-test; first switch is a ~10-15 min
`docker run` and stops qwen (binds :8000). qwen served throughout.

## Open issues (tracked)

- **outputSchema wiring (P3)** — the deferred find_stations drift item 5, now
  tracked as its own open task (it was untracked when I closed tuxlink-eig6e — the
  ADR-0022 lesson). Advisory only, no functional impact.
- tuxlink-kbh4t, tuxlink-eig6e, tuxlink-8rpw5, tuxlink-m5oia — CLOSED.

## Process note (for the record)

I initially shipped #1253 (no-op fix) and deferred the consent-affordance gap as an
untracked "follow-up," then closed the issue — an ADR-0022 violation the operator
caught. Corrected: filed tuxlink-kbh4t + the outputSchema task as tracked open work,
and built kbh4t whole (design → Codex consult → spec → plan → TDD → verify → ship)
rather than deferring. No "follow-up" framing; no closing an issue with diagnosed-
but-unbuilt pieces left in prose.

## Build / verify environment

- **No cargo on the dev Pi.** Compile + test on **R2** (`ssh r2-poe`,
  `~/.cargo/bin/cargo` 1.96). CI mirrors `--workspace --all-targets --locked -D warnings`.
- R2 build worktree `~/tuxlink-eig6e-build` is the reusable compile box (currently on
  the kbh4t branch; `git fetch origin main && git reset --keep origin/main` to get to
  1e07aa15, then rebuild `elmer_battery`/`elmer_score`). Provenance: verify tree ==
  origin/main and record the main SHA in `binary-git-sha.txt`.
- Lift harness (on R2, in `~/tuxlink-eig6e-build`): the `run-lift-initial-2.sh`
  pattern — `elmer_battery --corpus tests/battery/corpus.json --model
  qwen35-122b-nvfp4 --endpoint https://inference.twin-bramble.ts.net/v1/chat/completions
  --arm <arm> --prompt <cell> --temperature 0.2 --turn-cap 40`, arms preflighted
  with `--list-arms` (base|matched-control|skill), keyless
  `OPENROUTER_API_KEY=local-vllm-nokey`. Corpus: 18 cells (P1-3 S1-4 A1-2 C1-3 E1-3
  EU1-3). Launch detached: `( setsid nohup bash <script> > <dir>/run.log 2>&1
  </dev/null & )`. Score with `elmer_score --root <dir> --corpus <corpus>`, then read
  deterministic `validates_green` + judge the `judge-queue.jsonl` LLM rungs. Thermal
  caveat: ~89 F internal / ~115 F external; the Spark may throttle → attribute
  latency spikes to heat, not the arm/model.
- Prior (superseded) lift dirs on R2: `battery-results/lift-initial-1` (stopped at
  10/36 for the no-op defect), `lift-initial-2` (stopped mid-run to build kbh4t).
  Use a NEW dir for the next iteration.

## Worktree state (dev Pi)

- `worktrees/bd-tuxlink-eig6e-drift-fidelity` — branch MERGED (#1252), dead. Dispose
  per ADR 0009 (node_modules + target + gitignored `.beads/embeddeddolt` class).
- `worktrees/bd-tuxlink-m5oia-routine-edit-noop-applied` — MERGED (#1253), dead.
- `worktrees/bd-tuxlink-kbh4t-consent-authoring-disposition` — MERGED (#1254), dead.
- `worktrees/handoff-osprey` — throwaway detached worktree used to commit this
  handoff; dispose after.
- `worktrees/handoff-tmp` — a pre-existing stray worktree (not mine; left as found).
- Main checkout is another session's (`bd-tuxlink-ant8s/...`), leased. Do write work
  in a worktree off main.
