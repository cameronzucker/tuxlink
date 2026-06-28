# Connect an AI agent — design

Date: 2026-06-27 · Issue: tuxlink-qwyw5 · Status: approved (brainstorm)

## Problem

Tuxlink's embedded MCP server exposes the running client to an AI agent (Claude
Code, Codex, Gemini CLI, any MCP client). Connecting one currently requires the
user to hand-configure their agent with two values they have no easy way to
know: the `tuxlink-mcp` shim's filesystem path and the Unix-domain socket path.
That paths-and-JSON chore is hostile to a non-technical operator — the WLE-style
"you must already be an expert" trap the MCP epic exists to avoid.

Both unknowns are deterministic for a given install: the shim ships beside the
app binary (Tauri `externalBin`), and the socket path is resolved at startup by
the existing `lib.rs` logic (PR #924: `$XDG_RUNTIME_DIR/tuxlink/mcp.sock`, with a
hardened private `/tmp/tuxlink-<uid>/...` fallback). So Tuxlink can hand the user
a ready-to-paste connect command with those values already filled in.

## Goals

- A user installs an MCP-capable agent, opens one menu entry, copies the command
  for their agent, runs it once, and the agent can then operate the station.
- The emitted command is correct for *this* install (no hardcoded paths).
- Enable an end-to-end new-user-setup test with Gemini CLI (cvx84.11) instead of
  hand-wiring the agent config.

## Non-goals

- **No auto-writing** of any agent's config file. Tuxlink does not edit files it
  does not own (`~/.claude.json`, `~/.codex/config.toml`, `~/.gemini/settings.json`).
  Show + copy only.
- No change to the MCP server, its transport, or its tool surface.
- No agent-discovery / auto-registration. MCP has no such mechanism; registration
  is explicit, and this feature makes the explicit step one paste.

## Placement

- **Tools menu → "Connect an AI agent…"** (MCP is a tool surface; it belongs in
  Tools, not Help).
- **Help cross-reference:** the `docs/user-guide/35-agent-mcp.md` "Connecting an
  agent" section points at Tools → Connect an AI agent… so the in-app guide and
  the action reinforce each other.

## UX

A modal (following the existing `src/help/ReportIssueModal.tsx` pattern), opened
from the menu action. One section per agent, each with the exact command and a
**Copy** button, the shim + socket paths filled in live:

- **Claude Code** — `claude mcp add tuxlink -- <shim> <socket>`
- **Codex CLI** — `codex mcp add tuxlink -- <shim> <socket>`
- **Gemini CLI** — `gemini mcp add tuxlink <shim> <socket>` (exact flag form
  verified against the installed Gemini CLI during implementation; fall back to
  the generic JSON if the CLI's `mcp add` syntax differs)
- **Other (generic MCP JSON)** — a `{ "command": "<shim>", "args": ["<socket>"] }`
  snippet for any other MCP client's `mcpServers` config

A short informational line states the authorization model plainly: connecting
grants the agent the read/diagnose tool surface; transmitting or changing
configuration still requires arming **Agent send** on the dashboard. No alarm.

**Server-down edge case:** if the MCP server is not running (`server_running`
false — e.g. a pre-epic build, or the server failed to bind), the modal still
shows the commands with the resolved expected paths and a quiet note that the
MCP server starts automatically with the app. In a build with the epic the
server auto-starts, so this is rare.

## The one Rust piece: `mcp_connection_info` command

A new read-only Tauri command:

```
mcp_connection_info() -> { socket_path: String, shim_path: String, server_running: bool }
```

- `socket_path` — reuse the existing `lib.rs` socket-path resolution (PR #924) so
  the value matches exactly where the server actually binds. Factor that
  resolution into a shared function if it is currently inline in `setup`.
- `shim_path` — resolve the `tuxlink-mcp` `externalBin` path next to the running
  executable (Tauri sidecar resolution / `current_exe().parent()/tuxlink-mcp`),
  matching how the app launches it.
- `server_running` — whether the socket currently has a live listener (cheap
  probe, or read a flag set at server startup).

The command performs no mutation and exposes no secret. The frontend builds every
per-agent command string from `socket_path` + `shim_path`.

## Components and files (isolation)

Self-contained; touches **none** of the MCP-core files PR 932 edits:

- `src/shell/chrome/menuModel.ts` — one Tools entry `menu:tools:connect_agent`.
- `src/shell/chrome/dispatchMenuAction.ts` — one handler opening the modal.
- `src/shell/ConnectAgentModal.tsx` (+ `.css`) — the new modal component.
- A small command-string builder module (pure function: paths → per-agent
  strings) so it is unit-testable without the DOM.
- `src-tauri/src/...` — the `mcp_connection_info` command + registration in the
  invoke handler.
- `docs/user-guide/35-agent-mcp.md` — add the Tools-menu cross-reference.

## Testing

- **vitest** — the command-string builder (given socket/shim paths, asserts the
  exact Claude/Codex/Gemini/JSON strings), and the modal (renders a section +
  Copy button per agent, shows the security note, handles `server_running`
  false).
- **Rust unit test** — `mcp_connection_info` path resolution (socket fallback
  branch, shim path beside the exe). The Tauri command compiles in CI.
- The live end-to-end paste-and-connect test runs against a build with the MCP
  epic (the Gemini rung of cvx84.11), once releases unfreeze and a build ships.

## Security framing

The emitted command grants the agent the always-available read/diagnose tier
only. Transmit, connect, and config-write remain behind the operator-armed
**Agent send** gate (and the taint lock). The modal states this so a user
understands that connecting an agent is not the same as authorizing it to
transmit.

## Deconfliction

Branched off current `origin/main` (`788fdbb9`, after #930/#932/#934 merged).
No file overlap with the in-flight MCP-core work. The feature ships its code now;
the live-drive test waits for an MCP-bearing release.
