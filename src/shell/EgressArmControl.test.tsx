/**
 * EgressArmControl — operator ARM surface tests (MCP phase 3.6).
 *
 * Covers the four operator-visible states (disarmed / armed / tainted /
 * error), that arming calls back with the chosen duration, that disarming
 * fires, the live countdown, and the pure remaining-time formatter.
 *
 * The component is presentational (state + actions via props), so these tests
 * drive it directly with synthetic EgressStatusDto values — no invoke mock
 * needed here. The invoke wiring is exercised in useEgressArm.test.ts.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { EgressArmControl } from './EgressArmControl';
import {
  formatEgressRemaining,
  EGRESS_STATUS_DISARMED,
  type EgressStatusDto,
} from '../security/egressTypes';

function makeStatus(overrides: Partial<EgressStatusDto> = {}): EgressStatusDto {
  return { ...EGRESS_STATUS_DISARMED, ...overrides };
}

describe('formatEgressRemaining (pure)', () => {
  it('formats sub-hour as MM:SS', () => {
    expect(formatEgressRemaining(0)).toBe('00:00');
    expect(formatEgressRemaining(5)).toBe('00:05');
    expect(formatEgressRemaining(65)).toBe('01:05');
    expect(formatEgressRemaining(15 * 60)).toBe('15:00');
  });

  it('formats >= 1 hour as H:MM:SS', () => {
    expect(formatEgressRemaining(3600)).toBe('1:00:00');
    expect(formatEgressRemaining(4 * 3600 + 2 * 60 + 9)).toBe('4:02:09');
  });

  it('clamps negatives to zero', () => {
    expect(formatEgressRemaining(-10)).toBe('00:00');
  });
});

describe('<EgressArmControl> — disarmed state', () => {
  it('shows OFF + duration presets, no countdown', () => {
    render(
      <EgressArmControl status={makeStatus()} onArm={vi.fn()} onDisarm={vi.fn()} />,
    );
    expect(screen.getByTestId('egress-state').textContent).toContain('OFF');
    expect(screen.getByTestId('egress-presets')).toBeTruthy();
    expect(screen.queryByTestId('egress-countdown')).toBeNull();
    expect(screen.queryByTestId('egress-disarm')).toBeNull();
  });

  it('clicking a preset calls onArm with that duration in seconds', () => {
    const onArm = vi.fn();
    render(
      <EgressArmControl status={makeStatus()} onArm={onArm} onDisarm={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId('egress-arm-3600'));
    expect(onArm).toHaveBeenCalledWith(3600);
  });

  it('disables presets while busy', () => {
    render(
      <EgressArmControl
        status={makeStatus()}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
        busy
      />,
    );
    expect(
      (screen.getByTestId('egress-arm-900') as HTMLButtonElement).disabled,
    ).toBe(true);
  });
});

describe('<EgressArmControl> — armed state', () => {
  it('shows ON + countdown + Disarm, no presets', () => {
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 2535 })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('egress-state').textContent).toContain('ON');
    // 2535s = 42:15
    expect(screen.getByTestId('egress-countdown').textContent).toContain('42:15');
    expect(screen.getByTestId('egress-disarm')).toBeTruthy();
    expect(screen.queryByTestId('egress-presets')).toBeNull();
  });

  it('clicking Disarm calls onDisarm', () => {
    const onDisarm = vi.fn();
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 600 })}
        onArm={vi.fn()}
        onDisarm={onDisarm}
      />,
    );
    fireEvent.click(screen.getByTestId('egress-disarm'));
    expect(onDisarm).toHaveBeenCalledTimes(1);
  });
});

describe('<EgressArmControl> — live countdown ticks down', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('decrements the displayed countdown each second', () => {
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 65 })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('egress-countdown').textContent).toContain('01:05');
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.getByTestId('egress-countdown').textContent).toContain('01:03');
  });
});

describe('<EgressArmControl> — tainted state', () => {
  it('shows LOCKED + the locked explanation, no arm/disarm affordance', () => {
    render(
      <EgressArmControl
        status={makeStatus({ tainted: true })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('egress-state').textContent).toContain('LOCKED');
    expect(screen.getByTestId('egress-locked')).toBeTruthy();
    expect(screen.queryByTestId('egress-presets')).toBeNull();
    expect(screen.queryByTestId('egress-disarm')).toBeNull();
  });

  it('taint wins even when armed is still true (terminal lock)', () => {
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 999, tainted: true })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('egress-state').textContent).toContain('LOCKED');
    expect(screen.queryByTestId('egress-countdown')).toBeNull();
  });
});

describe('<EgressArmControl> — error surfacing', () => {
  it('renders the error message inline', () => {
    render(
      <EgressArmControl
        status={makeStatus()}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
        error="arm duration must be greater than zero"
      />,
    );
    expect(screen.getByTestId('egress-error').textContent).toContain(
      'arm duration must be greater than zero',
    );
  });
});
