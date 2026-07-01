/**
 * GetKeyCard tests — Task 9: guided get-a-free-key flow (F7, F12).
 *
 * Coverage:
 *   (a) Open-key-page button calls mocked plugin-shell `open()` with EXACTLY
 *       the Gemini constant (https://aistudio.google.com/apikey), never a
 *       constructed or config-derived URL;
 *   (b) key field is type="password" with a reveal toggle that switches to type="text";
 *   (c) trim + sanity-validate: short or whitespace-containing paste shows error and blocks Save;
 *       valid paste enables Save; Gemini-style key with period is accepted (Fix 2 regression);
 *   (d) "stuck?" affordance renders an alternate-provider (Groq) suggestion.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, act } from '@testing-library/react';
import type { ComponentProps } from 'react';

// Mock @tauri-apps/plugin-shell BEFORE any component import so the module is
// intercepted from the first import (matches the pattern in AboutDialog.test.tsx,
// ArdopRadioPanel.test.tsx, AccountCreate.test.tsx).
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(() => Promise.resolve()),
}));

import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { GetKeyCard } from './GetKeyCard';
import { PRESETS } from './elmerModelConfig';

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const geminiPreset = PRESETS.find((p) => p.id === 'gemini')!;
const groqPreset = PRESETS.find((p) => p.id === 'groq')!;
const openaiPreset = PRESETS.find((p) => p.id === 'openai')!;

function baseProps(
  overrides: Partial<ComponentProps<typeof GetKeyCard>> = {},
): ComponentProps<typeof GetKeyCard> {
  return {
    preset: geminiPreset,
    onSave: vi.fn(async () => {}),
    agentModel: 'gemini-2.5-flash',
    agentTurnTimeoutSecs: 900,
    ...overrides,
  };
}

describe('GetKeyCard', () => {
  // -----------------------------------------------------------------------
  // (a) Open-key-page button — must call open() with the EXACT constant
  // -----------------------------------------------------------------------

  it('renders an "Open key page" button', () => {
    render(<GetKeyCard {...baseProps()} />);
    // The button must exist for Gemini.
    const btn = screen.getByTestId('get-key-open-page');
    expect(btn).toBeTruthy();
  });

  it('clicking "Open key page" calls shellOpen with EXACTLY the Gemini keyPageUrl constant', async () => {
    render(<GetKeyCard {...baseProps()} />);
    await act(async () => {
      fireEvent.click(screen.getByTestId('get-key-open-page'));
    });
    expect(shellOpen).toHaveBeenCalledTimes(1);
    // Must be the HARDCODED constant, not a config-derived or constructed URL.
    expect(shellOpen).toHaveBeenCalledWith('https://aistudio.google.com/apikey');
  });

  it('clicking "Open key page" for Groq calls shellOpen with the Groq constant', async () => {
    render(<GetKeyCard {...baseProps({ preset: groqPreset, agentModel: 'llama-3.3-70b-versatile' })} />);
    await act(async () => {
      fireEvent.click(screen.getByTestId('get-key-open-page'));
    });
    expect(shellOpen).toHaveBeenCalledWith('https://console.groq.com/keys');
  });

  it('clicking "Open key page" for OpenAI (paygo) calls shellOpen with the OpenAI constant', async () => {
    render(<GetKeyCard {...baseProps({ preset: openaiPreset, agentModel: 'gpt-4o-mini' })} />);
    await act(async () => {
      fireEvent.click(screen.getByTestId('get-key-open-page'));
    });
    expect(shellOpen).toHaveBeenCalledWith('https://platform.openai.com/api-keys');
  });

  // -----------------------------------------------------------------------
  // (b) Password field + reveal toggle
  // -----------------------------------------------------------------------

  it('renders the key input as type="password" initially', () => {
    render(<GetKeyCard {...baseProps()} />);
    const input = screen.getByTestId('get-key-input') as HTMLInputElement;
    expect(input.type).toBe('password');
  });

  it('reveal toggle switches the input from type="password" to type="text"', () => {
    render(<GetKeyCard {...baseProps()} />);
    const input = screen.getByTestId('get-key-input') as HTMLInputElement;
    expect(input.type).toBe('password');
    fireEvent.click(screen.getByTestId('get-key-reveal-toggle'));
    expect(input.type).toBe('text');
  });

  it('reveal toggle switches back to type="password" on second click', () => {
    render(<GetKeyCard {...baseProps()} />);
    const input = screen.getByTestId('get-key-input') as HTMLInputElement;
    fireEvent.click(screen.getByTestId('get-key-reveal-toggle'));
    expect(input.type).toBe('text');
    fireEvent.click(screen.getByTestId('get-key-reveal-toggle'));
    expect(input.type).toBe('password');
  });

  // -----------------------------------------------------------------------
  // (c) Trim + sanity validation
  // -----------------------------------------------------------------------

  it('Save button is disabled when the key field is empty', () => {
    render(<GetKeyCard {...baseProps()} />);
    const saveBtn = screen.getByTestId('get-key-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(true);
  });

  it('a short paste enables Save — no client-side length/format gate (the provider validates)', () => {
    render(<GetKeyCard {...baseProps()} />);
    fireEvent.change(screen.getByTestId('get-key-input'), { target: { value: 'shortkey' } });
    // No length gate: the front end never blocks a paste on a guessed format;
    // the provider accepts/rejects at Test/Save. And no client-side error element.
    expect((screen.getByTestId('get-key-save') as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByTestId('get-key-error')).toBeNull();
  });

  it('whitespace-only input leaves Save disabled (trimmed empty)', () => {
    render(<GetKeyCard {...baseProps()} />);
    fireEvent.change(screen.getByTestId('get-key-input'), { target: { value: '   ' } });
    expect((screen.getByTestId('get-key-save') as HTMLButtonElement).disabled).toBe(true);
  });

  it('surrounding whitespace is trimmed — a non-empty trimmed value enables Save', () => {
    render(<GetKeyCard {...baseProps()} />);
    fireEvent.change(screen.getByTestId('get-key-input'), { target: { value: '  AQ.Ab8key  ' } });
    expect((screen.getByTestId('get-key-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('a paste with internal spaces still enables Save — we do not format-gate (provider rejects it)', () => {
    render(<GetKeyCard {...baseProps()} />);
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: 'AIza has spaces key1234' },
    });
    expect((screen.getByTestId('get-key-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('REGRESSION: a Gemini-style key containing a period enables Save (no charset gate, Fix 2)', () => {
    // Real Gemini keys have the shape AQ.Ab8... — the old /^[A-Za-z0-9_-]+$/ regex
    // hard-blocked them on the period. We now do NO client-side format validation,
    // so it saves. Synthetic key (not a real credential).
    render(<GetKeyCard {...baseProps()} />);
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: 'AQ.Ab8SYNTHETICplaceholderKey-1234567890-abcdef' },
    });
    expect((screen.getByTestId('get-key-save') as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByTestId('get-key-error')).toBeNull();
  });

  it('enables Save for a short-ish key with hyphens and underscores (no charset gate)', () => {
    render(<GetKeyCard {...baseProps()} />);
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: 'gsk_ABC-123' },
    });
    expect((screen.getByTestId('get-key-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('trims whitespace before passing to onSave when the trimmed key is valid', async () => {
    const onSave = vi.fn(async () => {});
    render(<GetKeyCard {...baseProps({ onSave })} />);
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: '  AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ12  ' },
    });
    const saveBtn = screen.getByTestId('get-key-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(false);
    await act(async () => {
      fireEvent.click(saveBtn);
    });
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        key: { action: 'set', value: 'AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ12' },
      }),
    );
  });

  // -----------------------------------------------------------------------
  // (d) "stuck?" affordance
  // -----------------------------------------------------------------------

  it('renders a "stuck?" affordance for Gemini that mentions Groq as an alternate free provider', () => {
    render(<GetKeyCard {...baseProps()} />);
    const affordance = screen.getByTestId('get-key-stuck');
    expect(affordance).toBeTruthy();
    // Must mention Groq as an alternate free provider.
    expect(affordance.textContent?.toLowerCase()).toMatch(/groq/i);
  });

  it('renders a "stuck?" affordance for Groq that mentions Gemini as an alternate free provider', () => {
    render(<GetKeyCard {...baseProps({ preset: groqPreset, agentModel: 'llama-3.3-70b-versatile' })} />);
    const affordance = screen.getByTestId('get-key-stuck');
    expect(affordance.textContent?.toLowerCase()).toMatch(/gemini/i);
  });

  it('renders a "stuck?" affordance for paygo providers (OpenAI) mentioning a free option', () => {
    render(<GetKeyCard {...baseProps({ preset: openaiPreset, agentModel: 'gpt-4o-mini' })} />);
    const affordance = screen.getByTestId('get-key-stuck');
    expect(affordance).toBeTruthy();
    // paygo stuck? hint mentions a free alternative
    expect(affordance.textContent?.toLowerCase()).toMatch(/gemini|groq/i);
  });

  // -----------------------------------------------------------------------
  // Step copy is outcome-oriented
  // -----------------------------------------------------------------------

  it('renders outcome-oriented step copy for Gemini (mentions sign-in and create API key)', () => {
    render(<GetKeyCard {...baseProps()} />);
    const card = screen.getByTestId('get-key-card');
    // Sign-in prerequisite must be mentioned.
    expect(card.textContent?.toLowerCase()).toMatch(/sign in|google account/i);
    // Key creation outcome, not generic button label.
    expect(card.textContent?.toLowerCase()).toMatch(/api key/i);
  });

  // -----------------------------------------------------------------------
  // (e) keyStatus='present' — settings-path key-saved affordance
  //
  // When reopened in the settings-surface path and the key is already stored
  // for this origin, GetKeyCard must:
  //   - Show a "Key saved" badge (get-key-saved-badge) instead of the input.
  //   - Enable Save WITHOUT requiring a new key (sends {action:'keep'}).
  //   - Offer a "Replace key" button that switches to input mode.
  //   - After typing a valid new key and clicking Save, send {action:'set', value}.
  // -----------------------------------------------------------------------

  it('keyStatus=present renders the key-saved badge and hides the key input', () => {
    const onSave = vi.fn(async () => {});
    render(<GetKeyCard {...baseProps({ onSave, keyStatus: 'present' })} />);
    // Badge must be visible.
    expect(screen.getByTestId('get-key-saved-badge')).toBeTruthy();
    // Key input must NOT be in the DOM (no forced re-entry in settings path).
    expect(screen.queryByTestId('get-key-input')).toBeNull();
  });

  it('keyStatus=present: Save is enabled without typing a key (keep path)', () => {
    const onSave = vi.fn(async () => {});
    render(<GetKeyCard {...baseProps({ onSave, keyStatus: 'present' })} />);
    const saveBtn = screen.getByTestId('get-key-save') as HTMLButtonElement;
    // Must be enabled — no new key required when one is already stored.
    expect(saveBtn.disabled).toBe(false);
  });

  it('keyStatus=present: Save sends {action:"keep"} without typing a new key', async () => {
    const onSave = vi.fn(async () => {});
    render(<GetKeyCard {...baseProps({ onSave, keyStatus: 'present' })} />);
    await act(async () => {
      fireEvent.click(screen.getByTestId('get-key-save'));
    });
    expect(onSave).toHaveBeenCalledTimes(1);
    // Keep path: must NOT clear or overwrite the stored key.
    // Endpoint is always the preset constant, never a config-derived value.
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        key: { action: 'keep' },
        agentEndpoint: geminiPreset.endpoint,
      }),
    );
  });

  it('keyStatus=present: "Replace key" button switches to input mode', () => {
    render(<GetKeyCard {...baseProps({ keyStatus: 'present' })} />);
    // Replace button must be present in the saved-key state.
    const replaceBtn = screen.getByTestId('get-key-replace-btn');
    expect(replaceBtn).toBeTruthy();
    fireEvent.click(replaceBtn);
    // After clicking Replace, the key input appears and the badge disappears.
    expect(screen.getByTestId('get-key-input')).toBeTruthy();
    expect(screen.queryByTestId('get-key-saved-badge')).toBeNull();
  });

  it('keyStatus=present + Replace key + type new key: Save sends {action:"set", value}', async () => {
    const onSave = vi.fn(async () => {});
    render(<GetKeyCard {...baseProps({ onSave, keyStatus: 'present' })} />);
    // Enter replace mode.
    fireEvent.click(screen.getByTestId('get-key-replace-btn'));
    // Type a valid new key.
    const VALID_REPLACEMENT = 'AIzaSy-replacement-key-9876543210';
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: VALID_REPLACEMENT },
    });
    await act(async () => {
      fireEvent.click(screen.getByTestId('get-key-save'));
    });
    expect(onSave).toHaveBeenCalledTimes(1);
    // Replace path: must send action:'set' with the new key, NOT 'keep'.
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        key: { action: 'set', value: VALID_REPLACEMENT },
        agentEndpoint: geminiPreset.endpoint,
      }),
    );
  });
});
