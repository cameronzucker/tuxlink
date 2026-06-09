import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CatalogBuilderPanel } from './CatalogBuilderPanel';

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

  it('queues info-category requests via catalog_send_inquiry and confirms', async () => {
    render(<CatalogBuilderPanel onClose={() => {}} />);
    fireEvent.click(await screen.findByLabelText(/area weather/i));
    fireEvent.click(screen.getByRole('button', { name: /queue 1 request/i }));
    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith('catalog_send_inquiry', { filenames: expect.any(Array) }),
    );
    expect(await screen.findByText(/arrive in your inbox after the next connect/i)).toBeTruthy();
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
