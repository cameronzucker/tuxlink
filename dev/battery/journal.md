# Battery journal — stage-gated ladder (bd tuxlink-hwgdi)

Tracked record of every sweep: judged results, attribution, fixes, spend.
Bundles themselves are gitignored (`battery-results/`, full bundles live on
R2 at `~/tuxlink-battery-build/battery-results/`); THIS file is the durable
cross-session record. Newest entries first.

Ladder: P2 → P1 → S1 → S2 → S4 → S3 → P3 (advance only when the stage is
fully addressed). Models: qwen/qwen3.5-122b-a10b, z-ai/glm-5.2,
anthropic/claude-sonnet-5, openai/gpt-5.5. FABLE 5 DISCONTINUED
2026-07-21 (operator: >95% of account usage was Fable across GUI testing +
battery; disproportionate at $10/$50 per M — its P2 PASS stands as recorded
evidence, no further cells). Account re-upped $50 same day; cap stands.
Budget: $50 hard cap (ledger at battery-results/ledger.json on R2;
harness refuses ≥ $45).

Attribution vocabulary (bd tuxlink-6zkb6): tuxlink-design-defect |
model-family-trend | ambiguous. Compat is the belt, prose the suspenders.

---

## 2026-07-22 — Stage S2 (4/4 PASS) + Stage S4 (2/4): the composition-depth stage

**S2 (edit-in-place: 40m-dial routine → 15m cadence, +80m fallback, record
band).** All four PASS clean via routines_get + edit verbs (zero blind
resaves, zero sibling-routine creation - the quality-FAIL signals). qwen the
MOST efficient (5 calls, no thrash) and folded band+station into one
interpolated log message using the embedded-ref interpolation shipped in
6epl8 - a feature absorbed from ITS OWN earlier emission habit. Mode 4
(edit-verb thrash) did NOT recur on this small task; P1's 16-edit thrash is
plausibly task-size-dependent, not intrinsic.

**S4 (daily check-in: preset + ATU + compose→W7EOC + connect + APRS - the
deepest composition on the ladder).**

| model | verdict | notes |
|---|---|---|
| openai/gpt-5.5 | **PASS** | All 5 actions, correct order, success-gated APRS. Only finding: the expected missing-preset UNRESOLVED_REF (predicate allows it). |
| z-ai/glm-5.2 | **PASS** (harness-fix validated) | Complete correct def (apply_preset→tune_atu→compose to W7EOC w/ DM33→find→connect→aprs). ROUND 1 was harness-VOID (died turn 2 on a rig_status probe); after tuxlink-zvy6q it made the same probe (denied_calls:1), got the redirect, and CONTINUED to 30 turns and a clean authored routine. The fix turned a void into glm's best showing. |
| anthropic/claude-sonnet-5 | **FAIL** | Dropped 3 of 5 required actions (no tune_atu, no compose, no aprs_send) and hallucinated `$s1.callsigns` as a rig.apply_preset output (REF_UNKNOWN_OUTPUT). Substantively incomplete. |
| qwen/qwen3.5-122b-a10b | **FAIL (mode 1)** | Silent scope-narrowing in its PUREST form: 6 local.log steps NARRATING the task ("Applying 40m-digital preset and tuning ATU"...) with ZERO real actions. Validates clean because it does nothing. |

**Attribution: MODEL-FAMILY-TREND (composition-depth), NOT tuxlink-design-defect.**
Two failures, two different mechanisms (sonnet step-drop vs qwen narration);
gpt AND glm handled the surface as-is. Critically NOT a frontier-vs-local
split - glm (local target) PASSED while sonnet (frontier) FAILED - so the
divergence is idiosyncratic to composition depth, not a capability tier. No
Tuxlink code fix for the failures (no single missing affordance explains
both). The one real code fix S4 produced was the HARNESS one (zvy6q): the
allowlist denial was terminal, voiding glm's round-1 cell and biasing the
battery against exploratory agent behavior. Fixed + validated inline this
same stage.

**Fine-tune catalog (tuxlink-77620):** qwen mode-1 RECURRED at S4 (strong -
it is the deepest-composition, most-dangerous mode). glm competent at the
same task = glm-5.2 is plausibly the stronger local target, or qwen needs
mode-1-specific training. Modes 1 + 3 remain the live targets; mode 2
(product-absorbed via ARM_FALLTHROUGH_LEAK) and the S2 data confirm the
belt/suspenders + validator-teaching approach is closing the ADDRESSABLE
modes, leaving the intent-level ones (narrate-instead-of-act) for fine-tuning.

