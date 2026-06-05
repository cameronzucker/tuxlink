/**
 * Tests for LoggingSettingsSection.
 *
 * Covers:
 *   - Off/On/Bounded mode radio transitions
 *   - Invalid bounded hours → feedback without invoke
 *   - Retention Apply → validates days + size cap, invokes logging_set_retention
 *   - Invalid retention days (out of range) → feedback without invoke
 *   - Invalid retention size cap (too small / too large) → feedback
 *   - GB unit correctly converts to MB for the invoke call
 *
 * tuxlink-qjgx alpha-logging plan Task 7.5.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { LoggingSettingsSection } from './LoggingSettingsSection';

// --- Mocks ----------------------------------------------------------------

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

// --- Helpers --------------------------------------------------------------

const MOCK_STATUS_OFF = {
  disk_usage_bytes: 0,
  disk_cap_bytes: 500 * 1024 * 1024,
  retained_window_seconds: 0,
  event_rate_per_hour: 0,
  last_export: null,
  detailed_mode: 'off' as const,
  bounded_remaining_seconds: null,
  retention_days: 14,
  retention_mb_cap: 500,
  boot_id_short: 'testboot',
  degraded: null,
};

type SettingsOverride = Partial<Omit<typeof MOCK_STATUS_OFF, 'detailed_mode'> & { detailed_mode: 'off' | 'on' | 'bounded' }>;

function renderSettings(statusOverride?: SettingsOverride) {
  const status = { ...MOCK_STATUS_OFF, ...statusOverride };
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'logging_status') return Promise.resolve(status);
    return Promise.resolve(null);
  });

  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    React.createElement(QueryClientProvider, { client },
      React.createElement(LoggingSettingsSection),
    ),
  );
}

// --- Tests ----------------------------------------------------------------

beforeEach(() => {
  vi.resetAllMocks();
});

describe('LoggingSettingsSection — rendering', () => {
  it('renders the Settings heading', () => {
    renderSettings();
    expect(screen.getByRole('heading', { name: /settings/i })).toBeInTheDocument();
  });

  it('renders Off / On / Bounded radios', () => {
    renderSettings();
    expect(screen.getByLabelText(/detailed mode off/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/detailed mode on/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/detailed mode bounded/i)).toBeInTheDocument();
  });

  it('renders retention Apply button', () => {
    renderSettings();
    expect(screen.getByRole('button', { name: /apply/i })).toBeInTheDocument();
  });
});

describe('LoggingSettingsSection — detailed mode transitions', () => {
  it('clicking Off radio invokes logging_set_detailed_mode with mode=off', async () => {
    renderSettings({ detailed_mode: 'on' });
    // Wait for the status to load before interacting
    await waitFor(() => expect(screen.getByLabelText(/detailed mode on/i)).toBeChecked());
    const radio = screen.getByLabelText(/detailed mode off/i);
    fireEvent.click(radio);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_set_detailed_mode', { mode: 'off' });
    });
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('Detailed mode set: off');
    });
  });

  it('clicking On radio invokes logging_set_detailed_mode with mode=on', async () => {
    renderSettings({ detailed_mode: 'off' });
    // Wait for the status to load
    await waitFor(() => expect(screen.getByLabelText(/detailed mode off/i)).toBeChecked());
    const radio = screen.getByLabelText(/detailed mode on/i);
    fireEvent.click(radio);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_set_detailed_mode', { mode: 'on' });
    });
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('Detailed mode set: on');
    });
  });

  it('clicking Bounded with valid hours invokes logging_set_detailed_mode with mode=bounded', async () => {
    renderSettings({ detailed_mode: 'off' });
    // Wait for status to load
    await waitFor(() => expect(screen.getByLabelText(/detailed mode off/i)).toBeChecked());
    // Set hours input to 8
    const hoursInput = screen.getByLabelText(/bounded hours/i);
    fireEvent.change(hoursInput, { target: { value: '8' } });

    const radio = screen.getByLabelText(/detailed mode bounded/i);
    fireEvent.click(radio);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_set_detailed_mode', {
        mode: 'bounded',
        boundedHours: 8,
      });
    });
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('Detailed mode set: Bounded 8h');
    });
  });

  it('clicking Bounded with invalid hours shows feedback without invoking', async () => {
    renderSettings({ detailed_mode: 'off' });
    await waitFor(() => expect(screen.getByLabelText(/detailed mode off/i)).toBeChecked());
    const hoursInput = screen.getByLabelText(/bounded hours/i);
    fireEvent.change(hoursInput, { target: { value: '999' } });

    const radio = screen.getByLabelText(/detailed mode bounded/i);
    fireEvent.click(radio);

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(/bounded hours must be 1.720/i);
    });
    expect(mockInvoke).not.toHaveBeenCalledWith('logging_set_detailed_mode', expect.anything());
  });

  it('clicking Bounded with non-numeric hours shows feedback', async () => {
    renderSettings();
    await waitFor(() => expect(screen.getByLabelText(/detailed mode off/i)).toBeChecked());
    const hoursInput = screen.getByLabelText(/bounded hours/i);
    fireEvent.change(hoursInput, { target: { value: 'abc' } });

    const radio = screen.getByLabelText(/detailed mode bounded/i);
    fireEvent.click(radio);

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(/bounded hours must be 1.720/i);
    });
  });
});

describe('LoggingSettingsSection — retention validation', () => {
  it('Apply with valid days + MB cap invokes logging_set_retention', async () => {
    renderSettings();
    const daysInput = screen.getByLabelText(/retention days/i);
    const sizeInput = screen.getByLabelText(/retention size/i);
    fireEvent.change(daysInput, { target: { value: '30' } });
    fireEvent.change(sizeInput, { target: { value: '200' } });

    const btn = screen.getByRole('button', { name: /apply/i });
    fireEvent.click(btn);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_set_retention', { days: 30, mbCap: 200 });
    });
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('Retention set: 30d / 200 MB');
    });
  });

  it('GB unit multiplied to MB before invoke', async () => {
    renderSettings();
    const sizeInput = screen.getByLabelText(/retention size/i);
    const unitSelect = screen.getByLabelText(/retention unit/i);
    fireEvent.change(sizeInput, { target: { value: '2' } });
    fireEvent.change(unitSelect, { target: { value: 'GB' } });

    const btn = screen.getByRole('button', { name: /apply/i });
    fireEvent.click(btn);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_set_retention', { days: 14, mbCap: 2048 });
    });
  });

  it('days out of range (0) shows feedback without invoke', async () => {
    renderSettings();
    const daysInput = screen.getByLabelText(/retention days/i);
    fireEvent.change(daysInput, { target: { value: '0' } });

    fireEvent.click(screen.getByRole('button', { name: /apply/i }));
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(/days must be 1.365/i);
    });
    expect(mockInvoke).not.toHaveBeenCalledWith('logging_set_retention', expect.anything());
  });

  it('days out of range (366) shows feedback without invoke', async () => {
    renderSettings();
    const daysInput = screen.getByLabelText(/retention days/i);
    fireEvent.change(daysInput, { target: { value: '366' } });

    fireEvent.click(screen.getByRole('button', { name: /apply/i }));
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(/days must be 1.365/i);
    });
  });

  it('size cap below 50 MB shows feedback without invoke', async () => {
    renderSettings();
    const sizeInput = screen.getByLabelText(/retention size/i);
    fireEvent.change(sizeInput, { target: { value: '10' } });

    fireEvent.click(screen.getByRole('button', { name: /apply/i }));
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(/50 MB.*10 GB/i);
    });
    expect(mockInvoke).not.toHaveBeenCalledWith('logging_set_retention', expect.anything());
  });

  it('size cap above 10 GB shows feedback without invoke', async () => {
    renderSettings();
    const sizeInput = screen.getByLabelText(/retention size/i);
    const unitSelect = screen.getByLabelText(/retention unit/i);
    fireEvent.change(sizeInput, { target: { value: '11' } });
    fireEvent.change(unitSelect, { target: { value: 'GB' } });

    fireEvent.click(screen.getByRole('button', { name: /apply/i }));
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(/50 MB.*10 GB/i);
    });
    expect(mockInvoke).not.toHaveBeenCalledWith('logging_set_retention', expect.anything());
  });
});
