# Tuxlink agent guide

Read this first. It explains what Tuxlink is, the full MCP tool surface organized
by tier, the arm/taint authorization model, and where the rest of the
documentation lives.

## What Tuxlink is

Tuxlink is a native Linux Winlink client for amateur-radio emergency
communications: one Tauri desktop application with a Rust Winlink B2F engine and a
React frontend. It connects to the Winlink network over Telnet, packet (AX.25),
ARDOP, and VARA HF. This MCP server is a control surface onto the **running app**:
its tools read live status, search the mailbox and docs, look up station
intelligence, stage outbound messages, and — only under operator authorization —
change configuration or transmit. Use it to help the operator diagnose, set up,
and operate their station.

## New to amateur radio?

If terms such as *WARC bands*, *RMS gateway*, *grid square* (Maidenhead locator),
or *B2F* are unfamiliar, read `tuxlink://glossary` and
`tuxlink://glossary-supplement` for vocabulary and `tuxlink://reference/band-plan`
for band and frequency conventions before planning a flow. The
station-intelligence tools (`find_stations`, `predict_path`) and the transport
guides assume this domain vocabulary; resolve it first rather than guessing.

## The MCP surface by tier

### Diagnostic reads — always available, redacted, no authorization

`server_info`, `backend_status`, `modem_get_status` (reports both what is
`running` and what the operator has `selected`), `vara_status` (includes cmd-port
`reachable`), `vara_probe` (deep read-only banner/VERSION check: down /
socket-not-vara / vara-ok), `position_status`, `platform_info`, `get_wizard_completed`,
`p2p_peer_password_status` (status only, never the password), `user_folders_list`,
`docs_search`, `catalog_list`, `config_read`, `config_get_ardop`,
`config_get_vara`, `packet_config_get`, the device enumerators
(`packet_list_serial_devices`, `packet_list_bluetooth_devices`,
`ardop_list_audio_devices`).

These four return **untrusted message/wire content and TAINT the session**:
`mailbox_list`, `message_read`, `tauri_search_run`, `session_log_snapshot`. Once
tainted, egress and writes are locked for the rest of the session; clearing the
taint requires the operator to **re-arm**, which starts a fresh authorized
session and **DISCARDS the current conversation** (a quarantine — not a resume).
A plain ARM does not clear taint. See the arm/taint model below.

### Station intelligence — reads, no taint, no authorization

- `find_stations` — Winlink RMS gateway directory (callsign, frequencies, grid),
  filterable by transport/band/history. Cached public data. Each gateway also carries
  `distance_km`, `distance_mi`, and `bearing_deg` from the operator's grid (null when the
  operator grid is unset — the result echoes it as `operator_grid`); gateways are sorted
  nearest-first, unknown-distance last.
- `predict_path` — offline VOACAP HF path reliability/SNR/MUF-day by UTC hour from
  the operator's own grid to a target grid across candidate dial frequencies.
- `solar_conditions` — the **stored** space-weather indices (SFI/A/K) and the
  sunspot number used in predictions. It reads a cached snapshot and fetches
  nothing, so the data may be old: check `source` and `updated_at_ms` before
  presenting it as current. A `source` of `bundled` means the values shipped with
  the app and have **never** been updated — never report those as today's
  conditions.
- `wwv_capture_offair` — refresh the stored indices by capturing the WWV time
  station's space-weather bulletin **over the operator's own radio**, with no
  internet. Receive-only (it tunes the radio and listens; it never transmits), so
  it needs no armed send-authority. Takes about a minute — it waits for the next
  bulletin. `no_copy: true` means audio was captured but the decode was not
  confident, and the stored indices were left unchanged.
- `wwv_offair_available` — whether off-air WWV capture is possible (it needs rig
  CAT control to tune the dial). Call this before `wwv_capture_offair`.

### Remediation writes — require armed send-authority AND un-tainted

`config_set_ardop`, `config_set_vara`, `packet_config_set`, `config_set_grid`,
`position_set_source`, `config_set_privacy`, `packet_set_listen`, `mailbox_move`,
`message_attachment_save`. These mutate config or local state. Malformed input is
rejected as invalid even when disarmed; the gated mutation only runs when armed and
un-tainted.

### Compose / queue — local, ungated, never transmits

`message_send`, `send_form`, `catalog_send_inquiry`, `grib_send_request`. These
stage a draft in the local outbox and return its message id. **No transmission
happens here** — staging is always allowed. Transmission waits for a later gated
connect.

