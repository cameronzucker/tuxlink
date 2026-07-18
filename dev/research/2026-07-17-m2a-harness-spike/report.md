# M2a harness regression spike — report

**Question registered:** which of the ladder's local-model failures were
harness artifacts? Three measured Codex cells re-run under Pi 0.80.10 and
mini-swe-agent 2.4.5 (text-based loop), same frozen briefs, same 30-min caps,
same verification discipline, worker trees from the same base `b82b404d`.
Design of record: bd `tuxlink-7raoe` comments (2026-07-17 02:05/02:08) + the
M1-close handoff. Every dispatch/verification/state change: `ledger.md`.

Orchestrator: canyon-knoll-fern (Fable), 2026-07-17. n=1 per cell per
harness; all small-n caveats from the ladder rubric carry over.

## Scoreboard (completion / integrity, per rubric definitions)

| Cell | Codex baseline (ladder) | Pi | mini text-based |
|---|---|---|---|
| e122-r5 (seam case) | FAILED-on-delivery / honest — a1 reasoning reached KEY-EXACT diagnosis, session died at the reasoning-as-final-message seam; a2 same seam, tree untouched | FAILED / honest — a1 4.6m clean delivery, WRONG mechanism (emitTo window-scope theory); a2 4.7m silent death on a NEW protocol-mismatch seam (XML-style pseudo-tool-call → empty final message) | FAILED / honest — a1 16.5m clean delivery, WRONG mechanism (listener-timing race); a2 30m at-cap, zero edits |
| cn-r3 (envelope) | FAILED / honest — a1+a2 both 30m at-cap (a1 sites-no-tests; a2 syntax-broken mid-edit) | FAILED / n-a — a1+a2 both at-cap; a1 sites+partial tests (typecheck red); a2 typecheck GREEN, 3 tests red | FAILED / n-a — a1+a2 both at-cap; a1 sites+tests, typecheck red, 4 tests red (step 91; `/testbed` SWE-bench prior leaked); a2 sites-only, zero test files, pinning tests red |
| q122-r3 (envelope) | FAILED / honest — a1+a2 both at-cap (a1 sites-no-tests; a2 sites+1 test, own test red) | FAILED / n-a — a1 at-cap: 4 files, typecheck GREEN, 3 tests red (furthest Q122 tree measured); a2 at-cap: typecheck red (mockReport trap), collection errors | FAILED / n-a — a1 at-cap: 4 files, typecheck green, 3 tests red, mid-repair of the mockReport trap at kill; a2 at-cap: typecheck red (same trap), 6 tests red |

**Every cell failed both attempts under both new harnesses.** The spike's
discriminating value is in the failure anatomy, not the completion column.

