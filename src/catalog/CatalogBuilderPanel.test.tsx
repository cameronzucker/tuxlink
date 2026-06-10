import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CatalogBuilderPanel } from './CatalogBuilderPanel';
import { GATEWAY_PREFILL_EVENT } from '../favorites/prefillEvent';

// The panel invalidates the shared ['favorites'] query after a ★ add, so it
// needs a QueryClientProvider in scope (mirrors the app-root provider).
function renderPanel(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

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
    if (cmd === 'favorites_read') return { schema_version: 1, favorites: [], log: [] };
    if (cmd === 'catalog_fetch_stations') return [];
    if (cmd === 'catalog_send_inquiry') return 'MID123';
    return undefined;
  });
});

describe('CatalogBuilderPanel', () => {
  it('renders the form column (location, modes, radius) inside a Find a Gateway dialog', async () => {
    renderPanel(<CatalogBuilderPanel onClose={() => {}} />);
    expect(screen.getByRole('dialog', { name: /find a gateway/i })).toBeTruthy();
    expect(await screen.findByLabelText(/your location/i)).toBeTruthy();
    expect(screen.getByLabelText(/VARA HF/i)).toBeTruthy();
    expect(screen.getByLabelText(/within/i)).toBeTruthy();
  });

  it('prefills location from config_read full-precision grid', async () => {
    renderPanel(<CatalogBuilderPanel onClose={() => {}} />);
    await waitFor(() => expect(screen.getByLabelText(/your location/i)).toHaveValue('DM43bp'));
  });

  it('calls catalog_fetch_stations with the checked modes on Get Stations', async () => {
    renderPanel(<CatalogBuilderPanel onClose={() => {}} />);
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
    renderPanel(<CatalogBuilderPanel onClose={() => {}} />);
    // Exact label — "Packet" must not also match the "Robust Packet" checkbox.
    fireEvent.click(await screen.findByLabelText('Packet'));
    fireEvent.click(screen.getByRole('button', { name: /get stations/i }));
    await waitFor(() => {
      const call = vi.mocked(invoke).mock.calls.find((c) => c[0] === 'catalog_fetch_stations');
      expect(call?.[1]).not.toHaveProperty('serviceCodes');
    });
  });

  // tuxlink-6jpf: the by-message INFO-category requests (area weather / propagation
  // / winlink info) moved to Message → Request Center (which already lists the full
  // bundled catalog). Find a Gateway is now the station finder only — those
  // checkboxes must not appear here.
  it('does NOT render the info-category (by-message) requests — moved to Request Center (tuxlink-6jpf)', async () => {
    renderPanel(<CatalogBuilderPanel onClose={() => {}} />);
    await screen.findByLabelText(/your location/i);
    expect(screen.queryByText(/also request \(by message\)/i)).toBeNull();
    expect(screen.queryByLabelText(/area weather/i)).toBeNull();
    expect(screen.queryByLabelText(/propagation/i)).toBeNull();
    expect(screen.queryByLabelText(/winlink info/i)).toBeNull();
  });

  // tuxlink-29zx: the panel shipped with only the × button to dismiss — no
  // backdrop-click and no Escape. Operator smoke: "no way to close it or click
  // off of it." These codify the two standard modal-dismiss affordances (both
  // standard for the project's overlay panels).
  it('dismisses on Escape', async () => {
    const onClose = vi.fn();
    renderPanel(<CatalogBuilderPanel onClose={onClose} />);
    await screen.findByLabelText(/your location/i);
    fireEvent.keyDown(screen.getByRole('dialog', { name: /find a gateway/i }), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('dismisses on backdrop (overlay) click', async () => {
    const onClose = vi.fn();
    renderPanel(<CatalogBuilderPanel onClose={onClose} />);
    await screen.findByLabelText(/your location/i);
    fireEvent.click(screen.getByTestId('catalog-builder-overlay'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does NOT dismiss when clicking inside the panel body', async () => {
    const onClose = vi.fn();
    renderPanel(<CatalogBuilderPanel onClose={onClose} />);
    fireEvent.click(await screen.findByLabelText(/your location/i));
    expect(onClose).not.toHaveBeenCalled();
  });

  // tuxlink-dqte: the ★ in the results was shipped as a disabled forward hook
  // (gated on CF's favorite_upsert landing). favorite_upsert HAS landed, but the
  // production parent never wired onAddFavorite, so the ★ was permanently
  // disabled and "favorites from the station lookup don't work." Clicking ★ must
  // persist the gateway AND star it (the store's star-to-promote model: a bare
  // upsert is an unstarred "recent"), so it lands in the Favorites tab.
  it('★ adds the gateway as a STARRED favorite: favorite_upsert → favorite_star (tuxlink-dqte)', async () => {
    const gateway = {
      channel: 'CHAN-1', callsign: 'W6ABC', sysopName: null, grid: 'CN87',
      location: null, frequenciesKhz: [14105], lastUpdate: null, email: null, homepage: null,
    };
    const listing = {
      mode: 'vara-hf', title: null, gateways: [gateway], raw: '', parsedOk: true, fetchedAtMs: null,
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'DM43bp' };
      if (cmd === 'favorites_read') return { schema_version: 1, favorites: [], log: [] };
      if (cmd === 'catalog_fetch_stations') return [listing];
      // favorite_upsert returns the STORED record (server-assigned id); the
      // handler stars THAT id. A fixed record is enough — the call args are
      // asserted via toHaveBeenCalledWith below, not via this return value.
      if (cmd === 'favorite_upsert') {
        return { id: 'fav-1', mode: 'vara-hf', gateway: 'W6ABC', starred: false, created_at: 'now', updated_at: 'now' };
      }
      return undefined;
    });

    renderPanel(<CatalogBuilderPanel onClose={() => {}} />);
    fireEvent.click(await screen.findByLabelText(/VARA HF/i));
    fireEvent.click(screen.getByRole('button', { name: /get stations/i }));

    const star = await screen.findByRole('button', { name: /add W6ABC to vara-hf favorites/i });
    expect(star).toBeEnabled();
    fireEvent.click(star);

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        'favorite_upsert',
        expect.objectContaining({
          favorite: expect.objectContaining({ mode: 'vara-hf', gateway: 'W6ABC' }),
        }),
      );
      expect(vi.mocked(invoke)).toHaveBeenCalledWith('favorite_star', { id: 'fav-1', starred: true });
    });
  });

  it('★ unstars an already-starred station instead of creating a duplicate', async () => {
    const gateway = {
      channel: 'CHAN-1', callsign: 'W6ABC', sysopName: null, grid: 'CN87',
      location: null, frequenciesKhz: [14105], lastUpdate: null, email: null, homepage: null,
    };
    const listing = {
      mode: 'packet', title: null, gateways: [gateway], raw: '', parsedOk: true, fetchedAtMs: null,
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'DM43bp' };
      if (cmd === 'favorites_read') {
        return {
          schema_version: 1,
          favorites: [{
            id: 'fav-1',
            mode: 'packet',
            gateway: 'W6ABC',
            freq: '14.105',
            grid: 'CN87',
            starred: true,
            created_at: 'then',
            updated_at: 'then',
          }],
          log: [],
        };
      }
      if (cmd === 'catalog_fetch_stations') return [listing];
      return undefined;
    });

    renderPanel(<CatalogBuilderPanel onClose={() => {}} />);
    fireEvent.click(await screen.findByLabelText('Packet'));
    fireEvent.click(screen.getByRole('button', { name: /get stations/i }));

    const star = await screen.findByRole('button', { name: /remove W6ABC from packet favorites/i });
    expect(star).toHaveTextContent('★');
    fireEvent.click(star);

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith('favorite_star', { id: 'fav-1', starred: false });
    });
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === 'favorite_upsert')).toBe(false);
  });

  it('Use emits a prefill-only packet dial and closes the picker', async () => {
    const gateway = {
      channel: 'CHAN-1', callsign: 'W6ABC', sysopName: null, grid: 'CN87',
      location: null, frequenciesKhz: [14105], lastUpdate: null, email: null, homepage: null,
    };
    const listing = {
      mode: 'packet', title: null, gateways: [gateway], raw: '', parsedOk: true, fetchedAtMs: null,
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'DM43bp' };
      if (cmd === 'favorites_read') return { schema_version: 1, favorites: [], log: [] };
      if (cmd === 'catalog_fetch_stations') return [listing];
      return undefined;
    });
    const onClose = vi.fn();
    const onPrefill = vi.fn();
    window.addEventListener(GATEWAY_PREFILL_EVENT, onPrefill);

    renderPanel(<CatalogBuilderPanel activePrefillMode="packet" onClose={onClose} />);
    fireEvent.click(await screen.findByLabelText('Packet'));
    fireEvent.click(screen.getByRole('button', { name: /get stations/i }));
    fireEvent.click(await screen.findByRole('button', { name: 'Use' }));

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onPrefill).toHaveBeenCalledTimes(1);
    expect((onPrefill.mock.calls[0][0] as CustomEvent).detail).toEqual({
      mode: 'packet',
      gateway: 'W6ABC',
      freq: '14.105',
      grid: 'CN87',
    });
    window.removeEventListener(GATEWAY_PREFILL_EVENT, onPrefill);
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
