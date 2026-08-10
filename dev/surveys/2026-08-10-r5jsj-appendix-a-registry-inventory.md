# r5jsj Appendix A — full registry inventory (actual-vs-claimed)

Provenance: read-only survey pass over `src-tauri/tuxlink-mcp-core/` +
`src-tauri/src/mcp_ports.rs` at main `ed9e3bc2`, 2026-08-10, session
moss-tamarack-taiga (subagent pass A). Verbatim agent report; the curated
deliverable is `2026-08-10-mcp-tool-surface-gap-register.md`.

---

**Registry:** `src-tauri/tuxlink-mcp-core/src/router.rs` — `#[tool_router] impl TuxlinkMcp` (lines 170–1837). **92 tools**, all via `#[tool(name=…, description=…)]`. Budget pin: `docs/parity/parity-manifest.json:1262` `"tool_budget": 92`, enforced by `src-tauri/src/parity_check.rs:230`.
**Real backends:** `src-tauri/src/mcp_ports.rs` (6720 lines, 18 `Monolith*Port` adapters). DTOs: `src-tauri/tuxlink-mcp-core/src/ports.rs`.
**Gap-flag key:** `OVER` = description over-claims · `UNDER` = under-claims · `SILENT` = ok-empty/ok on invalid input · `IMPLICIT` = config the schema hides · `AMBIG` = undocumented boundary between tools · `TAINT` = taint behavior vs docs.

## Subsystem: status / diagnostics (10)

