import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { AprsDockTabs } from './AprsDockTabs';

describe('AprsDockTabs', () => {
  it('marks the active tab and shows an unread badge on APRS when not active', () => {
    render(<AprsDockTabs active="modem" unread={2} modemEnabled onSelect={() => {}} />);
    expect(screen.getByTestId('aprs-dock-tab-modem')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('aprs-dock-tab-aprs-unread')).toHaveTextContent('2');
  });
  it('calls onSelect with the clicked tab', () => {
    const onSelect = vi.fn();
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId('aprs-dock-tab-modem'));
    expect(onSelect).toHaveBeenCalledWith('modem');
  });
  it('disables the Modem tab when no connection is available', () => {
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled={false} onSelect={() => {}} />);
    expect(screen.getByTestId('aprs-dock-tab-modem')).toBeDisabled();
  });
});
