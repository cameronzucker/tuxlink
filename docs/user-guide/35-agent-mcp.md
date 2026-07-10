# AI agent integration (MCP)

Tuxlink runs a Model Context Protocol (MCP) server that lets an AI assistant —
Claude, or any MCP-capable agent — read the station's state and help the operator
run it. The agent connects to the running application over a socket on the same
machine. The server opens no network port, and the operator holds the authority
to transmit: an agent reads and drafts freely, but it cannot connect to the CMS,
key the radio, or change configuration until the operator arms send-authority,
and that grant expires on its own.

The agent assists with work the operator already does: diagnosing why a
connection fails, finding an RMS gateway, predicting which band reaches it,
drafting an ICS-213, and queuing it for the operator to send.

## What an agent can and cannot do

| Action | Authorization |
|---|---|
| Read backend, modem, position, configuration, and device status | Always available |
| Look up RMS gateways, predict HF propagation, read space weather | Always available |
| Search the mailbox and read message or session-log content | Available, but taints the session (see below) |
| Compose and queue a message into the Outbox | Always available; queuing does not transmit |
| Change modem, rig, position, or privacy configuration | Requires armed send-authority |
| Connect to the CMS, run a B2F exchange, key the radio | Requires armed send-authority |
| Stop a connection or session | Always available |

A denied tool returns a plain-language reason — not armed, or session tainted —
which the agent relays to the operator.

## Arming agent send-authority

The **Agent send** control sits in the dashboard ribbon. It has three states:

- **OFF** — the default. The agent reads and drafts, but every transmit,
  connect, and configuration-write tool is denied. Bounded-window duration
  presets arm send-authority.
- **ON** — send-authority is armed. A live countdown shows the time remaining,
  and a **Disarm** button ends the grant early. When the countdown reaches zero
  the grant expires and the control returns to OFF; the operator re-arms to
  continue.
- **LOCKED** — the session is tainted (see below). No transmit is possible until
  the application restarts.

The armed window is the operator's choice from the presets. Nothing the agent
does extends it.

## The taint rule

Reading untrusted content — message bodies, search results, the session log —
taints the session. A tainted session locks send-authority: the **Agent send**
control shows **LOCKED**, and clearing it requires restarting the application.

This contains prompt injection. A message body or a wire capture can carry text
that reads like an instruction. Tainting ensures an instruction read out of
received content cannot become a transmission in the same session.

## On-air transmission and Part 97

Arming send-authority lets the agent drive the CMS and transmit tools, but the
licensed operator remains responsible for every on-air transmission under Part 97.
CMS connections over the internet (Telnet) are not transmissions. Arming grants
the agent a bounded send window; it does not transfer the operator's Part 97
responsibility.

## VARA HF by agent

The tool surface covers the full VARA HF lifecycle, under the same
authorization model:

- **Setup.** `vara_engine_available` and `vara_install_status` report
  whether this build carries the guided Wine setup engine (x86-64 Linux
  only) and how far an install has progressed; `vara_install_start`
  runs the install from a VARA installer `.exe` the operator has
  already downloaded. Installing software is not a transmission, so
  these run without armed send-authority — but the install prompts the
  operator for their OS password at the machine, so it cannot proceed
  unattended.
- **Diagnostics.** `vara_status` reports connection state and
  bandwidth; `vara_probe` opens VARA's command port read-only and
  classifies what answered (down, something-but-not-VARA, or a real
  VARA). Neither transmits, and both are always available.
- **Configuration.** `config_set_vara` sets the VARA bandwidth
  (500 / 2300 / 2750 Hz). Requires armed send-authority.
- **Operation.** `rig_tune` tunes the rig over CAT; `vara_open_session`
  opens the session and registers the callsign; `vara_b2f_exchange`
  dials a gateway and runs the message exchange, optionally tuning
  first. All three require armed send-authority and an untainted
  session. `vara_stop_session` stops the session and, like every stop,
  is always allowed.

## Connecting an agent

The quickest path is **Tools → Connect an AI agent…**, which shows a ready-to-paste connect command for Claude Code, Codex, Gemini CLI, or any MCP client, with this station's paths already filled in.

The server listens on a Unix-domain socket in the user's runtime directory
(`$XDG_RUNTIME_DIR/tuxlink/mcp.sock`, or a private fallback when the runtime
directory is not private to the user). A bundled stdio shim, `tuxlink-mcp`,
bridges an MCP client to that socket. Point an MCP-capable agent at the shim with
the socket path; the exact client configuration depends on the agent.

An agent's first read is the server's own guide resource,
`tuxlink://agents/guide`, which describes the full tool surface, the
authorization model, and the common workflows.
