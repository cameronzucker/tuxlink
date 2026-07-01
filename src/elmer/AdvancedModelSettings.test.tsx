/**
 * AdvancedModelSettings tests (tuxlink-65qhn).
 *
 * The shared Advanced panel extracted from GetKeyCard's T8 disclosure, now
 * reused by both the cloud editor (GetKeyCard, showNumCtx=false) and the
 * local/custom editor (ModelForm, showNumCtx=true).
 *
 * Coverage:
 *   - showNumCtx toggles the num_ctx field + memory-estimate line;
 *   - estimate fit badge green (fits) / red (exceeds);
 *   - estimate invocation error → "estimate unavailable" (non-fatal);
 *   - temperature slider round-trip (value display + onChange payload);
 *   - system-prompt edit + Reset → '' (empty override → backend default).
 *
 * Invoke-mock cleanup convention: the invoke mock is command-gated so the
 * no-arg teardown call (vi.clearAllMocks) does not throw on a bare invoke().
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import type { ComponentProps } from 'react';

// Command-gated invoke mock — returns a valid MemoryEstimateDto for
// elmer_estimate_memory; undefined otherwise. Individual tests override with
// mockResolvedValueOnce / mockRejectedValueOnce.
const mockInvoke = vi.fn(async (cmd?: string, _args?: unknown) => {
  if (cmd === 'elmer_estimate_memory') {
    return {
      weightsGb: 9.0,
      kvCacheGb: 3.1,
      computeHeadroomGb: 0.5,
      totalGb: 12.6,
      hostRamGb: 32,
      fits: true,
      numCtx: 32768,
      kvDtypeBytes: 1,
    };
  }
  return undefined;
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd?: string, args?: unknown) => mockInvoke(cmd, args),
}));

import { AdvancedModelSettings, type AdvancedModelValues } from './AdvancedModelSettings';

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const PREFIX = 'ams';

function baseValues(overrides: Partial<AdvancedModelValues> = {}): AdvancedModelValues {
  return {
    numCtxStr: '32768',
    temperature: 0.2,
    systemPrompt: '',
    ...overrides,
  };
}

/**
 * Render helper that makes the panel controlled: it holds the values in a
 * closure and re-renders on onChange so round-trip assertions reflect state.
 */
function renderPanel(
  props: Partial<ComponentProps<typeof AdvancedModelSettings>> = {},
) {
  let current: AdvancedModelValues = props.values ?? baseValues();
  const onChange = vi.fn((next: AdvancedModelValues) => {
    current = next;
    rerender(
      <AdvancedModelSettings
        values={current}
        onChange={onChange}
        showNumCtx={props.showNumCtx ?? true}
        endpoint={props.endpoint ?? 'http://127.0.0.1:11434/v1/chat/completions'}
        model={props.model ?? 'qwen2.5:14b'}
        defaultSystemPromptPlaceholder={props.defaultSystemPromptPlaceholder ?? 'DEFAULT PROMPT'}
        testIdPrefix={PREFIX}
      />,
    );
  });
  const { rerender } = render(
    <AdvancedModelSettings
      values={current}
      onChange={onChange}
      showNumCtx={props.showNumCtx ?? true}
      endpoint={props.endpoint ?? 'http://127.0.0.1:11434/v1/chat/completions'}
      model={props.model ?? 'qwen2.5:14b'}
      defaultSystemPromptPlaceholder={props.defaultSystemPromptPlaceholder ?? 'DEFAULT PROMPT'}
      testIdPrefix={PREFIX}
    />,
  );
  return { onChange, getValues: () => current };
}

