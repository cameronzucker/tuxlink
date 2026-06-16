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

  it('persists an arbitrary positive power (not just common levels)', () => {
    const onChange = vi.fn();
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={onChange} />);
    // 73 W is not a "round" preset — freeform entry must still take it.
    fireEvent.change(screen.getByTestId('tx-power-input'), { target: { value: '73' } });
    expect(onChange).toHaveBeenCalledWith({ ...DEFAULT_PROPAGATION_PREFS, txPowerW: 73 });
  });

  it('rejects a non-positive power', () => {
    const onChange = vi.fn();
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={onChange} />);
    fireEvent.change(screen.getByTestId('tx-power-input'), { target: { value: '0' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('shows an inline error when provided', () => {
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={() => {}} error="Could not save antenna settings." />);
    expect(screen.getByRole('alert').textContent).toMatch(/could not save/i);
  });

  // ---- Antenna picker (tuxlink-bl01 Group E) ----

  it('hides the height slider and shows ground-mounted for a vertical', () => {
    render(<AntennaControl prefs={{ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'base-vertical-radials' }} onChange={() => {}} />);
    expect(screen.queryByTestId('antenna-height-slider')).toBeNull();
    expect(screen.getByTestId('antenna-ground-mounted').textContent).toMatch(/ground-mounted/i);
  });

  it('shows a four-stop height slider for a horizontal', () => {
    render(<AntennaControl prefs={{ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'efhw-sloper' }} onChange={() => {}} />);
    const slider = screen.getByTestId('antenna-height-slider') as HTMLInputElement;
    expect(slider.min).toBe('0');
    expect(slider.max).toBe('3'); // 4 grid indices: 0..3
    expect(slider.step).toBe('1');
  });

  it('snaps the slider index to a grid height when dragged', () => {
    const onChange = vi.fn();
    render(<AntennaControl prefs={{ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'efhw-sloper' }} onChange={onChange} />);
    // index 0 → 2.5 m grid stop
    fireEvent.change(screen.getByTestId('antenna-height-slider'), { target: { value: '0' } });
    expect(onChange).toHaveBeenCalledWith({ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'efhw-sloper', antennaHeightM: 2.5 });
  });

  it('labels the single-ground limitation without leaking an internal phase ref', () => {
    render(<AntennaControl prefs={DEFAULT_PROPAGATION_PREFS} onChange={() => {}} />);
    const note = screen.getByText(/poor \/ dry-desert ground/i);
    expect(note.textContent).toMatch(/regardless of the ground selection/i);
    expect(note.textContent).not.toMatch(/phase\s*1/i);
  });
});
