# Connect an AI agent — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tools → "Connect an AI agent…" modal that hands the user a ready-to-paste MCP connect command per agent, with the shim + socket paths filled in for their install.

**Architecture:** A pure TS builder turns two runtime paths into per-agent command strings; a modal renders them with Copy buttons; a small read-only Tauri command (`mcp_connection_info`) supplies the two paths. Show-and-copy only — Tuxlink never edits an agent's config file.

**Tech Stack:** React 18 + TypeScript (Vite, WebKitGTK), Tauri 2.x (Rust), vitest, `@tauri-apps/api`.

## Global Constraints

- Voice (UI copy + docs): declarative, present-indicative, no first person, no temporal hedging, no defensive self-assertion. Copy the values verbatim where given below.
- **This Pi cannot finish a cold `cargo` build/test locally.** Write the Rust + its tests; let CI compile. Open a **draft PR** right after the Rust task so CI starts. `vitest run <file>` runs fine locally per file.
- MSRV is 1.75 — no APIs stabilized in 1.76+.
- Self-contained: touch only the files named below. Do **not** edit `src-tauri/tuxlink-mcp-core/**`, `docs/mcp-knowledge/**`, or the MCP router — concurrent PRs own those.
- Show-and-copy only. No writing to `~/.claude.json`, `~/.codex/config.toml`, `~/.gemini/settings.json`, or any agent config.
- Branch: `bd-tuxlink-qwyw5/connect-an-ai-agent` (already created off `origin/main`). Commit trailers: `Agent: <moniker>` + the Co-Authored-By line.

---

### Task 1: Per-agent command-string builder (pure TS)

The heart: given the two runtime paths, produce the exact per-agent strings. Pure and DOM-free so it is fully unit-testable locally.

**Files:**
- Create: `src/shell/connectAgentCommands.ts`
- Test: `src/shell/connectAgentCommands.test.ts`

**Interfaces:**
- Produces: `interface McpConnectionInfo { socketPath: string; shimPath: string; serverRunning: boolean }`, `interface AgentCommand { id: 'claude'|'codex'|'gemini'|'generic'; label: string; command: string; isConfig?: boolean }`, `function buildAgentCommands(info: McpConnectionInfo): AgentCommand[]`.

- [ ] **Step 1: Write the failing test**

```ts
// src/shell/connectAgentCommands.test.ts
import { test, expect } from 'vitest';
import { buildAgentCommands, type McpConnectionInfo } from './connectAgentCommands';

const INFO: McpConnectionInfo = {
  shimPath: '/usr/lib/tuxlink/tuxlink-mcp',
  socketPath: '/run/user/1000/tuxlink/mcp.sock',
  serverRunning: true,
};

test('builds one command per agent in stable order', () => {
  const cmds = buildAgentCommands(INFO);
  expect(cmds.map((c) => c.id)).toEqual(['claude', 'codex', 'gemini', 'generic']);
});

test('claude + codex use `mcp add tuxlink -- <shim> <socket>`', () => {
  const cmds = buildAgentCommands(INFO);
  expect(cmds.find((c) => c.id === 'claude')!.command).toBe(
    'claude mcp add tuxlink -- /usr/lib/tuxlink/tuxlink-mcp /run/user/1000/tuxlink/mcp.sock',
  );
  expect(cmds.find((c) => c.id === 'codex')!.command).toBe(
    'codex mcp add tuxlink -- /usr/lib/tuxlink/tuxlink-mcp /run/user/1000/tuxlink/mcp.sock',
  );
});

test('gemini uses the positional `mcp add tuxlink <shim> <socket>` form', () => {
  const cmds = buildAgentCommands(INFO);
  expect(cmds.find((c) => c.id === 'gemini')!.command).toBe(
    'gemini mcp add tuxlink /usr/lib/tuxlink/tuxlink-mcp /run/user/1000/tuxlink/mcp.sock',
  );
});

test('generic is a JSON mcpServers snippet flagged isConfig', () => {
  const generic = buildAgentCommands(INFO).find((c) => c.id === 'generic')!;
  expect(generic.isConfig).toBe(true);
  expect(JSON.parse(generic.command)).toEqual({
    mcpServers: { tuxlink: { command: INFO.shimPath, args: [INFO.socketPath] } },
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/shell/connectAgentCommands.test.ts`
Expected: FAIL — `buildAgentCommands` not found.

