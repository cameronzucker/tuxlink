# Routines round 2 build arc: compat-tree ranks 1-5 + observability O3/O4

**Status:** designed (alder-oriole-cedar, 2026-07-18); pending adversarial review.
**Parent work item:** bd `tuxlink-iizmk`.
**Requirements source:** [compat-tree spec §4](2026-07-18-routines-round2-compat-tree.md)
(ranked missing-action list, operator-directed build order ranks 1-5) +
[observability wire-walk](2026-07-18-routines-observability-wirewalk.md) O3/O4.
**Governing decisions:** [ADR 0024](../../adr/0024-dual-actionability-one-capability-tree.md)
(dual actionability, one authority model), [routines design spec](2026-07-13-routines-design.md)
§4 (consent model), §10 (one validator, no privileged path), ADR 0018 (built whole, no deferrals).

## 0. Design rules applied throughout

1. **Mirror the MCP DTO exactly.** Every new read surface reuses the agent
   tool's curation, redaction, and output shape verbatim (ADR 0024 P4: one
   implementation, two registrations). Field names, casing, and null semantics
   are the MCP tool's. Where the routines seam cannot literally call the MCP
   port (layering), it calls the same underlying command the port wraps.
2. **Flags, never names.** All new consent/capability behavior derives from
   `ActionDescriptor` flags, matching the existing invariant (capability.rs
   checks read flags; the one WWV name-special-case stays the only one).
3. **Untyped params stay untyped.** No param-schema machinery is introduced;
   params are validated at execute time via serde, matching every existing
   action. The palette needs zero changes (registry-driven).
4. **Journal events are additive.** New `RunEvent` variants use the
   established additive-serde pattern (`#[serde(default, skip_serializing_if)]`
   on optional fields, no `deny_unknown_fields`), so old journals replay and
   old readers tolerate new events.

## 1. Rank 1: status read sources (unblocks 14 cells)

Three new `data.read` sources (new `ReadSource` variants + `DataService` seam
methods + monolith impls). No new action.

| Source | Mirrors MCP tool | Output (verbatim MCP DTO shape) |
|---|---|---|
| `modem_status` | `modem_get_status` | `{"kind","connected","state","running":[{"kind","state"}],"selected":{"session_type","protocol"}\|null,"conflict"}` |
| `backend_status` | `backend_status` | `{"connected","transport","state"}` with the same curated state strings and the same `redact_freeform` on the `error:` arm |
| `app_status` | `server_info` | `{"name","version","armed","armed_remaining_secs","tainted","taint_reason"}` |

- Implementation path: the monolith `DataService` impl calls the same
  underlying gatherers the MCP port impls call (`gather_modem_status`/
  `derive_modem_status`, `derive_status_dto` + curation, and the guard's
  `armed_remaining`/`is_tainted`/`taint_reason` + app name/version). To avoid
  drift, the DTO structs are shared: the routines seam returns the mcp-core
  DTO types (`ModemStatusDto`, `BackendStatusDto`) where crate layering
  allows, else an identically-serialized struct with a serialization-equality
  test pinning the two shapes together.
- `app_status` exposes egress-authority state to routine authors deliberately:
  a pre-flight branch "is the agent armed / is the session tainted" is a
  legitimate guided-routine step, and the values are content-free tokens.
- All three are inert reads: `needs_radio:false, transmits:false,
  needs_internet:false`. They read live in-process state, never the network.
- None of these can be empty in a way needing an honest-gap error: all three
  gatherers always produce a value (`modem_status.kind:"idle"` when nothing
  runs; `backend_status.state:"not_configured"`; `server_info` always).

## 2. Rank 2: `data.find_stations` action (unblocks 10 cells)

New action (not a read source: it takes real params).

- **Descriptor:** name `data.find_stations`, label "Find gateway stations",
  `needs_radio:false, transmits:false, needs_internet:true`.
  `needs_internet` is honest: the shared `StationsCache` path may refresh over
  the network on TTL expiry (stale-on-error keeps it degrading gracefully
  off-grid, and the capability validator's `NEEDS_INTERNET_OFFGRID` warning
  surfaces the dependency at design time).
