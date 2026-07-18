# Routines round 2: compatibility tree (distillation scenarios vs. Routines actions)

**Status:** analysis complete; the ranked missing-action list below is the round-2
functional requirements set.
**Method (operator-approved, 2026-07-18):** decompose every distillation scenario
cell into steps, map each step to (routines action | agent tool | MISSING), and
let the vetted scenario corpus, not guesswork, produce the requirements.
**Companion ADR:** [ADR 0024: dual actionability over one capability tree](../../adr/0024-dual-actionability-one-capability-tree.md) (Proposed).

## 1. The three surfaces

| Surface | Size | Source of truth |
|---|---|---|
| Distillation scenario corpus | 24 cells (4 families x 3 depths x 2 taints), 11 distinct required tools | `dev/elmer-distill/src/elmer_distill/scenariogen.py` (R2 checkout) + `scenario.py` SuccessSpecs |
| Agent tool surface | 50 MCP tools | `dev/elmer-distill/reference/tools.json`, classified in `tool_surface.py` (taint / egress / tier2-write / staging / stop / read) |
| Routines action surface | 17 actions (`rig.*` x5, `data.*` x4, `local.*` x5, `radio.*` x3) + control flow | `src-tauri/src/routines/actions/` on `origin/main`; catalog in the routines design spec §6 |

The corpus families: `radio_debug` (modem triage, config fix, reconnect),
`emcomm` (position, gateway finding, report staging, CMS send), `helpdesk`
(docs search, config sanity, version/authority report), `blended` (radio_debug +
emcomm chained). Depth adds steps; `pre_tainted` cells drop egress and tier-2
write steps from the requirement (a tainted session must refuse them), which is
why gated tools appear in fewer cells than their family suggests.

## 2. Scenario-step coverage matrix

Each row is one distinct step (agent tool) required somewhere in the corpus.
"Cells" counts the (family, depth, taint) cells whose SuccessSpec requires it,
out of 24.

| Step (agent tool) | Cells | Routines mapping | Verdict |
|---|---|---|---|
| `modem_get_status` | 12 | none; `rig.read_state` is CAT state, not modem state | **MISSING** |
| `position_status` | 12 | `data.read` `source=grid` | COVERED |
| `find_stations` | 10 | none; `data.stationlist_update` refreshes the cache but nothing queries/filters/sorts it into the run | **MISSING** |
| `message_send` | 8 | `local.compose` (template + vars, stages to outbox) | COVERED |
| `config_get_ardop` | 6 | none; `data.read` has no config source | **MISSING** |
| `docs_search` | 6 | none | **MISSING** |
| `config_read` | 4 | `data.read` `source=grid` covers the grid field only (no CMS flag, transport, host, callsign) | PARTIAL |
| `config_set_ardop` | 3 | none; `rig.apply_preset` writes CAT presets, not modem config | **MISSING** |
| `cms_connect` | 2 | `radio.connect` (folds connect + forward staged outbox into one action) | COVERED |
| `server_info` | 2 | none (version + live egress-authority state) | **MISSING** |
| `ardop_connect` | 1 | `radio.connect` (station x band walk is a superset of single-target connect) | COVERED |

## 3. Cell-level result: 0 of 24

A cell is human-actionable via Routines only if every required step maps to an
action. Today that is **zero cells**:

- All 6 `radio_debug` cells block on `modem_get_status` + `config_get_ardop` at depth 2.
- All 6 `emcomm` cells block on `find_stations` at depth 2.
- All 6 `helpdesk` cells block on `docs_search` at depth 2.
- All 6 `blended` cells block on `modem_get_status` at depth 2.

**Minimum unblock set:** four read actions (modem/app status, gateway-directory
query, config read, docs search) unblock **21 of 24 cells**. The remaining 3
(the clean depth>=4 `radio_debug` and depth-6 `blended` cells) additionally need
the first config **write** action (`config_set_ardop` equivalent), which brings
tier-2 consent semantics with it.

## 4. Ranked missing-action list (the round-2 requirements)

Ranked by scenario cells blocked; the tail entries come from the full surface
diff (§5) and were operator-flagged in bd `tuxlink-iizmk`.

