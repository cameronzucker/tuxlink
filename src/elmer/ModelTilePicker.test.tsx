/**
 * ModelTilePicker tests — Task 8a.
 *
 * The component takes callbacks + state as PROPS (no hook calls, no Tauri
 * invoke), so tests pass vi.fn()s directly and never mock the invoke boundary.
 * `keyStatusByOrigin` is a prop — never a hook call.
 *
 * Coverage:
 *   (a) four tier headers render + a tile per preset;
 *   (b) the gemini tile carries a RECOMMENDED badge;
 *   (c) keyStatusByOrigin -> a per-tile "key saved" badge, NEVER a key value;
 *   (d) initialEndpoint=anthropic -> anthropic tile pre-selected (aria-checked)
 *       and the model field shows the SAVED model, not the tile default;
 *   (e) selecting the Other/custom tile renders the reused ModelForm.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, within } from '@testing-library/react';
import { ModelTilePicker } from './ModelTilePicker';
import { PRESETS } from './elmerModelConfig';
import type { DetectState } from './useElmer';

afterEach(cleanup);

const idleDetect: DetectState = { status: 'idle' };

function baseProps(overrides: Partial<React.ComponentProps<typeof ModelTilePicker>> = {}) {
  return {
    onSave: vi.fn(async () => {}),
    onDetect: vi.fn(async () => {}),
    detectState: idleDetect,
    keyStatusByOrigin: {},
    initialEndpoint: 'https://api.openai.com/v1/chat/completions',
    initialModel: 'gpt-4o-mini',
    initialKeyStatus: 'absent' as const,
    initialTurnTimeoutSecs: 900,
    ...overrides,
  };
}

describe('ModelTilePicker', () => {
  it('renders all four tier headers and one tile per preset', () => {
    render(<ModelTilePicker {...baseProps()} />);

    // Tier headers (scoped to the <h3> role so the localOllama tile label
    // "On this computer (Ollama)" doesn't collide with the "On this computer"
    // tier header).
    const headers = screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent);
    expect(headers).toContain('Free · no credit card');
    expect(headers).toContain('Pay-as-you-go · needs a card');
    expect(headers).toContain('On this computer');
    expect(headers).toContain('Other');

    // One radio tile per preset.
    const tiles = screen.getAllByRole('radio');
    expect(tiles.length).toBe(PRESETS.length);

    // Each preset's label appears.
    for (const p of PRESETS) {
      expect(screen.getAllByText(new RegExp(p.label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'i')).length).toBeGreaterThan(0);
    }
  });

  it('shows a RECOMMENDED badge on the gemini tile', () => {
    render(<ModelTilePicker {...baseProps()} />);
    const tile = screen.getByTestId('elmer-tile-gemini');
    expect(within(tile).getByText(/RECOMMENDED/i)).toBeTruthy();
  });

  it('shows a key-saved badge on a tile whose origin has a present key, never a key value', () => {
    render(
      <ModelTilePicker
        {...baseProps({ keyStatusByOrigin: { 'https://api.openai.com': 'present' } })}
      />,
    );
    const openaiTile = screen.getByTestId('elmer-tile-openai');
    expect(within(openaiTile).getByText(/key saved/i)).toBeTruthy();

    // A tile without a present key shows no badge.
    const geminiTile = screen.getByTestId('elmer-tile-gemini');
    expect(within(geminiTile).queryByText(/key saved/i)).toBeNull();

    // Never render any key value text anywhere in the picker.
    expect(document.body.textContent).not.toMatch(/sk-/);
  });

  it('pre-selects the tile matching initialEndpoint and shows the SAVED model not the tile default', () => {
    render(
      <ModelTilePicker
        {...baseProps({
          initialEndpoint: 'https://api.anthropic.com/v1/chat/completions',
          initialModel: 'claude-sonnet-4-5', // operator's saved model, NOT the default claude-haiku-4-5
        })}
      />,
    );
    const anthropicTile = screen.getByTestId('elmer-tile-anthropic');
    expect(anthropicTile.getAttribute('aria-checked')).toBe('true');

    // The OpenAI tile is NOT selected.
    expect(screen.getByTestId('elmer-tile-openai').getAttribute('aria-checked')).toBe('false');

    // The model field shows the saved model, not the tile default.
    const modelInput = screen.getByTestId('elmer-tile-model-input') as HTMLInputElement;
    expect(modelInput.value).toBe('claude-sonnet-4-5');
  });

  it('renders the reused ModelForm when the Other/custom tile is selected', () => {
    render(<ModelTilePicker {...baseProps()} />);
    // Initially (openai selected) the ModelForm is not shown.
    expect(screen.queryByTestId('elmer-model-form')).toBeNull();

    fireEvent.click(screen.getByTestId('elmer-tile-custom'));
    expect(screen.getByTestId('elmer-model-form')).toBeTruthy();
  });

  it('pre-fills the target provider default model when switching from an untouched tile', () => {
    render(<ModelTilePicker {...baseProps()} />); // openai selected, model = its default gpt-4o-mini
    fireEvent.click(screen.getByTestId('elmer-tile-anthropic'));
    const modelInput = screen.getByTestId('elmer-tile-model-input') as HTMLInputElement;
    expect(modelInput.value).toBe('claude-haiku-4-5');
  });

  it('calls onSave with the working endpoint and model from the per-tile summary', async () => {
    const onSave = vi.fn(async () => {});
    render(<ModelTilePicker {...baseProps({ onSave })} />);
    fireEvent.click(screen.getByTestId('elmer-tile-save'));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o-mini',
      }),
    );
  });
});
