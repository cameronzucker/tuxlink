/**
 * ModelTilePicker tests -- Task 8a (updated for Task 9 + Bug 1/2 fixes).
 *
 * The component takes callbacks + state as PROPS (no hook calls, no Tauri
 * invoke), so tests pass vi.fn()s directly and never mock the invoke boundary.
 * `keyStatusByOrigin` is a prop -- never a hook call.
 *
 * Coverage:
 *   (a) four tier headers render + a tile per preset;
 *   (b) the gemini tile does NOT carry a RECOMMENDED badge (removed per operator decision);
 *   (c) keyStatusByOrigin -> a per-tile "key saved" badge, NEVER a key value;
 *   (d) initialEndpoint=anthropic -> anthropic tile pre-selected (aria-checked);
 *       for tiles with a keyPageUrl, the GetKeyCard is shown;
 *   (e) selecting the Other/custom tile renders the reused ModelForm;
 *   (f) [Bug2 fix] localOllama tile renders ModelForm (not the removed bare summary)
 *       so Detect + model-select are available;
 *   (g) [Bug1 fix] local Ollama model does NOT survive a switch to a cloud tile
 *       (the fatal 404 regression guard);
 *   (h) GetKeyCard remounts on tile switch (T11 key-isolation fix).
 *
 * Task 9 note: cloud tiles (gemini, groq, openai, anthropic) render GetKeyCard
 * because they have keyPageUrl. Local (localOllama) + Other tier tiles render
 * ModelForm, which has detect + model-select + endpoint editing + loopback-keyless
 * handling. The bare-summary branch (elmer-tile-model-input / elmer-tile-save)
 * has been removed; elmer-save-btn (from ModelForm) is the Save affordance for
 * local tiles.
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

    // Tier headers (scoped to the <h3> role). Match with startsWith/includes
    // since some headers have a subtitle span appended (e.g. paygo gets
    // "· ~pennies/turn") and the textContent includes it.
    const headers = screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent ?? '');
    expect(headers.some((h) => h.startsWith('Free · no credit card'))).toBe(true);
    expect(headers.some((h) => h.startsWith('Pay-as-you-go · needs a card'))).toBe(true);
    expect(headers.some((h) => h.startsWith('Local · offline · no key'))).toBe(true);
    expect(headers.some((h) => h === 'Other')).toBe(true);

    // One radio tile per preset.
    const tiles = screen.getAllByRole('radio');
    expect(tiles.length).toBe(PRESETS.length);

    // Each preset's label appears.
    for (const p of PRESETS) {
      expect(screen.getAllByText(new RegExp(p.label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'i')).length).toBeGreaterThan(0);
    }
  });

  it('does NOT show a RECOMMENDED badge on the gemini tile (operator-removed endorsement)', () => {
    render(<ModelTilePicker {...baseProps()} />);
    const tile = screen.getByTestId('elmer-tile-gemini');
    // The RECOMMENDED / elmer-tile-badge--recommended label was removed per
    // Fix 3 (unapproved vendor endorsement). Assert it is absent.
    expect(within(tile).queryByText(/RECOMMENDED/i)).toBeNull();
    expect(tile.querySelector('.elmer-tile-badge--recommended')).toBeNull();
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
          initialEndpoint: 'https://api.anthropic.com/v1/messages',
          initialModel: 'claude-sonnet-4-5', // operator's saved model
        })}
      />,
    );
    const anthropicTile = screen.getByTestId('elmer-tile-anthropic');
    expect(anthropicTile.getAttribute('aria-checked')).toBe('true');

    // The OpenAI tile is NOT selected.
    expect(screen.getByTestId('elmer-tile-openai').getAttribute('aria-checked')).toBe('false');

    // GetKeyCard is shown for Anthropic (has keyPageUrl) -- not the plain summary.
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
    // This is the non-vacuous integration test for the handleTileSelect ->
    // nextModelForPreset -> GetKeyCard -> onSave seam.
    //
    // Start on OpenAI (gpt-4o-mini). Switch to Anthropic. nextModelForPreset sees
    // currentModel === outgoingDefault ('gpt-4o-mini') -> returns 'claude-haiku-4-5'.
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

    // Select Anthropic tile -> triggers handleTileSelect -> nextModelForPreset -> model='claude-haiku-4-5'
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
        agentEndpoint: 'https://api.anthropic.com/v1/messages',
        agentModel: 'claude-haiku-4-5',
        key: { action: 'set', value: 'sk-ant-test-key-12345678' },
      }),
    );
  });

  // BUG 2 TDD -- these tests FAIL before the fix (local tile renders bare summary,
  // not ModelForm), and PASS after (local tile uses ModelForm with Detect).

  it('[Bug2] selecting the localOllama tile renders ModelForm (elmer-model-form), not the bare summary', () => {
    // The bare summary (elmer-tile-model-input + elmer-tile-save) was a stripped-down
    // editor that dropped the Detect-models button. localOllama MUST render ModelForm
    // so detect + model-select are available for Ollama operators.
    render(
      <ModelTilePicker
        {...baseProps({
          initialEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
          initialModel: 'llama3',
        })}
      />,
    );
    expect(screen.getByTestId('elmer-model-form')).toBeTruthy();
    // The bare-summary-only input must NOT appear as a replacement for ModelForm.
    expect(screen.queryByTestId('elmer-tile-summary')).toBeNull();
  });

  it('[Bug2] localOllama tile ModelForm includes the Detect affordance (elmer-detect-btn)', () => {
    // Detect button is the key operator workflow for pulling local Ollama model lists.
    // Without ModelForm, it was absent -- operators had no way to pick detected models.
    render(
      <ModelTilePicker
        {...baseProps({
          initialEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
          initialModel: '',
        })}
      />,
    );
    expect(screen.getByTestId('elmer-detect-btn')).toBeTruthy();
  });

  it('[Bug2] calls onSave with working endpoint and model via ModelForm Save on localOllama tile', async () => {
    // Re-express the prior onSave test using ModelForm's Save button (elmer-save-btn)
    // instead of the removed bare-summary elmer-tile-save button.
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
    // ModelForm's Save button is elmer-save-btn (not the removed elmer-tile-save).
    fireEvent.click(screen.getByTestId('elmer-save-btn'));
    await new Promise((r) => setTimeout(r, 0));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
      }),
    );
  });

  // COMBINED FIX TEST -- guards the end-to-end fatal regression:
  // local Ollama model must not survive a switch from localOllama to Gemini tile.
  it('[Combined] local Ollama model does NOT carry over when operator switches to Gemini tile', () => {
    // Start on localOllama with a detected model (simulates post-detect state).
    // Switching to Gemini must set model to gemini-2.5-flash, not the local model.
    // This is the full component-level guard for the Bug 1 fatal 404 regression.
    const onSave = vi.fn(async () => {});
    render(
      <ModelTilePicker
        {...baseProps({
          onSave,
          initialEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
          initialModel: 'gpt-oss:20b',
        })}
      />,
    );

    // Verify localOllama tile is pre-selected.
    expect(screen.getByTestId('elmer-tile-localOllama').getAttribute('aria-checked')).toBe('true');

    // Switch to Gemini tile.
    fireEvent.click(screen.getByTestId('elmer-tile-gemini'));
    expect(screen.getByTestId('elmer-tile-gemini').getAttribute('aria-checked')).toBe('true');

    // GetKeyCard must render (Gemini has keyPageUrl), proving the Gemini branch activated.
    expect(screen.getByTestId('get-key-card')).toBeTruthy();

    // Type a valid key and save -- the model carried to onSave must be gemini-2.5-flash,
    // NOT the prior local model.
    const keyInput = screen.getByTestId('get-key-input') as HTMLInputElement;
    fireEvent.change(keyInput, { target: { value: 'AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ12' } });
    fireEvent.click(screen.getByTestId('get-key-save'));

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        agentEndpoint: 'https://generativelanguage.googleapis.com/v1beta/openai/chat/completions',
        agentModel: 'gemini-2.5-flash',
      }),
    );
  });

  it('switching cloud tiles remounts GetKeyCard so an unsaved key typed for the first tile does not carry over to the second tile (T11 fix)', () => {
    // Regression guard for the React component-reuse defect: without key={selectedPreset.id}
    // on GetKeyCard, React reconciles the same instance across tile switches, preserving
    // rawKey state from the previous tile. The operator could then save the stale key value
    // to the new provider's origin without noticing.
    //
    // With key={selectedPreset.id}, switching tiles forces a full remount of GetKeyCard,
    // resetting rawKey to '' and the reveal toggle + validation error to their initial states.
    //
    // This test must FAIL without the key prop and PASS with it.
    render(<ModelTilePicker {...baseProps()} />);

    // Select the Gemini tile -- it has a keyPageUrl so GetKeyCard renders.
    fireEvent.click(screen.getByTestId('elmer-tile-gemini'));
    expect(screen.getByTestId('get-key-card')).toBeTruthy();

    // Type a key into the Gemini GetKeyCard input -- do NOT save it.
    const geminiKeyInput = screen.getByTestId('get-key-input') as HTMLInputElement;
    fireEvent.change(geminiKeyInput, { target: { value: 'AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ12' } });
    expect(geminiKeyInput.value).toBe('AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ12');

    // Now switch to the Groq tile -- also a cloud tile with a keyPageUrl.
    fireEvent.click(screen.getByTestId('elmer-tile-groq'));
    expect(screen.getByTestId('get-key-card')).toBeTruthy();

    // The key input must be EMPTY -- the stale Gemini key must not carry over.
    const groqKeyInput = screen.getByTestId('get-key-input') as HTMLInputElement;
    expect(groqKeyInput.value).toBe('');
  });
});
