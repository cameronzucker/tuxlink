/**
 * GetKeyCard — guided "get a free key" flow for cloud provider tiles (Task 9, F7/F12).
 *
 * Renders per-provider step copy (written as OUTCOMES, not button labels), an
 * "Open key page" button that opens the hardcoded `preset.keyPageUrl` in the
 * system browser via `@tauri-apps/plugin-shell::open`, a masked paste field
 * with a reveal toggle, client-side sanity validation (trim → len>=20 &&
 * /^\S+$/ — no whitespace, any non-space chars including periods for Gemini keys),
 * and a "stuck?" affordance offering an alternate free provider or pay-as-you-go path.
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
import type { ProviderPreset, SetKey, KeyStatus } from './elmerModelConfig';

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
  /**
   * Key status for this tile's origin. When 'present', GetKeyCard shows a
   * "Key saved" state with a Replace key affordance and enables Save WITHOUT
   * requiring a new key (sends {action:'keep'}). When absent or omitted,
   * the original behavior is preserved (must type a valid key to enable Save).
   *
   * The 'keep' action means: leave whatever is stored for THIS origin unchanged.
   * The endpoint stays the hardcoded preset.endpoint, so 'keep' cannot leak
   * to a different origin.
   */
  keyStatus?: KeyStatus;
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
// No client-side key format validation (operator smoke-walk decision, 2026-06-30)
// ---------------------------------------------------------------------------
//
// We deliberately do NOT validate a key's FORMAT on the front end. The provider
// is the sole authority on validity and accepts/rejects the key at Test/Save
// time. A front-end format check can only GUESS the shape, and when a provider's
// format varies (e.g. Google's AIza... -> AQ.Ab8... with a period) the guess
// produces a false negative that hard-blocks a VALID key — a dead-end the user
// cannot work around. The only client-side condition is non-empty (to enable the
// Save button); real validation is the Test/Save round-trip. No network calls
// here (SSRF-1).

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function GetKeyCard({
  preset,
  onSave,
  agentModel,
  agentTurnTimeoutSecs,
  keyStatus,
}: GetKeyCardProps) {
  const [rawKey, setRawKey] = useState('');
  const [revealed, setRevealed] = useState(false);
  const [saving, setSaving] = useState(false);
  // When keyStatus='present': track whether the operator has chosen to replace
  // the stored key. Starts false (showing the "Key saved" affordance). Clicking
  // "Replace key" sets this true, revealing the key input.
  const [replaceKeyMode, setReplaceKeyMode] = useState(false);
  // Editable model — seeded from the incoming agentModel (the preset default or
  // the operator's saved model). Previously the cloud tile had no model field, so
  // users were locked to DEFAULT_MODEL_BY_PRESET (e.g. gpt-4o-mini). GetKeyCard is
  // keyed by preset id in the picker, so switching cloud tiles remounts this with
  // the new preset's default (tuxlink-p46qz).
  const [model, setModel] = useState(agentModel);

  const copy = getProviderCopy(preset.id);
  const trimmed = rawKey.trim();

  // Whether we're in the "key already saved" state.
  const keySaved = keyStatus === 'present' && !replaceKeyMode;

  // Save is enabled when:
  //   - key already saved (keep path): always enabled, sends {action:'keep'}
  //   - replace mode or absent: enabled once the field is non-empty. No format
  //     check — the provider validates at Test/Save; we never block a paste.
  // Also require a non-empty model so a cleared model field cannot save an empty
  // model (which would 404 at send time) — tuxlink-p46qz.
  const canSave = (keySaved || trimmed.length > 0) && model.trim().length > 0;

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
      // Keep path: key already saved and operator didn't type a new one.
      // CRITICAL: 'keep' means "keep whatever is stored for THIS origin" — the
      // endpoint is always preset.endpoint (a compile-time constant), so 'keep'
      // can never leak to a different origin.
      const key: SetKey = keySaved ? { action: 'keep' } : { action: 'set', value: trimmed };
      await onSave({
        agentEndpoint: preset.endpoint,
        agentModel: model.trim(),
        key,
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

      {/* API key section */}
      {keySaved ? (
        /* Key-saved state: show badge + Replace affordance; no input required. */
        <div className="elmer-form-row" data-testid="get-key-saved-row">
          <span className="elmer-form-label">API key</span>
          <div className="elmer-key-stored">
            <span
              className="elmer-key-stored-label"
              data-testid="get-key-saved-badge"
            >
              ✓ Key saved
            </span>
            <button
              type="button"
              className="elmer-key-action-btn"
              data-testid="get-key-replace-btn"
              onClick={() => setReplaceKeyMode(true)}
            >
              Replace key
            </button>
          </div>
        </div>
      ) : (
        /* Normal key-entry state: masked paste field + reveal toggle. */
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
              placeholder={replaceKeyMode ? 'Paste new key…' : 'Paste your key here'}
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
      )}

      {/* No client-side format error: the provider validates at Test/Save. */}

      {/* Model — editable so cloud users aren't locked to the preset default
          (tuxlink-p46qz). No format check; the provider validates at Save/send. */}
      <div className="elmer-form-row">
        <label className="elmer-form-label" htmlFor="get-key-model-field">
          Model
        </label>
        <input
          id="get-key-model-field"
          type="text"
          className="elmer-form-input elmer-form-input--mono"
          data-testid="get-key-model-input"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder={agentModel || 'model id (e.g. gpt-4o)'}
          spellCheck={false}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
        />
      </div>

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