## 2026-07-22 — Stage S1 COMPLETE (4/4): qwen re-run vs ARM_FALLTHROUGH_LEAK

qwen cell re-run on main cba0d3f4 (PR #1234: the validator finding its
round-1 FAIL demanded). Verdict: **PASS, and the discriminating datum is
gold** - tool_calls seq 2 shows qwen reproduced the IDENTICAL leaky layout,
the new warning fired in the save result, and seq 7 shows the exact taught
repair (`{"control":"end","id":"s4b"}` after the success log). Final def:
success path terminates before the else-arm; APRS fires only on failure;
clean validate (ATTENDED_UNDER_SCHEDULE + environment warnings only).

**Fine-tune catalog implication (bd tuxlink-77620): validator-guided
self-correction WORKS on the 122b target.** Failure mode #2
(execution-semantics-blind layout) reclassifies PRODUCT-ABSORBED - the
model does not need to know jump semantics if the validator names the leak
and the fix. Modes #1 (silent scope-narrowing) and #3 (narrative
overclaim) remain the live fine-tune targets: they are the ones no
boundary layer can catch, because the model never surfaces the intent.

STAGE GATE CLEARED. Ladder advances to S2 (sweep post-6epl8-1). Also
merged this session: PR #1235 (hook-enforced ban on sweep-staging +
chained mutating git ops, tuxlink-18san - session-tooling, not
battery-gated).

## 2026-07-22 — Stage S1 RE-RUN, sweep post-6epl8-1 (post-absorption, 4-model roster)

Run on merged main 9e111a67 (PR #1232: branch-dialect absorption belt +
catalog/refusal teaching suspenders, 6epl8). Whole stage: ~8 minutes, ~$0.95
(vs the pre-fix S1: 2 cancels at caps, ~$4.46).

| model | verdict | turns to done | spend | notes |
|---|---|---|---|---|
| z-ai/glm-5.2 | **PASS clean** | 21 calls | $0.0571 | Correct branch (on s2.connected, flat shape, id-list arms) AND correct jump+fall-through layout (then-log, end-ok, else-log, aprs, end-failed). Chose automatic mode and surfaced AUTO_TX_UNACKED as blocking with the exact right framing ("can only be recorded in the Tuxlink UI - I can't grant it here"). Was: 40-turn cancel after 7 dialects. |
| anthropic/claude-sonnet-5 | **PASS** | ~14 calls | $0.3880 | Correct layout; arm lists carry full path ids (harmless: executor jumps to arm.first()). Was: $2.21 cancel after 11 dialects. |
| openai/gpt-5.5 | **PASS** | ~15 calls | $0.5020 | Correct layout + end-failed semantics on the no-gateway path. Was: false "completed" on a branchless stub. |
| qwen/qwen3.5-122b-a10b | **FAIL (new class: layout)** | 8 calls | ~$0 (billing lag) | Authored a REAL branch (was: linear dodge) with the right condition, but storage layout [branch, then-log, else-aprs, log, end] leaks: the SUCCESS path falls through into radio.aprs_send - a false "no gateway" alert transmitted every successful cycle. validate said NOTHING (only ATTENDED_UNDER_SCHEDULE). |

**THE DIALECT WALL IS DOWN.** Zero branch_dialect absorption markers in any
transcript: all four families emitted the REAL flat shape natively - the
teaching suspenders (catalog controls section, branch-in-situ template,
honest refusals) sufficed, and the absorption belt sat unexercised (its
table-driven tests remain its evidence). invalid_args across the whole
sweep: 3 total (2 routine-name grammar, 1 patch key), all self-corrected
next call. Compare: 20+ branch-dialect refusals across the pre-fix sweep.

**qwen's FAIL attribution: tuxlink-design-gap + model-family-trend.** The
def is structurally valid and validation-honesty-silent, yet transmits
falsely. Filed bd tuxlink-ilrav (P1): ARM_FALLTHROUGH_LEAK validator
finding (fall-through walk from each arm entry; reaching the other arm's
entry warns, message teaches insert-an-end vs deliberate shared tail).
Warning not error: exclusive-prefix-shared-tail convergence is only
encodable in this exact shape. Fix + corpus fixture + tests on
bd-tuxlink-ilrav/arm-fallthrough-leak (PR #1234). Per stage-gating, S1 is
NOT fully addressed until the finding merges and the qwen cell re-runs
against it.

Also landed this session (CI-tax reduction, not battery-gated): PR #1233
retries ETXTBSY in tuxlink-jt9 decode_slot spawns - the fake_jt9 flake
that taxed both #1229 and #1232 with ~15-minute reruns (tuxlink-ux4t7
closed; tuxlink-b5qfw tracks any non-ETXTBSY residue).

