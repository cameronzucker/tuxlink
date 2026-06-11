import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { ServiceCodesField } from './ServiceCodesField';

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

/** Default backend: get returns `current`, set succeeds. */
function mockBackend(current: { value: string }) {
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === 'catalog_get_service_codes') return current.value as unknown as never;
    if (cmd === 'catalog_set_service_codes') {
      // Mimic the backend's normalize: trim + collapse whitespace, empty -> PUBLIC.
      const raw = (args as { codes: string }).codes;
      const joined = raw.split(/\s+/).filter(Boolean).join(' ');
      current.value = joined.length > 0 ? joined : 'PUBLIC';
      return undefined as unknown as never;
    }
    return undefined as unknown as never;
  });
}

describe('ServiceCodesField', () => {
  it('loads and displays the configured codes', async () => {
    mockBackend({ value: 'PUBLIC EMCOMM' });
    render(<ServiceCodesField />);
    await waitFor(() =>
      expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC EMCOMM'),
    );
  });

  it('saves an edited code and fires onApplied', async () => {
    mockBackend({ value: 'PUBLIC' });
    const onApplied = vi.fn();
    render(<ServiceCodesField onApplied={onApplied} />);
    await waitFor(() => expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC'));

    fireEvent.change(screen.getByTestId('service-codes-input'), {
      target: { value: 'MARS-MEMBER-CODE' },
    });
    fireEvent.click(screen.getByTestId('service-codes-apply'));

    await waitFor(() => expect(onApplied).toHaveBeenCalledTimes(1));
    expect(invoke).toHaveBeenCalledWith('catalog_set_service_codes', {
      codes: 'MARS-MEMBER-CODE',
    });
  });

  it('Apply is disabled until the value changes', async () => {
    mockBackend({ value: 'PUBLIC' });
    render(<ServiceCodesField />);
    await waitFor(() => expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC'));
    expect(screen.getByTestId('service-codes-apply')).toBeDisabled();

    fireEvent.change(screen.getByTestId('service-codes-input'), { target: { value: 'PUBLIC EMCOMM' } });
    expect(screen.getByTestId('service-codes-apply')).toBeEnabled();
  });

  it('a preset appends without duplicating', async () => {
    mockBackend({ value: 'PUBLIC' });
    render(<ServiceCodesField />);
    await waitFor(() => expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC'));

    fireEvent.click(screen.getByTestId('service-codes-preset-EMCOMM'));
    expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC EMCOMM');
    // PUBLIC already present → clicking it does not duplicate.
    fireEvent.click(screen.getByTestId('service-codes-preset-PUBLIC'));
    expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC EMCOMM');
  });

  it('does NOT offer MARS/SHARES presets (FOUO — operator-supplied only)', async () => {
    mockBackend({ value: 'PUBLIC' });
    render(<ServiceCodesField />);
    await waitFor(() => expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC'));
    expect(screen.queryByTestId('service-codes-preset-MARS')).toBeNull();
    expect(screen.queryByTestId('service-codes-preset-SHARES')).toBeNull();
  });

  it('keeps the PUBLIC default when the get command fails', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('no keyring'));
    render(<ServiceCodesField />);
    await waitFor(() => expect(screen.getByTestId('service-codes-input')).toHaveValue('PUBLIC'));
  });
});
