// ClassifierWeights.test.tsx — the shared weights-acquisition surface
// (tuxlink-13ofm). The hook is stubbed so each render states the exact
// status/progress under test; the action wrappers are stubbed so button
// wiring is asserted without a backend.

import { describe, it, expect, vi, afterEach, beforeEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import type { WeightsProgress, WeightsStatus } from './weightsApi';

const hookState: {
  status: WeightsStatus | null;
  progress: WeightsProgress | null;
} = { status: null, progress: null };

vi.mock('./useWeightsJob', () => ({
  useWeightsJob: () => ({
    status: hookState.status,
    progress: hookState.progress,
    refresh: vi.fn(),
    accept: vi.fn(),
  }),
}));

vi.mock('./weightsApi', async (importOriginal) => {
  const real = await importOriginal<typeof import('./weightsApi')>();
  return {
    ...real,
    weightsDownloadStart: vi.fn(async () => baseStatus()),
    weightsDownloadCancel: vi.fn(async () => baseStatus()),
    weightsSideloadImport: vi.fn(async () => baseStatus()),
  };
});

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(async () => '/media/usb0/models'),
}));

import {
  weightsDownloadCancel,
  weightsDownloadStart,
  weightsSideloadImport,
} from './weightsApi';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { ClassifierWeights } from './ClassifierWeights';

function baseStatus(overrides: Partial<WeightsStatus> = {}): WeightsStatus {
  return {
    modelId: 'bge-small-en-v1.5',
    totalBytes: 134_178_443,
    ready: false,
    integrity: null,
    location: null,
    summary: "'bge-small-en-v1.5' not found in 2 location(s): …",
    defaultSource: 'https://github.com/cameronzucker/tuxlink/releases/download/v0.106.0',
    job: null,
    ...overrides,
  };
}

beforeEach(() => {
  hookState.status = null;
  hookState.progress = null;
});

afterEach(() => vi.clearAllMocks());

