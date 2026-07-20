// src/dock/strips.test.tsx — tests for the popped-window mini status strips
// (bd tuxlink-dmwte, spec §4 "chrome option B"). review-loop-3 F4: strips.tsx
// shipped with zero tests.
//
// Every Tauri-touching dependency strips.tsx pulls in (useAprsPositions,
// useAprsChat, useRoutines, useParkedRuns, listRuns, listenRoutinesEvents) is
// mocked at the HOOK/API-MODULE boundary, mirroring routinesApi.test.ts's
// house per-file `vi.mock` pattern — strips.tsx itself never touches
// `@tauri-apps/api/core` directly, so there is no raw `invoke` mock in this
// file and the "invoke mocks get called with NO args at teardown" pitfall
// (feedback_vitest_invoke_mock_cleanup_call) doesn't apply here: nothing in
// this file registers an `invoke` mock for cleanup to call.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor, act } from '@testing-library/react';
import type { HeardPosition } from '../aprs/aprsTypes';
import type { HeardStation, ChannelMessage } from '../aprs/aprsTypes';
import type { RunListEntry } from '../routines/routinesApi';

const { mockUseAprsPositions } = vi.hoisted(() => ({ mockUseAprsPositions: vi.fn() }));
vi.mock('../aprs/useAprsPositions', () => ({ useAprsPositions: mockUseAprsPositions }));

const { mockUseAprsChat } = vi.hoisted(() => ({ mockUseAprsChat: vi.fn() }));
vi.mock('../aprs/useAprsChat', () => ({ useAprsChat: mockUseAprsChat }));

const { mockUseRoutines } = vi.hoisted(() => ({ mockUseRoutines: vi.fn() }));
vi.mock('../routines/useRoutines', () => ({ useRoutines: mockUseRoutines }));

const { mockUseParkedRuns } = vi.hoisted(() => ({ mockUseParkedRuns: vi.fn() }));
vi.mock('../routines/ConsentGate', () => ({ useParkedRuns: mockUseParkedRuns }));

const { mockListRuns } = vi.hoisted(() => ({ mockListRuns: vi.fn() }));
vi.mock('../routines/routinesApi', () => ({ listRuns: mockListRuns }));

const { mockListenRoutinesEvents, mockUnlisten } = vi.hoisted(() => ({
  mockListenRoutinesEvents: vi.fn(),
  mockUnlisten: vi.fn(),
}));
vi.mock('../routines/routinesEvents', () => ({ listenRoutinesEvents: mockListenRoutinesEvents }));

// bd tuxlink-9obx2 (Station Intelligence pop-out): mock at the same hook
// boundary as the three above.
const { mockUseStations } = vi.hoisted(() => ({ mockUseStations: vi.fn() }));
vi.mock('../catalog/useStations', () => ({ useStations: mockUseStations }));

import { RoutinesStrip, TacMapStrip, ChatStrip, StationIntelStrip } from './strips';
import type { StationListing } from '../catalog/stationTypes';

function position(partial: Partial<HeardPosition> = {}): HeardPosition {
  return {
    call: 'KI7ABC',
    lat: 0,
    lon: 0,
    symbolTable: '/',
    symbolCode: '>',
    comment: '',
    at: Date.now(),
    ambiguity: 0,
    ...partial,
  } as HeardPosition;
}

function heardStation(partial: Partial<HeardStation> = {}): HeardStation {
  return { call: 'KI7ABC', lastHeard: Date.now(), ...partial };
}

function channelMessage(partial: Partial<ChannelMessage> = {}): ChannelMessage {
  return {
    id: 'm1',
    direction: 'in',
    from: 'KK6XYZ',
    to: null,
    text: 'hi',
    kind: 'message',
    msgid: null,
    at: Date.now(),
    ...partial,
  };
}

function run(partial: Partial<RunListEntry> = {}): RunListEntry {
  return {
    runId: 'r1',
    routine: 'x',
    dryRun: false,
    startedUnix: 0,
    finishedUnix: null,
    state: 'running',
    ...partial,
  };
}

