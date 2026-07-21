# WLE parity audit refresh (2026-07-20): the map, no longer the bar

> Date: 2026-07-20 · Agent: kingfisher-yew-swallow · bd: tuxlink-2j343
>
> **Purpose reframe (operator decree 2026-07-20):** WLE parity is no longer
> the 1.0.0 milestone. "I care that Tuxlink is good, useful software. Not
> everything in WLE was good or useful... from a purely functional
> perspective, we've wildly surpassed what WLE actually aims to do: provide
> email-like communications in variable infrastructure situations,
> reliably." This document is therefore a MAP of where Tuxlink stands
> relative to WLE, including the capability set WLE has no counterpart for,
> not a gap-closure order book. The candidate 1.0.0 gate is CMS acceptance,
> tracked in bd tuxlink-241bj (blocked by this audit).
>
> Inputs: the 2026-05-29 feature inventory
> (`2026-05-29-winlink-express-feature-inventory.md`), its 2026-06-02
> verification + closure-plan corrections (carried), and a four-agent
> grounded re-score of every row against origin/main on 2026-07-20. Every
> status cites evidence; nothing is scored from memory.

## 0. Scorecard

98 inventory rows re-scored (sections 1-12):

| Status | Count | Share |
|---|---|---|
| SURPASSED (Tuxlink does it better/more) | 27 | 28% |
| PARITY | 31 | 32% |
| PARTIAL | 8 | 8% |
| SKIPPED-BY-DESIGN (recorded decisions) | 8 | 8% |
| ABSENT | 24 | 24% |

Parity-or-better: **58/98 (59%)**; with partials, 66/98. In May the
majority of these rows were unbuilt. Separately, section 13 below maps
nine capability families with **no WLE counterpart at all** - the
surpassed set the original inventory had no column for.

Status vocabulary: SURPASSED / PARITY / PARTIAL / ABSENT /
SKIPPED-BY-DESIGN (with the recording doc cited).

## 1. Core messaging (17 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 1.1 | Send plain-text message | SURPASSED | `src-tauri/src/winlink/compose.rs`, `src/compose/Compose.tsx` | Native Rust B2F outbound, multi-recipient + attachments + CMS 120KB gating |
| 1.2 | Receive plain-text | PARITY | `src-tauri/src/winlink/message.rs`, `src/mailbox/MessageView.tsx` | Adds body sanitize + form detection |
| 1.3 | Reply / Reply All | PARITY | `src/mailbox/replyActions.ts` | Wired to compose window via draft seam |
| 1.4 | Forward (with edit) | PARITY | `src/mailbox/replyActions.ts` | Attachments NOT carried into a forward (noted in source) |
| 1.5 | Forward without change | ABSENT | - | No unchanged-reroute mode |
| 1.6 | Send/receive attachments | SURPASSED | `src/compose/useAttachments.ts`, AttachmentStrip | Compose-side attach shipped since May + save-as native dialog |
| 1.7 | Image auto-resize | SURPASSED | `src/compose/attachmentFormat.ts` | Resize presets + jpeg/webp transcode + airtime estimate |
| 1.8 | Acknowledge receipt | ABSENT | - | EmComm-relevant gap remains |
| 1.9 | Spell check | ABSENT | - | No hunspell/spellcheck anywhere |
| 1.10 | Save message as file | ABSENT | - | Per-attachment save exists; whole-message export does not |
| 1.11 | Print message | PARTIAL | `src/mailbox/MessageView.tsx` | Print CSS scaffolding present; NO Print button renders (stale header comment) |
| 1.12 | Drafts folder | PARITY | `src/mailbox/draftMailbox.ts` | |
| 1.13 | Custom folders | PARITY | `src-tauri/src/user_folders.rs` | Single-tier (WLE splits Personal/Global) |
| 1.14 | System folders | PARITY | `src-tauri/src/winlink_backend.rs` | Correction to May audit: NOT "same 7" - Tuxlink lacks Saved/Read, adds Archive + Drafts |
| 1.15 | Find/search messages | SURPASSED | `src/search/`, `src-tauri/src/search/` | Operator query grammar + saved searches + backend index |
| 1.16 | Archive messages | PARITY | `src-tauri/src/native_mailbox.rs` | |
| 1.17 | Bulk export/import (.mbo) | ABSENT | - | No cross-install message move |

