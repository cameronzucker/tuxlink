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
