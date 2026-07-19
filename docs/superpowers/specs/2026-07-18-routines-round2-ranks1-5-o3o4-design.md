# Routines round 2 build arc: compat-tree ranks 1-5 + observability O3/O4

**Status:** designed (alder-oriole-cedar, 2026-07-18); pending adversarial review.
**Parent work item:** bd `tuxlink-iizmk`.
**Requirements source:** [compat-tree spec §4](2026-07-18-routines-round2-compat-tree.md)
(ranked missing-action list, operator-directed build order ranks 1-5) +
[observability wire-walk](2026-07-18-routines-observability-wirewalk.md) O3/O4.
**Governing decisions:** [ADR 0024](../../adr/0024-dual-actionability-one-capability-tree.md)
(dual actionability, one authority model), [routines design spec](2026-07-13-routines-design.md)
§4 (consent model), §10 (one validator, no privileged path),
[ADR 0022](../../adr/0022-ban-autonomous-agent-issue-splitting-and-deferrals.md)
(completeness invariant; renumbered from a colliding 0018).

## 0. Design rules applied throughout

1. **Mirror the MCP DTO exactly, modulo recorded divergences.** Every new
   read surface reuses the agent tool's curation, redaction, and output
   shape verbatim (ADR 0024 P4: one implementation, two registrations),
   except where a divergence is explicitly recorded in this spec as ADR
   0024 P4 shape divergence (rank 2's `limit` + `callsigns`, rank 4's
   `{"hits":[...]}` wrapper). Field names, casing, and null semantics are
   the MCP tool's. Where the routines seam cannot literally call the MCP
   port (layering), it calls the same underlying command the port wraps.
2. **Flags, never names.** All new consent/capability behavior derives from
   `ActionDescriptor` flags, matching the existing invariant (capability.rs
   checks read flags; the one WWV name-special-case stays the only one).
3. **Untyped params stay untyped, with two narrow registry-driven authoring
   affordances (R4 P2-3).** No param-schema machinery is introduced; params
   are validated at execute time via serde, matching every existing action.
   Two deliberate, narrow deviations ship because thirteen `data.read`
   sources authored blind into an empty key/value grid is a
   memorize-the-schema UX: (a) an optional `example_params` string on the
   descriptor, seeded into the params grid at palette-insert time (the
   operator edits a populated, shape-correct grid; `data.read` inserts as
   `{"source":"modem_status"}`, `data.find_stations` as
   `{"modes":["vara-hf"],"limit":3}`); (b) an optional `allowed_values`
   descriptor hint consumed by a contracts-pass lexical check,
   `UNKNOWN_READ_SOURCE` (Error), for a LITERAL (non-`$`) `source` string
   outside the known set — a vocabulary lint in the valbar's existing
   every-edit loop, not a schema. StepInspector also renders the action's
   `description` line (today it renders none of it, and the palette shows
   it only as a hover tooltip).
4. **Journal events are additive, and the reader becomes variant-tolerant
   (R2 P2-1).** New `RunEvent` variants use the established additive-serde
   pattern on optional fields, so old journals replay in new readers. The
   REVERSE is false today: an internally-tagged enum errors on an unknown
   `type` tag, `read_journal` fails the whole file on the first bad line,
   `list_runs` then hides the run and `scan_interrupted` mis-classifies it
   (appending a spurious second `run_finished` at recovery). The converge
   build runs origin/main, so a branch build writing `call_child` /
   `end_reached` would corrupt History for the operator's next converged
   run. Fix shipped WITH this arc, before the new variants: `read_journal`
   decodes per-line tolerantly (a line whose event fails to parse becomes an
   opaque `unknown` entry preserving `ts_unix`/`seq`; terminal-state scans
   treat it as a non-terminal entry), closing the class for all future
   variants.

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
  drift, the DTO structs are shared: the monolith already depends on
  `tuxlink-mcp-core` and imports its ports DTOs (`mcp_ports.rs:38`), so the
  routines seam returns the mcp-core DTO types (`ModemStatusDto`,
  `BackendStatusDto`) directly. Curation-equality pin tests still apply
  (the shared struct pins shape; the pins guard the curation logic around
  it).
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
  shape divergence) and is defined over **distinct callsigns** (R2 P2-5):
  dedup the distance-sorted callsign sequence first, truncate THAT to
  `limit`, and `gateways[]` contains every gateway row whose callsign
  survived (so per-band frequency rows for a kept station are never lost;
  "nearest 3 stations" means 3 dial targets, not 3 directory rows). Absent =
  no truncation. `limit:0` is rejected as invalid params. When the operator
  grid is unresolved, distances are null and the stable sort preserves
  directory order: `limit` then truncates in directory order, and the
  callsigns remain valid dial targets (`operator_grid: null` in the output
  is the honest marker).