### External egress — require armed send-authority AND un-tainted

`cms_connect`, `verify_cms_connection`, `ardop_connect`, `ardop_b2f_exchange`,
`vara_b2f_exchange`, `packet_connect`. These leave the box: they connect to the CMS
or key the transmitter. Denied unless armed and un-tainted.

### Abort — always allowed

`cms_abort`, `modem_ardop_disconnect`, `vara_stop_session`. Stopping is a safety
primitive and is never gated.

## The arm/taint model

This is the rule to internalize. The agent **cannot** transmit, connect, or write
config on its own.

- **Armed send-authority is operator-only.** The operator arms it in the app's GUI.
  The agent has no tool to arm itself. Egress and write tools are denied unless the
  operator has armed authority.
- **Reading untrusted content taints the session.** `mailbox_list`,
  `message_read`, `tauri_search_run`, and `session_log_snapshot` return content
  that may carry injected instructions. Calling any of them locks egress and writes
  for the rest of the session. The lock clears ONLY when the operator **re-arms**,
  which quarantines: it discards the current conversation and starts a fresh
  authorized session (a plain ARM does NOT clear taint). This contains prompt
  injection: an instruction read out of a message cannot be turned into a
  transmission — not even after a re-arm, because the re-arm drops the conversation
  that carried the instruction.
- **Compose stages; the gated connect transmits.** `message_send` / `send_form`
  build an outbox draft with no authorization. The message only leaves the station
  when the operator arms authority and a gated egress tool (e.g. `cms_connect`)
  runs.

A denied egress/write returns a clear `not authorized` error naming the cause.
That is expected behavior, not a bug — the denial no longer ends your turn, so
relay it and give the operator the **cause-specific** remedy:

- **Not armed / expired** — ask the operator to ARM send-authority. This preserves
  the conversation, so you can continue exactly where you left off. Never claim you
  sent anything.
- **Tainted** (you read untrusted content) — ask the operator to re-arm, but warn
  that re-arming **DISCARDS this conversation** (it is a quarantine — you will not
  be able to resume it), and that waiting for the arm timer does nothing. Never
  claim you sent anything.

## Typical flows

- **Diagnose a connection** — read `backend_status` / `modem_get_status` /
  `config_read`; for VARA, check `vara_status.reachable` then `vara_probe` to
  confirm a real VARA is answering the cmd port; then `session_log_snapshot` (taints), and consult
  `tuxlink://playbook/ardop-wont-connect` or
  `tuxlink://playbook/connection-troubleshooting`. Explain plainly; do not connect.
- **Find a gateway and predict bands** — `find_stations` to list reachable RMS
  gateways, then `predict_path` to the gateway's grid across candidate dials, with
  `solar_conditions` for context.
- **Compose and send** — `message_send` or `send_form` to stage the draft, then the
  operator arms send-authority and a gated `cms_connect` (or B2F exchange)
  transmits.

## Resources and prompts

Knowledge resources are served under `tuxlink://` URIs (read them with
`read_resource`): `tuxlink://glossary` and `tuxlink://glossary-supplement`;
playbooks `tuxlink://playbook/ardop-wont-connect`,
`tuxlink://playbook/connection-troubleshooting`,
`tuxlink://playbook/cms-z-password-lag`; device + transport guides
`tuxlink://device/uv-pro`, `tuxlink://guide/ptt`, `tuxlink://guide/ardop`,
`tuxlink://guide/vara`, `tuxlink://guide/packet`,
`tuxlink://guide/picking-a-transport`, `tuxlink://guide/emcomm-ics`; references
`tuxlink://reference/band-plan`, `tuxlink://reference/modem-capability-matrix`,
`tuxlink://reference/local-agent-deployment` (what local/edge hardware runs a
Tuxlink assistant offline, and how a local assistant compares to a cloud one).
Call `list_resources` for the full catalog.

Three prompts walk common operator workflows: `diagnose_my_connection` (optional
`transport`), `help_me_set_up` (required `device`), and `compose_an_ics_213`
(optional `to`, `subject`).

## Local docs

The full in-repo user guide lives at `docs/user-guide/` and is searchable with the
`docs_search` tool. Agent-authored knowledge lives at `docs/mcp-knowledge/`. When a
question goes beyond this guide, search the docs before guessing.
