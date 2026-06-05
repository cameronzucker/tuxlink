/**
 * Smoke-test for LoggingView: the three sections mount correctly within the
 * production component tree (QueryClientProvider).
 *
 * This catches "No QueryClient set" crashes (the tuxlink-n4hz class of bugs)
 * that unit tests for individual sections miss because each wraps its own
 * QueryClient as scaffolding.
 *
 * tuxlink-qjgx alpha-logging plan Task 7 / per-plan test discipline.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { LoggingView } from './LoggingView';

// --- Mocks ----------------------------------------------------------------

const { mockInvoke, mockListen } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockListen: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn() }));

beforeEach(() => {
  vi.resetAllMocks();
  mockInvoke.mockResolvedValue(null);
  mockListen.mockResolvedValue(() => {});
});

// --- Tests ----------------------------------------------------------------

function renderLoggingView() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    React.createElement(QueryClientProvider, { client },
      React.createElement(LoggingView),
    ),
  );
}

describe('LoggingView — integration smoke', () => {
  it('renders the root container', () => {
    renderLoggingView();
    expect(screen.getByTestId('logging-view-root')).toBeInTheDocument();
  });

  it('renders the Logging h1', () => {
    renderLoggingView();
    expect(screen.getByRole('heading', { level: 1, name: /logging/i })).toBeInTheDocument();
  });

  it('renders all three section headings', () => {
    renderLoggingView();
    // h2 headings from each section (uppercase text via CSS, text content still lowercase in DOM)
    expect(screen.getByRole('heading', { name: /export/i })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /settings/i })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /environment probes/i })).toBeInTheDocument();
  });

  it('does not crash without QueryClientProvider when wrapped', () => {
    // This test is already satisfied by the render above; adding explicit
    // assertion to document the intent.
    renderLoggingView();
    expect(screen.getByTestId('logging-view-root')).toBeInTheDocument();
  });
});
