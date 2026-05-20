import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusBar } from './StatusBar';
import type { StatusBarData } from './useStatus';

function data(over: Partial<StatusBarData> = {}): StatusBarData {
  return {
    callsign: 'W4PHS',
    grid: 'EM75',
    gridTooltip: null,
    state: { label: 'Idle', tone: 'idle' },
    ...over,
  };
}

describe('<StatusBar> (Mock D)', () => {
  it('renders nothing when show=false (zero height)', () => {
    const { container } = render(<StatusBar show={false} data={data()} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders dot+state, callsign · grid, and the version', () => {
    render(<StatusBar show data={data()} />);
    expect(screen.getByTestId('status-bar-state')).toHaveTextContent('Idle');
    expect(screen.getByTestId('status-bar-dot').className).toContain('idle');
    expect(screen.getByTestId('status-bar-station')).toHaveTextContent('W4PHS · EM75');
    expect(screen.getByTestId('status-bar-version')).toHaveTextContent('v0.0.1');
  });

  it('the dot tone tracks the connection state tone', () => {
    render(<StatusBar show data={data({ state: { label: 'Connected', tone: 'good' } })} />);
    expect(screen.getByTestId('status-bar-dot').className).toContain('good');
    expect(screen.getByTestId('status-bar-state')).toHaveTextContent('Connected');
  });

  it('omits the station segment when callsign + grid are both empty', () => {
    render(<StatusBar show data={data({ callsign: '', grid: null })} />);
    expect(screen.queryByTestId('status-bar-station')).toBeNull();
    expect(screen.getByTestId('status-bar-state')).toBeInTheDocument();
    expect(screen.getByTestId('status-bar-version')).toBeInTheDocument();
  });

  it('shows just the callsign when grid is absent', () => {
    render(<StatusBar show data={data({ grid: null })} />);
    expect(screen.getByTestId('status-bar-station')).toHaveTextContent('W4PHS');
  });
});