- **Output:** the MCP `StationListDto` shape verbatim, plus one routines-only
  convenience field:
  ```json
  {"gateways":[{...GatewayDto verbatim...}],
   "fetched_at_ms":<u64|null>, "operator_grid":"FN31"|null,
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
- **Effect (Codex R1 P2):** NOT the MCP port's current get-then-set pair
  (a documented lost-update path: `write_config_atomic` only makes the file
  replacement atomic, not the read-modify-write). The routine action performs
  the whole mutation inside the config writer lock via `config::update_config`
  (or a serialized ARDOP setter), computing `old` and `new` inside the same
  critical section it writes in. The MCP port's own get-then-set is
  upgraded to the same locked path as part of this arc (one implementation,
  two registrations; leaving the agent path racy while fixing the routine
  path would be exactly the divergence ADR 0024 P3 bans).
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
  operator-armed window. Two refinements (R2 P3-2, P3-3): the
  `AwaitingConsent` event gains a `parkKind: "transmit" | "write"` (named
  to avoid the app-event union's `kind` discriminant; R5) so the park
  dialog says "write station config" in write terms, never transmit
  language (the same honesty the ack layer insists on); and a
  `writes_config` step under a Retry wrapper parks per attempt, which is
  intended (each write attempt is separately consented, matching
  per-transmission consent).
- **Automatic mode:** a routine whose call-graph closure contains a
  `writes_config` step requires a **`write_ack`** recorded in the definition,
  a sibling of `transmit_ack` with identical protection: `routines_save` /
  `validate_draft` discard any body-supplied `write_ack` and re-read the
  on-disk value (so MCP cannot supply it); leaving automatic mode revokes it;
  it is recorded only by a dedicated UI act. Acknowledgment copy is
  write-specific plain words ("this routine changes station configuration
  unattended"), not Part 97 language.
- **Acks bind to the acknowledged closure (Codex R1 P1).** An ack that merely
  exists is replayable: an unarmed agent could edit the acked routine (or a
  library callee resolved live through the store) to add or change a write
  step, and the preserved on-disk ack would keep authorizing a closure the
  operator never saw. Both `write_ack` and the existing `transmit_ack`
  therefore carry a **`closure_digest`**: a canonical hash over the routine's
  transitive consent-relevant closure, computed from sorted
  `(routine_name, step_id, action_name, params_json)` tuples for every write
  step, respectively every transmit step, across the resolved call graph,
  **plus every Call step's `(routine_name, step_id, callee_name, args_json)`
  edge on paths reaching a consent-relevant step** (R2 P2-3: otherwise
  editing a Call's args changes what a callee writes while the digest still
  matches). Canonicalization is explicit (R2 P3-4): recursive JSON key sort
  before hashing, never relying on serde_json's default map ordering (a
  future `preserve_order` feature-unification flip must not invalidate
  acks); tuples sort by `(routine_name, step_id)`. The digest function lives
  in `tuxlink-routines` beside ONE parameterized closure walk (predicate
  over descriptor flags) that the start gate, the validator, and the digest
  all consume (R2 P3-5: the transmit walk already exists twice; a third
  copy would drift). The digest is computed and recorded by the UI ack
  command after validation; it is recomputed at save, enable, and start. A
  mismatch invalidates the ack (the routine drops back to
  `AUTO_WRITE_UNACKED` / `AUTO_TX_UNACKED`), including the callee-edit case
  where the acked routine's own file never changed. Existing `transmit_ack`
  values without a digest are treated as stale and require
  re-acknowledgment (serde-default migration; honest at current alpha
  scale, and the alternative silently grandfathers unverifiable acks).
- **What the digest does and does not pin (R2 P2-3, stated honestly).** The
  digest pins the closure's *shape*: which steps write, with which authored
  params, reached through which calls with which authored args. It does NOT
  pin run-time values: a write param of `"$args.level"` digests as that
  literal, and the invoker (human, schedule, or agent via `routines_run`)
  chooses the value, bounded only by the action's own validation. The
  consent-envelope doctrine carries this deliberately (acking a
  parameterized write is acking run-time-chosen values), and it is surfaced,
  not silent: a new validator Warning `WRITE_VALUE_RUNTIME` flags
  `$`-referenced params on `writes_config` steps inside automatic routines,
  and the ack UI renders it beside the acknowledgment ("this routine's
  write value is chosen at run time").
- **Mid-run callee-edit window (R2 P2-2).** Call targets resolve by name at
  invoke time (the run snapshot never inlines callees, a shipped divergence
  from the design spec's §7 snapshot doctrine that predates this arc). The
  save/enable/start digest checks therefore do not cover a callee edited
  while the parent is already running (parked, delayed, retrying). This arc
  closes the consent-relevant half surgically: at every child-start the
  invoker re-verifies the **run root's** ack digests against the live store
  and fails the Call step verbatim on mismatch ("callee changed after
  acknowledgment"). The anchor is the run root, NOT the immediate parent
  (R3 P1-1): at call depth >= 2 the immediate parent is a library callee
  with no ack of its own, so "verify the parent's ack" either no-ops (the
  replay attack one level deeper) or fails every legitimate deep call. The
  root's digests (BOTH classes, transmit and write) thread through
  `ExecCtx` / the invoker chain from the start gate. An attended root
  threads no digests and child-start verification is a no-op, which is
  correct: attended runs consent per step, not per envelope. The general
  callee-pinning gap (non-consent edits also swap mid-run, contra §7
  doctrine) is pre-existing shipped behavior, recorded as a bd follow-up
  rather than smuggled into this arc.
- **Validate-and-start are one read (Codex R1 P2).** The current run path
  validates one loaded definition and then reloads by name before
  snapshotting, so a concurrent `routines_save` could swap the definition
  between the gate and the snapshot. The start gate (including both ack
  digest checks) moves onto the same read that produces the executed
  snapshot, under the session's store access, eliminating the TOCTOU.
- **Validator closure** (mirrors the transmit walk, one validator, no
  privileged path):
  - `AUTO_WRITE_UNACKED` (Error): automatic + non-empty write closure +
    a `write_ack` that is missing, empty, **or digest-mismatched** (R3
    P2-1: without the third clause the validator and the start gate
    diverge on a present-but-stale ack, the exact "one validator, no
    privileged path" violation §10 bans). Blocks enable and run, never
    save. The existing `AUTO_TX_UNACKED` predicate gains the same third
    clause when `transmit_ack` gains its digest.
  - `MIXED_MODE_STALL_WRITE` (Warning): an automatic routine calling an
    attended routine whose closure writes gets the same "will stall on a
    click nobody gives" semantics as the existing transmit code, as a
    DISTINCT write-worded code (R4 P2-1; see the UI-honesty bullet below
    for why the code is not shared).
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
- **Dry-run gets shape-true fakes (R4 P1-1** — without this, every routine
  this arc unblocks is un-dry-runnable in composition): the optimistic fake
  returns `{"dry_run":true}` with no fields, and resolving an unset field is
  a hard `UnsetVariable` error, so `stations: "$s1.callsigns"` or a branch
  on `$s1.grid` fails a dry-run of a CORRECT routine with what reads as an
  authoring mistake. The monolith merges shape-true canned outputs for its
  own registry's actions before the optimistic default (registry-driven:
  keyed by the same action names, e.g. `data.find_stations` ->
  `{"gateways":[],"callsigns":["DRYRUN-1"],"fetched_at_ms":null,
  "operator_grid":null,"dry_run":true}`; each read source returns its DTO
  shape with obviously-fake values; `config.set_ardop` ->
  `{"field":"drive_level","old":0,"new":0,"dry_run":true}`). Dry-runs still
  touch NOTHING real, never park, never write; the fake shapes are pinned
  by test.
- **Consent findings stay class-honest in the UI (R4 P2-1):** the write
  analog of the mixed-mode warning is a DISTINCT code
  `MIXED_MODE_STALL_WRITE` (write-worded message), not an extended
  predicate on the transmit code: the settings surface gates its transmit
  section on transmit-class codes, and reusing the code would render Part
  97 transmit copy on a write-only routine. SettingsTab section visibility
  is CLOSURE-based, not direct-step-scan-based (R5 P2: a call-only closure
  with a VALID ack produces no finding and has no direct step, and a
  direct-step scan would hide the valid signature row): both the transmit
  section and the write ack row key on the `routines_consent_closure(name)`
  enumeration (transmit steps non-empty -> transmit section; write steps
  non-empty -> write ack row), with class-specific findings layered on top.
  A frontend test pins "automatic root that only CALLS a write callee, ack
  valid: the ack row still renders."
- **The ack is a visible signature, and validity is what renders (R4 P1-2,
  P1-3):** the settings ack panels branch on VALIDITY (ack present AND no
  `AUTO_TX_UNACKED` / `AUTO_WRITE_UNACKED` finding), never on mere
  presence; a present-but-invalid ack renders a third explicit state
  ("Acknowledgment no longer valid: the routine, or a routine it calls,
  changed after <by> acknowledged on <at>. Re-acknowledge to run
  automatically.") — the only place the callee-edit invalidation becomes
  explicable, and the state every pre-digest ack lands in on upgrade day.
  And the operator sees WHAT they sign: the shared closure walk is exposed
  as a UI-only command (`routines_consent_closure(name)` returning the
  same transmit/write step tuples the digest hashes), and both ack rows
  plus the acknowledged panels enumerate the covered steps verbatim
  ("This acknowledgment covers: checklist-b · s3 · config.set_ardop ·
  {...}"), with `WRITE_VALUE_RUNTIME` attached inline to runtime-valued
  rows. Attended parks already show resolved params; the higher-authority
  automatic grant must not show less.

## 6. O3: child run ids in the parent journal

- **Invoker split.** The `RoutineInvoker` port gains a two-phase shape:
  `start(routine, args, provenance, parent_cancel) -> RunHandle` (run_id
  known immediately, from the engine's existing `start_run_ext`) and
  awaiting the outcome via the handle. The existing single-shot `invoke` is
  reimplemented on top. Two contract obligations ride the split:
  - **Inline start in BOTH arms (R2 P1-1).** The fire-and-forget arm awaits
    `start()` inline in the Call step (child run id known, `call_child`
    journaled, then `step_ok {"dispatched": true}`); only the *outcome* is
    unawaited (handle dropped; the engine tolerates a dropped receiver). A
    detached-task start could otherwise journal `call_child` after the
    parent's `run_finished`, and a journal not ending in `run_finished` is
    classified interrupted at next launch (recovery would stamp a
    truthfully-completed run as interrupted). Behavior change made honest:
    a fire-and-forget start failure (unknown routine, unacked callee) is now
    observable and journals `step_err` instead of today's silent
    `dispatched: true` lie.
  - **Cancellation propagates, and every child is cancellable (R2 P1-2 +
    R3 P2-5).** Today a child run's cancel token is created fresh and never
    linked to the parent: cancelling a parent lets a child (including its
    transmit steps) run to completion, and child runs are absent from the
    session registry so the UI cannot cancel them directly either. O3 makes
    children operator-visible, so this ships with it: the child's token
    derives from the parent's (`ctx.cancel.child_token()`, threaded through
    `start_run_ext`), and the sync await races `ctx.cancel` (on cancel:
    forward to the child handle, journal `step_err` Cancelled).
    Fire-and-forget children stay detached from parent cancellation
    DELIBERATELY (dispatch means dispatch), but detachment must not mean
    uncancellable: **all engine-invoked child runs register in the session
    registry**, so `cancel_run(child_id)` works from the UI for sync and
    F&F children alike. A transmit-capable F&F child with no cancel
    affordance anywhere would fail the working-abort correctness bar.
    **The registry bridge is pinned (R5 P2):** child starts currently
    happen inside the engine's `EngineChildInvoker`, but registry insertion
    and the run watcher live only in the monolith session — so the monolith
    owns the real `RoutineInvoker::start` implementation (the engine calls
    out through the port; the monolith registers the child exactly as it
    registers session-started runs, then delegates to `start_run_ext`). A
    monolith test reads `child_run_id` from `call_child`, calls the cancel
    command with it, and asserts true + child termination.
- **New event:** `RunEvent::CallChild { step: StepId, child_run_id: String }`
  journaled by the parent the moment the child run id exists, in **all three
  paths**: sync-success, sync-failure (today the id is buried in an error
  string; the split makes it structural), and fire-and-forget (today the id
  is discarded). Ordering: `step_intent` -> `call_child` -> (`step_ok` |
  `step_err`).
- A call that fails **before** a child run exists (unknown routine, depth cap)
  journals no `call_child`, and that absence is the honest record. One edge
  closed (R2 P3-1): the child engine currently journals `run_started` BEFORE
  parsing its snapshot, so a snapshot-shape failure leaves an orphan child
  journal (no `call_child` in the parent, later stamped interrupted).
  Journal creation moves after the parse so a start that never ran leaves
  no journal.
- **Trust domain (Codex R1 P3):** all routine run journals form ONE trust
  domain. `routines_journal_get` already returns any journal by run id with
  no per-routine ACL (and taints the agent session); `call_child` adds
  navigability, not reachability, and is NOT an authorization boundary.
  Anything that must not appear in an agent-readable or export-bundle
  journal must not be journaled at all; the existing redaction-sink rule for
  exports is unchanged.
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
  are currently dropped by `run_tracks` and never reach `run_finished`
  (verified: an End-terminated run's `run_finished.reason` is always null
  today). The End step's reason is threaded through (`TrackEnd::Ended` gains
  the step id; `run_tracks` carries it into the outcome; `RunOutcome` loses
  `Copy` and gains `reason` + end step id, a small verified ripple).
- **Multi-track precedence (R2 P2-4), stated so it cannot be implemented by
  accident:** `run_finished.reason` is the reason of the TrackEnd that
  determined the final state. A failed-End reason wins over a success-End
  reason regardless of join arrival order (matching the existing state
  precedence); a propagated `StepErr` wins over both (the run failed on an
  error, not an End); between two same-class Ends on parallel tracks,
  first-arrival wins and is nondeterministic (both `end_reached` events are
  in the journal either way; only the summary line picks one).
- **Cross-track narrative (R2 P3-3):** when one track's End terminates the
  run, sibling tracks' unvisited steps sweep as skipped with the existing
  cancellation reason; the step list will show an End row alongside
  cancelled-sibling skips, which is the truthful record (no attempt to
  rewrite sibling reasons as "ended by track A").
- **History UI:** the End event renders as its own step-list row ("ended at
  s7: complete|failed, <reason>") ahead of the existing finished row.

## 8. Frontend deltas (complete list)

1. `routinesApi.ts`: extend the TS `RunEvent` union with `call_child` and
   `end_reached` (snake_case fields, no recasing layer, matching #1159's
   variants), and add `parkKind` to the `state_changed` awaiting-consent
   shape.
2. `RunsTab.tsx`: two new `stepListModel` cases + `StepListRow.kind` union
   members (`'call'`, `'end'`) + `ROW_ICON` entries + two JSX row branches +
   the child-run navigation click handler. Three presentation rules ride
   along (R4 P2-4, P3-1, P3-2): navigating to a run outside the current
   routine's rail renders a context strip above the header ("Viewing a run
   of <routine> (called by this routine) - back to run <parent short id>",
   one-deep stack captured at call-row click time) instead of today's
   unlabeled dead-end; the finished row suppresses its reason when it
   string-equals the winning end row's reason (state badge stays;
   divergent multi-track reasons stay visible); and park rows render the
   journaled kind ("awaiting consent (config write)") so History matches
   the write-branded dialog.
3. `PaletteRail.tsx` / `canvasModel.ts` `flagsFor`/category mapping: surface
   the `writes_config` badge (category stays LOCAL-group for `config.*` since
   `needs_radio`/`needs_internet` are false; badge disambiguates).
4. Designer settings grid: the `write_ack` acknowledgment row, rendered when
   the routine's write closure is non-empty and mode is automatic, mirroring
   the transmit-ack row's UI act (records callsign + timestamp via a
   dedicated UI-only command). A routine whose closure both transmits and
   writes shows BOTH ack rows and needs both acks (two findings, two rows,
   stated explicitly per R3 P3-4). The mode field stays named
   `transmit_mode` on the wire (definition-format stability), but the
   settings-grid label for the toggle reads mode-generic ("Unattended
   (automatic)") when the closure writes, so a write-only routine's
   operator is not flipping a transmit-labeled switch to authorize writes.
   The `WRITE_VALUE_RUNTIME` warning renders beside the ack row.
5. `ActionInfo` TS type + Rust `ActionInfo` DTO gain `writes_config`.
6. **The consent-park surface itself (R3 P2-2** — the dialog IS the
   write-consent surface, so it ships in this arc, not as a follow-up):
   `ConsentGate.tsx` copy branches on the park kind, covering the header,
   the Part 97 sub-line, the body, AND the confirm-button label (R4 P2-2:
   a write park must not render under a "Part 97 §97.109 · you are the
   control operator" headline; "Confirm transmit" stays for transmit
   parks, write parks render "Confirm config write" with write-specific
   copy throughout). The app-event payload field is named **`parkKind`**
   (the event union's discriminant is already literally `kind`, so the
   spec's earlier field name collides); `ParkedRun` carries it through
   `useParkedRuns`; and `parkKind` ALSO lands on the journaled
   `state_changed {state: awaiting_consent}` event, with the launch
   recovery scan (`recoverParkedStepId`) reading it from there, so a write
   park recovered after an app restart renders write copy.
7. `StepInspector.tsx`: the WRITES badge joins its hardcoded RIG/TX/NET
   flags row (R4 P3-3 — palette-only badging would vanish for the
   selected step), and the action `description` line renders (see §0.3).
8. `WRITE_VALUE_RUNTIME` finding message, per the findings-style rule that
   messages name the offending entity verbatim (R4 P3-4):
   `step "s3" write param "drive_level" is "$args.level" - the value is
   chosen at run time by whoever starts the run`.

## 8b. Definition format, migration, and propagation

- **`schema_version` stays 1 (R3 P2-6).** `RoutineDef::parse` rejects on
  strict version inequality with no migration path, so a naive bump would
  orphan every saved routine. `write_ack` and both `closure_digest` fields
  are `#[serde(default)]` optional fields on the v1 shape. Old builds load
  new-format defs fine (no `deny_unknown_fields`); an old build RE-SAVING
  such a def silently drops `write_ack` and both digests because its struct
  lacks them (R3 P3-5) — this fails safe (the new build then requires
  re-acknowledgment) and is recorded here as the accepted consequence.
