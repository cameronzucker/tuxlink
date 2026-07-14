/**
 * Tests for routinesEvents.ts — the `routines:event` channel wrapper
 * (plan-5 Task 6).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockListen } = vi.hoisted(() => ({ mockListen: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));

import { ROUTINES_EVENT, listenRoutinesEvents, type RoutinesEvent } from './routinesEvents';

describe('listenRoutinesEvents', () => {
  const unlisten = vi.fn();

  beforeEach(() => {
    mockListen.mockReset();
    unlisten.mockReset();
    mockListen.mockResolvedValue(unlisten);
  });

  it('subscribes on the routines:event channel', async () => {
    await listenRoutinesEvents(() => {});
    expect(mockListen).toHaveBeenCalledTimes(1);
    expect(mockListen.mock.calls[0]?.[0]).toBe(ROUTINES_EVENT);
    expect(ROUTINES_EVENT).toBe('routines:event');
  });

  it('unwraps the Tauri event envelope down to the bare payload', async () => {
    const handler = vi.fn();
    await listenRoutinesEvents(handler);
    const registeredCallback = mockListen.mock.calls[0]?.[1] as (e: { payload: RoutinesEvent }) => void;

    const payload: RoutinesEvent = { kind: 'runStarted', runId: 'run-1', routine: 'x', dryRun: false };
    registeredCallback({ payload });

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('resolves the unlisten function `listen` returned', async () => {
    const result = await listenRoutinesEvents(() => {});
    expect(result).toBe(unlisten);
  });

  it('round-trips every RoutinesEvent variant through the handler', async () => {
    const handler = vi.fn();
    await listenRoutinesEvents(handler);
    const registeredCallback = mockListen.mock.calls[0]?.[1] as (e: { payload: RoutinesEvent }) => void;

    const variants: RoutinesEvent[] = [
      { kind: 'runStarted', runId: 'r1', routine: 'morning-ics', dryRun: false },
      { kind: 'runFinished', runId: 'r1', state: 'completed' },
      { kind: 'runFinished', runId: 'r2', state: 'interrupted', reason: 'process terminated' },
      { kind: 'stateChanged', runId: 'r1', state: 'awaiting_radio' },
      { kind: 'stepCompleted', runId: 'r1', stepId: 's1', ok: true },
      { kind: 'awaitingConsent', runId: 'r1', stepId: 's2' },
      { kind: 'libraryChanged', entity: 'stationSet', name: 'or-gateways' },
      { kind: 'scheduledFire', routine: 'morning-ics', runId: 'r3', at: 1_752_400_000 },
      { kind: 'scheduleSkipped', routine: 'morning-ics', at: 1_752_400_000, reason: 'previous run still active' },
      { kind: 'scheduleRefused', routine: 'auto-tx', at: 1_752_400_000, reason: 'no recorded acknowledgment' },
      { kind: 'missedFires', routine: 'overnight-sync', missed: 7, policy: 'run_once_on_launch', ran: true },
    ];

    for (const payload of variants) {
      registeredCallback({ payload });
    }

    expect(handler).toHaveBeenCalledTimes(variants.length);
    variants.forEach((payload, i) => {
      expect(handler).toHaveBeenNthCalledWith(i + 1, payload);
    });
  });
});