## 2. Transports (17 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 2.1 | Telnet to CMS | SURPASSED | `src-tauri/src/winlink/telnet.rs`, `cms_health.rs` | TLS + secure login + CMS health + inbound selection |
| 2.2 | Telnet P2P | SURPASSED | `telnet_p2p.rs`, `telnet_listen.rs` | Dial AND inbound listener (keyring station password + allowlist; allow-all defaults FALSE) |
| 2.3 | Packet (AX.25) to CMS | SURPASSED | `src-tauri/src/winlink/ax25/`, managed Dire Wolf | Native AX.25 stack |
| 2.4 | Packet P2P | SURPASSED | `packet_listen`, `listener/packet_gate.rs` | WLE Packet-P2P is outbound-only; Tuxlink adds the listener |
| 2.5 | Pactor to CMS | SKIPPED-BY-DESIGN | closure plan §2 row 2.5 | Commercial SCS hardware, no OSS modem; defer indefinitely |
| 2.6 | ARDOP to CMS | SURPASSED | `src-tauri/src/winlink/modem/ardop/` | Native engine + propagation-aware QSY/CAT tune |
| 2.7 | ARDOP P2P | SURPASSED | `ardop/listener.rs` | WLE has NO ARDOP listener; Tuxlink does, with allowlist gate |
| 2.8 | VARA HF to CMS | PARITY | `src-tauri/src/winlink/modem/vara/` | Full CMS exchange (was wire-only in May) |
| 2.9 | VARA FM to CMS | PARITY | same module, `connectDispatch.ts` | Digipeater VIA path supported |
| 2.10 | Robust Packet to CMS | SKIPPED-BY-DESIGN | closure plan row 2.10 | Upstream sunset ~July 2026 (hardcoded in WLE source) |
| 2.11 | Iridium GO to CMS | SKIPPED-BY-DESIGN | verification doc §2.11 | Niche commercial sat hardware |
| 2.12 | AREDN MESH to CMS | PARITY | `sessionTypes.ts` network-po | Shipped as Network Post Office dial |
| 2.13 | Radio-only / relay client | SURPASSED | `relay_banner.rs`, sessionTypes | C/R/L routing flags + banner-phrase parser |
| 2.14 | Post Office mode (BE the hub) | ABSENT | - | Client dial exists; hub-server role not built (see §9) |
| 2.15 | Best-channel selection (HF) | SURPASSED | `src-tauri/src/propagation/`, Station Intelligence | Offline VOACAP + antenna patterns exceeds dvoa.dll |
| 2.16 | Auto-poll / scheduled connect | SURPASSED | `src-tauri/src/routines/scheduler.rs` | General automation engine, superset of AutoConnect |
| 2.17 | Multi-leg connect script | SKIPPED-BY-DESIGN | closure plan §1.2 | Real feature is Packet-only digipeater chaining (deferred post-v0.5); the imagined cross-transport version never existed in WLE |

## 3. Position & GPS (5 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 3.1 | Manual grid entry | SURPASSED | `src/shell/GridEdit.tsx`, `src/map/GridPicker.tsx` | Inline ribbon edit + map grid-picker |
| 3.2 | GPS auto-fetch (NMEA) | PARITY | `src-tauri/src/position/gpsd.rs` + fix-it helper | gpsd subsystem + probe |
| 3.3 | Send position report | PARITY | `forms/templates/position.rs`, `PositionFormV2.tsx` | Was a "missing piece" in May |
| 3.4 | Privacy / precision controls | SURPASSED | `src-tauri/src/position/mod.rs` | 4-char broadcast default; on-air vs UI locator split |
| 3.5 | Periodic auto-position | PARTIAL | routines scheduler | Schedulable in principle; no verified position-report preset |

## 4. Structured forms (11 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 4.1 | Render inbound form | SURPASSED | `WebviewFormViewer.tsx`, `forms/http_server.rs`, native ICS views | HTML view-template AND native structured views |
| 4.2 | Author a new form | PARITY | `FormPicker.tsx`, `forms/serialize.rs` | Native ICS-213/Position/CheckIn/Bulletin/Damage + generic webview autofill |
| 4.3 | Community forms catalog | PARITY | `src-tauri/resources/wle-forms/Standard_Forms/` | Full WLE set vendored with SHA256SUMS |
| 4.4 | Auto-update form catalog | PARITY | `src-tauri/src/forms/updater.rs` | Staged atomic install |
| 4.5 | Import custom form | PARITY | `src-tauri/src/forms/import.rs` | Validate-before-write side-load |
| 4.6 | Generate ICS-309 log | SURPASSED | `forms/templates/ics309.rs`, CSV export | Date-range comms-log builder |
| 4.7 | Form-data report/aggregator | ABSENT | - | No cross-form aggregation |
| 4.8 | Map of form-data origins | ABSENT | - | Same aggregation gap |
| 4.9 | Export form data CSV/KML | PARTIAL | ICS-309 CSV only | No KML, no general extractor |
| 4.10 | Private boilerplate templates | ABSENT | - | |
| 4.11 | Favorite-templates quick-pick | ABSENT | - | (station favorites are unrelated) |

