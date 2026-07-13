// src/wwv/WwvOffairControl.test.tsx
//
// Tests for WwvOffairControl (Task 15, wwv offair spec). The hook module is
// mocked entirely so the component renders deterministically without a real
// Tauri context — see useWwvOffair.ts for the real (invoke-backed) shape.

import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import type { UseWwvOffairResult, WwvOffairStatus } from './useWwvOffair';
import type { SolarSnapshot } from './wwvApi';

// ---------------------------------------------------------------------------
// Module-level mock (hoisted above imports per vi.mock hoisting rules)
// ---------------------------------------------------------------------------

const mockArm = vi.fn((_nowMs: number) => {});
const mockCancel = vi.fn(() => {});
const mockRefreshSnapshot = vi.fn(() => Promise.resolve());
const mockRefreshCat = vi.fn(() => Promise.resolve());
const mockManualIngest = vi.fn((_sfi: number, _aIndex: number | null, _kIndex: number | null) => Promise.resolve());

let mockStatus: WwvOffairStatus = 'idle';
let mockSnapshot: SolarSnapshot | null = null;
let mockWindowLabel: string | null = null;
let mockWavPath: string | null = null;
let mockCatConfigured: boolean | null = null;

vi.mock('./useWwvOffair', () => ({
  useWwvOffair: (): UseWwvOffairResult => ({
    status: mockStatus,
    result: null,
    snapshot: mockSnapshot,
    windowLabel: mockWindowLabel,
    wavPath: mockWavPath,
    catConfigured: mockCatConfigured,
    arm: mockArm,
    cancel: mockCancel,
    refreshSnapshot: mockRefreshSnapshot,
    refreshCat: mockRefreshCat,
    manualIngest: mockManualIngest,
  }),
}));

vi.mock('./wwvApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./wwvApi')>();
  return {
    ...actual,
    readClip: vi.fn(() => Promise.resolve(new Uint8Array([1, 2, 3]))),
  };
});

// ---------------------------------------------------------------------------
// Subject under test (imported AFTER the mock is set up)
// ---------------------------------------------------------------------------

import { WwvOffairControl } from './WwvOffairControl';
import { readClip } from './wwvApi';

// jsdom doesn't implement the Blob URL registry; stub both for the whole
// file's lifetime (beforeAll/afterAll, not per-test) so the component's
// unmount cleanup — which unconditionally calls URL.revokeObjectURL when a
// clip URL was ever set — never crashes the global afterEach(cleanup()) in
// src/test-setup.ts. A per-test beforeEach/afterEach pairing would race that
// same-named hook: hook execution order between a setup file's afterEach and
// this file's afterEach isn't something to depend on, so the stub has to
// outlive every per-test cleanup() call, not just the test body.
let origCreateObjectURL: typeof URL.createObjectURL;
let origRevokeObjectURL: typeof URL.revokeObjectURL;

beforeAll(() => {
  origCreateObjectURL = URL.createObjectURL;
  origRevokeObjectURL = URL.revokeObjectURL;
  URL.createObjectURL = vi.fn(() => 'blob:mock');
  URL.revokeObjectURL = vi.fn();
});

afterAll(() => {
  URL.createObjectURL = origCreateObjectURL;
  URL.revokeObjectURL = origRevokeObjectURL;
});

beforeEach(() => {
  vi.clearAllMocks();
  mockStatus = 'idle';
  mockSnapshot = null;
  mockWindowLabel = null;
  mockWavPath = null;
  mockCatConfigured = null;
});

