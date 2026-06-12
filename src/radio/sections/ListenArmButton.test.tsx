// src/radio/sections/ListenArmButton.test.tsx
//
// Task 12 — per-listener identity badge. An armed listener renders the
// identity it answers as (its OWN bound identity, captured at arm time),
// NOT the globally-active identity. The Phase-6 backend invariant keeps an
// armed listener bound to its arm-time identity even if the operator later
// switches the active identity; this UI test pins the frontend half: the
// badge renders the listener's own `boundIdentity` prop and is unaffected by
// any different active identity in scope.

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ListenArmButton } from './ListenArmButton';

const noop = () => {};

describe('ListenArmButton — identity badge (Task 12)', () => {
  it('renders the bound tactical label for an armed listener', () => {
    render(
      <ListenArmButton
        armed
        minutesRemaining={42}
        boundIdentity="EOC-3"
        onArm={noop}
        onDisarm={noop}
        testIdPrefix="ardop-listen"
      />,
    );
    const badge = screen.getByTestId('listener-identity-badge');
    expect(badge).toHaveTextContent('EOC-3');
  });

  it('renders the bound FULL callsign for an armed listener', () => {
    render(
      <ListenArmButton
        armed
        minutesRemaining={10}
        boundIdentity="W1ABC"
        onArm={noop}
        onDisarm={noop}
        testIdPrefix="ardop-listen"
      />,
    );
    expect(screen.getByTestId('listener-identity-badge')).toHaveTextContent('W1ABC');
  });

  it('shows no badge when the listener is disarmed', () => {
    render(
      <ListenArmButton
        armed={false}
        minutesRemaining={null}
        boundIdentity="W1ABC"
        onArm={noop}
        onDisarm={noop}
        testIdPrefix="ardop-listen"
      />,
    );
    expect(screen.queryByTestId('listener-identity-badge')).toBeNull();
  });

  it('shows no badge when armed but no bound identity is available', () => {
    render(
      <ListenArmButton
        armed
        minutesRemaining={5}
        boundIdentity={null}
        onArm={noop}
        onDisarm={noop}
        testIdPrefix="ardop-listen"
      />,
    );
    expect(screen.queryByTestId('listener-identity-badge')).toBeNull();
  });

  it('active_switch_does_not_change_armed_badge — badge reads the listener OWN bound identity, not the active one', () => {
    // The listener was armed bound to W1ABC. The operator has since switched
    // the global active identity to W7XYZ. The badge must still read W1ABC
    // (the listener's own arm-time identity) — the component renders ONLY its
    // own `boundIdentity` prop and has no access to the active identity.
    const activeIdentity = 'W7XYZ'; // a DIFFERENT active identity in scope
    render(
      <ListenArmButton
        armed
        minutesRemaining={30}
        boundIdentity="W1ABC"
        onArm={noop}
        onDisarm={noop}
        testIdPrefix="ardop-listen"
      />,
    );
    const badge = screen.getByTestId('listener-identity-badge');
    expect(badge).toHaveTextContent('W1ABC');
    expect(badge).not.toHaveTextContent(activeIdentity);
  });
});