## 5. Data products (6 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 5.1 | Catalog product request | PARITY | `src-tauri/src/catalog/composer.rs`, Request Center | Templated query builder |
| 5.2 | GRIB request | PARITY | `src-tauri/src/grib/composer.rs`, map bbox select | Saildocs |
| 5.3 | Display catalog/GRIB responses | PARITY | `catalog/reply.rs` (structured NWS render), GRIB external-viewer handoff | Reply rendering exceeds WLE; GRIB handoff matches WLE by design |
| 5.4 | Weatherfax viewer prompt | ABSENT | - | No fax-attachment detection |
| 5.5 | HF propagation forecast | SURPASSED | `src-tauri/src/propagation/engine.rs` (voacapl) | Full offline VOACAP + antenna patterns + band matrix |
| 5.6 | Propagation data refresh | SURPASSED | `solar_update.rs`, `src-tauri/src/wwv_offair/` | SSN/solar refresh + novel WWV OFF-AIR decode |

## 6. Maps & visualization (5 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 6.1 | RMS gateway map | SURPASSED | `StationFinderMap.tsx`, gateway layers | Embedded Leaflet with reachability-tier pins |
| 6.2 | Per-message origin map | PARITY | `PositionMapWidget.tsx` | |
| 6.3 | Form-data origins map | ABSENT | - | Same §4.8 aggregation gap |
| 6.4 | Map provider config | SURPASSED | `src-tauri/src/basemap/` (self-hosted vector OSM + offline packs + LAN raster) | Commercial providers SKIPPED-BY-DESIGN (self-hosted basemap design doc) |
| 6.5 | Region picker | PARITY | `src/map/gribRegion.ts`, GridPicker | |

## 7. Address book & organization (6 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 7.1 | Address book | SURPASSED | `src-tauri/src/contacts/`, ContactsPanel | Tactical field, notes, reachability tiers, observation-derived suggestions |
| 7.2 | Groups / distribution lists | PARITY | contacts `group_upsert`, user guide 34 | Live member resolution at send time |
| 7.3 | Import/export contacts (CSV) | ABSENT | - | |
| 7.4 | Recent-callsign quick-pick | PARTIAL | `contacts_recent_gateways`, suggestions | No dedicated call-history picker |
| 7.5 | Multi-callsign install | SURPASSED | `src-tauri/src/identity/` | Full + tactical identities, keyring-backed, CMS-verified |
| 7.6 | Auxiliary/served-agency callsign | PARITY | TacticalIdentity | |

## 8. Encryption (1 row)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 8.1 | AES message encryption | SKIPPED-BY-DESIGN | pitfalls RADIO-2 (2026-05-17) | Deliberately gated behind the Part 97 evaluation, not omitted by accident |

## 9. Hub / network roles (7 rows)

WLE's §9 rows are "run AS a hub." Tuxlink shipped the inverse client modes
(connect TO a Post Office / relay / mesh, plus P2P listeners). Scored
against the row as written, client-mode shipment noted.

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 9.1 | Run AS Post Office hub | ABSENT | client dial shipped (`telnet_post_office_connect`) | Hub-server role unbuilt |
| 9.2 | Run AS radio-only/relay hub | ABSENT | client session shipped | Hub role unbuilt |
| 9.3 | Hybrid network parameters | PARTIAL | pool-R routing flags, RelayFavorite | No MPS/propagation-list config surface |
| 9.4 | MPS list | ABSENT | - | |
| 9.5 | AREDN mesh services discovery | SURPASSED | `src-tauri/src/mesh/mod.rs` + design doc | sysinfo.json discovery + liveness probe + RTT ranking |
| 9.6 | P2P-Telnet allowed stations | PARITY | `listener/allowed_stations.rs` + station password | |
| 9.7 | Known relay stations | PARITY | RelayFavorite persistence (corrected framing per closure plan) | |

