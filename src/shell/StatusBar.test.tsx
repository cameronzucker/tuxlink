import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusBar } from './StatusBar';

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
});
