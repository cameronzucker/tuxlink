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

import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import type { ProviderPreset, SetKey, KeyStatus, KeySource, ConfigReadDto } from './elmerModelConfig';
import { isLoopback } from './elmerModelConfig';
import type { DetectState } from './useElmer';

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
// Memory estimate DTO (mirrors Rust MemoryEstimateDto camelCase shape)
// ---------------------------------------------------------------------------

interface MemoryEstimateDto {
  weightsGb: number;
  kvCacheGb: number;
  computeHeadroomGb: number;
  totalGb: number;
  hostRamGb: number;
  fits: boolean;
  numCtx: number;
  kvDtypeBytes: number;
}

// ---------------------------------------------------------------------------
// Default system prompt displayed as placeholder text in the Advanced panel.
// This is the text shown when systemPromptOverride is null (backend default).
// Matches the ELMER_SYSTEM_PROMPT constant in the Rust backend. A Reset clears
// the override (sends null), restoring backend-default behavior.
// ---------------------------------------------------------------------------

const DEFAULT_SYSTEM_PROMPT_PLACEHOLDER =
  'You are Elmer, an AI assistant embedded in Tuxlink — a Winlink and ' +
  'amateur-radio station application… You can call tools as many times as a ' +
  'request needs…';

// Default num_ctx for the local Ollama native path (D5: 32k baseline for
// the 32 GB+ class). Shown as the initial value in the Advanced disclosure.
const DEFAULT_NUM_CTX = 32768;

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

  // num_ctx — local-only field. Seeded from initialConfig (saved value) or the
  // 32k default. Stored as a string so the input is always editable (a number
  // input with a controlled numeric value rejects typed leading zeros etc.).
  const [numCtxStr, setNumCtxStr] = useState<string>(
    () => (initialConfig?.numCtx != null ? String(initialConfig.numCtx) : String(DEFAULT_NUM_CTX)),
  );

  // Temperature slider — 0.0 to 1.0, step 0.05. Null = unset (use provider default).
  // Seeded from initialConfig.temperature or a sensible 0.2 default.
  const [temperature, setTemperature] = useState<number>(
    () => (initialConfig?.temperature != null ? initialConfig.temperature : 0.2),
  );

  // System prompt override. Empty string = no override (use backend default).
  const [systemPrompt, setSystemPrompt] = useState<string>(
    () => (initialConfig?.systemPromptOverride != null ? initialConfig.systemPromptOverride : ''),
  );

  // ---------------------------------------------------------------------------
  // T8: Memory estimate state
  // ---------------------------------------------------------------------------

  type EstimateState =
    | { status: 'idle' }
    | { status: 'loading' }
    | { status: 'ok'; dto: MemoryEstimateDto }
    | { status: 'error' };

  const [estimate, setEstimate] = useState<EstimateState>({ status: 'idle' });

  // Debounce timer ref for the estimate call.
  const estimateTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Derived: parsed numCtx as a number. Falls back to DEFAULT_NUM_CTX if the
  // field is empty or non-numeric so the estimate is always called with a valid
  // number.
  const numCtxParsed = (() => {
    const n = parseInt(numCtxStr, 10);
    return Number.isFinite(n) && n > 0 ? n : DEFAULT_NUM_CTX;
  })();

  // Trigger a debounced estimate call whenever model/numCtx changes (local tiles
  // only, and only when the Advanced section is open).
  useEffect(() => {
    if (!isLocalTile || !advancedOpen) return;
    if (!model.trim()) return;

    if (estimateTimerRef.current !== null) {
      clearTimeout(estimateTimerRef.current);
    }
    setEstimate({ status: 'loading' });
    estimateTimerRef.current = setTimeout(() => {
      estimateTimerRef.current = null;
      invoke<MemoryEstimateDto>('elmer_estimate_memory', {
        model: model.trim(),
        numCtx: numCtxParsed,
        endpoint: preset.endpoint,
      })
        .then((dto) => {
          setEstimate({ status: 'ok', dto });
        })
        .catch(() => {
          setEstimate({ status: 'error' });
        });
    }, 600);

    return () => {
      if (estimateTimerRef.current !== null) {
        clearTimeout(estimateTimerRef.current);
        estimateTimerRef.current = null;
      }
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isLocalTile, advancedOpen, model, numCtxParsed, preset.endpoint]);

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
      const systemPromptArg: string | null = systemPrompt.trim() ? systemPrompt.trim() : null;

      await onSave({
        agentEndpoint: preset.endpoint,
        agentModel: model.trim(),
        key,
        agentTurnTimeoutSecs,
        numCtx: numCtxArg,
        temperature,
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
          <div className="elmer-advanced-body get-key-advanced-body" data-testid="get-key-advanced-body">
            {/* num_ctx — local/native tiles only */}
            {isLocalTile && (
              <div className="elmer-form-row" data-testid="get-key-num-ctx-row">
                <label className="elmer-form-label" htmlFor="get-key-num-ctx">
                  Context length (num_ctx)
                </label>
                <div className="get-key-num-ctx-row">
                  <input
                    id="get-key-num-ctx"
                    type="number"
                    min={512}
                    max={131072}
                    step={512}
                    className="elmer-form-input get-key-num-ctx-input"
                    data-testid="get-key-num-ctx"
                    value={numCtxStr}
                    onChange={(e) => setNumCtxStr(e.target.value)}
                    spellCheck={false}
                  />
                  <span className="get-key-num-ctx-hint">
                    Bigger = more room for tool schemas + history.
                  </span>
                </div>
                {/* CPU-prefill speed cue — the flagged gap in the plan */}
                <p className="get-key-advanced-cpu-hint" data-testid="get-key-cpu-hint">
                  Larger context is free to reserve but slow to fill on CPU.
                </p>
                {/* Memory estimate line */}
                {estimate.status === 'loading' && (
                  <p className="elmer-advanced-loading" data-testid="get-key-estimate-loading">
                    Estimating memory…
                  </p>
                )}
                {estimate.status === 'ok' && (
                  <p className="get-key-estimate-line" data-testid="get-key-estimate-line">
                    <span className="get-key-estimate-kv">
                      ≈ +{estimate.dto.kvCacheGb.toFixed(1)} GB
                    </span>
                    {' '}for this window (KV cache) · {model || '—'} ≈ {estimate.dto.weightsGb.toFixed(1)} GB →
                    {' '}~{estimate.dto.totalGb.toFixed(1)} GB total ·{' '}
                    {estimate.dto.fits ? (
                      <span
                        className="get-key-estimate-fits"
                        data-testid="get-key-estimate-fits"
                      >
                        fits, {estimate.dto.hostRamGb.toFixed(0)} GB host ✓
                      </span>
                    ) : (
                      <span
                        className="get-key-estimate-exceeds"
                        data-testid="get-key-estimate-exceeds"
                      >
                        exceeds {estimate.dto.hostRamGb.toFixed(0)} GB host
                      </span>
                    )}
                  </p>
                )}
                {estimate.status === 'error' && (
                  <p className="elmer-advanced-error" data-testid="get-key-estimate-error">
                    estimate unavailable
                  </p>
                )}
              </div>
            )}

            {/* Temperature slider — all tiles */}
            <div className="elmer-form-row" data-testid="get-key-temperature-row">
              <label className="elmer-form-label" htmlFor="get-key-temperature">
                Temperature
              </label>
              <div className="get-key-temperature-row">
                <input
                  id="get-key-temperature"
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  className="get-key-temperature-slider"
                  data-testid="get-key-temperature"
                  value={temperature}
                  onChange={(e) => setTemperature(parseFloat(e.target.value))}
                />
                <span className="get-key-temperature-value" data-testid="get-key-temperature-value">
                  {temperature.toFixed(2)}
                </span>
              </div>
            </div>

            {/* System prompt — all tiles */}
            <div className="elmer-form-row" data-testid="get-key-system-prompt-row">
              <label className="elmer-form-label" htmlFor="get-key-system-prompt">
                System prompt
              </label>
              <textarea
                id="get-key-system-prompt"
                className="elmer-form-input get-key-system-prompt"
                data-testid="get-key-system-prompt"
                rows={4}
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                placeholder={DEFAULT_SYSTEM_PROMPT_PLACEHOLDER}
                spellCheck={false}
              />
              <button
                type="button"
                className="get-key-reset-prompt"
                data-testid="get-key-reset-prompt"
                onClick={() => setSystemPrompt('')}
              >
                ↺ Reset to default
              </button>
            </div>
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
