import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StationResults, stationFavoriteKey } from './StationResults';
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
    const onToggleFavorite = vi.fn();
    const pactor: StationListing = { ...listing, mode: 'pactor' };
    render(
      <StationResults listings={[pactor]} error={null} originGrid="DM43bp" radiusMi={300} onToggleFavorite={onToggleFavorite} />,
    );
    const star = screen.getAllByRole('button', { name: /favorites/i })[0];
    expect(star).toBeDisabled();
  });

  it('renders favorite star state from persisted favorites and toggles that state', () => {
    const onToggleFavorite = vi.fn();
    const favoriteStates = new Map([
      [stationFavoriteKey('ardop-hf', listing.gateways[1]), { id: 'fav-near', starred: true }],
    ]);
    render(
      <StationResults
        listings={[listing]}
        error={null}
        originGrid="DM43bp"
        radiusMi={300}
        favoriteStates={favoriteStates}
        onToggleFavorite={onToggleFavorite}
      />,
    );

    const starred = screen.getByRole('button', { name: /remove NEAR from ardop-hf favorites/i });
    expect(starred).toHaveTextContent('★');
    expect(starred).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(starred);
    expect(onToggleFavorite).toHaveBeenCalledWith(
      expect.objectContaining({ callsign: 'NEAR' }),
      'ardop-hf',
      { id: 'fav-near', starred: true },
    );

    const unstarred = screen.getByRole('button', { name: /add FAR to ardop-hf favorites/i });
    expect(unstarred).toHaveTextContent('☆');
    expect(unstarred).toHaveAttribute('aria-pressed', 'false');
  });

  it('offers Use only for rows matching the active selectable modem mode', () => {
    const onSelectGateway = vi.fn();
    render(
      <StationResults
        listings={[listing]}
        error={null}
        originGrid="DM43bp"
        radiusMi={300}
        selectableMode="ardop-hf"
        onSelectGateway={onSelectGateway}
      />,
    );

    fireEvent.click(screen.getAllByRole('button', { name: 'Use' })[0]);
    expect(onSelectGateway).toHaveBeenCalledWith(
      expect.objectContaining({ callsign: 'NEAR' }),
      'ardop-hf',
    );
  });
});
