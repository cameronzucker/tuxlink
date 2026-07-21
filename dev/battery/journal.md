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

Harness observation (not a Tuxlink defect): `routines_set_enabled` is excluded
from the battery allowlist per adrev disposition 3, but that diverges from the
production agent surface without a safety need (enable of attended parks; of
automatic needs un-grantable acks; scratch profile has no rig). Candidate:
add it next harness iteration so enablement dialect is observable.