// bd tuxlink-9obx2: a minimal one-gateway listing, matching the N0DAJ fixture
// shape StationFinderPanel.test.tsx already uses for the same backend command.
function stationListing(partial: Partial<StationListing> = {}): StationListing {
  return {
    mode: 'vara-hf',
    title: null,
    parsedOk: true,
    raw: '',
    fetchedAtMs: Date.now(),
    gateways: [
      {
        channel: 'N0DAJ', callsign: 'N0DAJ', sysopName: 'Doug', grid: 'DM34oa',
        location: 'Wickenburg, AZ', frequenciesKhz: [7103], lastUpdate: null,
        email: null, homepage: null, antenna: null,
      },
    ],
    ...partial,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockUseAprsPositions.mockReturnValue({ positions: [] });
  mockUseAprsChat.mockReturnValue({ heardStations: [], messages: [] });
  mockUseRoutines.mockReturnValue({ nextFires: {} });
  mockUseParkedRuns.mockReturnValue({ parked: [] });
  mockListRuns.mockResolvedValue([]);
  mockUnlisten.mockClear();
  mockListenRoutinesEvents.mockImplementation(() => Promise.resolve(mockUnlisten));
  mockUseStations.mockReturnValue({ listings: [], loading: false, error: null, fetch: vi.fn() });
});

afterEach(() => {
  cleanup();
});

describe('useNowTick / TacMapStrip liveness (review-loop-3 F4a)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('ticks every second so the "ago" text advances, and clears its interval on unmount (no timer leak)', () => {
    const mountedAt = Date.now();
    mockUseAprsPositions.mockReturnValue({
      positions: [position({ at: mountedAt - 5000 })],
    });

    const { unmount } = render(<TacMapStrip />);
    expect(screen.getByTestId('pop-strip-tac-map').textContent).toContain('5s ago');

    // Exactly one interval timer is live while mounted.
    expect(vi.getTimerCount()).toBe(1);

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.getByTestId('pop-strip-tac-map').textContent).toContain('1m ago');

    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('renders "no packets heard" when nothing has been received', () => {
    mockUseAprsPositions.mockReturnValue({ positions: [] });
    render(<TacMapStrip />);
    expect(screen.getByTestId('pop-strip-tac-map').textContent).toContain('no packets heard');
  });
});

describe('useRunningCount subscription lifecycle (review-loop-3 F4b)', () => {
  it('subscribes via listenRoutinesEvents on mount and unsubscribes on unmount', async () => {
    const { unmount } = render(<RoutinesStrip />);

    await waitFor(() => expect(mockListenRoutinesEvents).toHaveBeenCalledTimes(1));
    expect(mockUnlisten).not.toHaveBeenCalled();

    unmount();
    expect(mockUnlisten).toHaveBeenCalledTimes(1);
  });

  it('re-fetches listRuns() when a run-count-relevant event fires', async () => {
    let handler: ((e: { kind: string }) => void) | undefined;
    mockListenRoutinesEvents.mockImplementation((cb: (e: { kind: string }) => void) => {
      handler = cb;
      return Promise.resolve(mockUnlisten);
    });
    mockListRuns.mockResolvedValueOnce([]).mockResolvedValueOnce([run({ state: 'running' })]);

    render(<RoutinesStrip />);
    await waitFor(() => expect(mockListRuns).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId('pop-strip-routines').textContent).toContain('0 running');

    act(() => {
      handler?.({ kind: 'runStarted' });
    });
    await waitFor(() => expect(screen.getByTestId('pop-strip-routines').textContent).toContain('1 running'));
  });
});