## 10. Diagnostics & operations (7 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 10.1 | Session log | SURPASSED | `session_log.rs`, redaction boundary | Live log + export redaction rules |
| 10.2 | Background-task viewer | ABSENT | - | |
| 10.3 | Usage statistics | ABSENT | - | |
| 10.4 | Log files viewer | SURPASSED | `src/help/LoggingView.tsx`, `src-tauri/src/logging/` | History, retention, redacted export, env probes |
| 10.5 | Backup/restore database | ABSENT | - | No-SQLite design (ADR 0003); config atomic-write is not backup |
| 10.6 | Restore backup .INI on corruption | PARTIAL | `config.rs` preserves corrupt file | No auto-restore |
| 10.7 | Confirm-before-connect (REFRAMED) | PARTIAL | `relay_banner.rs` | Post-connect relay continuation prompt (corrected framing); banner parsed, continuation prompt UNVERIFIED |

## 11. Lifecycle & integrity (11 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 11.1 | Self-update binary | SKIPPED-BY-DESIGN | ADR 0020, release workflows | Distro packaging owns updates |
| 11.2 | Patch installer | SKIPPED-BY-DESIGN | same | |
| 11.3 | Forms catalog update | PARITY | `forms/updater.rs` | Integrity-checked atomic swap + rollback |
| 11.4 | Mesh node list refresh | PARITY | `mesh_discover_post_offices` | On-demand re-discovery; no persisted list by design |
| 11.5 | Registration nag | SKIPPED-BY-DESIGN | - | No registration server to nag for |
| 11.6 | Change CMS password (online) | PARITY | `cms_account.rs`, wizard UI | |
| 11.7 | Change CMS password (offline) | ABSENT | - | No next-connection password stage |
| 11.8 | Password recovery email | ABSENT | - | |
| 11.9 | License display | PARITY | LICENSE + AboutDialog | |
| 11.10 | Revision history | PARITY | CHANGELOG.md (release-please) | |
| 11.11 | About box | PARITY | AboutDialog | |

## 12. UI/UX (5 rows)

| § | WLE capability | Status | Evidence | Note |
|---|---|---|---|---|
| 12.1 | Color theme | SURPASSED | `colorScheme.ts` (8 presets incl. night/tactical red) + ThemeDesigner | vs WLE's single SetColors |
| 12.2 | Text font setting | ABSENT | Help-only text-size control | OS default elsewhere |
| 12.3 | List font setting | ABSENT | - | |
| 12.4 | Message-editor defaults | ABSENT | - | |
| 12.5 | Notification preferences | PARTIAL | `dock/park_notify.rs` | Event-specific desktop notifications; no preference surface |

## 13. The surpassed set: Tuxlink capabilities with no WLE counterpart

This is the map the original inventory had no column for. Maturity is
scored honestly per code-exists-is-not-functional; caveats stated.

