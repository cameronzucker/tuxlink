import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { AprsDockTabs } from './AprsDockTabs';

describe('AprsDockTabs', () => {
  it('marks the active tab and shows an unread badge on APRS when not active', () => {
    render(<AprsDockTabs active="modem" unread={2} modemEnabled onSelect={() => {}} onClose={() => {}} />);
    expect(screen.getByTestId('aprs-dock-tab-modem')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('aprs-dock-tab-aprs-unread')).toHaveTextContent('2');
  });
  it('orders tabs APRS Chat, Station Data, then Modem (Modem far right)', () => {
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={() => {}} onClose={() => {}} />);
    const tabs = screen.getAllByRole('tab').map((t) => t.getAttribute('data-testid'));
    expect(tabs).toEqual([
      'aprs-dock-tab-aprs',
      'aprs-dock-tab-stations',
      'aprs-dock-tab-modem',
    ]);
  });
  it('calls onSelect with the clicked tab', () => {
    const onSelect = vi.fn();
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={onSelect} onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('aprs-dock-tab-modem'));
    expect(onSelect).toHaveBeenCalledWith('modem');
  });
  it('disables the Modem tab when no connection is available', () => {
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled={false} onSelect={() => {}} onClose={() => {}} />);
    expect(screen.getByTestId('aprs-dock-tab-modem')).toBeDisabled();
  });
  it('calls onClose when the close control is clicked (frees the dock)', () => {
    const onClose = vi.fn();
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={() => {}} onClose={onClose} />);
    fireEvent.click(screen.getByTestId('aprs-dock-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('renders a Station Data tab with a live station count and routes selection', () => {
    const onSelect = vi.fn();
    render(
      <AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={onSelect} onClose={() => {}} stationCount={3} />,
    );
    const tab = screen.getByTestId('aprs-dock-tab-stations');
    expect(tab).toHaveTextContent('3');
    fireEvent.click(tab);
    expect(onSelect).toHaveBeenCalledWith('stations');
  });

  it('omits the station count when zero heard', () => {
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={() => {}} onClose={() => {}} stationCount={0} />);
    expect(screen.queryByTestId('aprs-dock-tab-stations-count')).not.toBeInTheDocument();
  });

  it('shows a pop-out control only when onPopOut is provided, and invokes it', () => {
    const onPopOut = vi.fn();
    const { rerender } = render(
      <AprsDockTabs active="stations" unread={0} modemEnabled onSelect={() => {}} onClose={() => {}} stationCount={1} />,
    );
    expect(screen.queryByTestId('aprs-dock-popout')).not.toBeInTheDocument();
    rerender(
      <AprsDockTabs active="stations" unread={0} modemEnabled onSelect={() => {}} onClose={() => {}} stationCount={1} onPopOut={onPopOut} />,
    );
    fireEvent.click(screen.getByTestId('aprs-dock-popout'));
    expect(onPopOut).toHaveBeenCalledTimes(1);
  });
});
