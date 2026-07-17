/**
 * Tests for RoutinesSurface.tsx's tuxlink-9se1x navigation affordances: the
 * dashboard's "← Mailbox" close button and the dashboard-only Escape-to-close
 * shortcut (with its dialog / typing-surface / designer guards).
 *
 * `@tauri-apps/api/core` is mocked at module scope, keyed by command name
 * (feedback_vitest_invoke_mock_cleanup_call — the no-arg teardown call must
 * be inert). The dashboard mounts with an EMPTY fleet: these tests exercise
 * surface chrome, not fleet rendering (RoutinesDashboard.test.tsx owns that).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

const { mockListen } = vi.hoisted(() => ({ mockListen: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));

import { RoutinesSurface } from './RoutinesSurface';

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd?: string) => {
    switch (cmd) {
      case 'routines_list':
      case 'routines_missed_fires':
      case 'routines_next_fires':
      case 'routines_fleet_check':
      case 'routines_actions_list':
      case 'routines_runs_list':
        return Promise.resolve([]);
      default:
        // Teardown calls arrive with no args — stay inert.
        return Promise.resolve(undefined);
    }
  });
  mockListen.mockReset();
  mockListen.mockResolvedValue(() => {});
});

async function renderDashboardSurface(onClose?: () => void) {
  render(
    <RoutinesSurface view={{ view: 'dashboard' }} onNavigate={vi.fn()} onClose={onClose} />,
  );
  await waitFor(() => expect(screen.getByTestId('routines-dashboard')).toBeInTheDocument());
}

describe('RoutinesSurface — back to mailbox (tuxlink-9se1x)', () => {
  it('renders "← Mailbox" on the dashboard when onClose is provided, and clicking it closes', async () => {
    const onClose = vi.fn();
    await renderDashboardSurface(onClose);
    const btn = screen.getByTestId('routines-dashboard-close');
    expect(btn).toHaveTextContent('← Mailbox');
    fireEvent.click(btn);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('renders no close button without onClose (the popped window has no mailbox)', async () => {
    await renderDashboardSurface(undefined);
    expect(screen.queryByTestId('routines-dashboard-close')).toBeNull();
  });

  it('Escape on the dashboard closes to the mailbox', async () => {
    const onClose = vi.fn();
    await renderDashboardSurface(onClose);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('Escape is ignored while a dialog is open (the dialog owns Escape)', async () => {
    const onClose = vi.fn();
    await renderDashboardSurface(onClose);
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.appendChild(dialog);
    try {
      fireEvent.keyDown(window, { key: 'Escape' });
      expect(onClose).not.toHaveBeenCalled();
    } finally {
      dialog.remove();
    }
  });

  it('Escape is ignored while a row menu is open (the menu owns Escape — Codex P3)', async () => {
    const onClose = vi.fn();
    await renderDashboardSurface(onClose);
    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    document.body.appendChild(menu);
    try {
      fireEvent.keyDown(window, { key: 'Escape' });
      expect(onClose).not.toHaveBeenCalled();
    } finally {
      menu.remove();
    }
  });

  it('Escape is ignored while typing in an input', async () => {
    const onClose = vi.fn();
    await renderDashboardSurface(onClose);
    const input = document.createElement('input');
    document.body.appendChild(input);
    try {
      input.focus();
      fireEvent.keyDown(input, { key: 'Escape' });
      expect(onClose).not.toHaveBeenCalled();
    } finally {
      input.remove();
    }
  });

  it('Escape stays inert in the designer (an unsaved draft must never be discarded by a stray key)', async () => {
    const onClose = vi.fn();
    render(
      <RoutinesSurface
        view={{ view: 'designer', routine: '', tab: 'design' }}
        onNavigate={vi.fn()}
        onClose={onClose}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('routine-designer')).toBeInTheDocument());
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });
});