- **Merge ordering (R3 P3-3):** the tolerant journal reader lands as the
  arc's FIRST merged PR, and the operator's converge build is rebuilt
  before any branch-build probe writes new-variant events into the shared
  journal dir; a pre-arc converged binary reading `call_child` /
  `end_reached` would otherwise hide those runs and stamp them interrupted
  (recoverable History mis-classification only).
- **Propagation contract (R3 P2-4).** This arc amends the canonical consent
  model, so the canonical docs move in the same arc: the 2026-07-13
  routines design spec §4 gains the write-consent class + the digest
  binding on both acks (and §14's definition format gains `write_ack` +
  `closure_digest`); §7's snapshot doctrine gains a divergence note (Call
  targets resolve live in shipped code; bd follow-up tracks pinning). ADR
  0024 needs no new exception entries, and no CLAUDE.md rule changes, so
  no AGENTS.md parity action is required.
- **User-guide reference (R4 P3-5).** `docs/user-guide/` has no routines
  chapter, so the operator who searches Help for "find stations" or "read
  source" gets zero hits and the only in-app reference for action params is
  a hover tooltip. This arc adds a routines-actions reference page (action
  catalog: names, params, sources, consent classes) — which rank 4's
  `data.docs_search` then also makes searchable BY a routine.

## 9. Testing strategy