| Capability | Maturity | Evidence | Note |
|---|---|---|---|
| Elmer: fully-local AI assistant that OPERATES the station | SHIPPED | `src-tauri/src/elmer/`, `src/elmer/`, docs/ELMER.md | Same tool router as external agents; dial/retry/QSY; docs-grounded answers; provider setup (Ollama/OpenAI-compat/Anthropic) w/ SSRF guards; pop-out (PR #1210) |
| Routines: automation engine | SHIPPED (active post-ship defect cycle) | `src-tauri/src/routines/`, `tuxlink-routines/` | ~18-action catalog; full control flow (branch w/ comparisons, retry, delay, call, end); scheduler w/ anacron catch-up; Part-97-modeled consent (Attended vs Automatic); run journal/History; Run Artifact export; import; designer UI |
| Agent-native MCP surface | SHIPPED transport/router; completeness IN-FLIGHT | `tuxlink-mcp-core` (79 tools), `tuxlink-mcp` UDS shim, `tuxlink-mcp-testserver` | Egress/taint security core (operator-armed egress, injection taint re-locks send); ADR 0025 still Proposed; tool-shape remediation (to358) open |
| Station Intelligence | SHIPPED (active usability defect cycle) | `src-tauri/src/propagation/`, `src/catalog/` | VOACAP-modeled finder + FT-8 live-decode evidence corroboration + band matrix + bearing/aim + map layers; 5th poppable surface |
| Multi-window dock system | SHIPPED | `src/dock/`, `src-tauri/src/dock/` | 5 poppable surfaces, state carry, consent-host model, park-not-discard. (README prose lags: names only 3) |
| APRS Tac Chat + positions map + telemetry/WX | SHIPPED | `src/aprs/`, `src-tauri/src/winlink/aprs/` | Chat w/ delivery acks; positions map w/ sprites + digipeat-path animation; telemetry + WX sitrep |
| FT-8 receive integration | SHIPPED | `src-tauri/src/ft8/`, `tuxlink-jt9`, `src/ft8ui/` | jt9-based decode, waterfall, band strip w/ CAT sweep; RX-only by design |
| WWV off-air space weather | SHIPPED | `src-tauri/src/wwv_offair/` | Decodes A/K-index + solar flux from receiver audio, no internet; routines action `data.spacewx_wwv` |
| Modern platform | SHIPPED | workflows, `config.rs`, `src/search/`, themes | deb/rpm/AppImage x86_64+arm64 + ECT low-floor .deb; release-please; config schema v9 migrations; 8 themes + designer; message search + docs index |
| Native UV-Pro Bluetooth control | SHIPPED | `src-tauri/src/winlink/ax25/uvpro/` | Direct RFCOMM/GAIA + KISS, no cable/TNC |
| Managed Dire Wolf | SHIPPED | `managed_direwolf.rs` | App-owned soundcard TNC lifecycle |
| VARA provisioning wizard | PARTIAL | `src/radio/VaraProvision.tsx` | Wine install scripted; audio/CAT provisioning still manual |
| Self-hosted offline basemap | SHIPPED | `src-tauri/src/basemap/`, `src-tauri/src/tiles/` | Vector OSM + offline region packs + LAN raster |
| Identity system (FULL + tactical) | SHIPPED | `src-tauri/src/identity/` | Keyring-backed, CMS verify |
| Diagnostics + Report Issue | SHIPPED | `src-tauri/src/logging/`, ReportIssueModal | Redacted .tar.zst export, env probes, retention, disk guard |
| Onboarding wizard + tour + in-app Help | SHIPPED | `src/wizard/`, `src/onboarding/`, `src/help/` (39 user-guide topics, searchable) | |
| Uninstall cleanup | SHIPPED | `uninstall_cleanup.rs` | |
| Contacts reachability/suggestions + P2P peers | SHIPPED | `src-tauri/src/contacts/reachability.rs`, `src/peers/` | |
| SSTV / media transport foundation | PARTIAL / IN-FLIGHT | `src-tauri/src/media/` | Foundation only |

## 14. Residual ABSENT rows, grouped (dispositions are NOT decided here)

For the operator's next planning pass; this audit maps, it does not decide.

- **Form-data aggregation cluster** (4.7, 4.8, 6.3, 4.9-KML): the one
  coherent WLE capability family with no Tuxlink counterpart. EmComm
  data-aggregation value; relates the forms EPIC (tuxlink-zkuk).
- **Messaging conveniences** (1.5 forward-unchanged, 1.8 receipt ack, 1.9
  spellcheck, 1.10 save-as-file, 1.11 print button, 1.17 bulk export):
  small, independent; 1.8 is the most EmComm-relevant.
- **Hub roles** (2.14/9.1, 9.2, 9.4, 9.3-full): running AS infrastructure.
  The May inventory already recommended dropping hub-mode from client
  scope.
- **Account edge flows** (11.7 offline password change, 11.8 recovery
  email): CMS-relationship dependent; natural to revisit alongside the CMS
  acceptance work (tuxlink-241bj).
- **Cosmetic/preferences** (12.2-12.4 fonts/editor defaults, 12.5
  notification prefs, 7.3 contacts CSV, 7.4 call-history picker, 10.2
  task viewer, 10.3 usage stats, 10.6 auto-restore, 5.4 weatherfax
  prompt, 3.5 position-report preset).

## 15. Incidental findings (filed or flagged)

- §1.11: stale header comment advertises a Print action; no button renders.
- README prose names 3 poppable surfaces; the registry ships 5.
- §10.7's continuation prompt is UNVERIFIED beyond banner parsing.
- §3.5: no verified periodic position-report preset despite the scheduler
  supporting it.

## 16. What this feeds

bd tuxlink-241bj: the 1.0.0 gate decision (CMS acceptance as candidate).
This audit establishes that strict WLE parity is neither achieved nor the
point: the functional aim (reliable email-like comms over variable
infrastructure) is met and exceeded, the deltas are mapped, and the
remaining ABSENT set is dominated by hub roles and conveniences rather
than core capability. VERSIONING.md's milestone sentence changes only when
241bj lands.
