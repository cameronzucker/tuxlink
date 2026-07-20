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

  // tuxlink-dmwte task 9 (spec §5, behavior 1): a ↗ pop-out control sits beside
  // the Map toggle when NOT popped, and invokes onPopOutMap.
  it('shows a Tac Map pop-out control beside the Map toggle when onPopOutMap is provided, and invokes it', () => {
    const onPopOutMap = vi.fn();
    render(
      <AprsDockTabs
        active="aprs"
        unread={0}
        modemEnabled
        onSelect={() => {}}
        onClose={() => {}}
        mapOpen={false}
        onToggleMap={() => {}}
        onPopOutMap={onPopOutMap}
      />,
    );
    expect(screen.getByTestId('aprs-map-toggle')).toBeInTheDocument();
    // Text-labeled, never icon-only (spec §1/§5): the control carries a visible
    // "Pop out" label and an accessible name including it.
    const popout = screen.getByTestId('aprs-map-popout');
    expect(popout).toHaveTextContent('Pop out');
    expect(popout).toHaveAccessibleName(/pop out/i);
    fireEvent.click(popout);
    expect(onPopOutMap).toHaveBeenCalledTimes(1);
  });

  it('omits the Tac Map pop-out control when onPopOutMap is not provided', () => {
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
    expect(screen.queryByTestId('aprs-map-popout')).not.toBeInTheDocument();
  });

  // Behavior 2 (spec §5): while popped, the Map toggle + pop-out slot SWAPS to
  // the "Tac Map ↗ — in window" focus pathway + an adjacent "⇤ dock back"
  // action — the Map toggle and pop-out button are both gone.
  it('while mapPopped, renders the "in window" pathway + dock-back INSTEAD of the Map toggle/pop-out', () => {
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
        onPopOutMap={() => {}}
        mapPopped
        onFocusMap={onFocusMap}
        onDockBackMap={onDockBackMap}
      />,
    );
    expect(screen.queryByTestId('aprs-map-toggle')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-map-popout')).not.toBeInTheDocument();
    const focus = screen.getByTestId('aprs-map-focus');
    expect(focus).toHaveTextContent('Tac Map ↗ — in window');
    fireEvent.click(focus);
    expect(onFocusMap).toHaveBeenCalledTimes(1);

    const dockBack = screen.getByTestId('aprs-map-dockback');
    expect(dockBack).toHaveTextContent('⇤ dock back');
    fireEvent.click(dockBack);
    expect(onDockBackMap).toHaveBeenCalledTimes(1);
  });
});

// tuxlink-mxqjp: the map companion controls + tab strip + close previously
// shared ONE flex-wrap row; at the dock's ~400px floor the wrap point was
// arbitrary, rendering Map/Pop out as an orphaned tab-shaped row above the
// real tabs (R2 operator report 2026-07-20). The split is now INTENTIONAL:
// a surface bar (map controls + close) above a clean tab row; callers
// without map controls keep the original single row.
describe('two-row dock header (tuxlink-mxqjp)', () => {
  it('map controls and close live in a surface bar above the tab row', () => {
    render(
      <AprsDockTabs
        active="aprs"
        unread={0}
        modemEnabled
        onSelect={() => {}}
        onClose={() => {}}
        mapOpen={false}
        onToggleMap={() => {}}
        onPopOutMap={() => {}}
      />,
    );
    const bar = screen.getByTestId('aprs-dock-surfacebar');
    expect(bar).toContainElement(screen.getByTestId('aprs-map-toggle'));
    expect(bar).toContainElement(screen.getByTestId('aprs-map-popout'));
    expect(bar).toContainElement(screen.getByTestId('aprs-dock-close'));
    const tabrow = screen.getByTestId('aprs-dock-tabrow');
    expect(tabrow).toContainElement(screen.getByTestId('aprs-dock-tab-modem'));
    expect(tabrow).not.toContainElement(screen.getByTestId('aprs-dock-close'));
  });

  it('the popped pathway owns the surface bar (the widest state gets a full row)', () => {
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
    const bar = screen.getByTestId('aprs-dock-surfacebar');
    expect(bar).toContainElement(screen.getByTestId('aprs-map-focus'));
    expect(bar).toContainElement(screen.getByTestId('aprs-map-dockback'));
    expect(bar).toContainElement(screen.getByTestId('aprs-dock-close'));
  });

  it('renders no surface bar for callers without map controls (single legacy row)', () => {
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={() => {}} onClose={() => {}} />);
    expect(screen.queryByTestId('aprs-dock-surfacebar')).not.toBeInTheDocument();
    expect(screen.getByTestId('aprs-dock-close')).toBeInTheDocument();
  });
});