- **Params** (serde, all optional):
  `{"modes":["vara-hf"|"packet"|"ardop-hf"|"pactor"|"robust-packet"],
    "bands":["40m",...], "history_hours":u32<=720, "limit":usize}`.
  `modes`/`bands`/`history_hours` are byte-identical to the MCP
  `find_stations` params. `limit` is routines-only shape (ADR 0024 P4 allows
  shape divergence): it truncates the *sorted* gateway list so a routine can
  feed "nearest N" into `radio.connect` without dialing the whole directory.
  Absent = no truncation. `limit:0` is rejected as invalid params (a routine
  asking for zero stations is authored wrong, and an empty `stations` list
  would fail `radio.connect` anyway).
- **Output:** the MCP `StationListDto` shape verbatim, plus one routines-only
  convenience field:
  ```json
  {"gateways":[{...GatewayDto verbatim...}],
   "fetched_at_ms":<i64|null>, "operator_grid":"FN31"|null,
   "callsigns":["W1ABC","K7XYZ",...]}
  ```
  `callsigns` is the distance-sorted, post-filter, post-limit callsign list,
  exactly the array `radio.connect`'s `stations` param consumes. This is the
  R3 "gateway-continuity" composability path made first-class:
  `stations: "$s1.callsigns"` wires rank 2 into the existing connect walk.
  Deduplicated preserving order (a gateway listed on multiple channels
  appears once; `radio.connect` resolves per-band frequencies itself).
- **Empty result is not an error.** Zero matching gateways returns
  `{"gateways":[],"callsigns":[],...}`; the authoring pattern for "no
  gateways" is a branch on `$sN.callsigns` (or downstream `radio.connect`
  failing verbatim). An honest-gap error would make "search, then decide" un-
  authorable.
- Same distance sort, same 4-char own-grid resolution, same PII curation as
  the MCP tool (shared implementation path through
  `catalog_fetch_stations` + `curate_gateway` + `sort_gateways_by_distance`).

## 3. Rank 3: config read sources (unblocks 10 cells)

Five new `data.read` sources. **Deviation from the compat-tree "proposed
shape" recorded:** the spec sketched `config` + parameterized `modem_config`;
this design uses explicit per-modem sources instead (`data.read` params stay
`{source}` only, no second param axis, each source maps 1:1 onto one MCP tool
and one DTO). Same capability set, simpler contract.

| Source | Mirrors | Output |
|---|---|---|
| `config` | `config_read` | The curated 5-field `{"connect_to_cms","transport","host","callsign","grid"}` with the same forced 4-char grid redaction |
| `ardop_config` | `config_get_ardop` | `{"host","port","drive_level","bandwidth"}` |
| `vara_config` | `config_get_vara` | `{"host","port","bandwidth","drive_level":0}` (same "VARA owns TX level" semantics) |
| `packet_config` | `packet_config_get` | `{"kiss_host","kiss_port","baud","tx_delay"}` (same KISS-only minimization) |
| `rig_config` | `config_get_rig` | The 9-field rig DTO verbatim |

All inert reads, no flags. The curation invariant is pinned by test: the
routines-side `config` output must serialize byte-identically to the MCP
`config_read` output for the same underlying config (ADR 0024 P3: divergence
here is a security defect, and the test makes it a compile-time/CI fact).

## 4. Rank 4: `data.docs_search` action (unblocks 6 cells)

- **Descriptor:** name `data.docs_search`, label "Search app docs",
  all flags false (local FTS5 index, no network).
- **Params:** `{"query":<non-empty string>}`; empty/missing query is invalid
  params at execute time.
- **Output:** `{"hits":[{"title","slug","snippet"},...]}` , the MCP
  `Vec<DocsHitDto>` wrapped in an object (routine `$`-paths need a named
  field; a bare array output would be unaddressable). Same FTS5
  raw-then-OR-fallback query strategy, same `<mark>` snippet format, same
  30-cap, via the same `SearchService.docs` seam. Zero hits returns
  `{"hits":[]}` (not an error), same rationale as rank 2.
- Journal note: snippets carry `<mark>` markup; the History step list renders
  outputs as JSON text (existing `<details>` pretty-print), so no XSS surface
  is added (no dangerouslySetInnerHTML anywhere in RunsTab).

## 5. Rank 5: first config-write action family (unblocks the last 3 cells)

### 5.1 The action

- **Name:** `config.set_ardop` (new `config.*` namespace: the write family
  grows here; `rig.apply_preset` stays CAT-side). Label "Set ARDOP config".
- **Params:** `{"drive_level":u8}`. Validation before effect, mirroring the
  MCP write path exactly: `>100` fails with invalid params before anything is
  read or written. This is the compat-tree's named smallest honest slice (the
  corpus step is ARDOP drive level); siblings (`config.set_vara` bandwidth
  etc.) follow the same plumbing later as additive registry entries.
