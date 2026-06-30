/**
 * ModelTilePicker tests — Task 8a (updated for Task 9 GetKeyCard integration).
 *
 * The component takes callbacks + state as PROPS (no hook calls, no Tauri
 * invoke), so tests pass vi.fn()s directly and never mock the invoke boundary.
 * `keyStatusByOrigin` is a prop — never a hook call.
 *
 * Coverage:
 *   (a) four tier headers render + a tile per preset;
 *   (b) the gemini tile carries a RECOMMENDED badge;
 *   (c) keyStatusByOrigin -> a per-tile "key saved" badge, NEVER a key value;
 *   (d) initialEndpoint=anthropic -> anthropic tile pre-selected (aria-checked);
 *       for tiles with a keyPageUrl, the GetKeyCard is shown instead of the
 *       plain summary with elmer-tile-model-input;
 *   (e) selecting the Other/custom tile renders the reused ModelForm;
 *   (f) the plain tile-summary (model input + Save) is shown for local tiles
 *       (localOllama has no keyPageUrl), where the pre-fill and onSave tests run.
 *
 * Task 9 note: cloud tiles (gemini, groq, openai, anthropic) now render GetKeyCard
 * because they have keyPageUrl. The elmer-tile-model-input and elmer-tile-save
 * data-testids are only present for tiles WITHOUT a keyPageUrl (localOllama).
 * Tests that assert on those elements have been updated to use localOllama.
 */

// Mock @tauri-apps/plugin-shell since GetKeyCard (imported by ModelTilePicker
// for cloud tiles) calls shellOpen and needs this intercepted in tests.
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(() => Promise.resolve()),
}));

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

  it('pre-selects the tile matching initialEndpoint (anthropic tile aria-checked=true)', () => {
    // Task 9: Anthropic has a keyPageUrl so it shows GetKeyCard, not the plain
    // summary with elmer-tile-model-input. This test verifies tile selection only;
    // see GetKeyCard.test.tsx for key-entry and Save behavior.
    render(
      <ModelTilePicker
        {...baseProps({
          initialEndpoint: 'https://api.anthropic.com/v1/chat/completions',
          initialModel: 'claude-sonnet-4-5', // operator's saved model
        })}
      />,
    );
    const anthropicTile = screen.getByTestId('elmer-tile-anthropic');
    expect(anthropicTile.getAttribute('aria-checked')).toBe('true');

    // The OpenAI tile is NOT selected.
    expect(screen.getByTestId('elmer-tile-openai').getAttribute('aria-checked')).toBe('false');

    // GetKeyCard is shown for Anthropic (has keyPageUrl) — not the plain summary.
    expect(screen.getByTestId('get-key-card')).toBeTruthy();
    expect(screen.queryByTestId('elmer-tile-model-input')).toBeNull();
  });

  it('renders the reused ModelForm when the Other/custom tile is selected', () => {
    render(<ModelTilePicker {...baseProps()} />);
    // Initially (openai selected) the ModelForm is not shown.
    expect(screen.queryByTestId('elmer-model-form')).toBeNull();

    fireEvent.click(screen.getByTestId('elmer-tile-custom'));
    expect(screen.getByTestId('elmer-model-form')).toBeTruthy();
  });

  it('switching to a cloud tile pre-fills the preset defaultModel and GetKeyCard Save carries it through onSave', async () => {
    // This is the non-vacuous integration test for the handleTileSelect →
    // nextModelForPreset → GetKeyCard → onSave seam.
    //
    // Start on OpenAI (gpt-4o-mini). Switch to Anthropic. nextModelForPreset sees
    // currentModel === outgoingDefault ('gpt-4o-mini') → returns 'claude-haiku-4-5'.
    // The picker sets model state to 'claude-haiku-4-5' and renders GetKeyCard with
    // agentModel='claude-haiku-4-5'. When the user saves a valid key, onSave must
    // receive agentEndpoint=Anthropic's endpoint AND agentModel='claude-haiku-4-5'.
    //
    // Fails if nextModelForPreset is broken, if agentModel is not passed to GetKeyCard,
    // or if GetKeyCard ignores it in the onSave call.
    const onSave = vi.fn(async () => {});
    render(
      <ModelTilePicker
        {...baseProps({
          onSave,
          initialEndpoint: 'https://api.openai.com/v1/chat/completions',
          initialModel: 'gpt-4o-mini',
        })}
      />,
    );

    // Select Anthropic tile → triggers handleTileSelect → nextModelForPreset → model='claude-haiku-4-5'
    fireEvent.click(screen.getByTestId('elmer-tile-anthropic'));

    // GetKeyCard must now be visible (Anthropic has a keyPageUrl).
    expect(screen.getByTestId('get-key-card')).toBeTruthy();

    // Type a valid key (≥20 chars, alphanumeric/hyphen/underscore).
    const keyInput = screen.getByTestId('get-key-input') as HTMLInputElement;
    fireEvent.change(keyInput, { target: { value: 'sk-ant-test-key-12345678' } });

    // Save must NOT be disabled (validation passes).
    const saveBtn = screen.getByTestId('get-key-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(false);

    // Click Save and await the async chain.
    fireEvent.click(saveBtn);
    await new Promise((r) => setTimeout(r, 0));

    // The critical assertion: model carried through is the Anthropic default, not the
    // prior OpenAI model. If nextModelForPreset or agentModel wiring is broken, this fails.
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        agentEndpoint: 'https://api.anthropic.com/v1/chat/completions',
        agentModel: 'claude-haiku-4-5',
        key: { action: 'set', value: 'sk-ant-test-key-12345678' },
      }),
    );
  });

  it('calls onSave with the working endpoint and model via the plain summary (localOllama — no keyPageUrl)', async () => {
    // Task 9: cloud tiles render GetKeyCard which has its own Save pathway tested in
    // GetKeyCard.test.tsx. The per-tile plain-summary Save (elmer-tile-save) is still
    // rendered for local tiles that have no keyPageUrl. localOllama is the test vehicle.
    const onSave = vi.fn(async () => {});
    render(
      <ModelTilePicker
        {...baseProps({
          onSave,
          initialEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
          initialModel: 'llama3',
        })}
      />,
    );
    fireEvent.click(screen.getByTestId('elmer-tile-save'));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
      }),
    );
  });
});
