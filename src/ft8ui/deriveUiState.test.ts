// src/ft8ui/deriveUiState.test.ts
//
// Tests for the total ServiceAxisDto (+ SlotPhaseDto, blocked reason,
// configured-device) -> Ft8UiState mapping (Task B2).
//
// Structure:
//   1. Totality — every axis x every blocked reason x representative
//      slotPhase yields a DEFINED state, and the phase rows
//      (waiting-first-slot / band-dead / decoding) appear ONLY when
//      axis === 'listening'.
//   2. Named-row assertions from the brief's §States table.
//   3. Flags — all three computed and returned as a separate overlay,
//      independent of which state was chosen.

import { describe, it, expect } from 'vitest';
import { deriveUiState } from './deriveUiState';
import type { BlockedReasonDto, Ft8Snapshot, Ft8UiState, ServiceAxisDto, SlotPhaseDto } from './ft8Types';

// ---------------------------------------------------------------------------
// Builder — mirrors the makeSnapshot() pattern in useFt8Listener.test.ts.
// ---------------------------------------------------------------------------

function makeSnapshot(over: Partial<Ft8Snapshot> = {}): Ft8Snapshot {
  return {
    service: { axis: 'listening' },
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
    slotPhase: 'waiting-first-slot',
    band: '20m',
    dialHz: 14074000,
    bandSource: 'cat-confirmed',
    bandLabelConfirmedUtcMs: 1000,
    sweep: { mode: 'inactive', bandIdx: null, dwellProgress: null },
    engineVersion: 'jt9 2.6.1',
    nConsecutive: 0,
    kConsecutive: 0,
    lastSlotUtcMs: null,
    lastFailure: null,
    availableDevices: null,
    ringTail: [],
    sweepConfig: { enabled: false, bands: [], dwellSlots: 4 },
    configuredDeviceName: null,
    ...over,
  };
}

const PHASE_ROWS: readonly Ft8UiState[] = ['waiting-first-slot', 'band-dead', 'decoding'];

const NON_BLOCKED_AXES: readonly ServiceAxisDto[] = [
  { axis: 'stopped' },
  { axis: 'starting' },
  { axis: 'listening' },
  { axis: 'yielded' },
  { axis: 'stopping' },
];

const BLOCKED_REASONS: readonly BlockedReasonDto[] = [
  'device-absent',
  'needs-device-selection',
  'wsjtx-absent',
  'unsupported-sample-rate',
  'capture-wedged',
];

const SLOT_PHASES: readonly SlotPhaseDto[] = ['waiting-first-slot', 'decoded', 'band-dead'];

// ---------------------------------------------------------------------------
// 1. Totality
// ---------------------------------------------------------------------------

describe('deriveUiState — totality', () => {
  for (const service of NON_BLOCKED_AXES) {
    for (const slotPhase of SLOT_PHASES) {
      it(`axis=${service.axis} slotPhase=${slotPhase} yields a defined state, phase-row gated on listening`, () => {
        const { state } = deriveUiState(makeSnapshot({ service, slotPhase }));
        expect(state).toBeDefined();
        expect(typeof state).toBe('string');

        if (service.axis === 'listening') {
          expect(PHASE_ROWS).toContain(state);
        } else {
          // A stopped/starting/yielded/stopping service must NEVER render a
          // phase row (never green), regardless of stale slotPhase.
          expect(PHASE_ROWS).not.toContain(state);
        }
      });
    }
  }

  for (const reason of BLOCKED_REASONS) {
    for (const slotPhase of SLOT_PHASES) {
      for (const configuredDeviceName of [null, 'ICOM IC-7300'] as const) {
        it(`axis=blocked reason=${reason} slotPhase=${slotPhase} device=${configuredDeviceName ?? 'none'} yields a defined non-phase state`, () => {
          const { state } = deriveUiState(
            makeSnapshot({ service: { axis: 'blocked', reason }, slotPhase, configuredDeviceName })
          );
          expect(state).toBeDefined();
          expect(typeof state).toBe('string');
          // A blocked service must NEVER render a phase row.
          expect(PHASE_ROWS).not.toContain(state);
        });
      }
    }
  }
});

// ---------------------------------------------------------------------------
// 2. Named-row assertions
// ---------------------------------------------------------------------------

