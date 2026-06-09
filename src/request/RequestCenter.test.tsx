import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { RequestCenter } from './RequestCenter';
import type { CatalogEntry } from '../catalog/types';

// Mock the Tauri invoke surface so the shell drives without a backend.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

function entry(category: string, filename: string, description = '', size_bytes = 0): CatalogEntry {
  return { category, filename, description, size_bytes };
}

const FIXTURE_ENTRIES: CatalogEntry[] = [
  entry('WL2K_RMS', 'PUB_PACKET', 'Packet Public Gateways Frequency List', 219867),
  entry('PROPAGATION', 'PROP_WWV', 'Daily WWV Solar Flux summary', 621),
];

// Default mock: catalog loads, config_read returns a grid.
function mockHappy(grid: string | null = 'CN87') {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
    if (cmd === 'config_read') return { grid };
    return null;
  });
}

describe('<RequestCenter>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders a dialog labelled "Request Center"', async () => {
    mockHappy();
    render(<RequestCenter onClose={() => {}} />);
    const dialog = await screen.findByRole('dialog', { name: 'Request Center' });
    expect(dialog).toBeInTheDocument();
  });

  it('renders the header chrome: location chip, search input, content + basket regions', async () => {
    mockHappy();
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    expect(screen.getByTestId('request-center-location')).toBeInTheDocument();
    expect(screen.getByTestId('request-search')).toBeInTheDocument();
    expect(screen.getByTestId('request-content')).toBeInTheDocument();
    expect(screen.getByTestId('request-basket')).toBeInTheDocument();
  });

  it('the location chip shows "Near CN87" once config_read resolves with a grid', async () => {
    mockHappy('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('request-center-location')).toHaveTextContent('Near CN87'),
    );
  });

  it('the Close button calls onClose', async () => {
    mockHappy();
    const onClose = vi.fn();
    render(<RequestCenter onClose={onClose} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    fireEvent.click(screen.getByTestId('request-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('ESC calls onClose', async () => {
    mockHappy();
    const onClose = vi.fn();
    render(<RequestCenter onClose={onClose} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  // --- Adrev #3: single catalog-load owner; shell renders loading/empty/error states ---

  it('renders a loading placeholder while the catalog fetches', () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'config_read') return Promise.resolve({ grid: 'CN87' });
      // catalog_list never resolves → stay in loading.
      return new Promise(() => {});
    });
    render(<RequestCenter onClose={() => {}} />);
    expect(screen.getByTestId('request-catalog-loading')).toBeInTheDocument();
  });

  it('renders an empty state when the catalog loads with zero entries', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return [];
      if (cmd === 'config_read') return { grid: 'CN87' };
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    expect(await screen.findByTestId('request-catalog-empty')).toBeInTheDocument();
  });

  it('renders an error message (no crash) when catalog_list rejects', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') throw new Error('catalog backend offline');
      if (cmd === 'config_read') return { grid: 'CN87' };
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    const err = await screen.findByTestId('request-catalog-error');
    expect(err).toHaveTextContent('catalog backend offline');
    // Dialog still renders — no crash.
    expect(screen.getByRole('dialog', { name: 'Request Center' })).toBeInTheDocument();
  });

  // --- Adrev #9: config_read failure path → neutral location chip, never "Near null" ---

  it('shows a neutral location state when config_read resolves with no grid', async () => {
    mockHappy(null);
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    // config_read resolved with no grid → chip settles on the neutral label.
    await waitFor(() =>
      expect(screen.getByTestId('request-center-location')).toHaveTextContent('Location not set'),
    );
    const chip = screen.getByTestId('request-center-location');
    expect(chip.textContent).not.toMatch(/null|undefined/);
  });

  it('shows a neutral location state (no crash) when config_read rejects', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      if (cmd === 'config_read') throw new Error('config unreadable');
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    const chip = screen.getByTestId('request-center-location');
    expect(chip).toHaveTextContent('Location not set');
    expect(chip.textContent).not.toMatch(/null|undefined|Near/);
  });
});
