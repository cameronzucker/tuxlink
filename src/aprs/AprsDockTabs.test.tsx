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

  // tuxlink-w68mb: the ↗ pop-out entry point lives ON the map surface
  // (AprsPositionsMap), NOT in this row — the ~85px text button is exactly
  // what broke the single-row width budget and forced mxqjp's two-row bar.
  it('never renders a pop-out control in the dock row (moved to the map surface)', () => {
    render(
      <AprsDockTabs
        active="aprs"
        unread={0}
        modemEnabled
        onSelect={() => {}}
        onClose={() => {}}
        mapOpen={false}
        onToggleMap={() => {}}
      />,
    );
    expect(screen.getByTestId('aprs-map-toggle')).toBeInTheDocument();
    expect(screen.queryByTestId('aprs-map-popout')).not.toBeInTheDocument();
  });

  // Behavior 2 (spec §5, AMD-3): while popped, the Map slot SWAPS to the
  // compact "Map ↗" focus pathway (text-labeled — the §1 rule) + an adjacent
  // ⇤ dock-back glyph with an accessible name.
  it('while mapPopped, renders the compact "Map ↗" pathway + accessible ⇤ dock-back INSTEAD of the Map toggle', () => {
    const onFocusMap = vi.fn();
    const onDockBackMap = vi.fn();
    render(
      <AprsDockTabs
        active="aprs"
        unread={0}
        modemEnabled
        onSelect={() => {}}
        onClose={() => {}}
        mapOpen={false}
        onToggleMap={() => {}}
        mapPopped
        onFocusMap={onFocusMap}
        onDockBackMap={onDockBackMap}
      />,
    );
    expect(screen.queryByTestId('aprs-map-toggle')).not.toBeInTheDocument();
    const focus = screen.getByTestId('aprs-map-focus');
    expect(focus).toHaveTextContent('Map ↗');
    fireEvent.click(focus);
    expect(onFocusMap).toHaveBeenCalledTimes(1);

    const dockBack = screen.getByTestId('aprs-map-dockback');
    expect(dockBack).toHaveAccessibleName(/dock the tac map back/i);
    fireEvent.click(dockBack);
    expect(onDockBackMap).toHaveBeenCalledTimes(1);
  });
});

// tuxlink-w68mb (supersedes the mxqjp two-row surface bar, operator-rejected
// as restyled jank): the dock header is ONE row in EVERY state. All controls —
// map slot, tabs, station-data pop-out, close — are direct children of the
// single .aprs-dock-tabs flex row; no surface bar, no tab-row wrapper.
describe('single-row dock header (tuxlink-w68mb)', () => {
  const directChildren = () =>
    Array.from(screen.getByTestId('aprs-dock-tabs').children).map(
      (el) => el.getAttribute('data-testid') ?? el.className,
    );

  it('docked with map controls: one row holds [Map toggle][tabs][x]', () => {
    render(
      <AprsDockTabs
        active="aprs"
        unread={0}
        modemEnabled
        onSelect={() => {}}
        onClose={() => {}}
        mapOpen={false}
        onToggleMap={() => {}}
      />,
    );
    expect(screen.queryByTestId('aprs-dock-surfacebar')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-dock-tabrow')).not.toBeInTheDocument();
    expect(directChildren()).toEqual(['aprs-map-toggle', expect.any(String), 'aprs-dock-close']);
  });

  it('popped: one row holds [pathway][tabs][x] (the widest state still fits the single row)', () => {
    render(
      <AprsDockTabs
        active="aprs"
        unread={0}
        modemEnabled
        onSelect={() => {}}
        onClose={() => {}}
        onToggleMap={() => {}}
        mapPopped
        onFocusMap={() => {}}
        onDockBackMap={() => {}}
      />,
    );
    expect(screen.queryByTestId('aprs-dock-surfacebar')).not.toBeInTheDocument();
    expect(directChildren()).toEqual([
      'aprs-map-popped-controls',
      expect.any(String),
      'aprs-dock-close',
    ]);
  });

  it('callers without map controls keep the same single row', () => {
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={() => {}} onClose={() => {}} />);
    expect(screen.queryByTestId('aprs-dock-surfacebar')).not.toBeInTheDocument();
    expect(screen.getByTestId('aprs-dock-close')).toBeInTheDocument();
  });
});
