import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
const invoke = vi.fn().mockResolvedValue({ sourceSsid: 0, tocall: 'APZTUX', path: 'WIDE1-1,WIDE2-1' });
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import { AprsSettings } from './AprsSettings';

describe('AprsSettings', () => {
  it('loads and displays the current APRS config', async () => {
    render(<AprsSettings />);
    await waitFor(() => expect(screen.getByDisplayValue('WIDE1-1,WIDE2-1')).toBeInTheDocument());
    expect(screen.getByText('APZTUX')).toBeInTheDocument();
  });

  it('persists a changed path via aprs_config_set', async () => {
    render(<AprsSettings />);
    await waitFor(() => screen.getByDisplayValue('WIDE1-1,WIDE2-1'));
    fireEvent.change(screen.getByLabelText(/path/i), { target: { value: 'WIDE2-1' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('aprs_config_set',
      expect.objectContaining({ dto: expect.objectContaining({ path: 'WIDE2-1' }) })));
  });
});
