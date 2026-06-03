import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { GribRequestPanel } from './GribRequestPanel';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

describe('<GribRequestPanel>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders all sections with sensible defaults', () => {
    render(<GribRequestPanel onClose={() => {}} />);
    expect(screen.getByTestId('grib-panel')).toBeInTheDocument();
    // Region defaults (canonical Saildocs example: 40N/60N/140W/120W)
    expect((screen.getByTestId('grib-lat0-deg') as HTMLInputElement).value).toBe('40');
    expect((screen.getByTestId('grib-lat0-dir') as HTMLSelectElement).value).toBe('N');
    expect((screen.getByTestId('grib-lon1-deg') as HTMLInputElement).value).toBe('120');
    expect((screen.getByTestId('grib-lon1-dir') as HTMLSelectElement).value).toBe('W');
    // Grid defaults to 2,2
    expect((screen.getByTestId('grib-dlat') as HTMLInputElement).value).toBe('2');
    expect((screen.getByTestId('grib-dlon') as HTMLInputElement).value).toBe('2');
    // Subject default
    expect((screen.getByTestId('grib-subject') as HTMLInputElement).value).toBe('GRIB request');
    // Mode default: send
    expect((screen.getByTestId('grib-mode-send') as HTMLInputElement).checked).toBe(true);
    expect((screen.getByTestId('grib-mode-sub') as HTMLInputElement).checked).toBe(false);
  });

  it('shows an error message for malformed forecast-times input', () => {
    render(<GribRequestPanel onClose={() => {}} />);
    fireEvent.change(screen.getByTestId('grib-times'), { target: { value: 'abc' } });
    expect(screen.getByTestId('grib-times-error')).toBeInTheDocument();
    // Send button is disabled while there's a parse error
    expect(screen.getByTestId('grib-send')).toBeDisabled();
  });

  it('clears the forecast-times error when input becomes valid', () => {
    render(<GribRequestPanel onClose={() => {}} />);
    fireEvent.change(screen.getByTestId('grib-times'), { target: { value: 'abc' } });
    expect(screen.queryByTestId('grib-times-error')).toBeInTheDocument();
    fireEvent.change(screen.getByTestId('grib-times'), { target: { value: '24,48' } });
    expect(screen.queryByTestId('grib-times-error')).toBeNull();
  });

  it('switching to sub-mode reveals the days+time fields', () => {
    render(<GribRequestPanel onClose={() => {}} />);
    expect(screen.queryByTestId('grib-sub-days')).toBeNull();
    fireEvent.click(screen.getByTestId('grib-mode-sub'));
    expect(screen.getByTestId('grib-sub-days')).toBeInTheDocument();
    expect(screen.getByTestId('grib-sub-time')).toBeInTheDocument();
  });

  it('selecting parameter checkboxes adds them to the outgoing request', async () => {
    let sentRequest: unknown = null;
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'grib_send_request') {
        sentRequest = (args as { request: unknown }).request;
        return 'MID-GRIB-1';
      }
      return null;
    });
    render(<GribRequestPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('grib-param-WIND'));
    fireEvent.click(screen.getByTestId('grib-param-WAVES'));
    fireEvent.click(screen.getByTestId('grib-send'));
    await waitFor(() => expect(screen.getByTestId('grib-send-success')).toBeInTheDocument());
    const req = sentRequest as { params: string[]; mode: string };
    expect(req.params).toEqual(['WIND', 'WAVES']);
    expect(req.mode).toBe('send');
  });

  it('Send button disabled when subject is empty after trim', () => {
    render(<GribRequestPanel onClose={() => {}} />);
    fireEvent.change(screen.getByTestId('grib-subject'), { target: { value: '   ' } });
    expect(screen.getByTestId('grib-send')).toBeDisabled();
  });

  it('surfaces a backend error from grib_send_request', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'grib_send_request') throw new Error('backend offline');
      return null;
    });
    render(<GribRequestPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('grib-send'));
    await waitFor(() => expect(screen.getByTestId('grib-send-error')).toBeInTheDocument());
    expect(screen.getByTestId('grib-send-error')).toHaveTextContent('backend offline');
  });

  it('Cancel + backdrop click both call onClose; panel click does not', () => {
    const onClose = vi.fn();
    render(<GribRequestPanel onClose={onClose} />);
    fireEvent.click(screen.getByTestId('grib-panel'));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('grib-cancel'));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId('grib-overlay'));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('sub-mode with days and time round-trips through the request payload', async () => {
    let sentRequest: unknown = null;
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'grib_send_request') {
        sentRequest = (args as { request: unknown }).request;
        return 'MID-2';
      }
      return null;
    });
    render(<GribRequestPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('grib-mode-sub'));
    fireEvent.change(screen.getByTestId('grib-sub-days'), { target: { value: '30' } });
    fireEvent.change(screen.getByTestId('grib-sub-time'), { target: { value: '06:00' } });
    fireEvent.click(screen.getByTestId('grib-send'));
    await waitFor(() => expect(screen.getByTestId('grib-send-success')).toBeInTheDocument());
    const req = sentRequest as { mode: string; sub_days: number; sub_time: string };
    expect(req.mode).toBe('sub');
    expect(req.sub_days).toBe(30);
    expect(req.sub_time).toBe('06:00');
  });
});