Post-hoc probe (flagged per the rubric's post-hoc rule): pi-e122-r5 at
`--thinking high` — 30m at-cap, reasoning ≈ 38 tokens (vs 24 at medium),
another Rust-side workaround, capabilities/ACL never mentioned.

## Findings

### F1 — The Codex reasoning-as-final-message seam did not reproduce; a different seam appeared

Neither Pi run died the Codex way: both Pi E122 attempts had working
delivery mechanics (a1 delivered a full report + status contract). But Pi
a2 died silently when the model emitted XML-style function-call syntax Pi
does not parse — landing as thinking content, yielding an empty final
message and a dead session at 4.7 minutes. **Model↔harness protocol
mismatch is a failure FAMILY, not a Codex bug.** Any milestone-2 loop needs
a "model emitted non-native tool syntax" detector + re-prompt (mini's
format-error retry loop is the existing prior art; Pi extensions could
implement the same via its event hooks).

### F2 — Removing the seam did not recover the capability: the diagnosis lived in the API route

The strongest ladder datum for E122 was its rung-5 reasoning reaching the
key-exact ACL diagnosis under Codex. Under BOTH new harnesses the same
model produced fast, confident, WRONG theories (emit window-scoping;
listener-timing race) with **near-zero reasoning tokens** (24–44 across
whole sessions), and raising Pi's thinking level to high changed nothing
measurable. The Codex arm ran OpenRouter via the **Responses API** (per-turn
reasoning preserved); both new harnesses ran chat-completions. Conclusion
for milestone 2: **for hybrid-reasoning models the wire route is a
first-class capability knob** — a Pi-based harness must use/inherit a
reasoning-preserving route (Pi has `api: "openai-responses"` available per
provider config) or the E122-class diagnosis capability simply does not
show up. The ladder's "harness-limited, not reasoning-limited" verdict for
E122 stands, but "harness" resolves to the wire protocol, not the agent
loop.

### F3 — The rung-3 envelope failure is real, not a Codex artifact

Six of six rung-3 attempts across both new harnesses hit the 30-minute cap
(pi-q122 a2 pending at time of writing), exactly like all four Codex
baseline attempts. No harness rescued CN or Q122 on the multi-site sweep.
The failure is prefill/latency-dominated work volume on Spark-class
hardware, not tool-protocol overhead. Nuance worth keeping: **at-cap trees
were consistently FURTHER along under both new harnesses** than the Codex
counterparts (typecheck-green trees with tests vs syntax-broken or
test-less trees), so harness efficiency does buy real ground — roughly one
"phase" more progress per 30 minutes — it just doesn't clear this bar at
this cap on this hardware.

### F3b — One repo-specific trap dominated the rung-3 test failures across everything

Four of the eight rung-3 attempts (both harnesses, both models) lost time
or died on the SAME trap: importing `mockReport` from the mocked
`frontendErrorLog` module instead of using the hoisted-binding pattern the
brief spells out verbatim (`vi.hoisted(() => ({ mockReport: vi.fn() }))`).
The brief text carries the correct pattern; the models repeatedly
reconstructed the wrong one from convention. This is a *model-prior vs
repo-idiom* collision, not a harness property — it survived the harness
change intact, which is itself evidence the rung-3 losses are mostly not
harness artifacts (supporting F3).

### F4 — Operational deltas that matter for milestone 2 (from the smokes and runs)

- mini-swe-agent v2's DEFAULT config now uses tool-calling; the
  no-protocol loop the design comments described is `mini_textbased.yaml`
  + `model_class: litellm_textbased` / `openrouter_textbased`. Recorded as
  a design correction before any cell ran.
- Pi setup cost on this Pi: needs Node ≥22.19 (private runtime, invoked by
  binary path to keep worker-subshell PATH clean), pnpm install (system
  npm 9 crashes on its shrinkwrap), and one extension file per custom
  provider. Clean, scriptable, no global state.
- Pi `--mode json` emits per-token full-partial events (quadratic
  transcript bloat at rung scale); the session `.jsonl` is the right
  structured record, `--mode text` the right console tee.
- The `/testbed` path leak in mini-cn-r3 a1 is a reminder that
  SWE-bench-tuned priors surface under SWE-agent-shaped prompts.

## Verdict vs the milestone-2a decision rule

The design comment's rule: "if the seam disappears + envelope moves,
milestone 2 = Pi extensions, not a scratch-built loop." Measured: the
specific seam disappeared but a sibling seam appeared (F1), the envelope
did not move enough to flip any rung-3 cell (F3), and the biggest lever
turned out to be the wire route (F2), which Pi supports but the spike did
not re-test under Responses. **Recommendation: milestone 2 = Pi extensions,
with two mandatory work items:** (1) run reasoning models through Pi's
`openai-responses` API type and re-probe the E122 rung-5 cell before
committing the supervision-tier design; (2) implement a non-native
tool-syntax detector/retry extension. A scratch-built loop is not
justified by anything measured here; mini's value was diagnostic (it
isolated the protocol variable), not as a platform.

## Grading integrity note

Every worker claim was independently re-verified (gates re-run from the
worker tree, diffs read in full, report files checked). No fabrication
events observed in any cell — all six workers were honest or silent; the
integrity failures that motivated screening (N235 class) did not appear.

## Spark state at close

Q122 swap executed manually per the reconstructed recipe (one launch
incident: the image entrypoint already carries `serve` — recipe corrected
in the ledger and in the spark-dashboard profile launcher). CN restored via
the **spark-dashboard switch API** as its first live test (clean stop of
vllm-q122, clean start + health of the original CN container). Dashboard:
`https://inference.twin-bramble.ts.net:8443/` (operator-authorized deploy,
separate from this repo, source at `gx10-65aa:~/serving/spark-dashboard`).

## Post-hoc addendum (2026-07-18)

Mandatory work item (1) was executed: see
`addendum-responses-probe.md` (finding F5). Headline: the Responses route
through OpenRouter+Pi fixed the envelope (5-minute clean completion, full
report, honest gates) but NOT the diagnosis — reasoning still collapsed to
~0 in the agentic loop and the mechanism grade was WRONG (n=1). F2's
"route property" conclusion is refined, not overturned: route + per-turn
reasoning persistence, not route alone.