- **Effect:** the same read-modify-write the MCP port performs
  (`config_get_ardop` -> set `drive_level` -> `config_set_ardop`, atomic
  whole-config write).
- **Output (this is the journaled old->new requirement, satisfied
  structurally):**
  ```json
  {"field":"drive_level","old":80,"new":95}
  ```
  `old` is the pre-mutation value (`null` when previously unset). Because
  every action output lands verbatim in `step_ok`, the run journal carries
  the old->new diff with zero new journal machinery, and History's existing
  params/output disclosure renders it.

### 5.2 The consent model for writes (the load-bearing design)

Config writes are a consent-relevant authority class distinct from RF
transmit. Reusing `transmits:true` would lie (TX badge, Part 97 ack language,
transmit closure semantics) on a step that never keys the radio. Design:

- **New descriptor flag `writes_config: bool`** (default false), the seventh
  `ActionDescriptor` field. UI palette badge "WRITES" via the existing
  `flagsFor` pattern.
- **Attended mode:** the executor parks a `writes_config` step exactly as it
  parks a transmit step (`ctx.attended && (d.transmits || d.writes_config)`),
  same `ConsentPort`, same `AwaitingConsent` state, same UI consent surface.
  Per-invocation operator click = consent parity with the agent side's
  operator-armed window.
- **Automatic mode:** a routine whose call-graph closure contains a
  `writes_config` step requires a **`write_ack`** recorded in the definition,
  a sibling of `transmit_ack` with identical protection: `routines_save` /
  `validate_draft` discard any body-supplied `write_ack` and re-read the
  on-disk value (so MCP cannot supply it); leaving automatic mode revokes it;
  it is recorded only by a dedicated UI act. Acknowledgment copy is
  write-specific plain words ("this routine changes station configuration
  unattended"), not Part 97 language.
- **Validator closure** (mirrors the transmit walk, one validator, no
  privileged path):
  - `AUTO_WRITE_UNACKED` (Error): automatic + non-empty write closure +
    missing/empty `write_ack`. Blocks enable and run, never save.
  - `MIXED_MODE_STALL` extends its predicate: an automatic routine calling an
    attended routine whose closure transmits **or writes** gets the same
    "will stall on a click nobody gives" warning.
  - `ATTENDED_WRITE_UNDER_SCHEDULE` (Warning): scheduled attended routine
    with a write closure, sibling of `ATTENDED_UNDER_SCHEDULE`.
- **Start gate:** the monolith start gate extends `closure_transmits` /
  `transmit_action_names` with the write analog; an unacked automatic
  write-routine is not startable from any invoker (UI, schedule, MCP).
- **Why this closes the ADR 0024 P3 hole:** without it, an unarmed or tainted
  agent could bypass `guarded_egress` by saving a routine containing the
  write step and calling `routines_run`. With it, the write fires only inside
  a routine the operator either acked in the UI (automatic) or is present to
  click through (attended). The consent envelope doctrine (§4: after
  acknowledgment all invokers are equivalent) is preserved verbatim, applied
  to a second authority class.
- **Dry-run:** unchanged machinery; the fake mirrors `writes_config`, dry-runs
  never park and never write (fake world).

## 6. O3: child run ids in the parent journal

- **Invoker split.** The `RoutineInvoker` port gains a two-phase shape:
  `start(routine, args, provenance) -> RunHandle` (run_id known immediately,
  from the engine's existing `start_run_ext`) and awaiting the outcome via
  the handle. The existing single-shot `invoke` is reimplemented on top.
- **New event:** `RunEvent::CallChild { step: StepId, child_run_id: String }`
  journaled by the parent the moment the child run id exists, in **all three
  paths**: sync-success, sync-failure (today the id is buried in an error
  string; the split makes it structural), and fire-and-forget (today the id
  is discarded). Ordering: `step_intent` -> `call_child` -> (`step_ok` |
  `step_err`).
- A call that fails **before** a child run exists (unknown routine, depth cap)
  journals no `call_child`, and that absence is the honest record.
- **History UI:** new `'call'` row kind: renders "call:<routine>" with the
  short child run id as a clickable element that navigates the journal view
  to the child run (`setSelectedRunId(childRunId)`). Works for children of
  other routines too: the journal fetch is by run id, and the runs rail
  simply shows no highlighted row when the selected run belongs to another
  routine (verified behavior, not assumed; if the rail breaks on a foreign
  id, the fix is scoped to selection tolerance, not new navigation).

## 7. O4: end-step events + the dropped-reason fix

- **New event:** `RunEvent::EndReached { step: StepId, failed: bool,
  reason: Option<String> }` journaled at the executor's End site (the step id
  is in scope there today and currently discarded). Ordering is naturally
  `end_reached` -> skip sweep -> `run_finished` (the sweep runs after the
  track returns), which is the correct narrative order.
- **Latent defect fixed as part of O4:** `TrackEnd::Ended`'s `failed`/`reason`
  are currently dropped by `run_tracks` and never reach `run_finished`. The
  End step's reason is threaded through (`TrackEnd::Ended` gains the step id;
  `run_tracks` carries it into the outcome) so `run_finished.reason` on an
  End-terminated run names the End step's declared reason verbatim instead
  of losing it.