describe('<ClassifierWeights>', () => {
  it('shows a probe line until the first status lands', () => {
    render(<ClassifierWeights variant="wizard" />);
    expect(screen.getByText(/checking classifier model/i)).toBeInTheDocument();
  });

  it('ready state names the digest provenance and continues (wizard)', () => {
    hookState.status = baseStatus({
      ready: true,
      integrity: 'digest-pinned',
      location: '/home/op/.local/share/tuxlink/models/bge-small-en-v1.5',
    });
    const onContinue = vi.fn();
    render(<ClassifierWeights variant="wizard" onContinue={onContinue} />);
    expect(screen.getByTestId('cw-ready').textContent).toMatch(
      /digest-verified against this release/,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(onContinue).toHaveBeenCalledTimes(1);
  });

  it('panel variant when ready renders no wizard actions', () => {
    hookState.status = baseStatus({ ready: true, integrity: 'digest-pinned' });
    render(<ClassifierWeights variant="panel" />);
    expect(screen.queryByRole('button', { name: 'Continue' })).toBeNull();
  });

  it('a running download shows progress, continue-while-downloading, and cancel', async () => {
    hookState.status = baseStatus({
      job: {
        state: 'downloading',
        detail: null,
        errorClass: null,
        file: 'model.safetensors',
        filesDone: ['config.json'],
        source: 'https://github.com/cameronzucker/tuxlink/releases/download/v0.106.0',
        startedUnix: 1,
        updatedUnix: 2,
      },
    });
    hookState.progress = { file: 'model.safetensors', got: 66_733_152, total: 133_466_304 };
    const onContinue = vi.fn();
    render(<ClassifierWeights variant="wizard" onContinue={onContinue} />);

    expect(screen.getByTestId('cw-job-phase').textContent).toMatch(/model\.safetensors/);
    expect(screen.getByTestId('cw-job-bytes').textContent).toMatch(/64 MB of 127 MB/);
    expect(screen.getByTestId('cw-job-bytes').textContent).toMatch(/1 of 3 files done/);

    fireEvent.click(screen.getByTestId('cw-continue-while-downloading'));
    expect(onContinue).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTestId('cw-cancel'));
    await waitFor(() => expect(weightsDownloadCancel).toHaveBeenCalledTimes(1));
  });

  it('a network failure offers Resume and a digest mismatch explains the refusal', () => {
    hookState.status = baseStatus({
      job: {
        state: 'failed',
        detail: 'model.safetensors: transfer ended early at 5 of 133466304 bytes',
        errorClass: 'network',
        file: null,
        filesDone: [],
        source: 'x',
        startedUnix: 1,
        updatedUnix: 2,
      },
    });
    const { unmount } = render(<ClassifierWeights variant="panel" />);
    expect(screen.getByTestId('cw-download').textContent).toBe('Resume download');
    unmount();

    hookState.status = baseStatus({
      job: {
        state: 'failed',
        detail: 'model.safetensors: downloaded bytes hash to 1234… where this release pins 97a5…',
        errorClass: 'digest-mismatch',
        file: null,
        filesDone: [],
        source: 'x',
        startedUnix: 1,
        updatedUnix: 2,
      },
    });
    render(<ClassifierWeights variant="panel" />);
    expect(screen.getByTestId('cw-download').textContent).toBe('Retry download');
    expect(screen.getByTestId('cw-job-error').textContent).toMatch(/hash to/);
    expect(screen.getByText(/NOT what this release vouches for/)).toBeInTheDocument();
  });

  it('a failed replacement outranks an existing install (ready is not a mask)', () => {
    // Codex P2: an existing usable copy + a failed replacement job must show
    // the failure and its guidance, not the green ready line.
    hookState.status = baseStatus({
      ready: true,
      integrity: 'structure',
      job: {
        state: 'failed',
        detail: 'model.safetensors: downloaded bytes hash to 1234… where this release pins 97a5…',
        errorClass: 'digest-mismatch',
        file: null,
        filesDone: [],
        source: 'x',
        startedUnix: 1,
        updatedUnix: 2,
      },
    });
    render(<ClassifierWeights variant="panel" />);
    expect(screen.queryByTestId('cw-ready')).toBeNull();
    expect(screen.getByTestId('cw-job-error')).toBeInTheDocument();
    expect(screen.getByTestId('cw-ready-despite-failure').textContent).toMatch(
      /not digest-verified against this release/,
    );
  });

  it('idle wizard offers download with the size, skip, and a custom source', async () => {
    hookState.status = baseStatus();
    const onSkip = vi.fn();
    render(<ClassifierWeights variant="wizard" onSkip={onSkip} />);

    expect(screen.getByTestId('cw-download').textContent).toBe('Download (128 MB)');
    fireEvent.click(screen.getByTestId('cw-skip'));
    expect(onSkip).toHaveBeenCalledTimes(1);

    // Custom source: revealed by the toggle, passed through to the start call.
    fireEvent.click(screen.getByTestId('cw-source-toggle'));
    fireEvent.change(screen.getByTestId('cw-source-input'), {
      target: { value: 'https://mirror.example/weights' },
    });
    fireEvent.click(screen.getByTestId('cw-download'));
    await waitFor(() =>
      expect(weightsDownloadStart).toHaveBeenCalledWith('https://mirror.example/weights'),
    );
  });

  it('the default start passes no source override', async () => {
    hookState.status = baseStatus();
    render(<ClassifierWeights variant="panel" />);
    fireEvent.click(screen.getByTestId('cw-download'));
    await waitFor(() => expect(weightsDownloadStart).toHaveBeenCalledWith(undefined));
  });

  it('sideload picks a folder and imports it', async () => {
    hookState.status = baseStatus();
    render(<ClassifierWeights variant="panel" />);
    fireEvent.click(screen.getByTestId('cw-sideload'));
    await waitFor(() => expect(weightsSideloadImport).toHaveBeenCalledWith('/media/usb0/models'));
    expect(openDialog).toHaveBeenCalledWith({ directory: true, multiple: false });
  });

  it('a cancelled folder pick imports nothing', async () => {
    hookState.status = baseStatus();
    (openDialog as Mock).mockResolvedValueOnce(null);
    render(<ClassifierWeights variant="panel" />);
    fireEvent.click(screen.getByTestId('cw-sideload'));
    await waitFor(() => expect(openDialog).toHaveBeenCalled());
    expect(weightsSideloadImport).not.toHaveBeenCalled();
  });
});