describe('WwvOffairControl', () => {
  it('renders a "Refresh from WWV" button that arms a capture on click', async () => {
    render(<WwvOffairControl />);
    const button = screen.getByRole('button', { name: 'Refresh from WWV' });
    fireEvent.click(button);
    await waitFor(() => {
      expect(mockArm).toHaveBeenCalledOnce();
    });
    // arm(Date.now()) — just check it was called with a number.
    expect(typeof mockArm.mock.calls[0][0]).toBe('number');
  });

  it('mounts without crashing and kicks a background snapshot refresh', () => {
    render(<WwvOffairControl />);
    expect(mockRefreshSnapshot).toHaveBeenCalledOnce();
  });

  it('shows "Capturing…" and disables the button while status is capturing', () => {
    mockStatus = 'capturing';
    render(<WwvOffairControl />);
    const button = screen.getByRole('button', { name: 'Capturing…' });
    expect(button).toBeDisabled();
  });

  it('shows the off-air provenance stamp when the snapshot is rf-wwv-voice sourced', () => {
    mockSnapshot = {
      indices: { sfi: 150, k_index: 2 },
      updated_at_ms: 1_700_000_000_000,
      source: 'rf-wwv-voice',
      forecast_updated: true,
    };
    render(<WwvOffairControl />);
    const stamp = screen.getByTestId('wwv-offair-provenance');
    expect(stamp.textContent).toContain('SFI');
    expect(stamp.textContent).toContain('150');
    expect(stamp.textContent).toContain('K');
    expect(stamp.textContent).toContain('2');
  });

  it('omits the K index from the provenance stamp when absent', () => {
    mockSnapshot = {
      indices: { sfi: 140 },
      updated_at_ms: 1_700_000_000_000,
      source: 'rf-wwv-voice',
      forecast_updated: true,
    };
    render(<WwvOffairControl />);
    const stamp = screen.getByTestId('wwv-offair-provenance');
    expect(stamp.textContent).toContain('SFI');
    expect(stamp.textContent).not.toContain('K ');
  });

  it('does not show the provenance stamp for a non-off-air-sourced snapshot', () => {
    mockSnapshot = {
      indices: { sfi: 150, k_index: 2 },
      updated_at_ms: 1_700_000_000_000,
      source: 'swpc',
      forecast_updated: true,
    };
    render(<WwvOffairControl />);
    expect(screen.queryByTestId('wwv-offair-provenance')).toBeNull();
  });

  it('shows the window label and a working Cancel button when status is armed', () => {
    mockStatus = 'armed';
    mockWindowLabel = 'WWV :18';
    render(<WwvOffairControl />);

    const armedNote = screen.getByTestId('wwv-offair-armed');
    expect(armedNote.textContent).toContain('WWV :18');

    const cancelButton = screen.getByTestId('wwv-offair-cancel');
    fireEvent.click(cancelButton);
    expect(mockCancel).toHaveBeenCalledOnce();

    // The "Refresh from WWV" button is disabled while armed.
    const refreshButton = screen.getByRole('button', { name: 'Refresh from WWV' });
    expect(refreshButton).toBeDisabled();
  });

  it('shows a no-copy note when status is nocopy', () => {
    mockStatus = 'nocopy';
    render(<WwvOffairControl />);
    expect(screen.getByTestId('wwv-offair-nocopy')).toBeInTheDocument();
  });

  it('shows an error note when status is error', () => {
    mockStatus = 'error';
    render(<WwvOffairControl />);
    expect(screen.getByTestId('wwv-offair-error')).toBeInTheDocument();
  });

  it('shows the manual-tune hint instead of the armed note when CAT is not configured', () => {
    mockStatus = 'armed';
    mockWindowLabel = 'WWV :18';
    mockCatConfigured = false;
    render(<WwvOffairControl />);

    const hint = screen.getByTestId('wwv-offair-manual-tune');
    expect(hint.textContent).toContain('Tune your radio to WWV');
    expect(hint.textContent).toContain('WWV :18');
    expect(screen.queryByTestId('wwv-offair-armed')).toBeNull();

    // Cancel still works from the manual-tune variant.
    fireEvent.click(screen.getByTestId('wwv-offair-cancel'));
    expect(mockCancel).toHaveBeenCalledOnce();
  });

  it('keeps the plain armed note when CAT is configured', () => {
    mockStatus = 'armed';
    mockWindowLabel = 'WWV :18';
    mockCatConfigured = true;
    render(<WwvOffairControl />);

    expect(screen.getByTestId('wwv-offair-armed')).toBeInTheDocument();
    expect(screen.queryByTestId('wwv-offair-manual-tune')).toBeNull();
  });

  it('shows the provenance stamp for a manual-source snapshot, labeled as manual', () => {
    mockSnapshot = {
      indices: { sfi: 130 },
      updated_at_ms: 1_700_000_000_000,
      source: 'rf-wwv-manual',
      forecast_updated: true,
    };
    render(<WwvOffairControl />);
    const stamp = screen.getByTestId('wwv-offair-provenance');
    expect(stamp.textContent).toContain('(manual)');
    expect(stamp.textContent).toContain('130');
  });

  it('renders a Play clip button when nocopy has a wavPath, and it reads the clip', async () => {
    mockStatus = 'nocopy';
    mockWavPath = '/tmp/wwv-clip.wav';

    render(<WwvOffairControl />);
    const playButton = screen.getByTestId('wwv-offair-play');
    fireEvent.click(playButton);

    await waitFor(() => {
      expect(readClip).toHaveBeenCalledWith('/tmp/wwv-clip.wav');
    });
    await waitFor(() => {
      expect(document.querySelector('audio')).toBeTruthy();
    });
  });

  it('does not render the Play clip button when nocopy has no wavPath', () => {
    mockStatus = 'nocopy';
    mockWavPath = null;
    render(<WwvOffairControl />);
    expect(screen.queryByTestId('wwv-offair-play')).toBeNull();
  });

  it('clicking Play clip twice rapidly revokes the first blob URL exactly once (no ref race)', async () => {
    mockStatus = 'nocopy';
    mockWavPath = '/tmp/wwv-clip.wav';
    let call = 0;
    vi.mocked(URL.createObjectURL).mockImplementation(() => `blob:mock-${++call}`);

    render(<WwvOffairControl />);
    const playButton = screen.getByTestId('wwv-offair-play');

    // Two rapid clicks before either readClip promise settles — the ref
    // update in the Play handler must be synchronous (not deferred to the
    // clipUrl-change effect) so the second resolution sees the first URL on
    // clipUrlRef.current and revokes it instead of orphaning it.
    fireEvent.click(playButton);
    fireEvent.click(playButton);

    await waitFor(() => {
      expect(readClip).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:mock-1');
    });
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(1);
  });

  it('renders the SFI input and Save button when nocopy, and submits parsed values', async () => {
    mockStatus = 'nocopy';
    render(<WwvOffairControl />);

    const sfiInput = screen.getByTestId('wwv-sfi-input');
    fireEvent.change(sfiInput, { target: { value: '145' } });
    fireEvent.click(screen.getByTestId('wwv-manual-save'));

    await waitFor(() => {
      expect(mockManualIngest).toHaveBeenCalledWith(145, null, null);
    });
  });

  it('parses optional A and K index inputs when present on manual submit', async () => {
    mockStatus = 'nocopy';
    render(<WwvOffairControl />);

    fireEvent.change(screen.getByTestId('wwv-sfi-input'), { target: { value: '120' } });
    fireEvent.change(screen.getByTestId('wwv-a-input'), { target: { value: '5' } });
    fireEvent.change(screen.getByTestId('wwv-k-input'), { target: { value: '2' } });
    fireEvent.click(screen.getByTestId('wwv-manual-save'));

    await waitFor(() => {
      expect(mockManualIngest).toHaveBeenCalledWith(120, 5, 2);
    });
  });

  it('does not call manualIngest when SFI is blank', () => {
    mockStatus = 'nocopy';
    render(<WwvOffairControl />);
    fireEvent.click(screen.getByTestId('wwv-manual-save'));
    expect(mockManualIngest).not.toHaveBeenCalled();
  });

  it('K-index input accepts decimal steps so a fractional value like 1.33 submits', async () => {
    mockStatus = 'nocopy';
    render(<WwvOffairControl />);

    const kInput = screen.getByTestId('wwv-k-input');
    expect(kInput).toHaveAttribute('step', 'any');

    fireEvent.change(screen.getByTestId('wwv-sfi-input'), { target: { value: '120' } });
    fireEvent.change(kInput, { target: { value: '1.33' } });
    fireEvent.click(screen.getByTestId('wwv-manual-save'));

    await waitFor(() => {
      expect(mockManualIngest).toHaveBeenCalledWith(120, null, 1.33);
    });
  });
});
