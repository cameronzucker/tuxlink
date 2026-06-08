import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StationResults } from './StationResults';
import type { Gateway, StationListing } from './stationTypes';

function gw(callsign: string, grid: string): Gateway {
  return {
    channel: `${callsign}.WINLINK`,
    callsign,
    sysopName: null,
    grid,
    location: null,
    frequenciesKhz: [7101],
    lastUpdate: null,
    email: null,
    homepage: null,
  };
}

const listing: StationListing = {
  mode: 'ardop-hf',
  title: 't',
  parsedOk: true,
  raw: 'r',
  fetchedAtMs: Date.now(),
  gateways: [gw('FAR', 'JN49'), gw('NEAR', 'DM43')],
};

describe('StationResults', () => {
  it('sorts by distance from origin (nearer first)', () => {
    render(<StationResults listings={[listing]} error={null} originGrid="DM43bp" radiusMi={300} />);
    const rows = screen.getAllByTestId('gateway-row');
    expect(rows[0]).toHaveTextContent('NEAR');
  });

  it('dims rows beyond the radius rather than hiding them', () => {
    render(<StationResults listings={[listing]} error={null} originGrid="DM43bp" radiusMi={50} />);
    expect(screen.getByText('FAR')).toBeTruthy(); // still present
    expect(screen.getAllByTestId('gateway-row').some((r) => r.className.includes('is-dim'))).toBe(true);
  });

  it('shows the message-request fallback offer on error', () => {
    render(<StationResults listings={[]} error="couldn't reach listing service" originGrid="" radiusMi={300} />);
    expect(screen.getByText(/request by message instead/i)).toBeTruthy();
  });

  it('shows an "as of <time>" freshness caption when fetchedAtMs is present', () => {
    render(<StationResults listings={[listing]} error={null} originGrid="DM43bp" radiusMi={300} />);
    expect(screen.getByText(/as of/i)).toBeTruthy();
  });

  it('marks a cached listing as stale when older than the TTL', () => {
    const stale = { ...listing, fetchedAtMs: 0 }; // epoch → very old
    render(<StationResults listings={[stale]} error={null} originGrid="DM43bp" radiusMi={300} />);
    expect(screen.getByText(/cached — may be stale/i)).toBeTruthy();
  });

  it('disables the ★ for pactor/robust-packet (no CF favorite target) even when wired', () => {
    const onAddFavorite = vi.fn();
    const pactor: StationListing = { ...listing, mode: 'pactor' };
    render(
      <StationResults listings={[pactor]} error={null} originGrid="DM43bp" radiusMi={300} onAddFavorite={onAddFavorite} />,
    );
    const star = screen.getAllByRole('button', { name: /favorites/i })[0];
    expect(star).toBeDisabled();
  });
});