| Rank | Missing capability | Cells blocked | Proposed shape |
|---|---|---|---|
| 1 | **Status reads**: modem status, backend/CMS status, app version + live egress-authority state | 14 (`modem_get_status` 12, `server_info` 2) | New `data.read` sources: `modem_status`, `backend_status`, `app_status`. Read-only, no capability flags. |
| 2 | **Gateway-directory query** (`find_stations`) | 10 | New `data.find_stations`: query the station list by transport/band, distance-sorted from own grid; outputs a stations array that feeds `radio.connect`'s `stations` param (the composability path proven by R3 "gateway-continuity"). Compose with `data.stationlist_update` for freshness. |
| 3 | **Config reads** (`config_get_ardop`, full `config_read`; by extension `config_get_vara`, `packet_config_get`, `config_get_rig`) | 10 | New `data.read` sources: `config` (curated non-secret top-level) and `modem_config` (per-modem). Same non-secret curation as the agent tools. |
| 4 | **Docs search** (`docs_search`) | 6 | New `data.docs_search`. App-owned content, read-only. Low mechanism risk; mostly useful so helpdesk-family flows are expressible as guided routines. |
| 5 | **Config writes** (`config_set_ardop`; by extension the tier-2 write family) | 3 (all clean cells) | First write-action family, e.g. `config.set_modem`. Must carry tier-2 semantics: consent-relevant capability flag, validator closure, journaled old->new values. Smallest honest slice: ARDOP drive level (the corpus step). |
| 6 | **Mailbox reads beyond inbox summary** (`message_read`, `user_folders_list`, `tauri_search_run`, per-folder `mailbox_list`) | 0 in corpus; operator-flagged | Extend `data.read` (per-folder summary, single message) once a consumer routine exists. |
| 7 | **Staging parity** (`send_form`, `grib_send_request`) | 0 in corpus; operator-flagged | Sibling actions to `local.compose_catalog_request`; same stage-then-connect model. |
| 8 | **`predict_path`** | 0 in corpus; operator-flagged | `data.predict_path`; pairs with `data.find_stations` for propagation-gated gateway choice (the R2 "propagation-gated band plan" proof scenario). |
| 9 | **Real `rig_tune`** (arbitrary freq/mode, not preset-bound) | 0 in corpus; operator-flagged | Either a `rig.tune` action or an inline-frequency form of `rig.apply_preset`. |
| 10 | **`mailbox_move`**, `verify_cms_connection`, remaining tier-2 writes | 0 in corpus | With the rank-5 write-family plumbing in place these are additive registry entries. |

## 5. Full surface diff (50 agent tools -> routines), for the ADR appendix

Disposition counts: **8 covered, 7 partial, 29 missing, 6 not-routine-shaped.**

- **Covered (8):** `position_status`, `solar_conditions` (`data.read`), `rig_status` (`rig.read_state`), `message_send` (`local.compose`), `catalog_send_inquiry` (`local.compose_catalog_request`), `cms_connect` / `ardop_connect` (`radio.connect`), `mailbox_list` for the inbox case (`data.read` `inbox_summary`; per-folder is partial).
- **Partial (7):** `config_read` (grid only), `config_get_rig` (live CAT state exists via `rig.read_state`, configured-rig read does not), `rig_tune` (preset-bound only), `ardop_b2f_exchange` / `vara_b2f_exchange` / `packet_connect` (`radio.connect` covers the CMS-forwarding intent; per-intent routing (p2p, radio-only, post-office) and digipeater paths are not parameterized), per-folder `mailbox_list`.
- **Missing (29):** the §4 list: 4 status reads, `find_stations`, `predict_path`, 4 config reads, `docs_search`, `catalog_list`, `session_log_snapshot`, `message_read`, `user_folders_list`, `tauri_search_run`, `p2p_peer_password_status`, `platform_info`, `verify_cms_connection`, all 9 tier-2 writes (`config_set_ardop`, `config_set_vara`, `packet_config_set`, `config_set_grid`, `position_set_source`, `config_set_privacy`, `packet_set_listen`, `mailbox_move`, `message_attachment_save`), `send_form`, `grib_send_request`.
- **Not routine-shaped (6):** `get_wizard_completed` (first-run UI state), the 3 device enumerations (design-time concerns; device choice belongs in config, not in a run), the stop tools `cms_abort` / `modem_ardop_disconnect` / `vara_stop_session` counted as one engine concern (run cancel + arbiter release already own stopping; a deliberate mid-routine disconnect step is a possible low-priority addition).

## 6. Reverse diff (routines actions the agent surface lacks)

Dual actionability cuts both ways. Ten routines actions have no agent-tool
counterpart: `radio.listen`, `radio.aprs_send`, `data.spacewx_wwv`,
`data.stationlist_update`, `local.set_identity`, `local.log`, `local.notify`,
`rig.validate_preset`, `rig.switch_vfo`, `rig.tune_atu`. Control flow (branch,
delay, retry, parallel, call) is agent-native and needs no tool. The reverse
gaps are not round-2 scope (the corpus does not yet exercise them) but the ADR
makes them visible so the next tool-surface revision closes them deliberately.
