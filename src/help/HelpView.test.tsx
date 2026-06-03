import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { HelpView } from './HelpView';

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

beforeEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-font-size');
});

function renderHelp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    React.createElement(QueryClientProvider, { client }, React.createElement(HelpView)),
  );
}

describe('HelpView', () => {
  it('renders the layout skeleton', () => {
    renderHelp();
    expect(screen.getByTestId('tux-help-root')).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: /help topics/i })).toBeInTheDocument();
    expect(screen.getByRole('main')).toBeInTheDocument();
  });

  it('opens to the first topic by default', () => {
    renderHelp();
    expect(screen.getByRole('heading', { level: 1, name: /getting started/i })).toBeInTheDocument();
  });

  it('renders the header strip with the User Guide title', () => {
    renderHelp();
    expect(screen.getByText(/User Guide/)).toBeInTheDocument();
  });

  it('renders the text-size dropdown in the header', () => {
    renderHelp();
    expect(screen.getByRole('button', { name: /Text size:/ })).toBeInTheDocument();
  });

  it('renders the sidebar search input', () => {
    renderHelp();
    expect(screen.getByPlaceholderText(/Search topics/i)).toBeInTheDocument();
  });
});

describe('HelpView text-size keyboard shortcuts', () => {
  it('Ctrl+= steps the size up', () => {
    renderHelp();
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });
    // The dropdown button label changes to reflect the new preset.
    expect(screen.getByText('Large')).toBeInTheDocument();
  });

  it('Ctrl+- saturates at Normal on first press', () => {
    renderHelp();
    fireEvent.keyDown(window, { key: '-', ctrlKey: true });
    expect(screen.getByText('Normal')).toBeInTheDocument();
  });

  it('Ctrl+0 resets to Normal from any tier', () => {
    renderHelp();
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });  // → Large
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });  // → X-Large
    fireEvent.keyDown(window, { key: '0', ctrlKey: true });  // → Normal
    expect(screen.getByText('Normal')).toBeInTheDocument();
  });
});
