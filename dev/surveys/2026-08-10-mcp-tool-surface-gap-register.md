# MCP tool-surface gap register (tuxlink-r5jsj)

Surveyed surface: main @ `ed9e3bc2` (92 tools; stable 0.104.0 → today — the
bench dump, the live server, and the registry agree on the name set).
Session: moss-tamarack-taiga, 2026-08-10.

**Method** (per the issue: mine, don't re-derive) — four passes, full detail
in the appendices beside this file:

- **A** registry actual-vs-claimed (`…appendix-a-registry-inventory.md`) — all
  92 tools, handler-vs-description, ranked worst-15 with file:line.
- **B** guide/resources vs surface (`…appendix-b-guide-vs-surface.md`).
- **C** bench executed evidence (`…appendix-c-bench-evidence.md`) — 4,138
  executed calls, admission references, per-tool failure censuses. Pin
  caveat: bench pin `cff02cb`/0.104.0 vs operator-field 0.105.0.
- **D** field + self-survey transcripts (`…appendix-d-field-transcripts.md`) —
  five 2026-08-09 transcripts located in the app XDG store (cited nowhere
  in-repo before this; paths in appendix D).

Gap classes: `OVERCLAIM` · `UNDERCLAIM` · `SILENT-SUCCESS` · `HIDDEN-CONFIG`
· `AMBIGUOUS-SEMANTICS` · `DTO-GAP` · `DOCS-DRIFT` · `ERROR-UX` ·
`MISSING-TOOL` · `AUTHORITY-SCOPE`.

## Seed-corpus adjudication (what the issue asked first)

| Seed claim | Verdict |
|---|---|
| no list-all-tools/meta-discovery | **CONFIRMED** (all 92 names checked; operator + Elmer both believed one existed; Elmer's hand inventory measurably wrong: omitted 7+ tools, listed a routine action as a tool) |
| no session-taint status tool | **REFRAMED** — `server_info` already returns `tainted`+`taint_reason` (field DTO proves it live). The gap is UNDERCLAIM/discoverability: no doc says so; Elmer's own exported survey claims "armed/disarmed only" |
| verify_cms_connection misleading | **CONFIRMED + root-caused** — ephemeral backend, `NoActiveIdentity` always; already filed **tuxlink-sswik** (P2) + bench-bxa; also untestable (`cfg!(test)`/CI short-circuit) and error-shape defects |
| mailbox_list ok-empty on bad slug | **CONFIRMED** (`native_mailbox.rs:1253-1256`) — and it taints the session for nothing |
| cms_connect no host param | **CONFIRMED** (+ spec acknowledgment) — and it also **flushes the outbox**, which "connect" undersells |
| position_status no age / no operator-vs-station | **CONFIRMED, worse**: `has_fix` ANDed with privacy state (live GPS lock + broadcast-off ⇒ `has_fix:false`); `grid` is the privacy-clamped broadcast locator, unstated → **tuxlink-zqox2** |
| find_stations/find_peers boundary undocumented | **CONFIRMED** (guide enumerates the tier as exactly two tools; find_peers is also the only arm-gated read, denying in the wrong error shape) |
| catalog parallel naming | **CONFIRMED** in self-survey verbatim → **tuxlink-ch3e9** (epic lane) |
| routines revision integer-unsatisfiable | **CONFIRMED, reframed**: content-hash tokens; `applied` = hash-inequality (idempotent save reads as failure); `routines_validate` structurally cannot emit a revision |
| docs steered away from Request Center | **CONFIRMED, locus corrected**: `docs/user-guide/23-catalog-requests.md:159-178` (docs corpus), not the agents guide; contradicted by shipped code (`catalog/reply.rs`, `nws-zone-to-catalog.json`, 28 tabular-only WX buckets) |
| no CMS/telnet routine action | **CONFIRMED at 0.105.0 in the field** (twice) → **tuxlink-amjtz**. Correction to that issue's text: the bench infeasibility twin is AS-WEATHER-GAP (AS-SPACEWX-ALERT is the clean twin); the catch/on-error absence is independently confirmed (control vocab census over 1,470 payloads: branch/end/delay only) |
| no inference surface | → **tuxlink-obijd/9g70d** (filed) |
| no rig S-meter / USB diagnostics | **CONFIRMED** (bench-4p8: fault topology unobservable through the surface) → **tuxlink-to358** scope |

## The priority spine — 15 worst actual-vs-claimed (pass A, evidence in appendix A)

1. **verify_cms_connection fails for every caller** (ephemeral identityless
   backend; CI-short-circuit makes it untestable) — `sswik`, field-hit twice.
2. **rig_status narrates a live VFO/PTT read that is hardcoded `None` by
   design** (posture is right per bench-cqr — a live read would spawn an
   ungated rigctld — but the description fabricates a "serial busy" story).
3. **mailbox_list ok-empty on nonexistent folders** — validates guessed
   taxonomies and taints for nothing.
4. **Taint docs wrong: 5 tools taint, every enumeration says 4**
   (`routines_journal_get` missing from guide + server instructions +
   35-agent-mcp).
5. **Agents guide covers 56–57 of 92 while claiming the full surface** — the
   entire 20-tool routines tier invisible; no drift gate exists (CI pins the
   tool COUNT, nothing pins doc coverage).
6. **catalog_send_inquiry never validates ids against the catalog** — a
   hallucinated id becomes a real queued transmission on next connect
   (field-proven: unverified ids accepted, `ok=true`).
7. **cms_connect: zero params + flushes the outbox** — endpoint invisible and
   unchoosable; "connect" undersells "transmit staged mail."
8. **position_status conflates privacy with fix, no timestamp, broadcast
   locator unstated** (`zqox2`; field DM33/DM26 cases).
9. **Routines revisions are content hashes with hash-equality `applied`;
   validate can't emit a revision** — the advertised validate→remedy loop has
   no revision source.
10. **solar_conditions stamps `now` on never-updated shipped data** while
    telling agents to judge freshness by `updated_at_ms`.
11. **grib_send_request invents geometry** (silent ±5° box, 2×2 grid, empty
    times/params; nothing else requestable).
12. **user_folders_list returns display names mailbox_list can't consume**;
    no tool serves the slug.
13. **find_peers denies in the wrong shape** (`Unavailable`/internal-class,
    not "not authorized") — client denial classifiers miss the only arm-gated
    read.
14. **point_at hardcodes `outcome:"shown"`** (+ `1zocm`: ~9 anchors only).
15. **Twin-tool asymmetries**: `deny_unknown_fields` on ardop_b2f but not
    vara_b2f (stray `freq_hz` silently dropped); vara_open_session
    prerequisite one-sided; two audio-device tools with disjoint DTO/id
    spaces.

## Field-only findings (pass D, new this survey)

- **config_set_grid demands SEND authority** ("not authorized to write: send
  authority is not armed") for a local config write — authority-model scope
  question; meanwhile `vara_install_start` (pkexec system install) is
  **ungated**. The write-tier/transmit-arm coupling deserves an explicit
  taxonomy ruling.
- **routines_meta_set returns ok with `applied:false`** → Elmer reported
  "transmit_mode is now automatic" to the operator, falsely. Silent-success
  with a field-proven wrong-report consequence.
- **Error-shape defects**: Rust `Debug` leak (`Other { detail: … }`),
  "invalid arguments:" prefix on non-argument `-32603`s, benign negative
  states (no VARA install; engine not bundled) surfaced as internal errors,
  bare `not found` with no entity echo on routines_get/journal_get.
- **Schema fumbles at scale** (bench census + field): find_stations missing
  `goal`/`intent` (19 occurrences), distance/objective enum guesses,
  predict_path kHz/MHz unit trap (15×), grib mode enum undiscoverable (every
  natural word rejected), position_set_source **case-sensitive** ("only
  \"Gps\" accepted"), send_form canonical form-id undiscoverable (no tool
  lists form ids), routines envelope traps (name-vs-`routine`, placement
  oneOf absent from schema).
- **Elmer under-claimed its own surface twice** (denied having real-time
  propagation and RF help; operator corrected; retracted) — the same
  discoverability failure class as `server_info.tainted`, from the other
  direction.

## Axis B — missing tools / capabilities

| Missing | Evidence | Fix direction | Pri |
|---|---|---|---|
| Meta-discovery: lightweight tool inventory (name + one-liner + tier) | confirmed absent; both operator and agent assumed it existed; hand-inventory provably wrong | new tool or `server_info` expansion; also feeds the ~15k-prefill lean-surface problem (8dkcy) | P2 |
| Routine control vocabulary: no catch/on-error/retry; triggers manual+schedule only; no time-of-day primitive; no dry-dial connect-test; FT8/prop not routine-consumable | control census over 1,470 payloads; authored expected_gap strings; bench-53r | routine vocabulary completeness lane (sibling of amjtz/obijd/9g70d) | P2 |
| CMS/telnet routine action | field-confirmed twice at 0.105.0 | **tuxlink-amjtz** (filed) | P2 |
| Inference step / scheduled agent tasks | — | **tuxlink-obijd / 9g70d** (filed) | P2 |
| Rig S-meter, USB-topology diagnostics, AM/MW bands, band-scan, favorites exposure | bench-4p8; self-eval session 3; to358 scope | **tuxlink-to358** (filed) — this register feeds it | P1 |
| Attachment save reachable in the natural flow (taint walls it off; re-arm discards conversation) | bench-7u9 F4-ATTACH proven; 6/6 denied | design-level: consented save path or non-tainting metadata tier — natural **efk3k** classifier-architecture client | P2 |
| No timer/schedule primitive outside routines; no bulk-inbox summary; no persistent memory surface | self-survey/self-eval | **tuxlink-goe9p** (memory) + 9g70d | P2 |

## Axis C — docs drift (pass B; locus-precise)

1. `23-catalog-requests.md:159-178` NWS/Saildocs steering (the field
   incident) — false for 28 tabular-only WX buckets; `catalog_list`'s own
   description counter-steers. **P1 doc fix.**
2. `35-agent-mcp.md` says taint clears only by app restart; guide + code say
   operator re-arm — mutually exclusive remedies for the most consequential
   denial. Also repeats the four-taint error and covers only VARA tools (a
   set disjoint from the guide's own omissions).
3. Guide: 36 tools unmentioned; only-deprecated-alias steering
   (`ardop_list_audio_devices`); no arm-gated-read tier concept; write tiers
   non-exhaustive; ungated writes (export_report/print_document) have no
   slot; `server_info`'s taint fields undocumented.
4. FT-710 absent from `13-radio-specific-notes` (2nd-most-read doc in bench
   corpus; models fall back to FT-991A guidance) — operator's primary rig.
5. **Structural root cause: no drift gate.** `parity_check.rs` pins the tool
   count (92) rigorously; nothing ties guide/instruction content to the
   registry. Fix: a registry-derived coverage check (every registered tool
   name appears in guide or an explicit exclusions list; taint list generated
   from `TaintReason`, not hand-written).

## Child-issue split (filed as the survey's second deliverable)

New issues (this session, all citing this register):

| # | Title (lane) | Pri | Covers |
|---|---|---|---|
| N1 | Agent-docs truth reconciliation + CI drift gate | P1 | Axis-C items 1–5, taint 4→5, server_info taint discoverability |
| N2 | catalog_send_inquiry: validate item_ids against the catalog | P1 | spine #6 (transmit-adjacent) |
| N3 | Silent-success family: lookups must error on unknown keys | P1 | mailbox_list ok-empty (+taint-for-nothing), user_folders_list slug round-trip, routines_meta_set applied:false, p2p_peer_password_status, mailbox_move "ok", tauri_search_run bogus folder, has_attachments desync (bench-tue product half) |
| N4 | Error-shape hygiene: denial/negative-state contract | P2 | find_peers denial shape, Debug leak, "invalid arguments:" mis-prefix, benign-negative-as-internal, bare not-found echoes, predict_path Internal-on-validation |
| N5 | Overclaim corrections (align descriptions with code) | P2 | rig_status narrative, solar now-stamp, grib geometry, point_at shown, vara_status bandwidth, ft8_set_band condition, find_stations path_reliability, vara_install_status "offline" |
| N6 | Enum/schema discoverability | P2 | position_set_source case + free-string params, grib mode, find_stations required fields, kHz units, ft8 band list, send_form form-id catalog, routines placement oneOf, vara_b2f deny_unknown_fields parity, message_send no-attachments statement |
| N7 | Authority-tier taxonomy review | P2 | config_set_grid send-arm coupling, find_peers lone arm-gated read, vara_install_start ungated pkexec, attachment-save taint wall (with efk3k) |
| N8 | Meta-discovery / lightweight tool inventory | P2 | Axis-B row 1 |
| N9 | Routine vocabulary completeness | P2 | catch/on-error, trigger kinds, time-of-day, dry-dial, routine-consumable FT8/prop |
| N10 | ARDOP CODEC TRUE incompatibility (relay of bench-sx2) | P1 | Tuxlink cannot bring up post-May-2024 ardopcf |
| N11 | VARA config read/write asymmetry (relay of bench-m2h) | P2 | drive_level readable, not settable |
| N12 | DTO provenance stamps | P3 | predict_path model/SSN stamp, backend_status last_connected_at, session_log source field |

Evidence appended to existing issues: **sswik** (error shape, untestability,
field re-hits), **zqox2** (privacy/fix conflation code refs, source-flip),
**amjtz** (0.105.0 field confirmation + AS-WEATHER-GAP correction), **bzxwp**
(0.105.0 re-observation: improved errors, still 3 rounds). Cross-linked, not
duplicated: ch3e9, obijd, 9g70d, goe9p, to358, xib1x, 1zocm, hq9g0, dxx24,
p2e8l, and bench-* per appendix C.

## Not covered / bounds

- Elmer's in-process (non-MCP) surface; security-model classifier surfaces
  (ADR 0030 keeps them off the agent surface).
- Bench-side harness gaps (bench bd tracks them; appendix C cross-links).
- Executed-evidence bounds: 4 tools have zero bench reference coverage
  (`ardop_list_audio_devices`, `cms_abort`, `point_at`,
  `position_set_source`); 11 of 92 appear in zero executed calls — for those,
  pass A's code reading is the only evidence.
- Pin-vs-current: bench numbers are 0.104.0-pin; field observations are
  0.105.0; registry reading is today's main. Name-set stable across all
  three.
- The five 2026-08-09 transcripts remain only in the app XDG store
  (`~/.local/share/com.tuxlink.app/elmer-transcripts/`, paths in appendix D)
  — preserving/archiving them is an operator call (they contain operator
  content; this repo is public).
