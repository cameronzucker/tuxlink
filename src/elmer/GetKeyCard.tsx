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
 * T8 (tuxlink-65qhn): Advanced disclosure section below the model field (collapsed
 * by default). Contains:
 *   - num_ctx input (local/native tiles only; hidden for cloud tiles).
 *   - Live memory estimate line (debounced call to elmer_estimate_memory).
 *   - CPU-prefill speed cue.
 *   - Temperature slider (0–1, all tiles).
 *   - System prompt textarea with Reset-to-default button (all tiles).
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

import { useState, useCallback } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import type { ProviderPreset, SetKey, KeyStatus, KeySource, ConfigReadDto } from './elmerModelConfig';
import { isLoopback } from './elmerModelConfig';
import type { DetectState } from './useElmer';
import {
  AdvancedModelSettings,
  type AdvancedModelValues,
  DEFAULT_NUM_CTX,
  DEFAULT_SYSTEM_PROMPT_PLACEHOLDER,
} from './AdvancedModelSettings';

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
    numCtx?: number | null;
    temperature?: number | null;
    systemPromptOverride?: string | null;
  }) => Promise<void>;
  /**
   * Detect callback (mirrors the one on ModelForm). Called with the preset's
   * hardcoded endpoint and a KeySource derived from the current key state.
   * This is SSRF-safe: the endpoint is always `preset.endpoint`, a compile-time
   * constant — never a user-supplied or config-derived URL.
   */
  onDetect: (args: { agentEndpoint: string; keySource: KeySource }) => Promise<void>;
  /** Detect result state (mirrors ModelForm's prop). */
  detectState: DetectState;
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
  /**
   * T8: Initial advanced config values loaded from elmer_config_read. When
   * provided, the Advanced disclosure is pre-filled with the saved values.
   * All fields optional — omitting them leaves defaults in place.
   */
  initialConfig?: Pick<ConfigReadDto, 'numCtx' | 'temperature' | 'systemPromptOverride'>;
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
  onDetect,
  detectState,
  agentModel,
  agentTurnTimeoutSecs,
  keyStatus,
  initialConfig,
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

  // ---------------------------------------------------------------------------
  // T8: Advanced disclosure state
  // ---------------------------------------------------------------------------

  // Collapsed by default per spec.
  const [advancedOpen, setAdvancedOpen] = useState(false);

  // Whether this tile's endpoint is local/native (determines num_ctx visibility).
  // Cloud tiles show the disclosure minus num_ctx (per mock annotation).
  const isLocalTile = isLoopback(preset.endpoint);

  // T8: Advanced-panel values, seeded from initialConfig (saved values) or
  // sensible defaults. num_ctx is a string so the number input stays freely
  // editable; temperature defaults to 0.2; system prompt empty = no override.
  const [advanced, setAdvanced] = useState<AdvancedModelValues>(() => ({
    numCtxStr:
      initialConfig?.numCtx != null ? String(initialConfig.numCtx) : String(DEFAULT_NUM_CTX),
    temperature: initialConfig?.temperature != null ? initialConfig.temperature : 0.2,
    systemPrompt:
      initialConfig?.systemPromptOverride != null ? initialConfig.systemPromptOverride : '',
  }));

  // Parsed numCtx for the Save payload (falls back to the default if cleared).
  const numCtxParsed = (() => {
    const n = parseInt(advanced.numCtxStr, 10);
    return Number.isFinite(n) && n > 0 ? n : DEFAULT_NUM_CTX;
  })();

  const copy = getProviderCopy(preset.id);
  const trimmed = rawKey.trim();

  // Whether we're in the "key already saved" state.
  const keySaved = keyStatus === 'present' && !replaceKeyMode;

  // Build a KeySource for the Detect call — mirrors the logic in ModelForm's
  // buildKeySource / buildSetKey, adapted to GetKeyCard's simpler key state:
  //   - keySaved (stored key, operator hasn't entered a replacement): useStored.
  //   - operator typed a new key (trimmed non-empty in replace or absent mode): inline.
  //   - otherwise: none.
  // Endpoint is always preset.endpoint (SSRF-safe constant — matches handleSave).
  const buildKeySource = useCallback((): KeySource => {
    if (keySaved) {
      return { source: 'useStored' };
    }
    if (trimmed) {
      return { source: 'inline', value: trimmed };
    }
    return { source: 'none' };
  }, [keySaved, trimmed]);

  const handleDetect = useCallback(async () => {
    await onDetect({
      agentEndpoint: preset.endpoint,
      keySource: buildKeySource(),
    });
  }, [onDetect, preset.endpoint, buildKeySource]);

  const handleDetectedModelSelect = useCallback((selectedModel: string) => {
    setModel(selectedModel);
  }, []);

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

      // T8: Advanced fields — only include numCtx for local tiles (native Ollama
      // path). Cloud tiles omit numCtx (undefined → Tauri maps to None on backend).
      // An empty/unset system prompt sends null so the backend uses its default.
      const numCtxArg: number | null = isLocalTile ? numCtxParsed : null;
      const systemPromptArg: string | null = advanced.systemPrompt.trim()
        ? advanced.systemPrompt.trim()
        : null;

      await onSave({
        agentEndpoint: preset.endpoint,
        agentModel: model.trim(),
        key,
        agentTurnTimeoutSecs,
        numCtx: numCtxArg,
        temperature: advanced.temperature,
        systemPromptOverride: systemPromptArg,
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
          (tuxlink-p46qz). No format check; the provider validates at Save/send.
          Detect button fetches /v1/models from the provider and populates a
          select so the operator can pick from the actual available list. */}
      <div className="elmer-form-row">
        <label className="elmer-form-label" htmlFor="get-key-model-field">
          Model
        </label>
        <div className="elmer-model-row">
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
          <button
            type="button"
            className="elmer-detect-btn"
            data-testid="get-key-detect-btn"
            disabled={detectState.status === 'detecting'}
            onClick={() => { void handleDetect(); }}
          >
            {detectState.status === 'detecting' ? 'Detecting…' : 'Detect'}
          </button>
        </div>
      </div>

      {/* Detect results — mirrors ModelForm's detect-results section */}
      {detectState.status === 'success' && detectState.models.length > 0 && (
        <div className="elmer-detect-results">
          <span className="elmer-detect-count">
            ✓ {detectState.models.length} model{detectState.models.length !== 1 ? 's' : ''} detected
          </span>
          <select
            className="elmer-form-select"
            data-testid="get-key-detected-models"
            value={model}
            onChange={(e) => handleDetectedModelSelect(e.target.value)}
          >
            {detectState.models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </div>
      )}
      {detectState.status === 'success' && detectState.models.length === 0 && (
        <div className="elmer-detect-zero" data-testid="get-key-detect-zero">
          No models found at this endpoint.
        </div>
      )}
      {detectState.status === 'error' && (
        <div className="elmer-detect-error" data-testid="get-key-detect-error">
          {detectState.reason}
        </div>
      )}

      {/* T8: Advanced disclosure — collapsed by default. Cloud tiles show all
          fields except num_ctx. Local tiles show the full set including num_ctx
          and the live memory estimate line. */}
      <div className="get-key-advanced" data-testid="get-key-advanced">
        <button
          type="button"
          className="elmer-advanced-toggle"
          data-testid="get-key-advanced-toggle"
          aria-expanded={advancedOpen}
          onClick={() => setAdvancedOpen((o) => !o)}
        >
          {advancedOpen ? '▾' : '▸'} Advanced
          <span className="get-key-advanced-tag"> · power users</span>
        </button>

        {advancedOpen && (
          <div data-testid="get-key-advanced-body">
            <AdvancedModelSettings
              values={advanced}
              onChange={setAdvanced}
              showNumCtx={isLocalTile}
              endpoint={preset.endpoint}
              model={model}
              defaultSystemPromptPlaceholder={DEFAULT_SYSTEM_PROMPT_PLACEHOLDER}
              testIdPrefix="get-key"
            />
          </div>
        )}
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
