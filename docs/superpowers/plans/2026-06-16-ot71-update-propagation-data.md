# Plan — tuxlink-ot71: "Update propagation data" (internet primary + RF-catalog fallback)

**Issue:** tuxlink-ot71 · **Agent:** opossum-marsh-oriole · **Date:** 2026-06-16
**Operator source decision (2026-06-16):** NOAA SWPC, **model-correct** posture (Option A).

## Grounded design (endpoints verified live 2026-06-16)

### Internet primary (operator-approved)
- **SSN forecast (the VOACAP input):** `https://services.swpc.noaa.gov/json/solar-cycle/predicted-solar-cycle.json`
  — array of `{"time-tag":"YYYY-MM","predicted_ssn":F,"predicted_f10.7":F, ...}`. The
  `predicted_ssn` is the **smoothed** SSN VOACAP wants. Maps **directly** onto the
  existing `SsnForecast.monthly: BTreeMap<"YYYY-MM", f64>` (`time-tag` → key,
  `predicted_ssn` → value). Verified: `2026-01 → 107.8`.
- **Live conditions (context only, NOT the SSN model):** `https://services.swpc.noaa.gov/text/wwv.txt`
  — `"Solar flux 117 ... estimated planetary A-index 6. ... K-index ... was 1.33."`
  → SFI / A / K for the conditions bar.

### RF fallback (mandatory, robust — the defining offline scenario)
Reuse the shipped catalog inquiry sender (`catalog::composer::compose_inquiry_message`,
To: INQUIRY@winlink.org, Subject: REQUEST, body = filenames). Request **PROP_WWV**
(`Daily WWV Solar Flux, A & K Index summary`, 621 B — its reply format == the verified
`wwv.txt`) as the primary RF item; PROP_SGAS (`...used for SSN`, 1538 B) is a richer
secondary if its format is later grounded.

**Over radio only daily SFI is available** (no smoothed-SSN product crosses the air), so
the RF path **derives** SSN from SFI (WLE `GetSSN`-style) — necessarily less precise than
the internet smoothed product, but fully offline. This is the documented consequence of the
model-correct decision: smoothed SSN online, derived SSN over radio.

### Last resort (unchanged)
Bundled `resources/propagation/ssn-forecast.json` via `include_str!` — never blocks.

## Layered build (one branch `bd-tuxlink-ot71/propagation-update`; PR draft until the full vertical wires)

**Layer 1 — runtime-mutable forecast (PREREQUISITE) + pure parsers.** [this session]
- `ssn.rs`: `forecast_path(config_dir)`; `SsnForecast::load_writable_then_bundled(config_dir)`;
  `persist(config_dir)`. Managed `ReadyPropagation.forecast` becomes `Arc<RwLock<SsnForecast>>`
  so an update applies WITHOUT app restart; `run_prediction` reads a cheap clone.
- `solar.rs` (new): pure parsers, exhaustively unit-tested against the REAL fetched formats:
  - `parse_swpc_predicted_ssn(&str) -> Result<SsnForecast>` (the JSON array).
  - `parse_wwv(&str) -> Option<SolarIndices{ sfi, a_index, k_index }>` (tolerant of the
    verified prose form).
  - `derive_ssn_from_sfi(sfi) -> f64` (documented WLE-style mapping).
- All CI-tested (Rust runs on CI only here).

**Layer 2 — update orchestration + freshness/gating.**
- `update_propagation(force)`: internet primary (predicted-ssn JSON → forecast table; wwv.txt →
  conditions) → on failure, RF fallback (catalog PROP_WWV request) → bundled last-resort.
  Freshness stamp + cadence gate (auto only when stale, ≥ a cadence; manual button = force).
- RF reply ingestion: extend `reply.rs` `parse_reply` to recognize the WWV/SGAS subject/body →
  a `ReplyView::Solar{...}` → persist derived SSN + stamp.

**Layer 3 — Tauri command surface + catalog request wiring.**
- `propagation_update` command (force flag) returning a freshness/result summary.
- Wire the PROP_WWV catalog request through the existing send path.

**Layer 4 — UI (the user-reachable surface).**
- "Update propagation data" button in `StationFinderControls` (the reserved slot beside
  "Update station list"). Live SFI/K in the conditions bar (the existing `sfi`/`kIndex` props).
- Freshness caption ("solar data N old" already exists; wire it to the new stamp).

**Layer 5 — Codex adrev (parsers + orchestration; no-carveout discipline) + wire-walk gate.**

## Wire-walk definition-of-done (operator supplies the real flows at done-time)
Candidate motivating flows: (a) online operator clicks "Update propagation data" → forecast
table refreshes from SWPC, conditions bar shows live SFI/K, map re-colors; (b) **cold-booted
fully-offline station** clicks update → app sends a PROP_WWV catalog request over radio →
on reply, SSN is derived + persisted + applied. Flow (b) is the mandatory scenario.

## Notes
- Rust is CI-only on this Pi (no local cold cargo) — write + test + push to CI.
- MSRV 1.75 — avoid 1.76+ APIs (no `Result::inspect_err`).
- RADIO-1: the catalog request is agent-authorable; operator runs the on-air send.
