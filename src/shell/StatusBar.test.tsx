import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusBar } from './StatusBar';
import type { StatusTone } from './useStatus';
import type { PacketUiState } from '../packet/packetStatus';

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

describe('StatusBar — packet transport', () => {
  it('shows the packet status string when packet is active', () => {
    const packet: PacketUiState = {
      active: true, listening: true, connected: false,
      effectiveCall: 'N7CPZ-7', linkLabel: 'KISS-TCP Dire Wolf',
    };
    render(<StatusBar show unread={3} state={{ label: 'Idle', tone: 'idle' }} packet={packet} />);
    expect(screen.getByTestId('status-bar-state')).toHaveTextContent(
      'Packet 1200 · Listening as N7CPZ-7 · KISS-TCP Dire Wolf',
    );
  });
});
