import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

// Mock the tileSource wrappers; assert the exact camelCase source shape.
vi.mock('../map/tileSource', () => ({
  configureTileSource: vi.fn(),
  testTileSource: vi.fn(),
  clearTileCache: vi.fn(),
}));

// Mock the Tauri bridge for the mount-time config_read hydration (tuxlink-9rek).
const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}));

import {
  configureTileSource,
  testTileSource,
  clearTileCache,
  type TileSourceStatus,
} from '../map/tileSource';
import { MapTileSourceSettings } from './MapTileSourceSettings';

const testMock = testTileSource as unknown as ReturnType<typeof vi.fn>;
const configureMock = configureTileSource as unknown as ReturnType<typeof vi.fn>;
const clearMock = clearTileCache as unknown as ReturnType<typeof vi.fn>;

const LAN_LIVE: TileSourceStatus = { kind: 'lan-live', zoom: 13, label: 'Shack LAN', cachedAt: null };
const INCOMPATIBLE: TileSourceStatus = { kind: 'incompatible', zoom: 2, label: null, cachedAt: null };
const UNREACHABLE: TileSourceStatus = { kind: 'unreachable', zoom: 2, label: null, cachedAt: null };

beforeEach(() => {
  testMock.mockReset();
  configureMock.mockReset();
  clearMock.mockReset();
  invokeMock.mockReset();
  testMock.mockResolvedValue(LAN_LIVE);
  configureMock.mockResolvedValue(LAN_LIVE);
  clearMock.mockResolvedValue(undefined);
  // Default: no persisted source → mount hydration is a no-op (keeps defaults).
  invokeMock.mockResolvedValue({ map_tile_source: null });
});

// Fill in a LAN URL so default fields produce a valid camelCase source.
function fillLanUrl() {
  fireEvent.change(screen.getByLabelText(/Tile URL template/i), {
    target: { value: 'http://192.168.1.10:8080/{z}/{x}/{y}.png' },
  });
}

describe('MapTileSourceSettings', () => {
  it('renders all source fields with XYZ as the default scheme', () => {
    render(<MapTileSourceSettings />);
    expect(screen.getByLabelText(/Tile URL template/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Minimum zoom/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Maximum zoom/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Cache budget/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Attribution/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Source label/i)).toBeInTheDocument();
    // XYZ is the default scheme.
    const xyz = screen.getByRole('radio', { name: /XYZ/i });
    expect(xyz).toBeChecked();
    expect(screen.getByRole('radio', { name: /TMS/i })).not.toBeChecked();
  });

  it('Test source calls testTileSource with the camelCase source shape and shows lan-live status', async () => {
    render(<MapTileSourceSettings />);
    fillLanUrl();
    fireEvent.click(screen.getByRole('button', { name: /Test source/i }));
    await waitFor(() => {
      expect(testMock).toHaveBeenCalledWith(
        expect.objectContaining({
          url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
          scheme: 'Xyz',
          minZoom: expect.any(Number),
          maxZoom: expect.any(Number),
          cacheBudgetMb: expect.any(Number),
        }),
      );
    });
    // crs field must NOT be present on the wire.
    expect(testMock.mock.calls[0][0]).not.toHaveProperty('crs');
    expect(await screen.findByText(/source validated/i)).toBeInTheDocument();
  });

  it('Test source surfaces the incompatible message plainly', async () => {
    testMock.mockResolvedValue(INCOMPATIBLE);
    render(<MapTileSourceSettings />);
    fillLanUrl();
    fireEvent.click(screen.getByRole('button', { name: /Test source/i }));
    expect(
      await screen.findByText(/incompatible tile source — the server responded but did not return standard image tiles/i),
    ).toBeInTheDocument();
  });

  it('Test source surfaces the unreachable message plainly', async () => {
    testMock.mockResolvedValue(UNREACHABLE);
    render(<MapTileSourceSettings />);
    fillLanUrl();
    fireEvent.click(screen.getByRole('button', { name: /Test source/i }));
    expect(await screen.findByText(/tiles unreachable/i)).toBeInTheDocument();
  });

  it('Use this source calls configureTileSource and reflects the result', async () => {
    render(<MapTileSourceSettings />);
    fillLanUrl();
    fireEvent.click(screen.getByRole('button', { name: /Use this source/i }));
    await waitFor(() => {
      expect(configureMock).toHaveBeenCalledWith(
        expect.objectContaining({
          url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
          scheme: 'Xyz',
        }),
      );
    });
    // crs field must NOT be present on the wire.
    expect(configureMock.mock.calls[0][0]).not.toHaveProperty('crs');
    expect(await screen.findByText(/source validated/i)).toBeInTheDocument();
  });

  it('Clear tile cache calls clearTileCache', async () => {
    render(<MapTileSourceSettings />);
    fireEvent.click(screen.getByRole('button', { name: /Clear tile cache/i }));
    await waitFor(() => expect(clearMock).toHaveBeenCalledTimes(1));
  });

  it('warns (non-blocking) on a public-looking host; Test and Save stay enabled', async () => {
    render(<MapTileSourceSettings />);
    fireEvent.change(screen.getByLabelText(/Tile URL template/i), {
      target: { value: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png' },
    });
    // Warning renders.
    expect(await screen.findByRole('alert')).toHaveTextContent(/public/i);
    // Buttons remain enabled — warn, never block.
    expect(screen.getByRole('button', { name: /Test source/i })).toBeEnabled();
    expect(screen.getByRole('button', { name: /Use this source/i })).toBeEnabled();
    // And the operator can still test the public host.
    fireEvent.click(screen.getByRole('button', { name: /Test source/i }));
    await waitFor(() => expect(testMock).toHaveBeenCalled());
  });

  it('hydrates the form from the persisted source on mount (tuxlink-9rek)', async () => {
    invokeMock.mockResolvedValue({
      map_tile_source: {
        url: 'http://pandora.local/tiles/styles/positron/{z}/{x}/{y}.png',
        scheme: 'Tms',
        minZoom: 2,
        maxZoom: 18,
        cacheBudgetMb: 512,
        attribution: '© Geographica',
        label: 'Shack LAN',
      },
    });
    render(<MapTileSourceSettings />);
    await waitFor(() =>
      expect((screen.getByLabelText(/Tile URL template/i) as HTMLInputElement).value).toBe(
        'http://pandora.local/tiles/styles/positron/{z}/{x}/{y}.png',
      ),
    );
    expect((screen.getByLabelText(/Maximum zoom/i) as HTMLInputElement).value).toBe('18');
    expect((screen.getByLabelText(/Source label/i) as HTMLInputElement).value).toBe('Shack LAN');
    expect(screen.getByRole('radio', { name: /TMS/i })).toBeChecked();
  });

  it('emits a tile-source-changed event after Use this source (tuxlink-9rek)', async () => {
    const onChange = vi.fn();
    window.addEventListener('tuxlink:tile-source-changed', onChange);
    try {
      render(<MapTileSourceSettings />);
      fillLanUrl();
      fireEvent.click(screen.getByRole('button', { name: /Use this source/i }));
      await waitFor(() => expect(configureMock).toHaveBeenCalled());
      await waitFor(() => expect(onChange).toHaveBeenCalled());
    } finally {
      window.removeEventListener('tuxlink:tile-source-changed', onChange);
    }
  });
});