- [ ] **Step 3: Implement the builder**

```ts
// src/shell/connectAgentCommands.ts
// Pure builder: runtime paths -> per-agent MCP connect commands. No DOM, no
// side effects, so it is fully unit-testable. Show-and-copy only — Tuxlink does
// not write agent config files.

export interface McpConnectionInfo {
  /** The Unix-domain socket the running MCP server binds. */
  socketPath: string;
  /** The bundled `tuxlink-mcp` stdio shim, beside the app binary. */
  shimPath: string;
  /** Whether the MCP server currently has a live listener. */
  serverRunning: boolean;
}

export interface AgentCommand {
  id: 'claude' | 'codex' | 'gemini' | 'generic';
  label: string;
  /** A shell command to paste, or (generic) a JSON config snippet. */
  command: string;
  /** True for the generic JSON snippet — render as a config block, not a shell line. */
  isConfig?: boolean;
}

export function buildAgentCommands(info: McpConnectionInfo): AgentCommand[] {
  const { shimPath, socketPath } = info;
  return [
    { id: 'claude', label: 'Claude Code', command: `claude mcp add tuxlink -- ${shimPath} ${socketPath}` },
    { id: 'codex', label: 'Codex CLI', command: `codex mcp add tuxlink -- ${shimPath} ${socketPath}` },
    { id: 'gemini', label: 'Gemini CLI', command: `gemini mcp add tuxlink ${shimPath} ${socketPath}` },
    {
      id: 'generic',
      label: 'Other (generic MCP JSON)',
      isConfig: true,
      command: JSON.stringify(
        { mcpServers: { tuxlink: { command: shimPath, args: [socketPath] } } },
        null,
        2,
      ),
    },
  ];
}
```

