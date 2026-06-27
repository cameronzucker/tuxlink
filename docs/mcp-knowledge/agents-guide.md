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

`server_info`, `backend_status`, `modem_get_status`, `vara_status`,
`position_status`, `platform_info`, `get_wizard_completed`,
`p2p_peer_password_status` (status only, never the password), `user_folders_list`,
`docs_search`, `catalog_list`, `config_read`, `config_get_ardop`,
`config_get_vara`, `packet_config_get`, the device enumerators
(`packet_list_serial_devices`, `packet_list_bluetooth_devices`,
`ardop_list_audio_devices`).

These four return **untrusted message/wire content and TAINT the session**:
`mailbox_list`, `message_read`, `tauri_search_run`, `session_log_snapshot`. Once
tainted, egress and writes are locked until the operator re-arms (see the
arm/taint model below).

### Station intelligence — reads, no taint, no authorization

- `find_stations` — Winlink RMS gateway directory (callsign, frequencies, grid,
  last-heard), filterable by transport/band/history. Cached public data.
- `predict_path` — offline VOACAP HF path reliability/SNR/MUF-day by UTC hour from
  the operator's own grid to a target grid across candidate dial frequencies.
- `solar_conditions` — current space-weather indices (SFI/A/K) and the sunspot
  number used in predictions.

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
  until the operator re-arms. This contains prompt injection: an instruction read
  out of a message cannot be turned into a transmission in the same armed window.
- **Compose stages; the gated connect transmits.** `message_send` / `send_form`
  build an outbox draft with no authorization. The message only leaves the station
  when the operator arms authority and a gated egress tool (e.g. `cms_connect`)
  runs.

A denied egress/write returns a clear `not authorized` error naming the cause
(not armed, or tainted). That is expected behavior, not a bug — relay it to the
operator and ask them to arm send-authority (and note that reading a message
re-locks it).

## Typical flows

- **Diagnose a connection** — read `backend_status` / `modem_get_status` /
  `config_read`, then `session_log_snapshot` (taints), and consult
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
`tuxlink://reference/band-plan`, `tuxlink://reference/modem-capability-matrix`.
Call `list_resources` for the full catalog.

Three prompts walk common operator workflows: `diagnose_my_connection` (optional
`transport`), `help_me_set_up` (required `device`), and `compose_an_ics_213`
(optional `to`, `subject`).

## Local docs

The full in-repo user guide lives at `docs/user-guide/` and is searchable with the
`docs_search` tool. Agent-authored knowledge lives at `docs/mcp-knowledge/`. When a
question goes beyond this guide, search the docs before guessing.
