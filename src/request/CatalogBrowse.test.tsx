import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { CatalogBrowse } from './CatalogBrowse';
import type { CatalogEntry } from '../catalog/types';

function entry(category: string, filename: string, description = '', size_bytes = 0): CatalogEntry {
  return { category, filename, description, size_bytes };
}

// Insertion order matters: groupByCategory preserves source order, so the
// category nav lists WL2K_RMS first, then PROPAGATION, then WX_EASTPAC.
const ENTRIES: CatalogEntry[] = [
  entry('WL2K_RMS', 'PUB_PACKET', 'Packet Public Gateways Frequency List', 219867),
  entry('WL2K_RMS', 'PUB_VARA', 'VARA HF Public Gateways Frequency List', 180000),
  entry('PROPAGATION', 'PROP_WWV', 'Daily WWV Solar Flux summary', 621),
  entry('PROPAGATION', 'AUR_TONIGHT', 'Aurora Forecast Tonight', 900),
  entry('WX_EASTPAC', 'OFFNT01', 'Offshore Waters Forecast — Eastern Pacific', 4096),
];

describe('<CatalogBrowse>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the browse root with a data-category reflecting the active category', () => {
    render(
      <CatalogBrowse
        entries={ENTRIES}
        onAddCms={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    const root = screen.getByTestId('request-browse');
    expect(root).toBeInTheDocument();
    // No initialCategory → defaults to the first category in insertion order.
    expect(root).toHaveAttribute('data-category', 'WL2K_RMS');
  });

  it('lists every category present in entries with its item count', () => {
    render(<CatalogBrowse entries={ENTRIES} onAddCms={vi.fn()} onBack={vi.fn()} />);
    const nav = screen.getByTestId('catalog-browse-nav');
    // One row per category, insertion-ordered.
    expect(within(nav).getByTestId('catalog-browse-cat-WL2K_RMS')).toHaveTextContent('WL2K_RMS');
    expect(within(nav).getByTestId('catalog-browse-cat-WL2K_RMS')).toHaveTextContent('2');
    expect(within(nav).getByTestId('catalog-browse-cat-PROPAGATION')).toHaveTextContent('2');
    expect(within(nav).getByTestId('catalog-browse-cat-WX_EASTPAC')).toHaveTextContent('1');
  });

  it('defaults to the first category and shows its items in the center pane', () => {
    render(<CatalogBrowse entries={ENTRIES} onAddCms={vi.fn()} onBack={vi.fn()} />);
    // WL2K_RMS items visible by default.
    expect(screen.getByTestId('catalog-browse-item-PUB_PACKET')).toBeInTheDocument();
    expect(screen.getByTestId('catalog-browse-item-PUB_VARA')).toBeInTheDocument();
    // Other categories' items not rendered.
    expect(screen.queryByTestId('catalog-browse-item-PROP_WWV')).toBeNull();
  });

  it('selecting a category lists that category’s items', () => {
    render(<CatalogBrowse entries={ENTRIES} onAddCms={vi.fn()} onBack={vi.fn()} />);
    fireEvent.click(screen.getByTestId('catalog-browse-cat-PROPAGATION'));
    expect(screen.getByTestId('catalog-browse-item-PROP_WWV')).toBeInTheDocument();
    expect(screen.getByTestId('catalog-browse-item-AUR_TONIGHT')).toBeInTheDocument();
    // RMS items now hidden.
    expect(screen.queryByTestId('catalog-browse-item-PUB_PACKET')).toBeNull();
    // data-category reflects the new selection.
    expect(screen.getByTestId('request-browse')).toHaveAttribute('data-category', 'PROPAGATION');
  });

  it('renders item filename, description, and size', () => {
    render(<CatalogBrowse entries={ENTRIES} onAddCms={vi.fn()} onBack={vi.fn()} />);
    const item = screen.getByTestId('catalog-browse-item-PUB_PACKET');
    expect(within(item).getByText('PUB_PACKET')).toBeInTheDocument();
    expect(within(item).getByText('Packet Public Gateways Frequency List')).toBeInTheDocument();
    // 219867 bytes → 214.7 KB.
    expect(item).toHaveTextContent('KB');
  });

  it('clicking an item’s Add calls onAddCms with that entry', () => {
    const onAddCms = vi.fn();
    render(<CatalogBrowse entries={ENTRIES} onAddCms={onAddCms} onBack={vi.fn()} />);
    const item = screen.getByTestId('catalog-browse-item-PUB_PACKET');
    fireEvent.click(within(item).getByRole('button', { name: /add/i }));
    expect(onAddCms).toHaveBeenCalledTimes(1);
    expect(onAddCms).toHaveBeenCalledWith(ENTRIES[0]);
  });

  it('initialCategory pre-selects that category on mount when present', () => {
    render(
      <CatalogBrowse
        entries={ENTRIES}
        initialCategory="WX_EASTPAC"
        onAddCms={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByTestId('request-browse')).toHaveAttribute('data-category', 'WX_EASTPAC');
    expect(screen.getByTestId('catalog-browse-item-OFFNT01')).toBeInTheDocument();
  });

  it('null initialCategory defaults to the first category', () => {
    render(
      <CatalogBrowse
        entries={ENTRIES}
        initialCategory={null}
        onAddCms={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByTestId('request-browse')).toHaveAttribute('data-category', 'WL2K_RMS');
  });

  it('a deep-link initialCategory not yet loaded stays the active category (neutral center)', () => {
    // openBrowse can deep-link to a sea-area category whose entries are not in
    // this catalog load. The deep-link is authoritative: data-category reflects
    // it (so the openBrowse contract holds) and the center shows no items.
    render(
      <CatalogBrowse
        entries={ENTRIES}
        initialCategory="WX_WESTPAC_NOT_LOADED"
        onAddCms={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByTestId('request-browse')).toHaveAttribute(
      'data-category',
      'WX_WESTPAC_NOT_LOADED',
    );
    // No item rows for the absent category.
    expect(screen.queryByTestId(/^catalog-browse-item-/)).toBeNull();
  });

  it('addedFilenames renders an Added affordance instead of Add', () => {
    const onAddCms = vi.fn();
    render(
      <CatalogBrowse
        entries={ENTRIES}
        addedFilenames={new Set(['PUB_PACKET'])}
        onAddCms={onAddCms}
        onBack={vi.fn()}
      />,
    );
    const item = screen.getByTestId('catalog-browse-item-PUB_PACKET');
    // The Add control is replaced by an Added (disabled) affordance.
    expect(within(item).queryByRole('button', { name: /^add$/i })).toBeNull();
    const added = within(item).getByText(/added/i);
    expect(added).toBeInTheDocument();
    // PUB_VARA (not in the set) still shows Add.
    const other = screen.getByTestId('catalog-browse-item-PUB_VARA');
    expect(within(other).getByRole('button', { name: /add/i })).toBeInTheDocument();
  });

  it('Added affordance does not fire onAddCms when clicked', () => {
    const onAddCms = vi.fn();
    render(
      <CatalogBrowse
        entries={ENTRIES}
        addedFilenames={new Set(['PUB_PACKET'])}
        onAddCms={onAddCms}
        onBack={vi.fn()}
      />,
    );
    const item = screen.getByTestId('catalog-browse-item-PUB_PACKET');
    fireEvent.click(within(item).getByText(/added/i));
    expect(onAddCms).not.toHaveBeenCalled();
  });

  it('filtering by text narrows the items within the active category', () => {
    render(<CatalogBrowse entries={ENTRIES} onAddCms={vi.fn()} onBack={vi.fn()} />);
    fireEvent.change(screen.getByTestId('catalog-browse-filter'), { target: { value: 'VARA' } });
    expect(screen.getByTestId('catalog-browse-item-PUB_VARA')).toBeInTheDocument();
    expect(screen.queryByTestId('catalog-browse-item-PUB_PACKET')).toBeNull();
  });

  it('Back calls onBack', () => {
    const onBack = vi.fn();
    render(<CatalogBrowse entries={ENTRIES} onAddCms={vi.fn()} onBack={onBack} />);
    fireEvent.click(screen.getByTestId('catalog-browse-back'));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it('renders a neutral empty state when given no entries', () => {
    render(<CatalogBrowse entries={[]} onAddCms={vi.fn()} onBack={vi.fn()} />);
    expect(screen.getByTestId('request-browse')).toBeInTheDocument();
    expect(screen.getByTestId('catalog-browse-empty')).toBeInTheDocument();
  });

  // --- Task D2: global search mode (searchQuery prop) ---

  describe('search mode (searchQuery)', () => {
    it('a non-empty searchQuery renders a flat cross-category results list', () => {
      // "PUB" matches both WL2K_RMS gateway entries (filename) — same category.
      // "forecast" (in description) spans categories; use "VARA" + "Aurora" to
      // prove multi-category: needle "a" is too broad, so pick a needle that
      // hits >=2 categories. 'gateways' is in two WL2K_RMS descriptions only.
      // Use 'forecast' which appears in PROPAGATION (Aurora Forecast) and
      // WX_EASTPAC (Offshore Waters Forecast) descriptions.
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="forecast"
          onAddCms={vi.fn()}
          onBack={vi.fn()}
        />,
      );
      const results = screen.getByTestId('catalog-search-results');
      expect(results).toBeInTheDocument();
      // Master-detail nav is hidden in search mode.
      expect(screen.queryByTestId('catalog-browse-nav')).toBeNull();
      // Matches from MULTIPLE categories present.
      expect(within(results).getByTestId('catalog-browse-item-AUR_TONIGHT')).toBeInTheDocument();
      expect(within(results).getByTestId('catalog-browse-item-OFFNT01')).toBeInTheDocument();
      // Non-matching entries absent.
      expect(screen.queryByTestId('catalog-browse-item-PUB_PACKET')).toBeNull();
    });

    it('matches a category name case-insensitively (returns that category’s items)', () => {
      // 'wl2k' matches the WL2K_RMS category (case-insensitive) → both its items.
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="wl2k"
          onAddCms={vi.fn()}
          onBack={vi.fn()}
        />,
      );
      const results = screen.getByTestId('catalog-search-results');
      expect(within(results).getByTestId('catalog-browse-item-PUB_PACKET')).toBeInTheDocument();
      expect(within(results).getByTestId('catalog-browse-item-PUB_VARA')).toBeInTheDocument();
      // Only the WL2K_RMS items match this category needle.
      expect(screen.queryByTestId('catalog-browse-item-PROP_WWV')).toBeNull();
      expect(screen.queryByTestId('catalog-browse-item-AUR_TONIGHT')).toBeNull();
    });

    it('matches a filename case-insensitively', () => {
      // Filename match (case-insensitive): 'aur_tonight' lowercase needle.
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="aur_tonight"
          onAddCms={vi.fn()}
          onBack={vi.fn()}
        />,
      );
      const results = screen.getByTestId('catalog-search-results');
      expect(within(results).getByTestId('catalog-browse-item-AUR_TONIGHT')).toBeInTheDocument();
      // No other entry matches this filename needle.
      expect(screen.queryByTestId('catalog-browse-item-PROP_WWV')).toBeNull();
      expect(screen.queryByTestId('catalog-browse-item-PUB_PACKET')).toBeNull();
    });

    it('matches a description case-insensitively', () => {
      // Description match: 'solar flux' lives in PROP_WWV's description.
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="SOLAR FLUX"
          onAddCms={vi.fn()}
          onBack={vi.fn()}
        />,
      );
      const results = screen.getByTestId('catalog-search-results');
      expect(within(results).getByTestId('catalog-browse-item-PROP_WWV')).toBeInTheDocument();
      // No other entry matches this description needle.
      expect(screen.queryByTestId('catalog-browse-item-AUR_TONIGHT')).toBeNull();
      expect(screen.queryByTestId('catalog-browse-item-PUB_PACKET')).toBeNull();
    });

    it('clicking a search result’s Add fires onAddCms with that entry', () => {
      const onAddCms = vi.fn();
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="OFFNT"
          onAddCms={onAddCms}
          onBack={vi.fn()}
        />,
      );
      const item = screen.getByTestId('catalog-browse-item-OFFNT01');
      fireEvent.click(within(item).getByRole('button', { name: /add/i }));
      expect(onAddCms).toHaveBeenCalledTimes(1);
      expect(onAddCms).toHaveBeenCalledWith(ENTRIES[4]);
    });

    it('addedFilenames shows the Added affordance in search results', () => {
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="PUB"
          addedFilenames={new Set(['PUB_PACKET'])}
          onAddCms={vi.fn()}
          onBack={vi.fn()}
        />,
      );
      const item = screen.getByTestId('catalog-browse-item-PUB_PACKET');
      expect(within(item).queryByRole('button', { name: /^add$/i })).toBeNull();
      expect(within(item).getByText(/added/i)).toBeInTheDocument();
    });

    it('a no-match needle shows the empty state and no item rows', () => {
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="zzz_no_such_item"
          onAddCms={vi.fn()}
          onBack={vi.fn()}
        />,
      );
      expect(screen.getByTestId('catalog-search-results')).toBeInTheDocument();
      expect(screen.queryByTestId(/^catalog-browse-item-/)).toBeNull();
      expect(screen.getByText(/no items match/i)).toBeInTheDocument();
    });

    it('an empty / whitespace searchQuery restores the master-detail view', () => {
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="   "
          onAddCms={vi.fn()}
          onBack={vi.fn()}
        />,
      );
      // Master-detail nav present, search-results absent.
      expect(screen.getByTestId('catalog-browse-nav')).toBeInTheDocument();
      expect(screen.queryByTestId('catalog-search-results')).toBeNull();
    });

    it('no searchQuery prop behaves exactly as the D1 master-detail', () => {
      render(<CatalogBrowse entries={ENTRIES} onAddCms={vi.fn()} onBack={vi.fn()} />);
      expect(screen.getByTestId('catalog-browse-nav')).toBeInTheDocument();
      expect(screen.queryByTestId('catalog-search-results')).toBeNull();
    });

    it('the back control is labelled "Clear search" in search mode and still fires onBack', () => {
      const onBack = vi.fn();
      render(
        <CatalogBrowse
          entries={ENTRIES}
          searchQuery="forecast"
          onAddCms={vi.fn()}
          onBack={onBack}
        />,
      );
      // In search mode the control clears the search (parent semantics) rather
      // than navigating back, so it reads "Clear search", not "← Back".
      const control = screen.getByTestId('catalog-browse-back');
      expect(control).toHaveTextContent('Clear search');
      fireEvent.click(control);
      expect(onBack).toHaveBeenCalledTimes(1);
    });
  });
});
