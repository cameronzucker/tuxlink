import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { CatalogRequestPanel } from './CatalogRequestPanel';
import type { CatalogEntry } from './types';

// Mock the Tauri invoke surface so we can drive the panel without a backend.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

function entry(category: string, filename: string, description = '', size_bytes = 0): CatalogEntry {
  return { category, filename, description, size_bytes };
}

const FIXTURE_ENTRIES: CatalogEntry[] = [
  entry('WL2K_RMS', 'PUB_PACKET', 'Packet Public Gateways Frequency List', 219867),
  entry('WL2K_RMS', 'PUB_VARA', 'VARA Public Gateways Frequency List', 75234),
  entry('WL2K_USERS', 'CMS_STATUS', "Real time Operational Status of Winlink CMS's", 2018),
  entry('PROPAGATION', 'PROP_WWV', 'Daily WWV Solar Flux, A & K Index summary', 621),
];

describe('<CatalogRequestPanel>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the loading state while the catalog fetches', () => {
    // invoke returns a never-resolving promise — panel stays in loading.
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));
    render(<CatalogRequestPanel onClose={() => {}} />);
    expect(screen.getByTestId('catalog-loading')).toBeInTheDocument();
  });

  it('renders the category tree after the catalog loads', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      return null;
    });
    render(<CatalogRequestPanel onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('catalog-tree')).toBeInTheDocument());
    expect(screen.getByTestId('catalog-category-WL2K_RMS')).toBeInTheDocument();
    expect(screen.getByTestId('catalog-category-WL2K_USERS')).toBeInTheDocument();
    expect(screen.getByTestId('catalog-category-PROPAGATION')).toBeInTheDocument();
  });

  it('expanding a category reveals its inquiry checkboxes', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      return null;
    });
    render(<CatalogRequestPanel onClose={() => {}} />);
    await waitFor(() => screen.getByTestId('catalog-tree'));
    // Initially collapsed.
    expect(screen.queryByTestId('catalog-item-PUB_PACKET')).toBeNull();
    fireEvent.click(screen.getByTestId('catalog-category-header-WL2K_RMS'));
    expect(screen.getByTestId('catalog-item-PUB_PACKET')).toBeInTheDocument();
    expect(screen.getByTestId('catalog-item-PUB_VARA')).toBeInTheDocument();
  });

  it('typing in the filter shows only matching entries (auto-expanded)', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      return null;
    });
    render(<CatalogRequestPanel onClose={() => {}} />);
    await waitFor(() => screen.getByTestId('catalog-tree'));
    fireEvent.change(screen.getByTestId('catalog-filter'), { target: { value: 'WWV' } });
    // Filter auto-expands; PROP_WWV visible
    expect(screen.getByTestId('catalog-item-PROP_WWV')).toBeInTheDocument();
    // RMS items filtered out
    expect(screen.queryByTestId('catalog-item-PUB_PACKET')).toBeNull();
  });

  it('selecting items + clicking Send invokes catalog_send_inquiry with the chosen filenames', async () => {
    let sendCalledWith: string[] | null = null;
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      if (cmd === 'catalog_send_inquiry') {
        sendCalledWith = (args as { filenames: string[] }).filenames;
        return 'MID-FAKE-123';
      }
      return null;
    });
    render(<CatalogRequestPanel onClose={() => {}} />);
    await waitFor(() => screen.getByTestId('catalog-tree'));
    // Expand RMS and pick both
    fireEvent.click(screen.getByTestId('catalog-category-header-WL2K_RMS'));
    fireEvent.click(screen.getByTestId('catalog-item-PUB_PACKET'));
    fireEvent.click(screen.getByTestId('catalog-item-PUB_VARA'));
    // Selection summary updates
    expect(screen.getByTestId('catalog-selection-summary')).toHaveTextContent('2 items');
    // Send
    fireEvent.click(screen.getByTestId('catalog-send'));
    await waitFor(() => expect(screen.getByTestId('catalog-send-success')).toBeInTheDocument());
    expect(sendCalledWith).toEqual(['PUB_PACKET', 'PUB_VARA']);
    expect(screen.getByTestId('catalog-send-success')).toHaveTextContent('MID-FAKE-123');
  });

  it('Send button is disabled when nothing is selected', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      return null;
    });
    render(<CatalogRequestPanel onClose={() => {}} />);
    await waitFor(() => screen.getByTestId('catalog-tree'));
    expect(screen.getByTestId('catalog-send')).toBeDisabled();
  });

  it('surfaces a backend error from catalog_send_inquiry', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      if (cmd === 'catalog_send_inquiry') throw new Error('backend offline');
      return null;
    });
    render(<CatalogRequestPanel onClose={() => {}} />);
    await waitFor(() => screen.getByTestId('catalog-tree'));
    fireEvent.click(screen.getByTestId('catalog-category-header-WL2K_RMS'));
    fireEvent.click(screen.getByTestId('catalog-item-PUB_PACKET'));
    fireEvent.click(screen.getByTestId('catalog-send'));
    await waitFor(() => expect(screen.getByTestId('catalog-send-error')).toBeInTheDocument());
    expect(screen.getByTestId('catalog-send-error')).toHaveTextContent('backend offline');
  });

  it('Close (✕) calls onClose', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      return null;
    });
    const onClose = vi.fn();
    render(<CatalogRequestPanel onClose={onClose} />);
    await waitFor(() => screen.getByTestId('catalog-tree'));
    fireEvent.click(screen.getByTestId('catalog-close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('clicking the backdrop calls onClose; clicking the panel does not', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      return null;
    });
    const onClose = vi.fn();
    render(<CatalogRequestPanel onClose={onClose} />);
    await waitFor(() => screen.getByTestId('catalog-tree'));
    // Panel click (not backdrop) — must NOT close
    fireEvent.click(screen.getByTestId('catalog-panel'));
    expect(onClose).not.toHaveBeenCalled();
    // Backdrop click — must close
    fireEvent.click(screen.getByTestId('catalog-overlay'));
    expect(onClose).toHaveBeenCalled();
  });
});
