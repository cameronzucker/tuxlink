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
