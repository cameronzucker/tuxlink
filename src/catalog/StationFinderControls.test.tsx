import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StationFinderControls } from './StationFinderControls';
import type { Band } from './bandPlan';

const baseProps = {
  enabledBands: new Set<Band>(['40m']),
  onToggleBand: vi.fn(),
  enabledModes: new Set<'vara-hf' | 'ardop-hf' | 'packet'>(['vara-hf', 'ardop-hf', 'packet']),
  onToggleMode: vi.fn(),
  utcHour: 21,
  localTime: '14:20',
  ssn: 118,
  ssnAgeDays: 2,
  predictionAvailable: true,
  radiusMi: 500 as number | null,
  onRadiusChange: vi.fn(),
  hasOperatorGrid: true,
  search: '',
  onSearchChange: vi.fn(),
  onRefresh: vi.fn(),
  refreshing: false,
};

describe('StationFinderControls', () => {
  it('renders the full amateur HF band set and marks the selected one', () => {
    render(<StationFinderControls {...baseProps} />);
    // Every amateur HF band is offered (not the old ALE-derived 80/40/30/20m subset).
    // Anchor the name match — "60 m" is a substring of "160 m".
    for (const label of ['160 m', '80 m', '60 m', '40 m', '30 m', '20 m', '17 m', '15 m', '12 m', '10 m']) {
      expect(screen.getByRole('button', { name: new RegExp(`^${label}$`) })).toBeTruthy();
    }
    expect(screen.getByRole('button', { name: /^40 m$/ }).getAttribute('aria-pressed')).toBe('true');
    // Multi-select: VHF/UHF is now a selectable filter band (off by default here).
    expect(screen.getByRole('button', { name: /VHF\/UHF/ }).getAttribute('aria-pressed')).toBe('false');
  });

  it('toggles a band (multi-select) without disturbing others', () => {
    const onToggleBand = vi.fn();
    // 40m + 20m both selected → both pressed.
    render(
      <StationFinderControls
        {...baseProps}
        enabledBands={new Set<Band>(['40m', '20m'])}
        onToggleBand={onToggleBand}
      />,
    );
    expect(screen.getByRole('button', { name: /^40 m$/ }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: /^20 m$/ }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: /^15 m$/ }));
    expect(onToggleBand).toHaveBeenCalledWith('15m');
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

  it('fires onSearchChange as the operator types a callsign filter', () => {
    const onSearchChange = vi.fn();
    render(<StationFinderControls {...baseProps} onSearchChange={onSearchChange} />);
    fireEvent.change(screen.getByLabelText(/filter stations by callsign/i), { target: { value: 'N0D' } });
    expect(onSearchChange).toHaveBeenCalledWith('N0D');
  });

  it('changes the search radius', () => {
    const onRadiusChange = vi.fn();
    render(<StationFinderControls {...baseProps} onRadiusChange={onRadiusChange} />);
    fireEvent.change(screen.getByLabelText(/search radius/i), { target: { value: '250' } });
    expect(onRadiusChange).toHaveBeenCalledWith(250);
  });

  it('disables the radius selector + prompts when no operator grid is set', () => {
    render(<StationFinderControls {...baseProps} hasOperatorGrid={false} />);
    expect((screen.getByLabelText(/search radius/i) as HTMLSelectElement).disabled).toBe(true);
    expect(screen.getByText(/set your location/i)).toBeTruthy();
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

  it('surfaces a recomputing affordance while reachability re-sweeps (tuxlink-ziyu)', () => {
    const { rerender } = render(<StationFinderControls {...baseProps} recomputing={false} />);
    expect(screen.queryByTestId('reach-recomputing')).toBeNull();
    rerender(<StationFinderControls {...baseProps} recomputing={true} />);
    const status = screen.getByTestId('reach-recomputing');
    expect(status.textContent).toMatch(/updating reachability/i);
    expect(status.getAttribute('role')).toBe('status');
  });
});