describe('AdvancedModelSettings', () => {
  // -----------------------------------------------------------------------
  // showNumCtx toggles the num_ctx field + estimate
  // -----------------------------------------------------------------------

  it('renders the num_ctx field when showNumCtx=true', () => {
    renderPanel({ showNumCtx: true });
    expect(screen.getByTestId(`${PREFIX}-num-ctx-row`)).toBeTruthy();
    expect(screen.getByTestId(`${PREFIX}-num-ctx`)).toBeTruthy();
    expect(screen.getByTestId(`${PREFIX}-cpu-hint`)).toBeTruthy();
  });

  it('HIDES the num_ctx field when showNumCtx=false', () => {
    renderPanel({ showNumCtx: false });
    expect(screen.queryByTestId(`${PREFIX}-num-ctx-row`)).toBeNull();
    expect(screen.queryByTestId(`${PREFIX}-num-ctx`)).toBeNull();
    expect(screen.queryByTestId(`${PREFIX}-cpu-hint`)).toBeNull();
  });

  it('does NOT call elmer_estimate_memory when showNumCtx=false', async () => {
    renderPanel({ showNumCtx: false });
    // Wait past the debounce window.
    await new Promise((r) => setTimeout(r, 800));
    const estimateCalls = mockInvoke.mock.calls.filter((c) => c[0] === 'elmer_estimate_memory');
    expect(estimateCalls).toHaveLength(0);
  });

  it('calls elmer_estimate_memory with model + num_ctx + endpoint when showNumCtx=true', async () => {
    renderPanel({
      showNumCtx: true,
      model: 'qwen2.5:14b',
      endpoint: 'http://127.0.0.1:11434/v1/chat/completions',
      values: baseValues({ numCtxStr: '16384' }),
    });
    await waitFor(() => {
      const calls = mockInvoke.mock.calls.filter((c) => c[0] === 'elmer_estimate_memory');
      expect(calls.length).toBeGreaterThan(0);
    }, { timeout: 2000 });
    const call = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_estimate_memory')!;
    expect(call[1]).toMatchObject({
      model: 'qwen2.5:14b',
      numCtx: 16384,
      endpoint: 'http://127.0.0.1:11434/v1/chat/completions',
    });
  });

  // -----------------------------------------------------------------------
  // Estimate fit badge — green / red / error
  // -----------------------------------------------------------------------

  it('shows a green fits badge when estimate.fits=true', async () => {
    mockInvoke.mockResolvedValueOnce({
      weightsGb: 9.0, kvCacheGb: 3.1, computeHeadroomGb: 0.5, totalGb: 12.6,
      hostRamGb: 32, fits: true, numCtx: 32768, kvDtypeBytes: 1,
    });
    renderPanel({ showNumCtx: true });
    await waitFor(() => {
      expect(screen.getByTestId(`${PREFIX}-estimate-fits`)).toBeTruthy();
    }, { timeout: 2000 });
    const badge = screen.getByTestId(`${PREFIX}-estimate-fits`);
    expect(badge.textContent).toContain('32');
    expect(badge.textContent).toContain('✓');
  });

  it('shows a red exceeds badge when estimate.fits=false', async () => {
    mockInvoke.mockResolvedValueOnce({
      weightsGb: 9.0, kvCacheGb: 28.0, computeHeadroomGb: 0.5, totalGb: 37.5,
      hostRamGb: 32, fits: false, numCtx: 131072, kvDtypeBytes: 1,
    });
    renderPanel({ showNumCtx: true });
    await waitFor(() => {
      expect(screen.getByTestId(`${PREFIX}-estimate-exceeds`)).toBeTruthy();
    }, { timeout: 2000 });
    expect(screen.getByTestId(`${PREFIX}-estimate-exceeds`).textContent).toContain('32');
  });

  it('shows "estimate unavailable" on invocation error (non-fatal)', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('Ollama offline'));
    renderPanel({ showNumCtx: true });
    await waitFor(() => {
      expect(screen.getByTestId(`${PREFIX}-estimate-error`)).toBeTruthy();
    }, { timeout: 2000 });
    expect(screen.getByTestId(`${PREFIX}-estimate-error`).textContent).toContain('unavailable');
    // Panel is still usable — temperature + system prompt still render.
    expect(screen.getByTestId(`${PREFIX}-temperature`)).toBeTruthy();
    expect(screen.getByTestId(`${PREFIX}-system-prompt`)).toBeTruthy();
  });

  // -----------------------------------------------------------------------
  // Temperature round-trip
  // -----------------------------------------------------------------------

  it('temperature slider renders for both showNumCtx values', () => {
    renderPanel({ showNumCtx: false });
    expect(screen.getByTestId(`${PREFIX}-temperature`)).toBeTruthy();
    expect(screen.getByTestId(`${PREFIX}-temperature-value`)).toBeTruthy();
  });

  it('temperature round-trips: change updates the displayed value + onChange payload', () => {
    const { onChange } = renderPanel({ showNumCtx: true, values: baseValues({ temperature: 0.2 }) });
    fireEvent.change(screen.getByTestId(`${PREFIX}-temperature`), { target: { value: '0.75' } });
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ temperature: 0.75 }));
    // Controlled re-render reflects the new value.
    expect(screen.getByTestId(`${PREFIX}-temperature-value`).textContent).toBe('0.75');
  });

  // -----------------------------------------------------------------------
  // System prompt edit + Reset
  // -----------------------------------------------------------------------

  it('system prompt renders the default placeholder', () => {
    renderPanel({ showNumCtx: true, defaultSystemPromptPlaceholder: 'THE DEFAULT' });
    const ta = screen.getByTestId(`${PREFIX}-system-prompt`) as HTMLTextAreaElement;
    expect(ta.placeholder).toBe('THE DEFAULT');
  });

  it('system prompt edit propagates via onChange', () => {
    const { onChange } = renderPanel({ showNumCtx: true });
    fireEvent.change(screen.getByTestId(`${PREFIX}-system-prompt`), {
      target: { value: 'You are a helpful assistant.' },
    });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ systemPrompt: 'You are a helpful assistant.' }),
    );
  });

  it('Reset clears the system prompt to an empty string (→ backend default)', () => {
    const { onChange } = renderPanel({
      showNumCtx: true,
      values: baseValues({ systemPrompt: 'Custom prompt.' }),
    });
    // Precondition: the seeded value is shown.
    expect((screen.getByTestId(`${PREFIX}-system-prompt`) as HTMLTextAreaElement).value).toBe(
      'Custom prompt.',
    );
    fireEvent.click(screen.getByTestId(`${PREFIX}-reset-prompt`));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ systemPrompt: '' }));
    // Controlled re-render reflects the cleared value.
    expect((screen.getByTestId(`${PREFIX}-system-prompt`) as HTMLTextAreaElement).value).toBe('');
  });

  // -----------------------------------------------------------------------
  // num_ctx edit propagation
  // -----------------------------------------------------------------------

  it('num_ctx edit propagates the raw string via onChange', () => {
    const { onChange } = renderPanel({ showNumCtx: true });
    fireEvent.change(screen.getByTestId(`${PREFIX}-num-ctx`), { target: { value: '8192' } });
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ numCtxStr: '8192' }));
  });
});
