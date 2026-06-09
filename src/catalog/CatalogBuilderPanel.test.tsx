import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CatalogBuilderPanel } from './CatalogBuilderPanel';

// Raw CSS for the stacking-order invariant (tuxlink-tsl5). The modal overlay must
// paint ABOVE the app chrome (titlebar/menubar/resize) or its header + × land
// behind the chrome and become unreachable.
const CSS_RAW = import.meta.glob(
  ['./CatalogBuilderPanel.css', '../shell/chrome/chrome.css'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;
const builderCss = CSS_RAW['./CatalogBuilderPanel.css'];
const chromeCss = CSS_RAW['../shell/chrome/chrome.css'];

const maxZIndex = (css: string): number =>
  Math.max(...[...css.matchAll(/z-index:\s*(\d+)/g)].map((m) => Number(m[1])));
const overlayZIndex = (css: string): number => {
  const start = css.indexOf('.catalog-builder-overlay');
  const block = css.slice(start, css.indexOf('}', start));
  return Number(block.match(/z-index:\s*(\d+)/)?.[1]);
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'DM43bp' };
    if (cmd === 'catalog_fetch_stations') return [];
    if (cmd === 'catalog_send_inquiry') return 'MID123';
    return undefined;
  });
});

describe('CatalogBuilderPanel', () => {
  it('renders the form column (location, modes, radius) inside a Find a Gateway dialog', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    expect(screen.getByRole('dialog', { name: /find a gateway/i })).toBeTruthy();
    expect(await screen.findByLabelText(/your location/i)).toBeTruthy();
    expect(screen.getByLabelText(/VARA HF/i)).toBeTruthy();
    expect(screen.getByLabelText(/within/i)).toBeTruthy();
  });

  it('prefills location from config_read full-precision grid', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    await waitFor(() => expect(screen.getByLabelText(/your location/i)).toHaveValue('DM43bp'));
  });

  it('calls catalog_fetch_stations with the checked modes on Get Stations', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    fireEvent.click(await screen.findByLabelText(/VARA HF/i));
    fireEvent.click(screen.getByRole('button', { name: /get stations/i }));
    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        'catalog_fetch_stations',
        expect.objectContaining({ modes: ['vara-hf'] }),
      ),
    );
  });

  it('does NOT pass serviceCodes (PUBLIC-only is server-fixed)', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    // Exact label — "Packet" must not also match the "Robust Packet" checkbox.
    fireEvent.click(await screen.findByLabelText('Packet'));
    fireEvent.click(screen.getByRole('button', { name: /get stations/i }));
    await waitFor(() => {
      const call = vi.mocked(invoke).mock.calls.find((c) => c[0] === 'catalog_fetch_stations');
      expect(call?.[1]).not.toHaveProperty('serviceCodes');
    });
  });

  // tuxlink-6jpf: the by-message INFO-category requests (area weather / propagation
  // / winlink info) moved to Message → Catalog Request (which already lists the full
  // bundled catalog). Find a Gateway is now the station finder only — those
  // checkboxes must not appear here.
  it('does NOT render the info-category (by-message) requests — moved to Catalog Request (tuxlink-6jpf)', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    await screen.findByLabelText(/your location/i);
    expect(screen.queryByText(/also request \(by message\)/i)).toBeNull();
    expect(screen.queryByLabelText(/area weather/i)).toBeNull();
    expect(screen.queryByLabelText(/propagation/i)).toBeNull();
    expect(screen.queryByLabelText(/winlink info/i)).toBeNull();
  });

  // tuxlink-29zx: the panel shipped with only the × button to dismiss — no
  // backdrop-click and no Escape. Operator smoke: "no way to close it or click
  // off of it." These codify the two standard modal-dismiss affordances (the
  // sibling CatalogRequestPanel already has both).
  it('dismisses on Escape', async () => {
    const onClose = vi.fn();
    render(<CatalogBuilderPanel onClose={onClose} />);
    await screen.findByLabelText(/your location/i);
    fireEvent.keyDown(screen.getByRole('dialog', { name: /find a gateway/i }), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('dismisses on backdrop (overlay) click', async () => {
    const onClose = vi.fn();
    render(<CatalogBuilderPanel onClose={onClose} />);
    await screen.findByLabelText(/your location/i);
    fireEvent.click(screen.getByTestId('catalog-builder-overlay'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does NOT dismiss when clicking inside the panel body', async () => {
    const onClose = vi.fn();
    render(<CatalogBuilderPanel onClose={onClose} />);
    fireEvent.click(await screen.findByLabelText(/your location/i));
    expect(onClose).not.toHaveBeenCalled();
  });
});

// tuxlink-tsl5 (follow-up to tuxlink-29zx): the dialog shipped at z-index 50 —
// below the app chrome (menubar 90 / dropdown 100 / resize 200) — so a tall
// panel's header and × landed behind the chrome and were unreachable (operator:
// "controls outside the top of the main window"). The overlay must stack above
// ALL chrome.
describe('CatalogBuilderPanel.css stacking order (tuxlink-tsl5)', () => {
  it('overlay stacks above every app-chrome z-index', () => {
    expect(overlayZIndex(builderCss)).toBeGreaterThan(maxZIndex(chromeCss));
  });
});
