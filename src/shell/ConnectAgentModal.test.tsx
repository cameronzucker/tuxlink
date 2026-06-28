// src/shell/ConnectAgentModal.test.tsx
import { test, expect, vi } from 'vitest';
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
