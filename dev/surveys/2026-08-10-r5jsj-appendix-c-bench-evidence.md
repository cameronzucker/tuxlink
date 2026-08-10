# r5jsj Appendix C — tuxlink-bench executed-behavior evidence

Provenance: read-only mine of `/home/administrator/Code/tuxlink-bench` (+ the
main repo's `dev/elmer-distill/reference/tools.json`), 2026-08-10, session
moss-tamarack-taiga (subagent pass C). The curated deliverable is
`2026-08-10-mcp-tool-surface-gap-register.md`.

**Calibration (rides on every product-bug row):** bench pin =
`cff02cb` / server 0.104.0; operator field-tested 0.105.0 — annotate
pin-vs-current uncertainty. The live server exposed to this session is 92
tools with the same name set, so the surface is stable 0.104.0 → today.

## Sources located

1. **Tool-schema dump (the "claimed" args side):**
   `tuxlink-bench/worktrees/bd-bench-1k1-schema-args/dev/gen/tool-schemas.json`
   (canonical; 3 more byte-identical copies in sibling worktrees; absent from
   the bench main checkout). `bench_battery --dump-tool-schemas`, server
   0.104.0, **92 tools**, sha `2b0c8f69…`. Arg schemas only — **no
   descriptions**. Conformance gate: `schema_check.py`/`validate_rungs.py`.
2. **Admission references (executed proof):**
   `…/bd-bench-1k1-schema-args/dev/preflight/references/` — 151 refs;
   doctrine: a cell is admissible IFF its reference EXECUTED green at current
   shas. 144 `completes` / 7 `blocked_at` (the executed wall proofs:
   TR-CMS-VERIFY product-bug wall + six taint walls). **88/92 tools carry ≥1
   executed reference step; zero-coverage: `ardop_list_audio_devices`,
   `cms_abort`, `point_at`, `position_set_source`.**
3. **bench-bxa** (open P1): product bugs from preflight first-executions.
4. **Judge/battery artifacts:** `dev/judge*/…/tool_calls.jsonl` — **4,138
   executed calls, 81 distinct tools; ok 3,813 · invalid_args 280 · denied 44
   · cancelled 1.**
5. **Corpora:** `crates/tests/battery/floor-corpus.json` (79 per-tool floor
   cells); `dev/ported/corpus.json` (18 cells with authored `expected_gap`
   strings); `dev/ladder-v2/shakeout-corpus.json` (136 cells).
6. **Reports:** tool frontier ("101 corpus-matched references exercise 78 of
   92"), capability-coverage-map (76 exercised, 9 incidental-only),
   overnight-results §4 harness bugs, cell-validity census, admission
   inversion handoff, ARDOP/VARA lane spikes, cms-behavior-scope (1,244
   agent-authored routine defs → 18 distinct actions used).

## Consolidated per-tool findings (flags: P product · U upstream-suspect · M misleading · S schema/args · G gap-by-design proven · H harness-side · D docs/world drift)

| tool | observed (executed) | evidence | flag |
|---|---|---|---|
| `verify_cms_connection` | fails for EVERY caller (`no active identity`, ephemeral backend); 7/7 denied; `blocked_at` doctrine wall | bench-bxa #1; TR-CMS-VERIFY.json | P |
| ARDOP init (`ardop_connect`) | vendored `init_tnc` sends `CODEC TRUE`; ardopcf ≥May-2024 removed the handler → FAULT → init aborts pre-MYCALL: **cannot bring up current ardopcf** | bench-sx2, bench-bxa #2; 2026-08-04 ardop-lane spike | P/U |
| `vara_ini_apply` | first-ever execution panicked an unmanaged `VaraProcessSlot` worker and wedged past the 120s deadline; audit class: any tool-reachable `state()` not `manage()`d panics on first execution | bench-bxa #3 | P/H |
| `config_get_vara`↔`config_set_vara` | read/write asymmetry: get returns `{bandwidth,drive_level,host,port}`, set takes ONLY `bandwidth`; drive level requires `vara_ini_apply` (different family + modem bounce) | bench-m2h | P |
| `message_attachment_save` | valid but **agent-unreachable in the natural flow**: `mailbox_list`/`message_read` taint; save requires untainted+armed; re-arm discards conversation. 6/6 denied. Plus: dest parent must EXIST and no tool creates directories — nonexistent parent surfaces as "path escapes the attachment base" | bench-7u9 (F4-ATTACH PROVEN); commit 90751bb | G/M |
| `mailbox_list`↔`message_read` | `has_attachments` desync: list hardcodes false while read returns the attachment (MSG1042) | bench-tue | P/D |
| `mailbox_list` args | 7/7 invalid_args: folder-slug grammar undiscoverable; error is `internal error: Internal { detail: "invalid folder slug…" }` | tool_calls aggregate | S |
| `find_stations` | 28/195 invalid_args, all deserialization: missing `goal` (10) / `intent` (9); unknown distance variant (3: `upper_mi` vs `within100mi…beyond2500mi`); unknown objective (3: `nearest` vs `connect-now\|best-at`); missing `callsigns` (2) | tool_calls aggregate | S |
| `predict_path` | 45/62 invalid_args: 30× harness-unwired; **15× kHz/MHz unit trap** ("7.104 kHz outside 1800..=30000"); null-args crash killed 2 cells | tool_calls; overnight §4.2 | S/H |
| `ft8_*` (all 6) | **41/41 executed calls fail** — harness never managed `Ft8ListenerState`; zero successful FT8 call in the corpus | overnight §4.4 | H |
| FT8/prop as routine data | authored gap: no routine-consumable FT8/propagation ranking output; they exist only as live query tools | dev/ported/corpus.json P3; bench-53r | G |
| `grib_send_request` | 7/13 invalid_args: mode enum undiscoverable — `forecast/gfs/standard/weather/wx` all rejected (expected `send\|sub`) | tool_calls | S |
| `position_set_source` | 2/4 invalid_args: **case-sensitive enum** — 'gps' rejected, "only \"Gps\" is accepted"; zero reference coverage | tool_calls | S |
| `export_report` | 4/6 invalid_args: "Documents directory unavailable" (env-dependent) | tool_calls | D |
| `vara_ini_read` | 26/27 invalid_args ("no Primary VARA install") — read half fixture-backed while write half not: "half a surface is a trap" | tool_calls; fc3b8a3 | M/D |
| `wwv_capture_offair` | 1/1: "STT model not installed. Download to …ggml-base.en-q5_1.bin" | tool_calls | G |
| `send_form` | unknown-form on `winlink-check-in` vs canonical `Winlink_Check-In`; exact-match `find_form`; **no tool lists form ids** — canonical spelling undiscoverable. (Provoked the bench's admission inversion.) Also: two schema-invalid attempts never landed in tool_calls.jsonl (bench-zr4) | commit 04e6114; admission-inversion handoff | P/D |
| `find_peers` | 10/10 denied (8 arm, 2 taint) — read-shaped tool behind arm. Separately bench-t4t: contacts seed schema mismatch quarantined the store; content-free `expect` let preflight pass an empty-world cell | tool_calls; bench-t4t | G |
| `rig_status` | deliberately config-only (live read would spawn rigctld = ungated transmit-capable CAT server) — bench floor predicates were wrong, not the product; **but see pass A: the description still narrates a live read** | bench-cqr | G |
| `config_set_grid` | 5/11 denied: "not authorized to **write**: send authority is not armed" — write tier rides the transmit arm | tool_calls | G |
| `cms_connect`/`vara_open_session`/`vara_b2f_exchange` | all denied at gate when unarmed; even armed, `vara_b2f_exchange` → "VARA session not open" — the compose→arm→exchange flagship flow cannot complete | census Cluster 1 | G/H |
| `routines_actions_list` | catalog = **exactly 20 actions** (enumerated; no telnet/CMS, no `local.wait`); `controls` section NEVER queried in 261 calls; control vocab over 1,470 payloads = `end` 944 / `branch` 472 / `delay` 54 — **no catch/on_error/retry**; trigger kinds = `manual`+`schedule` only | tool_calls | G |
| `routines_save` | 20/156 invalid_args; envelope traps: name-vs-`routine` (6), missing `tracks` (6), `triggers`, `transmit_mode` | tool_calls | S |
| `routines_step_add` | 37/333 invalid_args: placement required (14), duplicate step id (7), "give ONE placement" (5) | tool_calls | S |
| `routines_step_update`/`_remove`/`trigger_set` | dominant failure `[REVISION_CONFLICT]`; 9 corpus prompts assert integer "revision 7" — unsatisfiable (content-hash revisions) | tool_calls; census Cluster 2 | S/D |
| `routines_get`/`journal_get` | bare `not found` (-32603) with no name echo | tool_calls | M |
| routine validator | 27 finding codes inventoried with counts (top: NO_RIG_CONFIGURED 1399, CONNECT_NOTHING_STAGED 732, ATTENDED_UNDER_SCHEDULE 444, NO_TERMINAL_PATH 247, ARM_FALLTHROUGH_LEAK 222…) | tool_calls | (inventory) |
| `docs_read`/`docs_search` | 371 searches, zero empty; most-read: 39-routines-actions (46), 13-radio-specific-notes (28), agents-guide (26). Docs gap: FT-710 absent from radio-specific notes → models fall back to FT-991A guidance | tool_calls; EU1 outcome | D |
| `list_audio_devices` | real sysfs read; no S-meter, no lsusb tool → codec-reset/CAT-health clues unobservable through the surface | bench-4p8 | G |
| VARA listener `peer_call` | answering side takes wrong CONNECTED token → would B2F-handshake against own callsign | bench-zvd; vara-lane spike | U |
| VARA read path | peer death mid-session hangs the read (no ArqState EOF gate; ARDOP lane has one) | bench-bta | U |
| harness ASSERT-NO-EGRESS | conflates tainted with armed — fails bundles where the anti-injection defence WORKED | bench-seb | P/M |

**Authored `expected_gap` strings** (dev/ported/corpus.json): C2 — no
dry-dial/preflight connect action distinct from `radio.connect`; C3 — no
time-of-day primitive for in-routine band selection; EU1 — no "verify VARA
setup" action; A1/AS-WEATHER-GAP — no local terrestrial-weather primitive.
Correction to the amjtz issue text: the infeasibility twin is
**AS-WEATHER-GAP** (AS-SPACEWX-ALERT is the CLEAN twin and passes); the
catch/on-error absence is real and independently confirmed via the control
vocabulary census.

## tools.json (main repo, distill reference) — the drift mechanism

`dev/elmer-distill/reference/tools.json` (2026-07-02, untracked): 50 entries,
OpenAI function format, real descriptions, **44/50 with EMPTY param schemas**
(6 hand-authored). Generator `build_tools.py` regex-scrapes
`#[tool(name=…, description=…)]` from router.rs — the **42 missing tools are
exactly the families not declared inline** (all routines_*, all ft8_*,
extended vara_*, docs_read, list_audio_devices, point_at, print/printer,
export_report, find_peers, wwv_*). Neither dump is complete:
tools.json = names+descriptions (50/92, args stubbed); bench dump =
names+args (92/92, no descriptions). Actual-vs-claimed needs both + live
router.rs.

## Bench bd cross-links (product-relevant)

bench-bxa (P1 preflight product bugs) · bench-sx2 (P1 ARDOP CODEC TRUE) ·
bench-m2h (P2 VARA get/set asymmetry) · bench-7u9 (P2 attach-save
unreachable) · bench-tue (P2 has_attachments desync) · bench-zvd (P2 VARA
peer_call token) · bench-seb (P1 tainted-vs-armed conflation) · bench-t4t
(closed P1 contacts seed) · bench-cqr (P1 rig_status posture) · bench-53r
(P2 FT8-ranking gap presentation) · bench-zr4 (P1 missing tool_calls rows) ·
bench-hl1 (P1 judge grades synthesized text) · bench-ju4 (P2 SUN_LEN) ·
bench-4p8 (P1 fixture observability) · bench-bta (P2 VARA read hang) ·
bench-0d2 · bench-x2x · bench-4q4 (P0 world-mutation frontier) · bench-xeu
(closed P0 fail-open preflight) · bench-1k1 (schema dump) · bench-ywb (P0
build excluded legs) · bench-uc9.

## Coverage bounds (for the register's "not covered" section)

Admission proves the corpus, not the product surface. Unexercised at the
101-reference mark: 14 verbs; at 151: 4 (`ardop_list_audio_devices`,
`cms_abort`, `point_at`, `position_set_source`). 11 of 92 dump names appear
in zero executed tool_calls rows (incl. `config_set_privacy`,
`print_document`, `printer_list`, `vara_install_start`,
`ardop_b2f_exchange`, `send_form`-partial, `point_at`).
