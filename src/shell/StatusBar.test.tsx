import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, act } from '@testing-library/react';

// tuxlink-8g28: the status bar now subscribes to the basemap download event
// stream (useActiveDownload). Mock `listen` so the tests can drive progress/done
// payloads; with no events emitted the existing mailbox-bar tests see no download
// segment (graceful idle).
const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => {
      delete handlers[name];
    });
  },
}));

import { StatusBar } from './StatusBar';
import { DOWNLOAD_PROGRESS_EVENT, DOWNLOAD_DONE_EVENT } from '../map/offlineMaps';

beforeEach(() => {
  for (const k of Object.keys(handlers)) delete handlers[k];
});

describe('<StatusBar> — mailbox-bar redesign (tuxlink-qxqj)', () => {
  it('renders nothing when show=false (zero height)', () => {
    const { container } = render(<StatusBar show={false} unread={3} outboxQueued={0} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders outbox queue + unread + version when outbox is non-empty', () => {
    render(<StatusBar show unread={3} outboxQueued={2} />);
    expect(screen.getByTestId('status-bar-outbox')).toHaveTextContent('2 to send');
    expect(screen.getByTestId('status-bar-unread')).toHaveTextContent('3 unread');
    // release-please bumps version.txt frequently; just verify the shape.
    expect(screen.getByTestId('status-bar-version').textContent ?? '').toMatch(/^v\d+\.\d+\.\d+/);
  });

  it('hides the outbox segment when the queue is empty (no zero-state noise)', () => {
    render(<StatusBar show unread={0} outboxQueued={0} />);
    expect(screen.queryByTestId('status-bar-outbox')).toBeNull();
    // The unread segment + version still render — the bar's anchors.
    expect(screen.getByTestId('status-bar-unread')).toHaveTextContent('0 unread');
    expect(screen.getByTestId('status-bar-version')).toBeInTheDocument();
  });

  it('does not render the connection state (now lives in DashboardRibbon)', () => {
    render(<StatusBar show unread={3} outboxQueued={1} />);
    // The pre-redesign data-testids must be gone — they belonged to the
    // duplicated connection chip the operator asked us to drop.
    expect(screen.queryByTestId('status-bar-state')).toBeNull();
    expect(screen.queryByTestId('status-bar-dot')).toBeNull();
  });

  // tuxlink-8g28: ambient map-download progress on the status bar.
  it('shows no download segment when nothing is downloading', () => {
    render(<StatusBar show unread={0} outboxQueued={0} />);
    expect(screen.queryByTestId('status-bar-download')).toBeNull();
  });

  it('shows the download segment with percent while a pack download runs', () => {
    render(<StatusBar show unread={0} outboxQueued={0} />);
    act(() => {
      handlers[DOWNLOAD_PROGRESS_EVENT]?.({
        payload: { packId: 'continent-na', bytes: 470, total: 1000 },
      });
    });
    expect(screen.getByTestId('status-bar-download')).toHaveTextContent('Downloading map 47%');
  });

  it('clears the download segment when the download completes', () => {
    render(<StatusBar show unread={0} outboxQueued={0} />);
    act(() => {
      handlers[DOWNLOAD_PROGRESS_EVENT]?.({
        payload: { packId: 'continent-na', bytes: 470, total: 1000 },
      });
    });
    expect(screen.getByTestId('status-bar-download')).toBeInTheDocument();
    act(() => {
      handlers[DOWNLOAD_DONE_EVENT]?.({ payload: { packId: 'continent-na', ok: true, error: null } });
    });
    expect(screen.queryByTestId('status-bar-download')).toBeNull();
  });
});
