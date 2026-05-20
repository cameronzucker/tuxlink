/**
 * SessionLog.tsx unit tests — Mock B human-shaped session log.
 *
 * Tauri IPC (listen + invoke) is mocked. DEV_FIXTURE is false under vitest
 * (MODE=test), so the component renders the projected IPC lines (not the dev
 * fixture steps). Verifies the strip + Human/Raw toggle + snapshot seeding +
 * live tail + the human projection suppressing raw wire noise.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import type { LogLineDto } from './logProjection';

type ListenCallback = (event: { payload: LogLineDto }) => void;
let capturedListeners: ListenCallback[] = [];
let mockSnapshotLines: LogLineDto[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((_eventName: string, cb: ListenCallback) => {
    capturedListeners.push(cb);
    return Promise.resolve(() => {
      capturedListeners = capturedListeners.filter((l) => l !== cb);
    });
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((_cmd: string) => Promise.resolve(mockSnapshotLines)),
}));

import { SessionLog } from './SessionLog';

const backendLine: LogLineDto = {
  timestampIso: '2026-05-19T12:00:00Z',
  level: 'info',
  source: 'backend',
  message: 'Pat process started',
};
const wireLine: LogLineDto = {
  timestampIso: '2026-05-19T12:00:01Z',
  level: 'debug',
  source: 'wire',
  message: ';PQ: WL2K AUTH REQUIRED',
};

beforeEach(() => {
  capturedListeners = [];
  mockSnapshotLines = [];
});
afterEach(() => {
  vi.clearAllMocks();
});

describe('SessionLog (Mock B human-shaped)', () => {
  it('renders the strip with a Human/Raw toggle (Human active by default)', async () => {
    await act(async () => {
      render(<SessionLog />);
    });
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();
    expect(screen.getByTestId('session-log-root').className).toContain('human');
    expect(screen.getByTestId('toggle-human')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('toggle-raw')).toHaveAttribute('aria-pressed', 'false');
  });

  it('seeds from the snapshot on mount', async () => {
    mockSnapshotLines = [backendLine];
    await act(async () => {
      render(<SessionLog />);
    });
    await waitFor(() =>
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Pat process started'),
    );
  });

  it('Human suppresses raw wire lines; Raw shows them', async () => {
    mockSnapshotLines = [backendLine, wireLine];
    await act(async () => {
      render(<SessionLog />);
    });
    await waitFor(() =>
      expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Pat process started'),
    );
    // Human projection drops non-annotated wire noise.
    expect(screen.getByTestId('session-log-lines')).not.toHaveTextContent('WL2K AUTH REQUIRED');

    await act(async () => {
      fireEvent.click(screen.getByTestId('toggle-raw'));
    });
    expect(screen.getByTestId('toggle-raw')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('session-log-lines')).toHaveTextContent('WL2K AUTH REQUIRED');
  });

  it('appends a line arriving via listen()', async () => {
    await act(async () => {
      render(<SessionLog />);
    });
    await act(async () => {
      capturedListeners.forEach((cb) => cb({ payload: backendLine }));
    });
    expect(screen.getByTestId('session-log-lines')).toHaveTextContent('Pat process started');
  });
});