## 2026-07-21 — harness bring-up

- Harness committed (2d32b7d8) + built clean on R2 first try (574 crates,
  rustup stable via ~/.cargo/bin; system cargo 1.75 cannot build the locked
  deps — use the full path in non-interactive SSH).
- Free smoke (invalid key): windowless `Builder::build()` + scratch
  isolation preflight PASSED on R2 under xvfb — the design's top build risk
  is retired; abort came at the credits gate as designed.
- Stage P2 sweep `smoke-1` started (qwen first).

## 2026-07-21 — Stage P2, sweep smoke-1

| model | verdict | turns | spend | notes |
|---|---|---|---|---|
| qwen/qwen3.5-122b-a10b | **PASS clean** | 7 | $0.0204 | All predicates + globals. Used the catalog's marquee `$s1.callsigns → radio.connect` run-time composition (find_stations limit 1 → connect → log; every 1h align hour, if_missed skip). Zero denials, zero string-coercion. Surfaced ATTENDED_UNDER_SCHEDULE to the user with the correct automatic-mode remedy and did NOT flip modes unilaterally. Wart: narrative claimed "saved and enabled" but never called (or attempted) enable. |
| z-ai/glm-5.2 | **PASS clean** | 12 | $0.0926 | Same run-time `$s3.callsigns` idiom, log-bracketed (start/complete logs), clean structure, zero denials. NOTE: the real-world empty-def failure (transcript 1784664175708-1) did NOT recur at this rung — consistent with the wall being at control-flow difficulty, not baseline. |
| anthropic/claude-sonnet-5 | **PASS (dialect note)** | 6 | $0.0530 | Baked the station at authoring time (`stations: ["N0DAJ"]` from a find_stations query during authoring) — satisfies the predicate but semantically weaker than run-time resolution for "closest". Dialect split recorded: qwen/glm/gpt resolve at run time, sonnet bakes. |
| openai/gpt-5.5 | **PASS+ (best def)** | 8 | $1.0909 | Added `data.stationlist_update` before find_stations (fresh directory each fire) and `listen_before_tx_s: 5` (clear-channel check) — most operationally polished artifact. Denied once on `routines_enable` (harness defect, see below). Cost outlier: $30/M output + reasoning tokens. |
| anthropic/claude-fable-5 | **artifact PASS; cell re-run** | 8 | $0.5236 | Complete valid def + validate call, then denied on `routines_enable`, then the HARNESS cancelled it on a 4x-overshooting cost estimate ($2.07 est vs $0.52 actual — anthropic prompt-cache billing) and the cancel path panicked on unmanaged ArdopListenState. Both harness defects. Clean re-run in flight on the fixed harness. |

**Stage P2 verdict: NO Tuxlink defects at this rung.** Five distinct valid
dialects; zero string-coercion events; consent surfacing correct everywhere.
Three HARNESS defects found and fixed (commit 4838c600): (1) enable/disable
falsely excluded from the allowlist — both frontier models correctly finished
the arc with `routines_enable` (an un-enabled scheduled routine never fires);
(2) abort-path states (ArdopListen/VaraListen/Aprs) unmanaged → worker panic
on any cancel; (3) watchdog cost gate now polls OpenRouter credits live —
token estimates overshoot 4x on cached-prompt providers.
Spend so far: ~$1.78 of $50.
GATE TO P1: fable clean re-run judged, then advance.

Harness observation (not a Tuxlink defect): `routines_set_enabled` is excluded
from the battery allowlist per adrev disposition 3, but that diverges from the
production agent surface without a safety need (enable of attended parks; of
automatic needs un-grantable acks; scratch profile has no rig). Candidate:
add it next harness iteration so enablement dialect is observable.
[RESOLVED same day: routines_enable/disable + journal_get/run_status admitted,
commits 4838c600 + c32ab4db.]

## 2026-07-21 — Stage P1, sweep smoke-1 (PARTIAL: budget-blocked)

