# r5jsj Appendix B — agents-guide / resources vs the real surface

Provenance: read-only survey pass over `docs/mcp-knowledge/`, `docs/user-guide/`,
`src-tauri/tuxlink-mcp-core/src/content.rs` + `router.rs`, and
`src-tauri/src/search/docs_bundle.rs` at main `ed9e3bc2`, 2026-08-10, session
moss-tamarack-taiga (subagent pass B). Condensed agent report; the curated
deliverable is `2026-08-10-mcp-tool-surface-gap-register.md`.

## Resources: 22 `tuxlink://` URIs

Registered in `content.rs:41-199` as a static `CATALOG`, each an `include_str!`
of a repo `docs/` file; served by `router.rs:2467/2475` (`knowledge_resources`
/ `knowledge_read`); unknown URI → `invalid_request`; MIME forced
`text/markdown`. 8 are `docs/mcp-knowledge/*` (agents-guide, glossary
supplement, 2 playbooks, device-uv-pro, band-plan, modem-capability-matrix,
local-agent-deployment), 12 are `docs/user-guide/*` re-exposures, plus
vara-wine-setup and audio-setup playbooks. Prompts: 3
(`diagnose_my_connection`, `help_me_set_up`, `compose_an_ics_213`),
router.rs:2545+.

## Coverage diff: guide documents 56–57 of 92 registered tools

Registered set: 92 (`#[tool(name=…)]` in router.rs; matches
`docs/parity/parity-manifest.json:1262` tool_budget, CI-enforced
`parity_check.rs:230`). The guide (`docs/mcp-knowledge/agents-guide.md`, 219
lines, 8 tiers) never mentions **36 tools (39%)**:

`config_get_rig docs_read export_report find_peers list_audio_devices point_at
print_document printer_list rig_status rig_tune routines_actions_list
routines_disable routines_dry_run routines_enable routines_get
routines_journal_get routines_list routines_meta_set routines_rename
routines_run routines_run_status routines_save routines_step_add
routines_step_move routines_step_remove routines_step_update
routines_track_add routines_track_remove routines_trigger_set
routines_validate vara_engine_available vara_ini_apply vara_ini_read
vara_install_start vara_install_status vara_open_session`

Highlights of why omissions bite:
- The **entire 20-tool routines tier** is invisible ("routine" occurs zero
  times in the guide) — including the tainting `routines_journal_get`.
- `docs_read` unmentioned: the guide teaches `docs_search` but not that the
  snippet "is NOT enough to answer from" and `docs_read` completes the flow.
- The guide advertises only the DEPRECATED alias `ardop_list_audio_devices`
  (router.rs:526 deprecation note, tuxlink-hq9g0) — never `list_audio_devices`.
- Arm-gated tools missing from the gated lists: `rig_tune`,
  `vara_open_session`, `vara_ini_apply`.
- Ungated writes with no taxonomy slot: `export_report`, `print_document` —
  the guide's tier model implies writes need arm; these don't.
- Phantom tools: **none** — every guide name resolves to a registered tool.

## Guidance that mis-describes behavior

1. **"These four … TAINT the session" — five do.** Guide line 41 + server
   `instructions` (router.rs:2452-2455) enumerate 4; code taints at
   router.rs:310, 331, 371, 589 AND **1834** (`routines_journal_get`,
   `TaintReason::RoutinesJournal`, tuxlink-security/src/lib.rs:64-66). Closed
   lists asserting completeness, both wrong.
2. **"Station intelligence: reads, no taint, no authorization" — `find_peers`
   requires the arm** (mcp_ports.rs:3252-3260, the ONLY port-level arm-gated
   read). Guide line 24 also enumerates the tier as exactly
   `find_stations`/`predict_path` — actively asserts completeness. No tier
   concept exists for "read that requires arm."
3. **Write tiers presented as exhaustive, aren't** (see omissions above) — an
   agent under-predicts what it can do disarmed and over-predicts what needs
   arming.
4. Internal comment drift: router.rs:1346 says "10 tools" for the routines
   block; 20 are registered.
5. **No drift gate exists**: the only guide test
   (router.rs:4290) checks the resource is present/readable; `parity_check`
   gates tool COUNT (92) rigorously; nothing ties guide coverage or
   instruction content to the registry. Drift is unguarded by construction.

## Arm/taint model vs code (what checks out)

- No arm tool exists among the 92; `EgressAuthority::Agent` checks taint first
  (tuxlink-security/src/lib.rs:96-97). ✅ guide claim "agent cannot arm itself".
- Re-arm-clears-taint is real and the ONLY sanctioned path:
  `rearm_clearing_taint` (lib.rs:176-186) via `egress_rearm`
  (src-tauri/src/lib.rs:3557). ✅ matches guide lines 157-162.
- Abort tier never gated (router.rs:908). ✅

## Docs corpus (`docs_search`/`docs_read`) — 51 topics, 3 sources

`src-tauri/src/search/docs_bundle.rs` `BUNDLED_TOPICS`: 39 user-guide topics,
2 knowledge topics (pat-winlink, winlink-express), 10 mcp-knowledge topics
(including agents-guide itself, slug `agents-guide`). Membership is test-gated
(`docs_registry_test.rs`); accuracy is not.

### Corpus drift findings

1. **`35-agent-mcp.md` contradicts the guide AND the code on taint recovery**:
   lines 41-42, 51 say taint clears only by **restarting the application**;
   code + guide + `server_info` description say operator **re-arm**. Two
   shipped docs give mutually exclusive remedies for the most consequential
   denial an agent hits.
2. `35-agent-mcp.md:70-89` is the corpus's only per-tool inventory and covers
   only VARA (9 tools) — naming five tools the agents guide omits. The two
   docs' coverage sets are disjoint in both directions; neither is complete.
   Line 19 ("always available" reads) collides with arm-gated `find_peers`;
   line 21 repeats the four-taint error.
3. **`23-catalog-requests.md:159-178` — the NWS/Saildocs steering** (the field
   incident's actual locus; the agents guide has no Request Center guidance at
   all). It claims weather is never a catalog inquiry; shipped code contradicts
   it: `src-tauri/src/catalog/reply.rs` (NWS area-weather INQUIRY replies,
   tuxlink-qyjr), `src/request/nws-zone-to-catalog.json` (NWS-zone →
   catalog-filename table), fixtures `reply-area-weather-nws.txt` /
   `reply-sft-tabular-abq.txt`, and `docs/plans/2026-06-09-request-center-plan.md:108`
   ("of 51 `WX_US_<ST>` buckets … 28 are tabular-only"). `catalog_list`'s own
   description (router.rs:441) explicitly counter-steers ("NOT limited to
   weather/GRIB"). Net effect: an agent following the corpus routes NWS
   state/zone forecasts to `grib_send_request`/Saildocs instead of
   `catalog_send_inquiry`.
4. Corpus silence: no user-guide topic documents `routines_*` over MCP
   (`39-routines-actions.md` never mentions MCP/agent), and `35-agent-mcp.md`
   points agents at the guide as authoritative — propagating all of the above.