- **History UI:** the End event renders as its own step-list row ("ended at
  s7: complete|failed, <reason>") ahead of the existing finished row.

## 8. Frontend deltas (complete list)

1. `routinesApi.ts`: extend the TS `RunEvent` union with `call_child` and
   `end_reached` (snake_case fields, no recasing layer, matching #1159's
   variants).
2. `RunsTab.tsx`: two new `stepListModel` cases + `StepListRow.kind` union
   members (`'call'`, `'end'`) + `ROW_ICON` entries + two JSX row branches +
   the child-run navigation click handler.
3. `PaletteRail.tsx` / `canvasModel.ts` `flagsFor`/category mapping: surface
   the `writes_config` badge (category stays LOCAL-group for `config.*` since
   `needs_radio`/`needs_internet` are false; badge disambiguates).
4. Designer settings grid: the `write_ack` acknowledgment row, rendered when
   the routine's write closure is non-empty and mode is automatic, mirroring
   the transmit-ack row's UI act (records callsign + timestamp via a
   dedicated UI-only command).
5. `ActionInfo` TS type + Rust `ActionInfo` DTO gain `writes_config`.

## 9. Testing strategy

- **Engine (tuxlink-routines, R2 cargo):** unit tests per new event variant
  (serde round-trip + additive-tolerance), executor tests for CallChild in
  all three paths, EndReached ordering vs skip sweep, reason threading,
  attended-park on `writes_config`, `AUTO_WRITE_UNACKED` /
  `ATTENDED_WRITE_UNDER_SCHEDULE` / extended `MIXED_MODE_STALL` validator
  tests, dry-run fake flag mirroring.
- **Monolith (R2 cargo):** seam impl tests with fakes for each new
  source/action; the config-curation serialization-equality pin
  (routines `config` == MCP `config_read` byte-identical); write action
  old->new output; ack-stripping on save/validate_draft for `write_ack`
  (mirroring the existing transmit_ack tests).
- **Composability proof:** extend `composability_proof.rs` with the rank-2
  wire: `data.find_stations` (faked directory) -> `$s1.callsigns` ->
  `radio.connect` `stations`, asserting the resolved param is the sorted
  callsign array.
- **Frontend (Pi vitest):** stepListModel cases for `call_child`/`end_reached`
  (row kinds, child-run click), palette badge, ack-row gating. Full
  `pnpm vitest run src/routines` + typecheck.
- **Acceptance (post-merge, live app):** converge rebuild, then a fresh
  wire-walk: a probe exercising branch + skip + call (child) + end +
  a config write in attended mode, captured via `routines_journal_get` and
  committed as a second harness fixture (`&real=2`, additive; `&real=1`
  keeps the 2026-07-18 capture as regression). The compat-tree coverage
  number is re-derived: ranks 1-4 => 21/24 cells, rank 5 => 24/24.

## 10. Explicit non-goals (this arc)

- Ranks 6-11 (mailbox reads, staging parity, predict_path, real rig_tune,
  remaining tier-2 writes, VARA setup): follow-on arcs per the compat-tree
  ranking. The rank-5 plumbing (flag, ack, validator closure) is what makes
  them additive registry entries.
- Typed param forms / output-field autocomplete in the designer: net-new
  machinery the palette deliberately does not have; not introduced here.
- `heard_stations` backend record: stays an honest gap.
- Agent-side tool additions for the reverse diff (ADR 0024 §6): separate
  tool-surface revision.