| # | tool | claim (1-line) | actually does | params / DTO notes | gaps |
|---|---|---|---|---|---|
|1|`server_info`|Report ARMED state, seconds remaining, TAINTED + `taint_reason`, app name/version; taint dominates arming|`server_info_view(&state)` → reads `EgressGuard::armed_remaining()`, `is_tainted()`, `taint_reason()` (lib.rs:~140-165). Pure read, no port|none. DTO `ServerInfoDto{name,version,armed,armed_remaining_secs,tainted,taint_reason}`|— (this **is** the taint-status tool; seed defect (f)'s "no session-taint status tool" is **false**)|
|2|`backend_status`|CMS engine connected/transport/state|`BackendState::snapshot()` → `derive_status_dto` → `curate_backend_status` (mcp_ports.rs:332-341)|none. `{connected,transport,state}` — no `last_connected_at`, no error detail|UNDER|
|3|`modem_get_status`|`running` list + primary `kind/connected/state` + `selected` + `conflict`|`gather_modem_status(&ModemSession,&VaraSession,selected-from-config)` (mcp_ports.rs:343-359)|none. Honest DTO; `kind` = `running[0]`|—|
|4|`vara_status`|connected/bandwidth/state + `reachable` cmd-port probe|`VaraSession::snapshot()` + `config_get_vara()` for bandwidth + `probe_reachable(host,cmd_port,timeout)` (mcp_ports.rs:361-385)|none. `bandwidth` is the **configured** value, not negotiated — description says "bandwidth" flatly|OVER (minor)|
|5|`vara_probe`|Read-only cmd-port banner probe → `down`/`socket-not-vara`/`vara-ok`|`transport::deep_probe(cfg)` on `spawn_blocking` (mcp_ports.rs:387-403)|none. `{classification,banner}`|—|
|6|`position_status`|"THE OPERATOR'S current station location… grid + GPS fix status"|`PositionArbiter::has_fresh_fix() && privacy.gps_state != Off`, then `effective_broadcast_locator` → `broadcast_grid(FourCharGrid)` (mcp_ports.rs:405-431)|none. **`PositionStatusDto{has_fix,grid,source}`** (ports.rs:516-520)|**seed (d) CONFIRMED** — no `observed_at`/fix age/`updated_at_ms`; `has_fix=false` when privacy is Off even with a live fix (conflates privacy with fix); `grid` is the **broadcast** locator, no operator-vs-station distinction. OVER + UNDER|
|7|`platform_info`|OS/arch/app version|`vara::commands::platform_info()` + `env!("CARGO_PKG_VERSION")` (mcp_ports.rs:433-441)|none|—|
|8|`rig_status`|"and, **best-effort, its live VFO frequency/mode/PTT via a transient rigctld read** … may report nulls if the rig is unconfigured or its serial is busy"|**Never reads the rig.** Hardcodes `vfo_hz: None, mode: None, ptt: None`; only `configured` is computed from config (mcp_ports.rs:459-479, explicit "stay None BY DESIGN" comment)|none. `RigStatusDto{vfo_hz,mode,ptt,configured}` — 3 of 4 fields are structurally always null|**OVER (worst in surface).** Description names a probe that does not exist; the null explanation ("serial busy") is fabricated|
|9|`get_wizard_completed`|Wizard done?|`wizard::get_wizard_completed()` (mcp_ports.rs:443-447)|none. Bare bool|—|
|10|`p2p_peer_password_status`|Password Set/NotSet for callsign; never returns it|`ui_commands::p2p_peer_password_status` → `matches!(status, Set)` (mcp_ports.rs:449-457)|`callsign` (req). Bare bool|SILENT — an unknown callsign returns `false` (NotSet), indistinguishable from "known peer, no password"|

## Subsystem: mailbox (5)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|11|`mailbox_list`|List message metadata in a folder; **taints**|`parse_folder_ref(folder)` → `list_mailbox` → `backend.list_messages` or `list_user_messages(slug)`; then `guard.taint(MailboxList)` (router.rs:302-312; mcp_ports.rs:521-531)|`folder` (req, string). `Vec<MessageMetaDto>` — no folder echo, no total|**seed (b) CONFIRMED.** Chain: `parse_folder_ref` (ui_commands.rs:551-556) accepts ANY string passing `validate_slug` (user_folders.rs:93-121, lowercase/digits/`-` only) → `Mailbox::list_user_ns` (native_mailbox.rs:1109-1115) → `Mailbox::list_dir` (native_mailbox.rs:1253-1256) **`if !dir.exists() { return Ok(Vec::new()) }`**. `mailbox_list{"folder":"nonexistent-folder"}` = `[]`, no error. SILENT|
|12|`message_read`|Parsed message by folder+id; **taints**|`read_message_in`/`read_user_message` → `parse_raw_rfc5322`; `guard.taint(MessageRead)` (router.rs:318-333; mcp_ports.rs:533-570)|`folder`,`id` (both req). `ParsedMessageDto{…,attachments[],has_form}` — attachment **bytes** never served|— (correctly errors `NotFound` on bad id, unlike `mailbox_list`) → **AMBIG** vs #11: same bad `folder` errors here, returns ok-empty there|
|13|`user_folders_list`|"Enumerate mailbox folders and their message counts. Structural metadata; does not taint"|System folders hardcoded as slugs `inbox/outbox/sent/archive/deleted`; user folders pushed as **`f.display_name`**, not `f.slug` (mcp_ports.rs:572-608). Count = `list_messages(f).len()` (5+N full folder scans per call)|none. `Vec<FolderDto{name,count}>`|**AMBIG/SILENT chain:** the `name` an agent gets back for a user folder is the display name (e.g. `"ARES Drills"`), but `mailbox_list` needs the **slug** (`ares-drills`). Feeding the returned name back fails `validate_slug` → `PortError::Internal`; a name that happens to slug-validate returns ok-empty (#11). Also UNDER: "does not taint" is true, but folder counts are derived from untrusted message files|
|14|`mailbox_move`|Move message between folders; WRITE-gated|Validates BOTH `parse_folder_ref` endpoints **before** the gate, then `guarded_egress(Agent,"mailbox_move")` → `ui_commands::mailbox_move` (mcp_ports.rs:2014-2050)|`from`,`to`,`id` (all req). Returns literal `"ok"` — no new location, no confirmation the id existed|SILENT (nonexistent user-folder `to` slug passes `parse_folder_ref`); UNDER (`"ok"` DTO asserts nothing)|
|15|`message_attachment_save`|Save attachment to "a destination path (validated to stay inside **the attachment base**)"|Base is hardcoded `<app_data>/agent-attachments`, created on demand; `validate_attachment_dest` pre-gate, then gated read+write (mcp_ports.rs:2052-2130)|`folder`,`id`,`filename`,`dest` (all req). Returns saved path|**IMPLICIT** — "the attachment base" is never named in the description or schema; the agent cannot know the root it is relative to (contrast `export_report`, which names `~/Documents/Tuxlink/reports/`)|

## Subsystem: search / docs / catalog (4)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|16|`tauri_search_run`|Search mailbox msgs; **taints**|`SearchService::run(QuerySpec)`; `try_state` degrades to `Unavailable`; `guard.taint(SearchResults)` (mcp_ports.rs:630-676)|`query`(req), `folder`, `limit`. `SearchResultsDto{items,total}` (`total` = pre-limit match count)|SILENT — a nonexistent `folder` filter yields `{items:[],total:0}`, not an error|
|17|`docs_search`|Keyword docs search; snippet is NOT enough to answer from|`index.search_docs(query)` (mcp_ports.rs:678-698)|`query`(req). `Vec<DocsHitDto{title,slug,snippet}>`|—|
|18|`docs_read`|Full page by slug; "if the slug is unknown, the result lists the valid slugs"|`read_doc(slug)`; on `None` re-runs `search_docs(slug)` and returns `{error,requested,hint,closest_slugs}` as a **success** result (router.rs:407-436)|`slug`(req)|SILENT **by design** (documented) — but the description says "lists the **valid** slugs" while the code lists *search hits for the bad slug*, and returns `closest_slugs: []` when nothing tokenizes. OVER (minor)|
|19|`catalog_list`|"Hundreds of items"; Request Center products; ids feed `catalog_send_inquiry`|`catalog::commands::catalog_list()` (bundled file parse) → `{id: e.filename, title: e.description, category}` (mcp_ports.rs:718-730)|none|— (field renames `filename→id`, `description→title` are undocumented but harmless)|

## Subsystem: config reads (5)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|20|`config_read`|Curated non-secret top-level config|`redact_config_view` (4-char grid clamp) → 5-field projection `curated_config_view` (mcp_ports.rs:746-771)|none. `{connect_to_cms,transport,host,callsign,grid}`|— (`transport` is `format!("{:?}")` of a Rust enum — undocumented token shape)|
|21|`config_get_ardop`|host/port/drive/bandwidth|ConfigPort::ardop|none|—|
|22|`config_get_vara`|host/port/bandwidth/drive; no license secrets|ConfigPort::vara|none|—|
|23|`packet_config_get`|KISS host/port/baud/txdelay|ConfigPort::packet|none|—|
|24|`config_get_rig`|hamlib model, rigctld endpoint, CAT serial, flags|ConfigPort::rig|none. `RigConfigDto` (9 fields)|—|

## Subsystem: devices / reports (7)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|25|`packet_list_serial_devices`|Serial devices for TNC/CAT|`ui_commands::packet_list_serial_devices` (mcp_ports.rs:872-883)|none|—|
|26|`packet_list_bluetooth_devices`|BT names + "minimized MACs"|same + `minimize_bt_mac` (mcp_ports.rs:885-898)|none|—|
|27|`list_audio_devices`|Station-level audio; `cards` w/ vid_pid, bus_path, `in_use`|`ardop_list_audio_devices()` + `read_sys_snapshot()` → `project_audio_cards`; `in_use` = `probe_device_busy(...).is_err()` (mcp_ports.rs:900-921)|none|OVER (minor) — `in_use` is inferred from *any* probe error, not solely "another app holds it"|
|28|`ardop_list_audio_devices`|"DEPRECATED alias of list_audio_devices"|literally `self.list_audio_devices().await` (router.rs:528-533)|none|— (honest)|
|29|`printer_list`|CUPS destinations from `lpstat -p -d`; empty ⇒ fall back to export_report|shells out, soft-fails to `""` → `parse_printers` (mcp_ports.rs:923-936)|none|SILENT — CUPS present-but-erroring is indistinguishable from "no printers"|
|30|`export_report`|Write markdown/text to `~/Documents/Tuxlink/reports/<filename>`; ungated|`agent_reports_dir()` + `validate_attachment_dest` + `O_NOFOLLOW` write (mcp_ports.rs:967-971, 1039-1069)|`filename`,`content` (req). Returns abs path|— (best-documented sandbox on the surface)|
|31|`print_document`|Print a previously-exported report via `lp -d`|resolves inside reports dir, rejects symlink, `lp -d <printer> <path>` (mcp_ports.rs:938-965)|`printer`,`filename` (req). Returns `"ok"`|UNDER — `"ok"` means `lp` exited 0 (job queued), not printed|

## Subsystem: logs (1)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|32|`session_log_snapshot`|Snapshot session log; **taints**|`SessionLogState::snapshot()`, `redact_freeform` per line, `guard.taint(SessionLog)` (mcp_ports.rs:1089-1122)|none. `Vec<LogLineDto{timestamp,level,message}>` — **`LogSource` dropped**|UNDER — no way to tell a wire line from a backend line; no line cap/pagination param|

## Subsystem: station intelligence + propagation (5)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|33|`find_stations`|Intent-tagged gateway finder (`recommend`/`explore`/`lookup`/`aggregate`/`export`); filters nested under `filters`|Fetches+curates the listing population (or reuses a pinned snapshot), optional FT-8 corroboration, then `StationQueryEngine::evaluate` (mcp_ports.rs:3158-3241)|`intent` **enum, required tag**; per-variant: `goal`(req for recommend), `filters`, `candidate_count`, `exclude_candidate_ids`, `snapshot_id`, `callsigns`(req for lookup), `group_by`(req for aggregate), `format`(req for export). All bounded newtypes (request.rs:276-486)|OVER — `unavailable_inputs: vec!["path_reliability"]` (mcp_ports.rs:3230) is hardcoded: `recommend`'s `estimated-success` objective ranks *without* path reliability, which the description never says. IMPLICIT — operator grid + `now_ms` + connection history injected server-side (documented as "supplied by the app", but not which)|
|34|`find_peers`|Private contact roster w/ P2P reachability; "**Requires the egress arm**"|`guard.authorize(Agent)` first (returns `PortError::Unavailable`, **not** a denial), then reads `ContactsStore` and curates (mcp_ports.rs:3252-3277)|none. `PeerListDto{peers[]}`; free text + telnet endpoints deliberately dropped|AMBIG — the only read tool gated by arm; a denial surfaces as `internal_error`-class *unavailable* text, not the `not authorized to …` shape every other gated tool produces (so client denial classifiers miss it)|
|35|`predict_path`|VOACAP path reliability/SNR/MUFday by UTC hour|validates grid+freqs, injects operator `tx_grid` from config (never agent-supplied), `propagation_predict_path` (mcp_ports.rs:3357-3426)|`rx_grid`,`frequencies_khz` (req), `gateway_antenna` (opt enum). `PathPredictionDto` w/ 24-long vectors|IMPLICIT (documented) — `tx_grid` injected. Validation errors map to `PortError::Internal` → **internal_error** instead of invalid_request (mcp_ports.rs:3364-3366)|
|36|`solar_conditions`|STORED indices; check `source`/`updated_at_ms`; `shipped` = never updated|`load_solar_snapshot_dto()`; **when no snapshot exists it stamps `updated_at_ms = now`** and `source:"shipped"` (mcp_ports.rs:3330-3340)|none|**OVER** — the description tells the agent to reason freshness from `updated_at_ms`, but the shipped-fallback path stamps *now*, making never-updated data look fresh. Only `source` disambiguates|
|37|`wwv_capture_offair`|Tune to WWV + listen ~1 min, update indices; receive-only|`wwv_offair_refresh(now_ms, RadioArbiter)` (mcp_ports.rs:3490-3520)|none. `{updated,no_copy,source,sfi,a_index,k_index}`|—|
|38|`wwv_offair_available`|Is CAT configured for WWV capture|`wwv_offair_cat_configured()` (mcp_ports.rs:3522-3526)|none. bare bool|—|

## Subsystem: VARA provisioning (5)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|39|`vara_engine_available`|Ships the WINE setup engine? x86_64 Linux only|`run_engine_available(&app)` path probe (mcp_ports.rs:3571-3580)|none. bool|—|
|40|`vara_install_status`|`ready` + per-checkpoint state; offline, read-only|`run_install_status` (spawns engine `status --json`) (mcp_ports.rs:3582-3600)|none|OVER (minor) — "offline and read-only" but it **spawns a subprocess**|
|41|`vara_ini_read`|Read VARA.ini REDACTED (reg code/password masked)|`resolve_prefix_arg`/`parse_instance_arg` → `run_vara_ini_read` (redacted by construction) (mcp_ports.rs:3624-3640)|`prefix`, `instance` (both opt, `instance` ∈ primary\|vara2). Returns raw string|— (tested non-tainting, router.rs:2868)|
|42|`vara_install_start`|Privileged install via pkexec; several minutes; NON-TRANSMIT so ungated|`run_install(&app,&installer_path)` on blocking thread; **no egress gate at all** (mcp_ports.rs:3602-3622)|`installer_path` (req)|IMPLICIT — an unarmed agent can trigger a pkexec-privileged system install; only guard is the OS password dialog. Not flagged in any tier doc|
|43|`vara_ini_apply`|stop-edit-start bounce; validated pre-gate; WRITE-gated|`validate_vara_ini_apply` (ports.rs:463-499) pre-gate → gated apply (router.rs:993-1005)|`prefix`,`instance`,`edits[]`(section/key/value),`relaunch`. Returns full report DTO|— (best-shaped write on the surface)|

## Subsystem: gated egress (8)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|44|`cms_connect`|"Connect to **the configured** CMS"|`guarded_egress(Agent,"cms_connect")` → `ui_commands::cms_connect(app,…)` which reads `config.connect.transport`/host (ui_commands.rs:2967-2991)|**zero params** (router.rs:776)|**seed (c) CONFIRMED — IMPLICIT.** No `host`/`transport`/`target` param exists; the agent cannot know or choose the endpoint. Spec acknowledges this: `docs/superpowers/specs/2026-07-01-elmer-agent-send-design.md:103` "accept **zero** params today (they read `cfg.connect.transport`)". Also: this **flushes the outbox** (transmits staged mail) — the description says only "connect"|
|45|`verify_cms_connection`|"Verify the **live** CMS connection with a network round-trip"|`guarded_egress` → `wizard::verify_cms_connection_impl` (wizard.rs:519-571), which builds an **ephemeral** `NativeBackend::new(config, tempdir)` (wizard.rs:539-544) and connects with it|zero params. Returns `"ok"`|**seed (a) CONFIRMED — root cause found.** The throwaway backend has no active identity, so `native_connect` hits `.ok_or(BackendError::NoActiveIdentity)?` (winlink_backend.rs:1901; also `active_identity()` winlink_backend.rs:1590-1596) and every call fails with "no active identity" **even while `cms_connect` succeeds in the same session**. Filed as open bug **`tuxlink-sswik` (P2, status `open`)**; forensics in `dev/handoffs/2026-06-27-cedar-magnolia-crag-mcp-cold-acceptance.md:38-44`. Note it also opens a **second** CMS connection instead of inspecting the live one. Extra hazard: `verify_cms_connection_impl` short-circuits `Ok(())` under `cfg!(test) || CI` (wizard.rs:522-524), so no test can catch this|
|46|`rig_tune`|CAT tune; `freq_hz` is audio-CENTER, do NOT pre-subtract|gated → `ardop_tune_rig(freq_hz, None)` (mcp_ports.rs:1282-1306)|`freq_hz` u64 (req). `"ok"`|— (good; center-vs-dial explicitly taught)|
|47|`ardop_connect`|Connect ARDOP to target; opt `freq_hz`, opt `qsy_candidates`|gated → `modem_ardop_connect(app, session, …)` (mcp_ports.rs:1308-1340)|`target`(req), `freq_hz`, `qsy_candidates[]{target,freq_hz}`. `"ok"`|UNDER — `"ok"` carries no negotiated dial / which QSY candidate won|
|48|`ardop_b2f_exchange`|B2F over the connected ARDOP link; no freq params (tuned at connect)|gated → ardop B2F|`target`(req), `intent` (enum, default `cms`). **`#[serde(deny_unknown_fields)]`** (router.rs:2183) — a stray `freq_hz` is rejected, deliberately|— (exemplary)|
|49|`vara_b2f_exchange`|VARA connect+tune+exchange in one call|gated → vara B2F|`target`(req), `intent`, `freq_hz`, `qsy_candidates`, `engine` (vara-hf\|vara-fm)|AMBIG — **no `deny_unknown_fields`** (router.rs:2196), unlike its ARDOP sibling: unknown keys are silently dropped here. Also AMBIG vs `vara_open_session` (which is "required before" it) — but this tool also connects|
|50|`vara_open_session`|Open VARA TCP + register MYCALL; "pre-air by itself" but gated|gated → `vara_open_session(intent, engine)`|`intent`, `engine` (both opt). `"ok"`|AMBIG — "Required before `vara_b2f_exchange`" is stated only here; `vara_b2f_exchange`'s own description does not mention the prerequisite|
|51|`packet_connect`|AX.25 session to callsign over optional digi path|gated → packet connect|`call`(req), `path[]` (default `[]`). `"ok"`|UNDER|

## Subsystem: ungated abort (3)

| # | tool | claim | actually does | params | gaps |
|---|---|---|---|---|---|
|52|`cms_abort`|Abort CMS; never gated|`ui_commands::cms_abort` + `audit_abort` (mcp_ports.rs:1544-1556)|none. `"ok"`|SILENT (benign) — `"ok"` when nothing was running|
|53|`modem_ardop_disconnect`|Disconnect ARDOP; never gated|`modem_ardop_disconnect(...)` (mcp_ports.rs:1558-1574)|none|SILENT (benign)|
|54|`vara_stop_session`|Stop VARA; never gated|`vara_stop_session_inner(&session)` (mcp_ports.rs:1576-1588)|none|SILENT (benign)|

## Subsystem: gated writes (8, plus #14/#15/#43 above)

All: validate → `guarded_egress(Agent, op)` → `write_audit_sink` `[write]` line. All return literal `"ok"`.

| # | tool | claim | actually does | params | gaps |
|---|---|---|---|---|---|
|55|`config_set_ardop`|Set drive level 0..=100|`validate_drive_level` pre-gate → gated persist (mcp_ports.rs:1661-1680)|`drive_level` u8 (req)|—|
|56|`config_set_vara`|Set bandwidth 500/2300/2750|validate pre-gate → gated|`bandwidth_hz` u32 (req)|—|
|57|`packet_config_set`|Set ssid/host/port/txdelay|gated|`ssid`,`tcp_host`,`tcp_port`,`txdelay_ms` (all req)|UNDER — description omits that all 4 are required (no partial patch)|
|58|`config_set_grid`|Set Maidenhead grid|gated|`grid` (req)|—|
|59|`position_set_source`|Set source "(e.g. gps/manual)"|gated `set_position_source(source)`|`source` **free string**, not an enum|OVER/AMBIG — "e.g." implies an open set; backend has exactly two variants|
|60|`config_set_privacy`|Set GPS broadcast state + precision|gated|`gps_state`,`precision` — **both free strings** ("e.g. on/off", "e.g. a Maidenhead precision selector")|AMBIG — neither enumerates its legal values|
|61|`packet_set_listen`|Enable/disable packet listen|gated|`enabled` bool (req)|—|

## Subsystem: ungated compose / staging (4) + UI (1)

| # | tool | claim | actually does | params | gaps |
|---|---|---|---|---|---|
|62|`message_send`|Stage in local outbox; returns MID; no transmission|validates recipients/subject/body → `ui_commands::message_send` (mcp_ports.rs:2225-2248)|`to[]`(req),`cc[]`,`subject`(req),`body`(req). Returns MID string. **No attachment support** (hardcoded `attachments: Vec::new()`)|UNDER — attachment impossibility unstated|
|63|`send_form`|Stage a form submission; validates recipients/headers|validates + **re-renders the subject template** and re-validates it (mcp_ports.rs:2250-2291)|`form_id`(req),`field_values{}`,`to[]`(req),`cc[]`,`senders_callsign`(req),`grid_square`(req)|SILENT — an unknown `form_id` is *tolerated* at the pre-check (`if let Some(form) = find_form(...)`), deferring to `send_form`'s own error. No tool lists valid `form_id`s (`catalog_list` serves Request Center items, not forms) → AMBIG|
|64|`catalog_send_inquiry`|Stage a Request Center inquiry for catalog item ids|`validate_address` per id (CR/LF + control chars only) → `catalog::commands::catalog_send_inquiry` → `build_inquiry_body` which only checks non-empty + no newline (catalog/composer.rs:47-61)|`item_ids[]` (req)|**SILENT** — **no id is ever checked against the catalog.** A hallucinated id stages an outbound message that will be transmitted on the next connect and silently return nothing|
|65|`grib_send_request`|Stage a GRIB request; validates the subject|validates subject + lat/lon range + `mode ∈ {send,sub}`; **synthesizes a fixed 10°-wide box** around the center and hardcodes `grid:(2,2)`, empty times/params (mcp_ports.rs:2312-2394)|`lat`,`lon`,`mode`,`subject` (all req)|**OVER/IMPLICIT** — the 10° box, 2×2 grid and empty parameter set are invented server-side and appear nowhere in the description; the agent cannot request a different area or resolution|
|66|`point_at`|Spotlight a UI element; errors list valid anchor IDs|Emits `POINT_AT_EVENT`, awaits frontend ack w/ timeout (mcp_ports.rs:3666-3712)|`anchor_id` (req). Returns `{outcome:"shown",anchor_id}`|OVER — the router hardcodes `"outcome":"shown"` (router.rs:1268-1270) regardless; a non-"shown" ack becomes an `internal_error`, so `outcome` is a constant asserting more than the code knows|

## Subsystem: FT-8 (6)

| # | tool | claim | actually does | params / DTO | gaps |
|---|---|---|---|---|---|
|67|`ft8_status`|state/band/dial/device/blocking reason|`listener.snapshot()` → `ft8_axis_tokens` (mcp_ports.rs:4032-4053); `sweep_enabled` = the **configured** sweep, not live dwell|none. `Ft8StatusDto` (8 fields, has `last_slot_utc_ms`)|—|
|68|`ft8_heard_stations`|call/grid/best SNR/times heard/last heard|`aggregate_heard(&snap.ring_tail)` (mcp_ports.rs:4055-4061)|none. `Ft8HeardStationDto` — **has `last_heard_utc_ms`**, unlike `position_status`|—|
|69|`ft8_start_listening`|Start listener; receive-only, no send authority|`ft8_listener_start_inner` on blocking thread; deliberately not gated (mcp_ports.rs:4063-4072)|none. `"ok"`|—|
|70|`ft8_stop_listening`|Stop + release device|`ft8_listener_stop_inner`|none|—|
|71|`ft8_set_band`|Set band; QSYs the dial if CAT configured|`ft8_set_band_inner` — validates band, QSYs **only if the listener is already running with CAT** (mcp_ports.rs:4082-4093)|`band` (req, free string)|OVER — "If rig CAT control is configured this QSYs the radio" omits the listener-must-be-running condition. AMBIG — no enum, no band list served|
|72|`ft8_list_audio_devices`|Capture devices + stable id|`ft8_list_devices_inner`, `stable_id.value` only (kind dropped) (mcp_ports.rs:4095-4114)|none|AMBIG vs #27 `list_audio_devices` (different DTO, different id space, no cross-reference in either description)|

## Subsystem: routines (20)

Shared: `expected_revision` is an **optional string** on every mutating verb; the revision token is `revision_of()` = **first 8 bytes of sha256, hex** (16 chars) of the stored file bytes (`src-tauri/src/routines/store.rs:36-52`). None taint except #92.

| # | tool | claim | actually does | params (req in **bold**) | gaps |
|---|---|---|---|---|---|
|73|`routines_list`|name/transmit_mode/enabled/trigger_kinds|`list_routines` (mcp_ports.rs RoutinesPort::list)|none. `RoutineSummaryDto` — **no `revision`**|UNDER — the list gives no revision, so an edit always needs a `routines_get` first|
|74|`routines_actions_list`|Full authoring catalog + `definition_template`; narrow w/ `action` or `section`|`actions_catalog()` then router-side filtering (router.rs:1393-1444)|`action`, `section` (both opt; `action` wins). Unknown values **error** with the valid set|— (well-behaved: errors, not ok-empty)|
|75|`routines_get`|`{revision, def, edit_protocol}`|`get_routine_with_revision` → `with_edit_protocol()` injects the 9 edit-verb names + `pass_expected_revision` (router.rs:118-144, 1450-1466)|**`name`**|—|
|76|`routines_validate`|"the SAME validator save/run use"; returns findings + disposition|`validate_routine` then `AuthoringDispositionDto::classify(&findings, name, **""**)` (mcp_ports.rs:4922-4931)|**`name`**. `ValidateResultDto{findings,disposition}` — **no `revision` field**|**seed (e) ROOT.** Validate returns no revision, and the empty-string revision makes `RemedyDto::set_attended` **omit** `expected_revision` (ports.rs:1610-1620). The advertised loop (validate → apply remedy with `expected_revision`) has no revision source; the agent must round-trip `routines_get`|
|77|`routines_save`|Whole-def save; "NEVER refused by validation findings"; `def` **or** `def_json`|`resolve_save_def` → `save_routine_checked(state, def_json, expected_revision)`; conflict → `[REVISION_CONFLICT] expected revision X but "r" is at Y` (routines/commands.rs:730-738)|`def` (object, string-of-object tolerated), `def_json` (deprecated string), `expected_revision`. Exactly one of def/def_json|**seed (e) part 2.** Revision is a **content hash**, not a counter: two different edits producing identical bytes yield the same token, and a no-op yields the *same* token. `EditResultDto.applied` is literally `revision != current_rev` (routines/commands.rs:912) — an agent modeling revisions as monotonic integers ("did my rev go up?") reads a legitimate idempotent save as a failure. Nothing in any description says the token is content-addressed|
|78-83|`routines_step_add` / `_update` / `_remove` / `_move` / `routines_track_add` / `_remove`|Fragment edits; routine must be DISABLED; saved even with error findings|all → `RoutinesPort::edit(RoutineEditRequestDto{routine, expected_revision, op})` (router.rs:1516-1659)|**`routine`** + verb-specific (**`step`** obj / **`step_id`** + **`patch`** obj / **`step_id`** / **`track`**), flattened `StepPlacementParams{track, after_step_id, branch_step_id, branch_arm, branch_after_step_id}`, `expected_revision`|AMBIG — "exactly one placement" is prose-only; the schema exposes 5 sibling optional fields with no `oneOf`. Composite params get a one-shot string→JSON coercion at the boundary that is **deliberately not advertised** (`arg_shape.rs:41-47`) — IMPLICIT|
|84|`routines_trigger_set`|Replace trigger list wholesale|edit(TriggerSet)|**`routine`**, **`triggers`** (array), `expected_revision`|—|
|85|`routines_meta_set`|Patch transmit_mode/on_interrupted/inputs|edit(MetaSet)|**`routine`**, **`patch`** (obj), `expected_revision`|—|
|86|`routines_rename`|Transactional rename incl. caller call-steps|`rename_routine` (mcp_ports.rs:4998-5018)|**`routine`**, **`new_name`**, `expected_revision`. `RenameResultDto{routine,revision,enabled,callers_updated}`|—|
|87|`routines_enable`|Enable; blocked-by-error returns a **result**, not an error|`set_enabled(name,true)` → `EnableResultDto{enabled,blocked,findings}`|**`name`**|— (documented)|
|88|`routines_disable`|Disable; never blocked|`set_enabled(name,false)`|**`name`**|—|
|89|`routines_run`|Real run; refused if blocked or unacked-automatic; returns run id|`run_routine(state,name,args)`; bad `args_json` → `Refused` (mcp_ports.rs:5028-5035); refusal surfaced **verbatim**, no remedy text (router.rs:154-160)|**`name`**, `args_json` (string, default `"{}"`)|AMBIG — `args_json` is a JSON-**string** while sibling verbs take real objects; the boundary coercion (`arg_shape`) deliberately excludes it|
|90|`routines_dry_run`|Fake-world rehearsal; "refused by NOTHING"|`dry_run_routine` → engine fake-world entry|**`name`**, `args_json`, `script_json` (both strings)|—|
|91|`routines_run_status`|Fast in-memory run state; **"Returns null for an unknown run id"**|`run_status(run_id)` → `Option<RunStatusDto>` → serialized as `null`|**`run_id`**|SILENT (documented) — still ok-null on a bogus id|
|92|`routines_journal_get`|Full verbatim journal; **taints**|`journal_get(run_id)` then `guard.taint(RoutinesJournal)` (router.rs:1821-1836)|**`run_id`**. `Vec<serde_json::Value>` (untyped)|**TAINT MISMATCH** — guide + server instructions say four tools taint; this is the fifth. Also SILENT: unknown run id returns `[]`|

## Prompts — 3

Defined + rendered inline in `router.rs:2540-2716`.

| name | args | renders |
|---|---|---|
| `diagnose_my_connection` | `transport` (optional; ardop/vara/packet/telnet) | read-only walk; names `backend_status`, `modem_get_status`, `config_read`, `session_log_snapshot` — **the last of which taints**, unannounced in the prompt (router.rs:2636-2638) |
| `help_me_set_up` | **`device` (required)** — missing → `invalid_request` (router.rs:2650-2655) | device/PTT setup walk |
| `compose_an_ics_213` | `to`, `subject` (both optional) | ICS-213 collect + stage via `send_form`/`message_send` |

**Server `instructions`** (router.rs:2447-2457) names 4 tainting tools. **No pagination** on `list_resources`/`list_prompts` (`_request` params ignored, `with_all_items`).

## Ranked: the 15 worst actual-vs-claimed gaps

| # | gap | evidence |
|---|---|---|
|1|**`verify_cms_connection` fails for every caller.** Claims to "verify the **live** CMS connection"; actually builds a throwaway `NativeBackend` over a temp mailbox with **no active identity**, so `NoActiveIdentity` is returned even while `cms_connect` succeeds in the same session. Compounded: the impl short-circuits `Ok(())` under `cfg!(test)`/CI, so no test can ever catch it.|claim `router.rs:783`; wiring `mcp_ports.rs:1273`; ephemeral backend `wizard.rs:539-544`; test/CI short-circuit `wizard.rs:522-524`; error origin `winlink_backend.rs:1901` + `1590-1596`; open bug `tuxlink-sswik` (P2); forensics `dev/handoffs/2026-06-27-cedar-magnolia-crag-mcp-cold-acceptance.md:38-44`|
|2|**`rig_status` advertises a live rig read that does not exist.** Hardcodes `vfo_hz`/`mode`/`ptt` to `None` by design while the description narrates a transient rigctld read and fabricates a "serial busy" explanation for nulls.|claim `router.rs:257`; impl `mcp_ports.rs:459-479`|
|3|**`mailbox_list` returns ok-empty for any nonexistent folder slug** — and still taints the session for nothing.|`ui_commands.rs:551-556` → `user_folders.rs:93-121` → `native_mailbox.rs:1109-1115` → `native_mailbox.rs:1253-1256`; taint `router.rs:308-310`|
|4|**Taint docs are wrong: `routines_journal_get` taints but is listed nowhere** (server instructions + guide both say "four").|taint sites `router.rs:310, 331, 371, 589, 1834`; `tuxlink-security/src/lib.rs:64-66, 79`; `router.rs:2452-2454`; `agents-guide.md:153-155`|
|5|**The "read first" guide covers 57 of 92 tools while claiming the full surface** — the 20-tool routines subsystem appears zero times.|`agents-guide.md:3-4`; `content.rs:47`; registry diff|
|6|**`catalog_send_inquiry` stages inquiries for ids that do not exist** — a hallucinated id becomes a real queued transmission on next connect.|`mcp_ports.rs:2293-2301`; `catalog/commands.rs:39-70`; `catalog/composer.rs:47-61`|
|7|**`cms_connect` has zero params and also flushes the outbox** — endpoint invisible/unchoosable; "connect" undersells "transmit staged mail."|`router.rs:772-779`; `mcp_ports.rs:1234-1259`; `ui_commands.rs:2967-2991`; `docs/superpowers/specs/2026-07-01-elmer-agent-send-design.md:103`|
|8|**`position_status` has no fix age, no operator-vs-station distinction, and conflates privacy with fix** (`has_fix=false` on a live lock when broadcast is off; `grid` is the privacy-clamped broadcast locator, unstated).|DTO `ports.rs:516-520`; impl `mcp_ports.rs:405-431`|
|9|**Routines revision is a content hash presented as an opaque revision; `routines_validate` cannot produce one** — idempotent saves read as `applied:false`; the advertised validate→remedy loop has no revision source.|`routines/store.rs:36-52,194-201`; `routines/commands.rs:912`; `mcp_ports.rs:4922-4931`; `ports.rs:1610-1620,1853-1857`|
|10|**`solar_conditions` stamps `now` on never-updated shipped data** while telling the agent to judge freshness by `updated_at_ms`.|claim `router.rs:655`; impl `mcp_ports.rs:3330-3340`|
|11|**`grib_send_request` invents the request geometry** (silent ±5° box, 2×2 grid, empty times/params; no way to request otherwise).|`mcp_ports.rs:2312-2394`|
|12|**`user_folders_list` returns display names `mailbox_list` cannot consume**; no tool serves the slug.|`mcp_ports.rs:602-607` vs `ui_commands.rs:551-556`|
|13|**`find_peers` is arm-gated but denies in the wrong shape** (`internal_error`-class unavailable, not "not authorized"), so client denial classifiers miss it.|`mcp_ports.rs:3252-3260`; `router.rs:52-110`|
|14|**`point_at` hardcodes `outcome:"shown"`** regardless of the real ack.|`router.rs:1268-1270`; `mcp_ports.rs:3685-3698`|
|15|**Twin-tool asymmetries with no documented boundary**: `deny_unknown_fields` on `ardop_b2f_exchange` but not `vara_b2f_exchange`; `vara_open_session` prerequisite stated only on itself; `list_audio_devices` vs `ft8_list_audio_devices` disjoint DTO/id spaces.|`router.rs:2183` vs `2196`; `router.rs:874` vs `851`; `router.rs:517`/`ports.rs:281-286` vs `router.rs:1334`/`ports.rs:1470-1473`|

**Seed-defect verdicts:** (a) confirmed, root cause + open bug `tuxlink-sswik`. (b) confirmed, exact `Ok(Vec::new())` line. (c) confirmed + spec acknowledgement. (d) confirmed, worse than stated (privacy/fix conflation). (e) confirmed, reframed: content-addressed hash + hash-equality `applied` + validate structurally revision-less. (f) **half-confirmed**: no meta/list-tools tool exists, but a session-taint status tool DOES — `server_info` returns `tainted`+`taint_reason`+`armed_remaining_secs`; the real gap is discoverability.
