# Battery journal — stage-gated ladder (bd tuxlink-hwgdi)

Tracked record of every sweep: judged results, attribution, fixes, spend.
Bundles themselves are gitignored (`battery-results/`, full bundles live on
R2 at `~/tuxlink-battery-build/battery-results/`); THIS file is the durable
cross-session record. Newest entries first.

Ladder: P2 → P1 → S1 → S2 → S4 → S3 → P3 (advance only when the stage is
fully addressed). Models: qwen/qwen3.5-122b-a10b, z-ai/glm-5.2,
anthropic/claude-sonnet-5, openai/gpt-5.5, anthropic/claude-fable-5.
Budget: $50 hard cap (ledger at battery-results/ledger.json on R2;
harness refuses ≥ $45).

Attribution vocabulary (bd tuxlink-6zkb6): tuxlink-design-defect |
model-family-trend | ambiguous. Compat is the belt, prose the suspenders.

---

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
