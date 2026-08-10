# Kickoff: Elmer classifier architecture (research → ADR → prototype)

You are a mainline Tuxlink session picking up the five-classifier security/triage
architecture. **Primary issue: `tuxlink-efk3k` — read ALL its notes addenda first;
they are the design record (operator + Claude, 2026-08-09).** Related: `tuxlink-ch3e9`
(request→catalog matching — the first prototype), `tuxlink-r5jsj` (tool-surface survey —
feeds classifier placement), `tuxlink-zqox2` (example surface-gap class), `tuxlink-amjtz`
/ `tuxlink-obijd` / `tuxlink-9g70d` (routines context), `tuxlink-goe9p` (memory/harness),
`tuxlink-xib1x` (README framing once shipped). This file is untracked — commit it per
session conventions when you start.

## The design (summary; efk3k addenda are authoritative)
Five roles: request (human ask → catalog), trend (journals/stats), content (triage),
security/injection (quarantine boundary), capability-grant (authority boundary).
Architecture: quarantined low-capability reader for untrusted content → TYPED
schema-validated extraction across the boundary (fields that must parse; prose never
crosses) → per-datum taint provenance (actions gate on the taint of their INPUTS) →
deterministic consent gate with FINAL say → scoped, time/count-bounded, auto-expiring
capability grants adjudicated by the grant classifier. Shell: structurally absent from
content-adjacent flows, never merely gated. Doctrine: every classifier ADVISES;
deterministic policy DECIDES; a fooled classifier cannot mint authority alone.

## Three-tier backend (one verdict schema, graceful degradation, offline-first)
- **T0 — deterministic rules**: always on, every deployment. Schema validation,
  taint tracking, allowlists, bounds. No ML.
- **T1 — CPU encoder (the mandatory floor)**: 30–300M-param fine-tuned encoder
  (DistilBERT/DeBERTa-small class) and/or embeddings+kNN, runs on Pi CPU in tens of
  ms, 60–300MB RAM. Catalog matching = precomputed per-item embeddings + query
  embedding + threshold (ask only when candidates are genuinely close).
- **T2 — guard LM on the Sparks (the ceiling)**: 1–8B guard model (Llama-Guard-3-1B/8B,
  ShieldGemma-2B class), NVFP4 ≈ 1–5GB weights + trivial KV (short prompts). Served
  ONLY via a control-plane profile (operator standing rule: no out-of-band containers).
  Pin away from live bench-serving nodes or declare the jitter.

## Placement rules (non-negotiable)
1. The security/injection classifier is NEVER the same model (or family) as Elmer's
   backend — shared weights share blind spots against attacks aimed at that backend.
2. Same-model single-token judgments (cheap prefill logit) are acceptable for
   request-fit and grant-fit residuals, never for the security boundary.
3. Offline-first: every classifier role must function at T0+T1 with no network.
4. Model licenses must be AGPLv3-compatible for anything vendored; weights are data.

## Training data
tuxlink-bench (PRIVATE — textual reference only, never vendor its corpus into this
repo) generates labeled pairs: catalog-request cells, injection corpora, fixture
worlds with known truth. Coordinate with the bench operator for sanitized exports.

## Phases
1. **Research spike**: validate T1 encoder candidates on Pi CPU (latency/RAM/accuracy)
   and embedding quality on the real catalog; confirm T2 fit beside Inkling at 256K.
2. **ADR**: five roles × three tiers, the shared verdict schema, degradation rules.
3. **Prototype request→catalog first** (`tuxlink-ch3e9`): highest UX value, lowest
   risk, no security dependencies.
4. **Security boundary last**: needs the quarantined-reader subagent design landed.

Conventions: pick a moniker, track in bd, per-task branch, operator reviews the ADR
before any implementation lands.