describe('strip vitals render from mocked hook data (review-loop-3 F4c)', () => {
  beforeEach(() => {
    // Fake ONLY `Date` (formatUtc's "isToday" check needs a pinned clock) —
    // leaving setTimeout/setInterval real so `waitFor`'s own internal
    // polling (used below to await the async listRuns() resolution) isn't
    // starved waiting on a fake timer nobody advances.
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-07-15T12:00:00.000Z'));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('RoutinesStrip shows parked count, running count, and the soonest scheduled fire', async () => {
    mockUseParkedRuns.mockReturnValue({ parked: [{ runId: 'a' }, { runId: 'b' }] });
    mockListRuns.mockResolvedValue([
      run({ runId: 'r1', state: 'running' }),
      run({ runId: 'r2', state: 'completed' }), // not counted — terminal
      run({ runId: 'r3', state: 'pending' }),
    ]);
    const soonest = Date.UTC(2026, 6, 15, 15, 30) / 1000; // 2026-07-15T15:30Z
    mockUseRoutines.mockReturnValue({ nextFires: { routineA: soonest, routineB: soonest + 3600 } });

    render(<RoutinesStrip />);

    await waitFor(() => {
      const text = screen.getByTestId('pop-strip-routines').textContent ?? '';
      expect(text).toContain('2 parked');
      expect(text).toContain('2 running');
      expect(text).toContain('next 15:30Z');
    });
  });

  it('RoutinesStrip shows "no scheduled fire" when nextFires is empty', () => {
    mockUseRoutines.mockReturnValue({ nextFires: {} });
    render(<RoutinesStrip />);
    expect(screen.getByTestId('pop-strip-routines').textContent).toContain('no scheduled fire');
  });

  it("TacMapStrip mounts its positions hook in client role (seeds from the host snapshot, spec §4)", () => {
    render(<TacMapStrip />);
    // Rider B: a bare useAprsPositions() would show "no packets heard" beside a
    // seeded live map — a false-liveness signal. The client role seeds it.
    expect(mockUseAprsPositions).toHaveBeenCalledWith({ snapshotRole: 'client' });
  });

  it('ChatStrip mounts its chat hook in client role (seeds last-heard from the host snapshot)', () => {
    render(<ChatStrip />);
    expect(mockUseAprsChat).toHaveBeenCalledWith({ snapshotRole: 'client' });
  });

  it("TacMapStrip formats the last-packet age from the mocked hook's positions", () => {
    const now = Date.now();
    mockUseAprsPositions.mockReturnValue({
      positions: [position({ call: 'A', at: now - 500_000 }), position({ call: 'B', at: now - 10_000 })],
    });
    render(<TacMapStrip />);
    // The newest of the two heard positions (10s ago) is what's shown.
    expect(screen.getByTestId('pop-strip-tac-map').textContent).toContain('10s ago');
  });

  it("ChatStrip shows the last-heard callsign from the mocked hook's heardStations", () => {
    mockUseAprsChat.mockReturnValue({
      heardStations: [heardStation({ call: 'N0CALL' })],
      messages: [],
    });
    render(<ChatStrip />);
    const text = screen.getByTestId('pop-strip-chat').textContent ?? '';
    expect(text).toContain('last heard N0CALL');
  });

  it('ChatStrip shows "no stations heard" when nothing has been heard', () => {
    mockUseAprsChat.mockReturnValue({ heardStations: [], messages: [] });
    render(<ChatStrip />);
    expect(screen.getByTestId('pop-strip-chat').textContent).toContain('no stations heard');
  });

  it('ChatStrip wires the REAL unread count (countUnread), reading 0 while the popped feed is viewed', () => {
    // The strip co-renders with the popped window's live feed, so it is always
    // the "viewing" context (AppShell's viewingAprsChat===true analogue): even
    // with inbound traffic present, countUnread against the advancing seen-
    // watermark honestly reports 0. A true-but-boring 0, not a fabricated count.
    const now = Date.now();
    mockUseAprsChat.mockReturnValue({
      heardStations: [heardStation({ call: 'N0CALL' })],
      messages: [
        channelMessage({ id: 'a', direction: 'in', at: now - 4000 }),
        channelMessage({ id: 'b', direction: 'in', at: now - 1000 }),
      ],
    });
    render(<ChatStrip />);
    const unread = screen.getByTestId('pop-strip-chat-unread');
    expect(unread).toHaveTextContent('0 unread');
  });

  // bd tuxlink-9obx2: StationIntelStrip's one vital is the cataloged station
  // count, the one number the panel itself never renders as text (spec §4
  // adrev R4-F8: the band/mode filters, list-freshness age, and FT-8
  // listener state are ALL already visible in-panel, so none of those
  // qualify).
  it('StationIntelStrip fetches on mount and shows the aggregated station count', async () => {
    const fetchMock = vi.fn();
    mockUseStations.mockReturnValue({
      listings: [stationListing()],
      loading: false,
      error: null,
      fetch: fetchMock,
    });
    render(<StationIntelStrip />);
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(screen.getByTestId('pop-strip-station-intelligence').textContent).toContain(
      '1 station cataloged',
    );
  });

  it('StationIntelStrip pluralizes for zero and for more than one station', () => {
    mockUseStations.mockReturnValue({ listings: [], loading: false, error: null, fetch: vi.fn() });
    render(<StationIntelStrip />);
    expect(screen.getByTestId('pop-strip-station-intelligence').textContent).toContain(
      '0 stations cataloged',
    );
    cleanup();

    mockUseStations.mockReturnValue({
      listings: [
        stationListing({
          gateways: [
            { channel: 'N0DAJ', callsign: 'N0DAJ', sysopName: null, grid: 'DM34oa', location: null, frequenciesKhz: [7103], lastUpdate: null, email: null, homepage: null, antenna: null },
            { channel: 'K7ABC', callsign: 'K7ABC', sysopName: null, grid: 'CN87xx', location: null, frequenciesKhz: [3590], lastUpdate: null, email: null, homepage: null, antenna: null },
          ],
        }),
      ],
      loading: false,
      error: null,
      fetch: vi.fn(),
    });
    render(<StationIntelStrip />);
    expect(screen.getByTestId('pop-strip-station-intelligence').textContent).toContain(
      '2 stations cataloged',
    );
  });
});
