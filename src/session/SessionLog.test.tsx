/**
 * SessionLog.tsx unit tests — Task 15 (spec §6 Task 15).
 *
 * All Tauri IPC (listen + invoke) is mocked so tests run headlessly.
 * These tests verify:
 *   (6) auto-scroll pause logic — stuckToBottom state transitions
 *   (7) LogLineDto level/source enum round-trip through rendered component
 * Plus structural tests:
 *   - Human / Raw toggle changes which lines are visible
 *   - Copy button is present
 *   - Session state label renders
 *   - Seeds from snapshot on mount (invoke mock)
 *   - New lines from listen() event arrive in the rendered list
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import type { LogLineDto } from './logProjection';

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/event and @tauri-apps/api/core
// ---------------------------------------------------------------------------
// Tauri is not available in jsdom; mock the two IPC calls Task 15 uses:
//   listen('session_log:line', handler)  → returns an unlisten fn
//   invoke('session_log_snapshot')        → returns LogLineDto[]

type ListenCallback = (event: { payload: LogLineDto }) => void;
let capturedListeners: ListenCallback[] = [];
let mockSnapshotLines: LogLineDto[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((_eventName: string, cb: ListenCallback) => {
    capturedListeners.push(cb);
    // Return a promise that resolves to an unlisten function
    return Promise.resolve(() => {
      capturedListeners = capturedListeners.filter(l => l !== cb);
    });
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((_cmd: string) => Promise.resolve(mockSnapshotLines)),
}));

// ---------------------------------------------------------------------------
// Now import the component AFTER the mocks are in place
// ---------------------------------------------------------------------------
import { SessionLog } from './SessionLog';

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

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

const annotatedLine: LogLineDto = {
  timestampIso: '2026-05-19T12:00:02Z',
  level: 'info',
  source: 'backend',
  message: '*** Session started',
};

const transportLine: LogLineDto = {
  timestampIso: '2026-05-19T12:00:03Z',
  level: 'info',
  source: 'transport',
  message: 'Connected to cms-ssl.winlink.org:8772',
};

// ---------------------------------------------------------------------------
// Setup / teardown
// ---------------------------------------------------------------------------

beforeEach(() => {
  capturedListeners = [];
  mockSnapshotLines = [];
});

afterEach(() => {
  vi.clearAllMocks();
});

// ---------------------------------------------------------------------------
// Structural tests
// ---------------------------------------------------------------------------

describe('SessionLog — structure', () => {
  it('renders without crashing (empty snapshot)', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    // Component root should be present
    expect(screen.getByTestId('session-log-root')).toBeTruthy();
  });

  it('shows the session-state label', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Connecting" />);
    });
    expect(screen.getByTestId('session-state-label').textContent).toMatch(/Connecting/);
  });

  it('renders Human toggle button', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    expect(screen.getByTestId('toggle-human')).toBeTruthy();
  });

  it('renders Raw toggle button', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    expect(screen.getByTestId('toggle-raw')).toBeTruthy();
  });

  it('renders Copy button', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    expect(screen.getByTestId('copy-button')).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// Snapshot seeding
// ---------------------------------------------------------------------------

describe('SessionLog — snapshot seeding', () => {
  it('seeds from session_log_snapshot on mount', async () => {
    mockSnapshotLines = [backendLine, annotatedLine];
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    // Both lines are backend/annotated → visible in Human projection (default)
    const logArea = screen.getByTestId('session-log-lines');
    expect(logArea.textContent).toContain('Pat process started');
    expect(logArea.textContent).toContain('*** Session started');
  });

  it('seeds wire lines that are hidden in Human but visible in Raw', async () => {
    mockSnapshotLines = [wireLine];
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    // Default is Human — wire ;PQ should NOT appear
    const logArea = screen.getByTestId('session-log-lines');
    expect(logArea.textContent).not.toContain(';PQ');

    // Switch to Raw
    fireEvent.click(screen.getByTestId('toggle-raw'));
    expect(screen.getByTestId('session-log-lines').textContent).toContain(';PQ');
  });
});

// ---------------------------------------------------------------------------
// Human / Raw toggle
// ---------------------------------------------------------------------------

describe('SessionLog — Human/Raw toggle', () => {
  it('default projection is Human', async () => {
    mockSnapshotLines = [backendLine, wireLine];
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    // Human: backend visible, wire ;PQ hidden
    const logArea = screen.getByTestId('session-log-lines');
    expect(logArea.textContent).toContain('Pat process started');
    expect(logArea.textContent).not.toContain(';PQ');
  });

  it('switching to Raw shows wire lines', async () => {
    mockSnapshotLines = [backendLine, wireLine];
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    fireEvent.click(screen.getByTestId('toggle-raw'));
    const logArea = screen.getByTestId('session-log-lines');
    expect(logArea.textContent).toContain('Pat process started');
    expect(logArea.textContent).toContain(';PQ');
  });

  it('switching back to Human re-hides wire lines', async () => {
    mockSnapshotLines = [backendLine, wireLine];
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    fireEvent.click(screen.getByTestId('toggle-raw'));
    fireEvent.click(screen.getByTestId('toggle-human'));
    const logArea = screen.getByTestId('session-log-lines');
    expect(logArea.textContent).toContain('Pat process started');
    expect(logArea.textContent).not.toContain(';PQ');
  });

  it('toggle buttons show active state correctly', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    const humanBtn = screen.getByTestId('toggle-human');
    const rawBtn = screen.getByTestId('toggle-raw');
    // Initially Human is active
    expect(humanBtn.getAttribute('aria-pressed')).toBe('true');
    expect(rawBtn.getAttribute('aria-pressed')).toBe('false');
    // Switch to Raw
    fireEvent.click(rawBtn);
    expect(humanBtn.getAttribute('aria-pressed')).toBe('false');
    expect(rawBtn.getAttribute('aria-pressed')).toBe('true');
  });
});

// ---------------------------------------------------------------------------
// Live event delivery
// ---------------------------------------------------------------------------

describe('SessionLog — live event delivery', () => {
  it('displays a new line delivered via session_log:line event', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    // Deliver a backend line via the captured listener
    await act(async () => {
      for (const cb of capturedListeners) {
        cb({ payload: backendLine });
      }
    });
    expect(screen.getByTestId('session-log-lines').textContent).toContain('Pat process started');
  });

  it('wire line delivered via event is hidden in Human, visible in Raw', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    await act(async () => {
      for (const cb of capturedListeners) {
        cb({ payload: wireLine });
      }
    });
    // Human (default) hides wire
    expect(screen.getByTestId('session-log-lines').textContent).not.toContain(';PQ');
    // Switch to Raw
    fireEvent.click(screen.getByTestId('toggle-raw'));
    expect(screen.getByTestId('session-log-lines').textContent).toContain(';PQ');
  });

  it('multiple lines accumulate correctly', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    await act(async () => {
      for (const cb of capturedListeners) {
        cb({ payload: backendLine });
        cb({ payload: transportLine });
        cb({ payload: wireLine });
      }
    });
    // Human shows backend + transport, hides wire
    const logArea = screen.getByTestId('session-log-lines');
    expect(logArea.textContent).toContain('Pat process started');
    expect(logArea.textContent).toContain('Connected to cms-ssl');
    expect(logArea.textContent).not.toContain(';PQ');
  });
});

// ---------------------------------------------------------------------------
// Test 6: Auto-scroll stuckToBottom state — spec §6 Task 15 item (6)
//
// Real scroll-geometry test: set scrollTop/scrollHeight/clientHeight on the
// scroll container, fire a scroll event, and assert state transitions.
// ---------------------------------------------------------------------------

describe('SessionLog — auto-scroll logic', () => {
  it('auto-scroll is ON by default (scroll-to-bottom button hidden)', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    // stuckToBottom starts true → "↓ Resume" button is NOT rendered
    expect(screen.queryByTestId('scroll-to-bottom')).toBeNull();
  });

  it('pauses auto-scroll when user scrolls up (not at bottom)', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });

    const scrollEl = screen.getByTestId('session-log-lines');

    // Simulate a scroll container where the user has scrolled up:
    // scrollHeight=500, clientHeight=100, scrollTop=100 → 300px from bottom
    Object.defineProperty(scrollEl, 'scrollHeight', { value: 500, configurable: true });
    Object.defineProperty(scrollEl, 'clientHeight', { value: 100, configurable: true });
    Object.defineProperty(scrollEl, 'scrollTop', { value: 100, configurable: true, writable: true });

    await act(async () => {
      fireEvent.scroll(scrollEl);
    });

    // stuckToBottom should now be false → "↓ Resume" button appears
    expect(screen.getByTestId('scroll-to-bottom')).toBeTruthy();
  });

  it('resumes auto-scroll when user scrolls back to bottom', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });

    const scrollEl = screen.getByTestId('session-log-lines');

    // First scroll up to pause
    Object.defineProperty(scrollEl, 'scrollHeight', { value: 500, configurable: true });
    Object.defineProperty(scrollEl, 'clientHeight', { value: 100, configurable: true });
    Object.defineProperty(scrollEl, 'scrollTop', { value: 100, configurable: true, writable: true });
    await act(async () => {
      fireEvent.scroll(scrollEl);
    });
    expect(screen.getByTestId('scroll-to-bottom')).toBeTruthy();

    // Now scroll to bottom: scrollHeight - scrollTop - clientHeight < 4
    Object.defineProperty(scrollEl, 'scrollTop', { value: 399, configurable: true, writable: true });
    await act(async () => {
      fireEvent.scroll(scrollEl);
    });

    // stuckToBottom should be true again → "↓ Resume" button gone
    expect(screen.queryByTestId('scroll-to-bottom')).toBeNull();
  });

  it('click "↓ Resume" button re-enables auto-scroll', async () => {
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });

    const scrollEl = screen.getByTestId('session-log-lines');
    Object.defineProperty(scrollEl, 'scrollHeight', { value: 500, configurable: true });
    Object.defineProperty(scrollEl, 'clientHeight', { value: 100, configurable: true });
    Object.defineProperty(scrollEl, 'scrollTop', { value: 100, configurable: true, writable: true });
    await act(async () => {
      fireEvent.scroll(scrollEl);
    });

    // Pause state: button visible
    const resumeBtn = screen.getByTestId('scroll-to-bottom');
    expect(resumeBtn).toBeTruthy();

    // Click the resume button
    await act(async () => {
      fireEvent.click(resumeBtn);
    });

    // Button should disappear (stuckToBottom = true again)
    expect(screen.queryByTestId('scroll-to-bottom')).toBeNull();
  });

  it('auto-scroll resumes on new session start (sessionState → In-session)', async () => {
    const { rerender } = await act(async () =>
      render(<SessionLog sessionState="Idle" />)
    );

    const scrollEl = screen.getByTestId('session-log-lines');
    Object.defineProperty(scrollEl, 'scrollHeight', { value: 500, configurable: true });
    Object.defineProperty(scrollEl, 'clientHeight', { value: 100, configurable: true });
    Object.defineProperty(scrollEl, 'scrollTop', { value: 100, configurable: true, writable: true });

    // Scroll up to pause auto-scroll
    await act(async () => {
      fireEvent.scroll(scrollEl);
    });
    expect(screen.getByTestId('scroll-to-bottom')).toBeTruthy();

    // New session starts — spec §5.5: auto-scroll resumes on new-session boundary
    await act(async () => {
      rerender(<SessionLog sessionState="In-session" />);
    });

    // stuckToBottom should be re-enabled → button gone
    expect(screen.queryByTestId('scroll-to-bottom')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Test 7: LogLineDto level/source round-trip through rendering
// ---------------------------------------------------------------------------

describe('SessionLog — LogLineDto enum round-trip', () => {
  const allLevels: LogLineDto['level'][] = ['trace', 'debug', 'info', 'warn', 'error'];
  const allSources: LogLineDto['source'][] = ['backend', 'pat', 'transport', 'wire'];

  it('renders lines for all level values (Raw projection)', async () => {
    const levelLines: LogLineDto[] = allLevels.map((level, i) => ({
      timestampIso: `2026-05-19T12:00:0${i}Z`,
      level,
      source: 'backend',
      message: `level-test-${level}`,
    }));
    mockSnapshotLines = levelLines;
    await act(async () => {
      render(<SessionLog sessionState="In-session" />);
    });
    // Switch to Raw so all lines show
    fireEvent.click(screen.getByTestId('toggle-raw'));
    const logArea = screen.getByTestId('session-log-lines');
    for (const level of allLevels) {
      expect(logArea.textContent).toContain(`level-test-${level}`);
    }
  });

  it('renders backend/pat/transport in Human, only wire needs Raw', async () => {
    const sourceLines: LogLineDto[] = allSources.map((source, i) => ({
      timestampIso: `2026-05-19T12:00:0${i}Z`,
      level: 'info' as const,
      source,
      message: `source-test-${source}`,
    }));
    mockSnapshotLines = sourceLines;
    await act(async () => {
      render(<SessionLog sessionState="Idle" />);
    });
    const logArea = screen.getByTestId('session-log-lines');
    // Human projection: backend, pat, transport visible; wire suppressed
    expect(logArea.textContent).toContain('source-test-backend');
    expect(logArea.textContent).toContain('source-test-pat');
    expect(logArea.textContent).toContain('source-test-transport');
    expect(logArea.textContent).not.toContain('source-test-wire');
  });
});
