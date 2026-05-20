import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusBar } from './StatusBar';
import type { StatusTone } from './useStatus';

// DEV_FIXTURE is false under vitest, so StatusBar renders the passed `state`.
const ready = { label: 'Telnet ready', tone: 'good' as StatusTone };

describe('<StatusBar> (Mock B)', () => {
  it('renders nothing when show=false (zero height)', () => {
    const { container } = render(<StatusBar show={false} unread={3} state={ready} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders connection state, unread count, and versions', () => {
    render(<StatusBar show unread={3} state={ready} />);
    expect(screen.getByTestId('status-bar-state')).toHaveTextContent('Telnet ready');
    expect(screen.getByTestId('status-bar-dot').className).toContain('good');
    expect(screen.getByTestId('status-bar-unread')).toHaveTextContent('3 unread');
    expect(screen.getByTestId('status-bar-version')).toHaveTextContent('v0.0.1 · Pat 1.0.0');
  });

  it('the dot tone tracks the connection state', () => {
    render(<StatusBar show unread={0} state={{ label: 'Idle', tone: 'idle' }} />);
    expect(screen.getByTestId('status-bar-dot').className).toContain('idle');
    expect(screen.getByTestId('status-bar-unread')).toHaveTextContent('0 unread');
  });
});
