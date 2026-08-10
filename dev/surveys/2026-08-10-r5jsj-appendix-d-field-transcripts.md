# r5jsj Appendix D — field + self-survey transcript harvest

Provenance: read-only search across both repos + the app's XDG transcript
store, 2026-08-10, session moss-tamarack-taiga (subagent pass D). The curated
deliverable is `2026-08-10-mcp-tool-surface-gap-register.md`.

## Where the primary sources live (REPORTABLE: cited nowhere in-repo)

`/home/administrator/.local/share/com.tuxlink.app/elmer-transcripts/` — six
files; five are the 2026-08-09 field batch (UTC stamps; local = AZT):

| File | Local start | Content |
|---|---|---|
| `1786297399644-0.jsonl` | 08-09 10:50 | Las Vegas / DM33 harmonization (19 lines) |
| `1786297984292-1.jsonl` | 08-09 10:55 | VARA probe → grid set → NWS tabular → routine authoring → verify_cms (116 lines) |
| `1786320353734-2.jsonl` | 08-09 17:06 | THE SELF-SURVEY (68 lines) |
| `1786323332869-3.jsonl` | 08-09 17:57 | Capability self-eval → classifier/subagent design (44 lines) — the ADR 0030 origin exchange (seqs 35–43) |
| `1786326322419-4.jsonl` | 08-09 18:46 | Las Vegas EOC exercise, ICS-213 + gateway planning (70 lines) |
| `1786329619324-5.jsonl` | 08-10 14:19 | 2 lines, cancelled |

Elmer's own `export_report` outputs (2026-08-09 17:38–17:53):
`/home/administrator/Documents/Tuxlink/reports/{tuxlink-session-survey.md,
session-turn-by-turn.md, session-turn-by-turn-full.md}`.

**Convention break:** these transcripts are primary evidence for ≥6 open
issues; prior convention (tuxlink-1zocm) cited the absolute transcript path in
issue notes; the 2026-08-09 batch broke it. (bench-j1q's prose account also
says "3 transcripts"; there are 5.)

## Self-survey (`…-2.jsonl`) — agent-flagged gaps, verbatim gist

Prompt (seq 0): "Survey your own tool coverage…" Flags at seq 5 (unprompted):

- `find_stations`/`find_peers`: "both return station lists but with different
  schemas… The boundary isn't documented in my surface."
- Four "connect"-shaped tools (`ardop_connect`/`ardop_b2f_exchange`/
  `vara_b2f_exchange`/`packet_connect`): "I have to infer which is 'dial' vs
  'full B2F session' — not obvious from names alone."
- `predict_path`: unsure if `rx_grid` is target grid; dial vs audio-center Hz.
- `catalog_send_inquiry` vs `catalog_list`: id-mapping uncertainty.
- Routine ACTION names vs tool names ("`data.find_stations`" vs
  "`find_stations`"): "I have to map between them; that's a real ambiguity."
- `send_form`: "I don't know where `form_id` values come from."
- `grib_send_request`: mode values unknown. `packet_config_set`: defaults
  unknown. `position_set_source`: "no 'get available sources' tool."
- Taint: "There's no tool to check 'is session tainted?'… I can see authority
  but not taint state" → proposes "a `session_status` read that reports
  `tainted: bool` alongside `armed`." **[CORRECTED by passes A+D field DTO:
  `server_info` ALREADY returns `tainted`+`taint_reason` — Elmer (and its own
  exported survey doc) did not know. UNDERCLAIM/discoverability, not missing.]**
- Meta-discovery: "there's no `list_tools`/`enumerate_capabilities` in my
  surface… no lightweight inventory — you either get nothing or you risk a
  big dump." Operator (seq 31) also believed such a tool existed.
- **Measured consequence:** Elmer's hand-built inventory (seq 42, "full
  surface reproduced by name") omits `point_at`, `export_report` (used three
  turns later!), `rig_tune`, `wwv_*`, `user_folders_list`,
  `modem_ardop_disconnect` — and lists routine action `local.set_identity` as
  an MCP tool. Direct evidence for both meta-discovery and the
  action-vs-tool-name conflation.
- Placement judgement (seq 12): disambiguation belongs in the docs/agent
  guide, not schema overloading; "`find_peers` has no `intent` param — that's
  the real separator."
- Catalog parallel naming (seq 26): `NY_TAB`/`NY_TAB_ALBAN`/… "no '7-day' vs
  '3-day' marker"; zone-vs-tabular is a second disambiguation layer. [ch3e9]
- "No timer/cron tool" outside routines; "you explicitly asked me to check
  the inbox — that taints" (no taint-free triage path).

## Session 0 (`…-0.jsonl`) — DM33 harmonization + schema fumbles

- `position_status` → `{"grid":"DM33","has_fix":true,"source":"gps"}` (no age
  field); Elmer seq 17: "you said Las Vegas, NV — that's consistent with
  DM33, so we're aligned." [tuxlink-zqox2]
