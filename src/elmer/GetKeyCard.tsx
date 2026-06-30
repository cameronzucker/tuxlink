/**
 * GetKeyCard — guided "get a free key" flow for cloud provider tiles (Task 9, F7/F12).
 *
 * Renders per-provider step copy (written as OUTCOMES, not button labels), an
 * "Open key page" button that opens the hardcoded `preset.keyPageUrl` in the
 * system browser via `@tauri-apps/plugin-shell::open`, a masked paste field
 * with a reveal toggle, client-side sanity validation (trim → len>=20 &&
 * /^[A-Za-z0-9_\-]+$/), and a "stuck?" affordance offering an alternate
 * free provider or pay-as-you-go path.
 *
 * Security:
 *   - `open()` is called with `preset.keyPageUrl` ONLY — a compile-time
 *     constant, never a config/endpoint-derived or user-supplied string
 *     (prevents SSRF / allowlist-bypass).
 *   - No renderer-side fetch/XHR to any provider endpoint (SSRF-1). All
 *     validation is client-side string sanity only.
 *   - Key value is never stored here; Save delegates to the caller's `onSave`
 *     which routes through the existing keyring path on the Rust side.
 */

import { useState } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import type { ProviderPreset, SetKey } from './elmerModelConfig';

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface GetKeyCardProps {
  preset: ProviderPreset;
  onSave: (args: {
    agentEndpoint: string;
    agentModel: string;
    key: SetKey;
    agentTurnTimeoutSecs: number;
  }) => Promise<void>;
  agentModel: string;
  agentTurnTimeoutSecs: number;
}

// ---------------------------------------------------------------------------
// Per-provider step copy — OUTCOMES not button labels.
// ---------------------------------------------------------------------------

interface ProviderStepCopy {
  steps: string[];
  stuckHint: string;
}

function getProviderCopy(presetId: string): ProviderStepCopy {
  switch (presetId) {
    case 'gemini':
      return {
        steps: [
          'Sign in with any Google account — Google AI Studio is free and does not require a billing card.',
          'Create an API key — usually labeled "Create API key" on the dashboard.',
          'Copy the key and paste it below.',
        ],
        stuckHint:
          'Alternatively, try Groq — also free, no credit card required. Or use a pay-as-you-go provider (OpenAI or Anthropic) if you already have a billing account.',
      };
    case 'groq':
      return {
        steps: [
          'Create a free Groq account — no billing card required.',
          'Generate an API key from the console keys page.',
          'Copy the key and paste it below.',
        ],
        stuckHint:
          'Alternatively, try Google Gemini — also free, no credit card required. Or use a pay-as-you-go provider (OpenAI or Anthropic) if you already have a billing account.',
      };
    case 'openai':
      return {
        steps: [
          'Sign in to your OpenAI account (billing card required for API access).',
          'Generate an API key from the API keys page.',
          'Copy the key and paste it below.',
        ],
        stuckHint:
          'Want to try before adding a card? Google Gemini and Groq are both free — no billing information required.',
      };
    case 'anthropic':
      return {
        steps: [
          'Sign in to your Anthropic Console account (billing card required for API access).',
          'Generate an API key from the API keys settings page.',
          'Copy the key and paste it below.',
        ],
        stuckHint:
          'Want to try before adding a card? Google Gemini and Groq are both free — no billing information required.',
      };
    default:
      return {
        steps: [
          'Obtain an API key from this provider.',
          'Copy the key and paste it below.',
        ],
        stuckHint:
          'If you are looking for a free option, try Google Gemini or Groq — both offer free API keys with no credit card required.',
      };
  }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/** Validation regex: alphanumeric, hyphen, underscore only. */
const KEY_CHARSET_RE = /^[A-Za-z0-9_-]+$/;

/**
 * Client-side key sanity check.
 *
 * Trims the raw input first, then requires:
 *   - length >= 20
 *   - charset: /^[A-Za-z0-9_\-]+$/
 *
 * Returns null on pass, or an error message string on failure.
 * No network calls — SSRF-1 prevention.
 */
function validateKey(raw: string): { trimmed: string; error: string | null } {
  const trimmed = raw.trim();
  if (trimmed.length < 20 || !KEY_CHARSET_RE.test(trimmed)) {
    return { trimmed, error: "That doesn't look like a complete key." };
  }
  return { trimmed, error: null };
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function GetKeyCard({
  preset,
  onSave,
  agentModel,
  agentTurnTimeoutSecs,
}: GetKeyCardProps) {
  const [rawKey, setRawKey] = useState('');
  const [revealed, setRevealed] = useState(false);
  const [saving, setSaving] = useState(false);

  const copy = getProviderCopy(preset.id);
  const { trimmed, error } = validateKey(rawKey);
  const canSave = error === null && trimmed.length > 0;

  function handleOpenPage() {
    // MUST use the hardcoded constant on the preset — never a constructed or
    // config-derived URL (SSRF / allowlist-bypass guard).
    if (preset.keyPageUrl) {
      void shellOpen(preset.keyPageUrl);
    }
  }

  async function handleSave() {
    if (!canSave) return;
    setSaving(true);
    try {
      await onSave({
        agentEndpoint: preset.endpoint,
        agentModel,
        key: { action: 'set', value: trimmed },
        agentTurnTimeoutSecs,
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="get-key-card elmer-tile-summary" data-testid="get-key-card">
      {/* Step-by-step outcome copy */}
      <ol className="get-key-steps">
        {copy.steps.map((step, i) => (
          <li key={i} className="get-key-step">
            {step}
          </li>
        ))}
      </ol>

      {/* Open key page in system browser */}
      {preset.keyPageUrl && (
        <div className="elmer-form-save-row">
          <button
            type="button"
            className="elmer-save-btn get-key-open-btn"
            data-testid="get-key-open-page"
            onClick={handleOpenPage}
          >
            Open key page
          </button>
        </div>
      )}

      {/* Masked paste field + reveal toggle */}
      <div className="elmer-form-row">
        <label className="elmer-form-label" htmlFor="get-key-input-field">
          API key
        </label>
        <div className="get-key-input-row">
          <input
            id="get-key-input-field"
            type={revealed ? 'text' : 'password'}
            className="elmer-form-input elmer-form-input--mono get-key-field"
            data-testid="get-key-input"
            value={rawKey}
            onChange={(e) => setRawKey(e.target.value)}
            placeholder="Paste your key here"
            spellCheck={false}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
          />
          <button
            type="button"
            className="elmer-key-action-btn get-key-reveal-btn"
            data-testid="get-key-reveal-toggle"
            aria-label={revealed ? 'Hide key' : 'Show key'}
            onClick={() => setRevealed((r) => !r)}
          >
            {revealed ? 'Hide' : 'Show'}
          </button>
        </div>
      </div>

      {/* Validation error */}
      {rawKey.length > 0 && error !== null && (
        <p className="get-key-error elmer-save-error" data-testid="get-key-error" role="alert">
          {error}
        </p>
      )}

      {/* Save button */}
      <div className="elmer-form-save-row">
        <button
          type="button"
          className="elmer-save-btn"
          data-testid="get-key-save"
          disabled={!canSave || saving}
          onClick={() => void handleSave()}
        >
          {saving ? 'Saving…' : 'Save & use'}
        </button>
      </div>

      {/* "stuck?" affordance */}
      <p className="get-key-stuck" data-testid="get-key-stuck">
        <strong>Stuck?</strong> {copy.stuckHint}
      </p>
    </div>
  );
}