> NOTE on the Gemini form: the positional `gemini mcp add tuxlink <shim> <socket>` is the assumed syntax. During Task 3's manual check, run `gemini mcp add --help` against the installed CLI; if the flag form differs, fix the `gemini` line here and its test, and rely on the generic JSON as the always-correct fallback.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/shell/connectAgentCommands.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/shell/connectAgentCommands.ts src/shell/connectAgentCommands.test.ts
git commit  # subject: feat(shell): per-agent MCP connect-command builder
```

---

### Task 2: `mcp_connection_info` Tauri command

Supplies the two paths + liveness. Rust compiles in CI (not locally).

**Files:**
- Create: `src-tauri/src/mcp_connection.rs`
- Modify: `src-tauri/src/lib.rs` — factor the inline MCP socket-path resolution into a shared fn; add the command to `tauri::generate_handler![` (around line 1503); add `mod mcp_connection;`.

**Interfaces:**
- Produces (Rust): `#[tauri::command] pub fn mcp_connection_info(app: tauri::AppHandle) -> McpConnectionInfoDto` returning `{ socket_path: String, shim_path: String, server_running: bool }` (serde `rename_all = "camelCase"` so the JS sees `socketPath`/`shimPath`/`serverRunning` — matches Task 1's `McpConnectionInfo`).
- Consumes: the existing socket-path resolution currently inline in `lib.rs` setup (the block guarded by `XDG_RUNTIME_DIR` with the hardened `/tmp/tuxlink-<uid>/tuxlink` fallback, PR #924).

- [ ] **Step 1: Factor the socket-path resolution**

In `src-tauri/src/lib.rs`, find the inline MCP socket-path computation (search `XDG_RUNTIME_DIR` / `mcp.sock`). Extract it verbatim into a pure helper in the new module:

```rust
// src-tauri/src/mcp_connection.rs
use std::path::PathBuf;
use serde::Serialize;

/// Resolve the MCP socket path the server binds: `$XDG_RUNTIME_DIR/tuxlink/mcp.sock`
/// when the runtime dir is private (0700, uid-owned), else the hardened private
/// `/tmp/tuxlink-<uid>/tuxlink/mcp.sock` fallback (PR #924). MOVE the existing
/// inline logic here unchanged; call this from both setup and the command.
pub fn mcp_socket_path() -> PathBuf {
    // <paste the existing resolution body from lib.rs setup, returning the PathBuf>
    todo!("move verbatim from lib.rs; do not change behavior")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectionInfoDto {
    pub socket_path: String,
    pub shim_path: String,
    pub server_running: bool,
}
```

Then replace the inline lib.rs use with a call to `crate::mcp_connection::mcp_socket_path()`, and add `mod mcp_connection;` near the other `mod` lines.

> The `todo!()` is a *plan* marker for "paste the existing code here" — the existing resolution already exists in lib.rs; this step relocates it. The committed code must contain the real body, no `todo!`.

- [ ] **Step 2: Write the command + a path-resolution unit test**

```rust
// append to src-tauri/src/mcp_connection.rs
#[tauri::command]
pub fn mcp_connection_info(_app: tauri::AppHandle) -> McpConnectionInfoDto {
    let socket = mcp_socket_path();
    // Shim ships beside the app binary (externalBin). Resolve current_exe's dir.
    let shim = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tuxlink-mcp")))
        .unwrap_or_else(|| PathBuf::from("tuxlink-mcp"));
    let server_running = socket.exists(); // socket inode present => server bound it
    McpConnectionInfoDto {
        socket_path: socket.to_string_lossy().into_owned(),
        shim_path: shim.to_string_lossy().into_owned(),
        server_running,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn socket_path_ends_with_tuxlink_mcp_sock() {
        let p = mcp_socket_path();
        assert!(p.ends_with("tuxlink/mcp.sock"), "got {p:?}");
    }
}
```

- [ ] **Step 3: Register the command**

In `src-tauri/src/lib.rs`, add `crate::mcp_connection::mcp_connection_info,` inside `tauri::generate_handler![ ... ]` (line ~1503).

- [ ] **Step 4: Open a draft PR so CI compiles the Rust**

```bash
git add src-tauri/src/mcp_connection.rs src-tauri/src/lib.rs
git commit  # subject: feat(mcp): mcp_connection_info command + factor socket-path resolver
git push -u origin bd-tuxlink-qwyw5/connect-an-ai-agent
gh pr create --draft --base main --title "[<moniker>] feat: Connect an AI agent helper" --body "Implements docs/superpowers/specs/2026-06-27-connect-an-ai-agent-design.md (tuxlink-qwyw5)."
```

Expected: CI `verify` + `build-linux` run; the Rust compiles there.

---

### Task 3: `ConnectAgentModal` component + connection hook

**Files:**
- Create: `src/shell/ConnectAgentModal.tsx`, `src/shell/ConnectAgentModal.css`, `src/shell/useMcpConnectionInfo.ts`
- Test: `src/shell/ConnectAgentModal.test.tsx`
- Pattern to mirror: `src/help/ReportIssueModal.tsx` (modal chrome, focus, Esc/backdrop close).

**Interfaces:**
- Consumes: `buildAgentCommands`, `McpConnectionInfo` (Task 1); `invoke<McpConnectionInfo>('mcp_connection_info')` (Task 2).
- Produces: `function ConnectAgentModal({ open, onClose }: { open: boolean; onClose: () => void }): JSX.Element | null`.

- [ ] **Step 1: Write the hook**

```ts
// src/shell/useMcpConnectionInfo.ts
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { McpConnectionInfo } from './connectAgentCommands';

export function useMcpConnectionInfo(enabled: boolean) {
  return useQuery({
    queryKey: ['mcp_connection_info'],
    queryFn: () => invoke<McpConnectionInfo>('mcp_connection_info'),
    enabled,
  });
}
```

- [ ] **Step 2: Write the failing modal test**

```tsx
// src/shell/ConnectAgentModal.test.tsx
import { test, expect, vi, beforeEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ConnectAgentModal } from './ConnectAgentModal';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => ({
    shimPath: '/usr/lib/tuxlink/tuxlink-mcp',
    socketPath: '/run/user/1000/tuxlink/mcp.sock',
    serverRunning: true,
  })),
}));

function renderModal() {
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}><ConnectAgentModal open onClose={() => {}} /></QueryClientProvider>,
  );
}

test('renders a section + Copy button per agent', async () => {
  renderModal();
  for (const label of ['Claude Code', 'Codex CLI', 'Gemini CLI', 'Other (generic MCP JSON)']) {
    const sec = await screen.findByTestId(`connect-agent-${label.startsWith('Claude') ? 'claude' : label.startsWith('Codex') ? 'codex' : label.startsWith('Gemini') ? 'gemini' : 'generic'}`);
    expect(within(sec).getByRole('button', { name: /copy/i })).toBeInTheDocument();
  }
});

test('shows the Agent-send security note', async () => {
  renderModal();
  expect(await screen.findByText(/arm .*Agent send/i)).toBeInTheDocument();
});

test('closed renders nothing', () => {
  const qc = new QueryClient();
  const { container } = render(
    <QueryClientProvider client={qc}><ConnectAgentModal open={false} onClose={() => {}} /></QueryClientProvider>,
  );
  expect(container).toBeEmptyDOMElement();
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `npx vitest run src/shell/ConnectAgentModal.test.tsx`
Expected: FAIL — `ConnectAgentModal` not found.

- [ ] **Step 4: Implement the modal**

Mirror `src/help/ReportIssueModal.tsx` for the chrome (backdrop, panel, Esc/backdrop close, focus). Body: when `open`, call `useMcpConnectionInfo(open)`; on data, `buildAgentCommands(data)`; render each `AgentCommand` as a section with `data-testid={`connect-agent-${id}`}`, a `<pre>` of `command`, and a Copy button that calls `navigator.clipboard.writeText(command)` (the WebKitGTK webview supports it; mirror any existing copy helper if one exists — grep `clipboard`). Include the intro line and the security note (verbatim):

> Intro: `Point an AI assistant at this station. Pick your agent, copy the command, run it once.`
> Security note: `This lets the agent read and diagnose your station. Transmitting or changing settings still needs you to arm "Agent send" on the dashboard.`
> When `data.serverRunning === false`, show above the list: `Tuxlink's MCP server starts automatically with the app.`

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npx vitest run src/shell/ConnectAgentModal.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 6: Manual Gemini-syntax check + commit**

Run `gemini mcp add --help` (or `~/.local/bin/gemini mcp add --help`); if the positional form is wrong, fix the `gemini` line in `connectAgentCommands.ts` + its test. Then:

```bash
git add src/shell/ConnectAgentModal.tsx src/shell/ConnectAgentModal.css src/shell/useMcpConnectionInfo.ts src/shell/ConnectAgentModal.test.tsx
git commit  # subject: feat(shell): ConnectAgentModal — per-agent copy-paste connect commands
```

---

### Task 4: Wire the Tools menu entry

**Files:**
- Modify: `src/shell/chrome/menuModel.ts` (Tools items, ~line 101), `src/shell/chrome/menuModel.test.ts` (`EXPECTED_IDS`), `src/shell/chrome/dispatchMenuAction.ts` (`MenuHandlers` + switch), `src/shell/chrome/dispatchMenuAction.test.ts`, `src/shell/AppShell.tsx` (open-state + handler + mount the modal).

**Interfaces:**
- Consumes: `ConnectAgentModal` (Task 3).
- Produces: menu action id `menu:tools:connect_agent`; `MenuHandlers.openConnectAgent: () => void`.

- [ ] **Step 1: Add the menu entry + fix the id-manifest test**

In `menuModel.ts` Tools items, after the `verify_cms` line:
```ts
{ id: 'menu:tools:connect_agent', label: 'Connect an AI agent…' },
```
In `menuModel.test.ts`, add `'menu:tools:connect_agent'` to `EXPECTED_IDS` in the same position (after `'menu:tools:verify_cms'`).

- [ ] **Step 2: Run the manifest test to verify it passes**

Run: `npx vitest run src/shell/chrome/menuModel.test.ts`
Expected: PASS (the `MENU_ACTION_IDS` toEqual `EXPECTED_IDS` test stays green).

- [ ] **Step 3: Add the handler + dispatch case (write the failing dispatch test first)**

In `dispatchMenuAction.test.ts`, add a test that `menu:tools:connect_agent` calls `handlers.openConnectAgent`. Run it (FAIL). Then in `dispatchMenuAction.ts`: add `openConnectAgent: () => void;` to `MenuHandlers` (with a doc comment), and a `case 'menu:tools:connect_agent': handlers.openConnectAgent(); return;` in the switch. Re-run (PASS).

Run: `npx vitest run src/shell/chrome/dispatchMenuAction.test.ts`

- [ ] **Step 4: Mount in AppShell**

In `src/shell/AppShell.tsx`: add `const [connectAgentOpen, setConnectAgentOpen] = useState(false);`, pass `openConnectAgent: () => setConnectAgentOpen(true)` in the `MenuHandlers` object given to the dispatcher (find where `reportIssue` / `openAbout` are supplied — mirror it), and render `<ConnectAgentModal open={connectAgentOpen} onClose={() => setConnectAgentOpen(false)} />` beside the other modals (e.g. near `ReportIssueModal`).

- [ ] **Step 5: Verify + commit**

Run: `npx vitest run src/shell/chrome/ src/shell/ConnectAgentModal.test.tsx` and `npx tsc --noEmit -p tsconfig.json`
Expected: PASS + clean typecheck.
```bash
git add src/shell/chrome/menuModel.ts src/shell/chrome/menuModel.test.ts src/shell/chrome/dispatchMenuAction.ts src/shell/chrome/dispatchMenuAction.test.ts src/shell/AppShell.tsx
git commit  # subject: feat(shell): wire Tools -> Connect an AI agent
```

---

### Task 5: Help cross-reference

**Files:**
- Modify: `docs/user-guide/35-agent-mcp.md` (the "Connecting an agent" section).

- [ ] **Step 1: Add the pointer**

In the "Connecting an agent" section, prepend a sentence (verbatim):
> The quickest path is **Tools → Connect an AI agent…**, which shows a ready-to-paste connect command for Claude Code, Codex, Gemini CLI, or any MCP client, with this station's paths already filled in.

- [ ] **Step 2: Lint + commit**

Run: `pnpm lint:docs`
Expected: `Link linter passed.`
```bash
git add docs/user-guide/35-agent-mcp.md
git commit  # subject: docs(user-guide): point the agent topic at Tools -> Connect an AI agent
```

---

### Task 6: Finalize

- [ ] **Step 1: Push + mark the PR ready when CI is green**

```bash
git push
gh pr ready    # once verify + build-linux pass
```

- [ ] **Step 2: Wire-walk the flow**

Trace the operator flow "open Tools → Connect an AI agent → copy a command" to code: `menuModel.ts` entry → `dispatchMenuAction` case → `AppShell` open-state → `ConnectAgentModal` → `mcp_connection_info` → `buildAgentCommands` → Copy. Confirm each hop `file:line`.

## Self-Review

- Spec coverage: Tools placement (T4) ✓; modal + per-agent copy (T1/T3) ✓; `mcp_connection_info` + factored socket resolver + shim path (T2) ✓; security note (T3) ✓; server-down state (T3) ✓; Help cross-ref (T5) ✓; show-copy-only (no auto-write anywhere) ✓; agents Claude/Codex/Gemini/generic (T1) ✓; isolation (no MCP-core files) ✓; tests (T1/T2/T3 + manifest) ✓.
- Placeholder scan: the only `todo!()` is an explicit "relocate existing code here" marker with a note that the committed code must contain the real body — not a behavior gap.
- Type consistency: `McpConnectionInfo {socketPath, shimPath, serverRunning}` (TS) ↔ `McpConnectionInfoDto` serde `camelCase` (Rust) ↔ `mcp_connection_info` invoke return — aligned across T1/T2/T3. `openConnectAgent` / `menu:tools:connect_agent` consistent T4.
