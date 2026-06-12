import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { AntennaControl } from './AntennaControl';
import { DEFAULT_PROPAGATION_PREFS } from './propagationPrefs';

describe('AntennaControl', () => {
  it('renders the current antenna preset as the selected option', () => {
    render(<AntennaControl prefs={{ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'mobile-hf-whip' }} onChange={() => {}} />);
    const select = screen.getByTestId('antenna-select') as HTMLSelectElement;
    expect(select.value).toBe('mobile-hf-whip');
  });

  it('calls onChange with the new preset when the antenna is changed', () => {
    const onChange = vi.fn();
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={onChange} />);
    fireEvent.change(screen.getByTestId('antenna-select'), { target: { value: 'base-vertical-radials' } });
    expect(onChange).toHaveBeenCalledWith({ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'base-vertical-radials' });
  });

  it('persists an in-range REQ.SNR change', () => {
    const onChange = vi.fn();
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={onChange} />);
    fireEvent.change(screen.getByTestId('req-snr-input'), { target: { value: '24' } });
    expect(onChange).toHaveBeenCalledWith({ ...DEFAULT_PROPAGATION_PREFS, reqSnrDb: 24 });
  });

  it('does NOT persist an out-of-range REQ.SNR (>=100, the Fortran-field overflow bound)', () => {
    const onChange = vi.fn();
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={onChange} />);
    fireEvent.change(screen.getByTestId('req-snr-input'), { target: { value: '150' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('persists a positive TX power but rejects a non-positive one', () => {
    const onChange = vi.fn();
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={onChange} />);
    fireEvent.change(screen.getByTestId('tx-power-input'), { target: { value: '50' } });
    expect(onChange).toHaveBeenCalledWith({ ...DEFAULT_PROPAGATION_PREFS, txPowerW: 50 });
    onChange.mockClear();
    fireEvent.change(screen.getByTestId('tx-power-input'), { target: { value: '0' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('shows an inline error when provided', () => {
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={() => {}} error="Could not save antenna settings." />);
    expect(screen.getByRole('alert').textContent).toMatch(/could not save/i);
  });
});
