import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StationFinderControls } from './StationFinderControls';

const baseProps = {
  band: '40m' as const,
  onBandChange: vi.fn(),
  enabledModes: new Set<'vara-hf' | 'ardop-hf' | 'packet'>(['vara-hf', 'ardop-hf', 'packet']),
  onToggleMode: vi.fn(),
  utcHour: 21,
  localTime: '14:20',
  ssn: 118,
  ssnAgeDays: 2,
  predictionAvailable: true,
  onRefresh: vi.fn(),
  refreshing: false,
};

describe('StationFinderControls', () => {
  it('renders the four HF bands and marks the selected one', () => {
    render(<StationFinderControls {...baseProps} />);
    expect(screen.getByRole('button', { name: /80 m/ })).toBeTruthy();
    expect(screen.getByRole('button', { name: /40 m/ }).getAttribute('aria-pressed')).toBe('true');
  });

  it('fires onBandChange when another band is clicked', () => {
    const onBandChange = vi.fn();
    render(<StationFinderControls {...baseProps} onBandChange={onBandChange} />);
    fireEvent.click(screen.getByRole('button', { name: /20 m/ }));
    expect(onBandChange).toHaveBeenCalledWith('20m');
  });

  it('shows SSN provenance and degrades SFI/K when absent', () => {
    render(<StationFinderControls {...baseProps} />);
    expect(screen.getByText(/SSN/)).toBeTruthy();
    expect(screen.getByText(/118/)).toBeTruthy();
    expect(screen.getByText(/solar data 2d old/)).toBeTruthy();
    expect(screen.queryByText(/SFI/)).toBeNull();
  });

  it('toggles a mode chip', () => {
    const onToggleMode = vi.fn();
    render(<StationFinderControls {...baseProps} onToggleMode={onToggleMode} />);
    fireEvent.click(screen.getByRole('button', { name: /VARA HF/ }));
    expect(onToggleMode).toHaveBeenCalledWith('vara-hf');
  });

  it('notes when prediction is unavailable (distance-only)', () => {
    render(<StationFinderControls {...baseProps} predictionAvailable={false} />);
    expect(screen.getByText(/no forecast/i)).toBeTruthy();
  });

  it('shows the station-list freshness caption when a fetch stamp is present', () => {
    const tenMinAgo = Date.now() - 10 * 60_000;
    render(<StationFinderControls {...baseProps} listFetchedAtMs={tenMinAgo} />);
    expect(screen.getByTestId('list-age').textContent).toMatch(/stations updated 10 min ago/);
  });

  it('omits the freshness caption when no fetch stamp is available', () => {
    render(<StationFinderControls {...baseProps} listFetchedAtMs={null} />);
    expect(screen.queryByTestId('list-age')).toBeNull();
  });
});