describe('deriveUiState — named rows', () => {
  it('stopped service with stale decoded phase renders off, never decoding', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'stopped' }, slotPhase: 'decoded' }));
    expect(state).toBe('off');
  });

  it('stopped service with any other stale slotPhase still renders off', () => {
    for (const slotPhase of SLOT_PHASES) {
      const { state } = deriveUiState(makeSnapshot({ service: { axis: 'stopped' }, slotPhase }));
      expect(state).toBe('off');
    }
  });

  it('starting renders transitional', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'starting' } }));
    expect(state).toBe('transitional');
  });

  it('stopping renders transitional', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'stopping' } }));
    expect(state).toBe('transitional');
  });

  it('blocked/unsupported-sample-rate renders needs-setup', () => {
    const { state } = deriveUiState(
      makeSnapshot({ service: { axis: 'blocked', reason: 'unsupported-sample-rate' } })
    );
    expect(state).toBe('needs-setup');
  });

  it('blocked/wsjtx-absent renders needs-setup', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'blocked', reason: 'wsjtx-absent' } }));
    expect(state).toBe('needs-setup');
  });

  it('blocked/needs-device-selection renders needs-setup', () => {
    const { state } = deriveUiState(
      makeSnapshot({ service: { axis: 'blocked', reason: 'needs-device-selection' } })
    );
    expect(state).toBe('needs-setup');
  });

  it('blocked/capture-wedged renders wedged (named assertion, RED restart banner)', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'blocked', reason: 'capture-wedged' } }));
    expect(state).toBe('wedged');
  });

  it('blocked/device-absent WITH a configured device renders device-lost', () => {
    const { state } = deriveUiState(
      makeSnapshot({
        service: { axis: 'blocked', reason: 'device-absent' },
        configuredDeviceName: 'ICOM IC-7300',
      })
    );
    expect(state).toBe('device-lost');
  });

  it('blocked/device-absent WITHOUT a configured device renders needs-setup', () => {
    const { state } = deriveUiState(
      makeSnapshot({ service: { axis: 'blocked', reason: 'device-absent' }, configuredDeviceName: null })
    );
    expect(state).toBe('needs-setup');
  });

  it('yielded renders yielded', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'yielded' } }));
    expect(state).toBe('yielded');
  });

  it('listening + slotPhase decoded renders decoding', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'listening' }, slotPhase: 'decoded' }));
    expect(state).toBe('decoding');
  });

  it('listening + slotPhase band-dead renders band-dead', () => {
    const { state } = deriveUiState(makeSnapshot({ service: { axis: 'listening' }, slotPhase: 'band-dead' }));
    expect(state).toBe('band-dead');
  });

  it('listening + slotPhase waiting-first-slot renders waiting-first-slot', () => {
    const { state } = deriveUiState(
      makeSnapshot({ service: { axis: 'listening' }, slotPhase: 'waiting-first-slot' })
    );
    expect(state).toBe('waiting-first-slot');
  });
});

// ---------------------------------------------------------------------------
// 3. Flags — computed independently of state, all three present.
// ---------------------------------------------------------------------------

describe('deriveUiState — flags overlay', () => {
  it('computes all three flags from snapshot.flags regardless of state', () => {
    const cases: ServiceAxisDto[] = [
      { axis: 'stopped' },
      { axis: 'starting' },
      { axis: 'listening' },
      { axis: 'yielded' },
      { axis: 'blocked', reason: 'capture-wedged' },
      { axis: 'stopping' },
    ];
    for (const service of cases) {
      const { flags } = deriveUiState(
        makeSnapshot({ service, flags: { clockUnsynced: true, catFixedBand: true, jt9Degraded: true } })
      );
      expect(flags).toEqual({ clockUnsynced: true, catFixedBand: true, jt9Degraded: true });
    }
  });

  it('flags reflect false when the snapshot reports no health issues', () => {
    const { flags } = deriveUiState(
      makeSnapshot({ flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false } })
    );
    expect(flags).toEqual({ clockUnsynced: false, catFixedBand: false, jt9Degraded: false });
  });

  it('flags do not vary with state — a wedged snapshot with all-clear flags reports all-clear', () => {
    const { state, flags } = deriveUiState(
      makeSnapshot({
        service: { axis: 'blocked', reason: 'capture-wedged' },
        flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
      })
    );
    expect(state).toBe('wedged');
    expect(flags).toEqual({ clockUnsynced: false, catFixedBand: false, jt9Degraded: false });
  });

  it('each flag can be independently true without affecting the others', () => {
    const { flags: f1 } = deriveUiState(
      makeSnapshot({ flags: { clockUnsynced: true, catFixedBand: false, jt9Degraded: false } })
    );
    expect(f1).toEqual({ clockUnsynced: true, catFixedBand: false, jt9Degraded: false });

    const { flags: f2 } = deriveUiState(
      makeSnapshot({ flags: { clockUnsynced: false, catFixedBand: true, jt9Degraded: false } })
    );
    expect(f2).toEqual({ clockUnsynced: false, catFixedBand: true, jt9Degraded: false });

    const { flags: f3 } = deriveUiState(
      makeSnapshot({ flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: true } })
    );
    expect(f3).toEqual({ clockUnsynced: false, catFixedBand: false, jt9Degraded: true });
  });
});