- **Engine (tuxlink-routines, R2 cargo):** unit tests per new event variant
  (serde round-trip + additive-tolerance), the tolerant reader (unknown
  event type -> opaque entry, file survives, terminal-scan unaffected),
  executor tests for CallChild in all three paths including inline-F&F
  ordering (call_child strictly before the parent's run_finished) and the
  now-observable F&F start failure, cancellation propagation (parent cancel
  reaches a sync child; F&F child deliberately survives), EndReached
  ordering vs skip sweep, reason threading + multi-track precedence
  (failed-End beats success-End beats neither when a StepErr propagated),
  attended-park on `writes_config` incl. per-attempt retry parks,
  `AUTO_WRITE_UNACKED` / `ATTENDED_WRITE_UNDER_SCHEDULE` / extended
  `MIXED_MODE_STALL_WRITE` / `WRITE_VALUE_RUNTIME` validator tests, digest
  canonicalization (key-order independence, param mutation, Call-args
  mutation, callee mutation each flip the digest), the child-start digest
  re-verification failure, and dry-run fake flag mirroring.
- **Monolith (R2 cargo):** seam impl tests with fakes for each new
  source/action; curation-equality pins for **every** read source, not just
  `config` (Codex R1 P3): `backend_status` error-arm redaction,
  `config` grid clamp, `find_stations` PII omission and `callsigns` derived
  only from post-curation gateways, each pinned routines-output ==
  MCP-tool-output for the same underlying state; write action old->new
  output computed under the config lock; ack-stripping on
  save/validate_draft for `write_ack`; closure-digest invalidation tests
  (routine edit, callee edit, digest-less legacy ack) for both ack classes.
- **Composability proof:** extend `composability_proof.rs` with the rank-2
  wire: `data.find_stations` (faked directory) -> `$s1.callsigns` ->
  `radio.connect` `stations`, asserting the resolved param is the sorted
  callsign array.
- **Frontend (Pi vitest):** stepListModel cases for `call_child`/`end_reached`
  (row kinds, child-run click, foreign-run context strip, finished-row
  reason suppression, park-row kind), palette + StepInspector WRITES badge,
  description render, example-params seeding, ack-panel VALIDITY branching
  (present-but-invalid third state), closure-enumeration render, ConsentGate
  parkKind copy branching incl. launch recovery from the journaled kind.
  Full `pnpm vitest run src/routines` + typecheck.
- **Dry-run shapes:** pinned tests that every new action's canned dry-run
  output contains the fields the spec's own authoring patterns reference,
  split per action/source (R5 P3): `data.find_stations` pins `callsigns`;
  `data.read grid` pins `grid`; the status sources pin `state`;
  `data.read ardop_config` pins `drive_level`; `config.set_ardop` pins
  `field`/`old`/`new`. The marquee composition dry-runs green end-to-end.
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
- General callee pinning at snapshot time (the §7-doctrine divergence R2
  P2-2 surfaced: Call targets resolve live, so NON-consent callee edits
  swap behavior mid-run): pre-existing shipped behavior, filed as a bd
  follow-up. This arc closes only the consent-relevant half (child-start
  digest re-verification).