- NEW: `find_stations` `ok=false "missing field goal"` (objective at top
  level); same class in session 4: `"missing field intent"` — two independent
  same-day required-field fumbles.
- NEW: snapshot monotonicity discovered only by tripping: "filters would
  widen the snapshot; snapshots only narrow."

## Session 1 (`…-1.jsonl`) — verify_cms, grid gate, NWS steering, routines

- **verify_cms_connection verbatim:** `cms_connect` → `ok=true`; then
  `verify_cms_connection` → `ok=false "invalid arguments: Mcp error: -32603:
  Other { detail: \"CMS connection failed: no active identity — authenticate
  before transmitting\" }"`. Operator: **"It worked fine. I think that took
  call was misleading."** [tuxlink-sswik] Note the error-shape defects: Rust
  `Debug` leak (`Other { detail: … }`) and an "invalid arguments:" prefix on a
  non-argument error.
- **cms_connect no-host confirmed live** (Elmer, unprompted): "the tool
  doesn't take a host param — it connects to the configured CMS" after the
  operator said "Send via Telnet to CMS-Z."
- **NWS docs steering, precise locus:** `docs_search` → slug
  `23-catalog-requests` → `docs_read` → Elmer offers GRIB/Saildocs; operator
  interrupt: **"The docs are misleading you. NWS tabulars do go through the
  request center."** (Locus = docs corpus page, NOT the agents guide.)
- **NEW — `config_set_grid` demands SEND authority:** `ok=false "not
  authorized to write: send authority is not armed"` for a local config
  write. Elmer's read: "it's treated as a station-config transmission."
  Authority-model mis-scope candidate.
- NEW: `vara_install_status`/`vara_ini_read` surface benign negative states
  ("engine not bundled", "no Primary VARA install") as `-32603 internal
  error` with the same misleading prefix.
- `routines_save` 3 consecutive rejections then recovery (error text named
  the fix each time — bzxwp partially improved at 0.105.0).
- **NEW — `routines_meta_set` ok-with-`applied:false`:** Elmer then told the
  operator "The `transmit_mode` is now `automatic`" — false. Field-proven
  SILENT-SUCCESS consequence.
- `[ROUTINE_ENABLED]` refusal well-worded, recovered twice (positive control).
- **amjtz field-verified at 0.105.0:** "no CMS/Telnet action exists in the
  routine catalog… Nothing for `cms_connect`, `telnet.connect`, or CMS
  delivery" (confirmed twice).

## Session 4 (`…-4.jsonl`) — EOC exercise

- verify_cms fails again right after cms_connect ok (same error).
- `catalog_send_inquiry` accepted `["CMS_STATUS","WL2K_NEARBY","WL2K_USERS"]`
  → `ok=true "WHDTMIGQBPH5"` with no per-id validation echo; again with
  `PROP_3DAY`. [pass A gap #6, field-proven]
- `find_stations` filter vocabulary: no "1 RF bounce"/hop concept; distance
  buckets only; predict_path is direct-path only.
- `predict_path` provenance not machine-legible: Elmer had to hand-assert
  "SSN=100, month=8" when the operator asked "did we update these based on
  current, real propagation? …not vibes."
- `position_status` now `{"grid":"DM26","has_fix":true,"source":"manual"}` —
  source silently flipped gps→manual after the earlier `config_set_grid`
  (zqox2's conflation, live).
- `server_info` (seq 4) field DTO: `{"armed":true,"armed_remaining_secs":858,
  "taint_reason":null,"tainted":false,…}` — the taint flag EXISTS on the wire.

## Session 3 (`…-3.jsonl`) — capability self-eval

- `rig_tune`/presets: no AM/MW/500–1600 kHz support; "no step that says 'tune
  to 810 kHz AM'." [to358]
- `radio.listen` "only reports busy/RMS, not content"; no
  transcribe/summarize action class.
- `send_form`: "The tool exists; the guidance layer is thin."
- **Elmer UNDER-claimed its own surface twice** — "no real-time propagation"
  and "no RF help" — operator corrected (WWV/CMS/FT-8 exist; CAT read + ATU
  command exist); Elmer retracted both. Same discoverability failure class as
  server_info.tainted.
- No auto-chained "best gateway now" (predict_path is per-candidate manual);
  no bulk inbox summary; no persistent memory/index surface [goe9p].
- Seqs 35–43 are the operator exchange that became ADR 0030.

## Searched and absent

No transcript copies or quotes in either repo beyond bd-issue prose; no
August bug-hunts; `dev/elmer-distill/` has only the 2026-07-02 reference
harness. The CPU-viability eval's "~15k-token full-surface prefill" line
(`dev/evals/2026-08-10-cpu-only-elmer-viability.md:66`) is the one
tool-surface-adjacent August finding — a cost argument for a lean/paginated
surface.
