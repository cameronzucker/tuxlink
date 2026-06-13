import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { GribForm } from './GribForm';
import type { GribRequest } from '../grib/types';

// Mock GridPicker at the module boundary — assert the box→region WIRING,
// not the map renderer (mirrors the former GRIB request panel's test). The mock
// exposes a button that fires onBoxChange with two signed corners (the NE→SW drag
// from gribRegion.test.ts case 1).
vi.mock('../map/GridPicker', () => ({
  GridPicker: ({
    onBoxChange,
  }: {
    onBoxChange?: (a: { lat: number; lon: number }, b: { lat: number; lon: number }) => void;
  }) => (
    <button
      type="button"
      data-testid="mock-box-drag"
      onClick={() => onBoxChange?.({ lat: 60.2, lon: -120.9 }, { lat: 40.8, lon: -140.1 })}
    >
      fire box
    </button>
  ),
}));

describe('<GribForm>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders all sections with sensible defaults', () => {
    render(<GribForm onAddSaildocs={() => {}} onBack={() => {}} />);
    expect(screen.getByTestId('request-grib')).toBeInTheDocument();
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

  it('shows an error message for malformed forecast-times input and disables Add', () => {
    render(<GribForm onAddSaildocs={() => {}} onBack={() => {}} />);
    fireEvent.change(screen.getByTestId('grib-times'), { target: { value: 'abc' } });
    expect(screen.getByTestId('grib-times-error')).toBeInTheDocument();
    // Add button is disabled while there's a parse error
    expect(screen.getByTestId('grib-add')).toBeDisabled();
  });

  it('clears the forecast-times error when input becomes valid', () => {
    render(<GribForm onAddSaildocs={() => {}} onBack={() => {}} />);
    fireEvent.change(screen.getByTestId('grib-times'), { target: { value: 'abc' } });
    expect(screen.queryByTestId('grib-times-error')).toBeInTheDocument();
    fireEvent.change(screen.getByTestId('grib-times'), { target: { value: '24,48' } });
    expect(screen.queryByTestId('grib-times-error')).toBeNull();
  });

  it('switching to sub-mode reveals the days+time fields', () => {
    render(<GribForm onAddSaildocs={() => {}} onBack={() => {}} />);
    expect(screen.queryByTestId('grib-sub-days')).toBeNull();
    fireEvent.click(screen.getByTestId('grib-mode-sub'));
    expect(screen.getByTestId('grib-sub-days')).toBeInTheDocument();
    expect(screen.getByTestId('grib-sub-time')).toBeInTheDocument();
  });

  it('selecting parameters + Add calls onAddSaildocs with the request (no send)', () => {
    const onAddSaildocs = vi.fn();
    render(<GribForm onAddSaildocs={onAddSaildocs} onBack={() => {}} />);
    fireEvent.click(screen.getByTestId('grib-param-WIND'));
    fireEvent.click(screen.getByTestId('grib-param-WAVES'));
    fireEvent.click(screen.getByTestId('grib-add'));
    expect(onAddSaildocs).toHaveBeenCalledTimes(1);
    const req = onAddSaildocs.mock.calls[0][0] as GribRequest;
    expect(req.params).toEqual(['WIND', 'WAVES']);
    expect(req.mode).toBe('send');
  });

  it('Add button disabled when subject is empty after trim', () => {
    render(<GribForm onAddSaildocs={() => {}} onBack={() => {}} />);
    fireEvent.change(screen.getByTestId('grib-subject'), { target: { value: '   ' } });
    expect(screen.getByTestId('grib-add')).toBeDisabled();
  });

  it('sub-mode with days and time round-trips into the request passed to onAddSaildocs', () => {
    const onAddSaildocs = vi.fn();
    render(<GribForm onAddSaildocs={onAddSaildocs} onBack={() => {}} />);
    fireEvent.click(screen.getByTestId('grib-mode-sub'));
    fireEvent.change(screen.getByTestId('grib-sub-days'), { target: { value: '30' } });
    fireEvent.change(screen.getByTestId('grib-sub-time'), { target: { value: '06:00' } });
    fireEvent.click(screen.getByTestId('grib-add'));
    expect(onAddSaildocs).toHaveBeenCalledTimes(1);
    const req = onAddSaildocs.mock.calls[0][0] as GribRequest;
    expect(req.mode).toBe('sub');
    expect(req.sub_days).toBe(30);
    expect(req.sub_time).toBe('06:00');
  });

  it('a valid forecast-times string round-trips into the emitted request', () => {
    const onAddSaildocs = vi.fn();
    render(<GribForm onAddSaildocs={onAddSaildocs} onBack={() => {}} />);
    fireEvent.change(screen.getByTestId('grib-times'), { target: { value: '6,12..96' } });
    // No parse error → the success branch wrote request.times.
    expect(screen.queryByTestId('grib-times-error')).toBeNull();
    fireEvent.click(screen.getByTestId('grib-add'));
    expect(onAddSaildocs).toHaveBeenCalledTimes(1);
    const req = onAddSaildocs.mock.calls[0][0] as GribRequest;
    expect(req.times).toEqual([{ Hour: 6 }, { Range: { start: 12, end: 96 } }]);
  });

  it('switching from sub back to send clears the stale sub fields in the emitted request', () => {
    const onAddSaildocs = vi.fn();
    render(<GribForm onAddSaildocs={onAddSaildocs} onBack={() => {}} />);
    // Enter sub mode and set a days value.
    fireEvent.click(screen.getByTestId('grib-mode-sub'));
    fireEvent.change(screen.getByTestId('grib-sub-days'), { target: { value: '30' } });
    // Switch back to send — sub fields must be reset so two logically-identical
    // send requests produce the same basket id.
    fireEvent.click(screen.getByTestId('grib-mode-send'));
    fireEvent.click(screen.getByTestId('grib-add'));
    expect(onAddSaildocs).toHaveBeenCalledTimes(1);
    const req = onAddSaildocs.mock.calls[0][0] as GribRequest;
    expect(req.mode).toBe('send');
    expect(req.sub_days).toBeNull();
    expect(req.sub_time).toBeNull();
  });

  it('map box-drag fills the region fields; manual inputs stay editable', () => {
    render(<GribForm onAddSaildocs={() => {}} onBack={() => {}} />);

    // Fire a box drag via the mocked picker → signedBboxToGribRegion wiring.
    fireEvent.click(screen.getByTestId('mock-box-drag'));

    // Region fields reflect signedBboxToGribRegion((60.2N,120.9W),(40.8N,140.1W)):
    // south=40N, north=61N, west=141W, east=120W (ordered, whole-degree, outward).
    expect((screen.getByTestId('grib-lat0-deg') as HTMLInputElement).value).toBe('40');
    expect((screen.getByTestId('grib-lat0-dir') as HTMLSelectElement).value).toBe('N');
    expect((screen.getByTestId('grib-lat1-deg') as HTMLInputElement).value).toBe('61');
    expect((screen.getByTestId('grib-lat1-dir') as HTMLSelectElement).value).toBe('N');
    expect((screen.getByTestId('grib-lon0-deg') as HTMLInputElement).value).toBe('141');
    expect((screen.getByTestId('grib-lon0-dir') as HTMLSelectElement).value).toBe('W');
    expect((screen.getByTestId('grib-lon1-deg') as HTMLInputElement).value).toBe('120');
    expect((screen.getByTestId('grib-lon1-dir') as HTMLSelectElement).value).toBe('W');

    // The manual region inputs remain present + editable (the map is an aid).
    const lat0 = screen.getByTestId('grib-lat0-deg') as HTMLInputElement;
    expect(lat0).toBeEnabled();
    fireEvent.change(lat0, { target: { value: '12' } });
    expect(lat0.value).toBe('12');
  });

  it('the Back button calls onBack', () => {
    const onBack = vi.fn();
    render(<GribForm onAddSaildocs={() => {}} onBack={onBack} />);
    fireEvent.click(screen.getByTestId('grib-back'));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it('does not render any send/cancel/close overlay affordances', () => {
    render(<GribForm onAddSaildocs={() => {}} onBack={() => {}} />);
    expect(screen.queryByTestId('grib-send')).toBeNull();
    expect(screen.queryByTestId('grib-cancel')).toBeNull();
    expect(screen.queryByTestId('grib-close')).toBeNull();
    expect(screen.queryByTestId('grib-overlay')).toBeNull();
  });
});