| model | verdict | turns | spend | notes |
|---|---|---|---|---|
| qwen/qwen3.5-122b-a10b | **PASS** | 21 | $0.1402 | Correct final artifact (find-5 run-time walk → `$s2.station` winner log, 30m). Chose `transmit_mode: automatic` (defensible; gate blocks until operator ack). Heavy edit-verb thrash (16 add/update/remove before converging) — efficiency dialect note, zero denials. |
| z-ai/glm-5.2 | **PASS** | 13 | $0.2632 | Clean run-time walk, winner logged, attended. THE HEADLINE: identical prompt to the 2026-07-21 real-world empty-def failure (transcript 1784664175708-1) now one-shots. Battery pins temperature=0.2; the GUI ran provider defaults — the real-world failure likely had a stochastic component. 6epl8 evidence updated. |
| anthropic/claude-sonnet-5 | **PASS** | 8 | $1.4458 | Reproduced its GUI reference def exactly (baked nearest-first 5, `$s1.station` log, 30m). |
| openai/gpt-5.5 | **BLOCKED (402)** | — | $0.5283 partial | OpenRouter refused pre-auth: provider requests max_tokens=65536; balance below the hold. |
| anthropic/claude-fable-5 | **BLOCKED (402)** | — | $0.0000 | Same, before any generation. |

## 2026-07-21 — Stage P1 completion + Stage S1, sweep smoke-1 (4-model roster)

- gpt-5.5 P1 re-run: completed (def to be spot-judged; measured delta $0.00 —
  billing lag).
- **STAGE S1: 4/4 FAIL — attribution: TUXLINK-DESIGN-DEFECT (all-fail rule).**

| model | outcome | turns | spend | failure shape |
|---|---|---|---|---|
| qwen 122b | completed | 18 | $0.6206 | Dodged Branch entirely: linear def, APRS "no gateway" fires UNCONDITIONALLY every cycle. Also emitted embedded `$refs` inside a log sentence → EMBEDDED_REF_IGNORED (absorption candidate: embedded interpolation). |
| glm-5.2 | cancelled @ turn cap 40 | 40 | $0.6056 | Thrashed 7+ branch dialects (`if:`/`when:`/`expr:`/`test:`/`condition:"$ref"`/params.if), all invalid_args; def frozen at find→connect→end. |
| sonnet-5 | cancelled @ $2.21 (live-credits gate worked) | 29 | $2.2070 | Thrashed ELEVEN branch dialects (`condition:{field,op,value}`, JSONLogic `{eq:[...]}`, `when`, `if`, bare probes) + 9 docs_search/3 docs_read hunting the shape; all invalid_args; same frozen def. |
| gpt-5.5 | completed (falsely) | 22 | $1.0300 | Declared done on a branchless stub with no APRS and no logging. |

**THE FINDING (feeds bd tuxlink-6epl8, battery-driven):** no model — frontier
included — can author `Control::Branch` through the MCP verbs. The real shape
(flat `on/op/value`, bare ref path, then/else id-lists) was guessed by NOBODY;
the `invalid_args` refusal does not teach it; the docs don't reach it. The
natural emissions form a small closed dialect set:
- condition-carrier keys: `condition` | `if` | `when` | `expr` | `test`
  (top-level or nested in `params`)
- condition value shapes: bare `"$sN.key"` string (strict-boolean),
  `{field,op,value}`, op-keyed `{eq:[ref,value]}`
- refs always `$`-prefixed where the schema wants bare paths
- arms: `then`/`else` arrays (the real shape — models got THIS right)

FIX (compat-first): absorption at the sq72z coercion site normalizing all
observed carriers/shapes → `on/op/value` + `$`-strip, kind-precise transcript
markers, table-driven tests from the actual thrash transcripts; schema+refusal
honesty as suspenders. Second absorption: embedded-`$ref` string interpolation
(qwen). Then re-run S1.

**BUDGET STOP (RESOLVED same day — account re-upped $50, Fable dropped).**
Historical record of the stop: account was at 100.0 lifetime credits, 98.40 used → ~$1.60
remaining. Recorded cell deltas total $5.13; account moved $8.82 since the
first cell — discrepancy ~$3.69, most plausibly OpenRouter billing lag
posting between per-cell snapshots. All sweeps STOPPED pending operator
direction. Harness note: the provider's max_tokens=65536 drives the 402
pre-auth hold (a configurable cap would let cheap-model cells run a thin
balance: qwen hold ≈ $0.14, fable ≈ $3.28).
