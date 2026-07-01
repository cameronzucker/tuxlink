/**
 * AdvancedModelSettings — the shared "Advanced" model-tuning panel (tuxlink-65qhn).
 *
 * Extracted from GetKeyCard's T8 Advanced disclosure so BOTH the cloud editor
 * (GetKeyCard) and the local/custom editor (ModelForm) render the SAME panel.
 * This is the reachability fix: num_ctx (the epic's core local-config field) was
 * previously only wired into GetKeyCard, which renders only for cloud tiles — so
 * the local Ollama tile (which renders ModelForm) had NO num_ctx surface at all.
 *
 * The panel renders the disclosure BODY only — it does NOT own the ▸ Advanced
 * toggle. The caller owns the collapse/expand state and the toggle button, and
 * renders <AdvancedModelSettings> inside the expanded body. (GetKeyCard has its
 * own `advancedOpen` state; ModelForm reuses its own.) This keeps a single
 * toggle per editor and lets each editor style the toggle to fit its layout.
 *
 * Contents (per the mock, dev/scratch/elmer-native-ollama-mock-v2.png):
 *   - num_ctx input + live memory-estimate line — rendered ONLY when
 *     `showNumCtx` is true (local/native tiles). Cloud tiles pass false.
 *   - CPU-prefill speed cue (with the num_ctx block).
 *   - Temperature slider (0–1) — all tiles.
 *   - System-prompt textarea + Reset-to-default — all tiles. Reset sends null
 *     (clears the override so the backend default applies).
 *
 * Memory estimate: when `showNumCtx` is true and the panel is mounted (the caller
 * only mounts it when its disclosure is open), a debounced call to
 * `elmer_estimate_memory` runs on model/num_ctx change. The fit badge is green
 * (fits) or red (exceeds); an invocation error degrades gracefully to
 * "estimate unavailable" and never throws.
 *
 * Security: no renderer-side fetch/XHR. The only backend touch is the vetted
 * `elmer_estimate_memory` Tauri command, called with the current endpoint/model
 * (SSRF-1: the estimate command does not egress to the endpoint — it reads local
 * model metadata / host RAM; the endpoint is passed for native-path selection).
 */

import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

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

// Default num_ctx for the local Ollama native path (D5: 32k baseline for the
// 32 GB+ class). Used as the seed when no saved value is supplied and as the
// fallback for the estimate call when the field is cleared / non-numeric.
export const DEFAULT_NUM_CTX = 32768;

// Default system prompt shown as placeholder text when systemPromptOverride is
// null (backend default). Matches the ELMER_SYSTEM_PROMPT constant in the Rust
// backend. A Reset clears the override (sends null), restoring backend-default
// behavior. Exported so BOTH editors (GetKeyCard cloud + ModelForm local) show
// the same placeholder from a single source.
export const DEFAULT_SYSTEM_PROMPT_PLACEHOLDER =
  'You are Elmer, an AI assistant embedded in Tuxlink — a Winlink and ' +
  'amateur-radio station application… You can call tools as many times as a ' +
  'request needs…';

// ---------------------------------------------------------------------------
// Value model — the three advanced fields the panel edits.
// ---------------------------------------------------------------------------

export interface AdvancedModelValues {
  /**
   * num_ctx as an editable STRING (a number input with a controlled numeric
   * value rejects typed leading zeros / partial entry). Parsed by the caller
   * at save time. Only meaningful when showNumCtx is true.
   */
  numCtxStr: string;
  /** Temperature 0.0–1.0. */
  temperature: number;
  /** System-prompt override. Empty string = no override (backend default). */
  systemPrompt: string;
}

export interface AdvancedModelSettingsProps {
  /** Current values (controlled). */
  values: AdvancedModelValues;
  /** Change handler — receives the next full value object. */
  onChange: (next: AdvancedModelValues) => void;
  /**
   * When true, render the num_ctx field + its live memory-estimate line
   * (local/native tiles). When false, hide both (cloud tiles).
   */
  showNumCtx: boolean;
  /** Endpoint for the debounced elmer_estimate_memory call (native-path select). */
  endpoint: string;
  /** Model id for the debounced elmer_estimate_memory call + estimate display. */
  model: string;
  /** Placeholder shown in the system-prompt textarea (the backend default prompt). */
  defaultSystemPromptPlaceholder: string;
  /**
   * Test-id prefix so each host editor keeps stable, distinct testids. GetKeyCard
   * passes "get-key" (preserving the existing T8 testids); ModelForm passes
   * "model-form-advanced".
   */
  testIdPrefix: string;
}

// ---------------------------------------------------------------------------
// Component — renders the disclosure BODY (no toggle).
// ---------------------------------------------------------------------------

export function AdvancedModelSettings({
  values,
  onChange,
  showNumCtx,
  endpoint,
  model,
  defaultSystemPromptPlaceholder,
  testIdPrefix,
}: AdvancedModelSettingsProps) {
  const { numCtxStr, temperature, systemPrompt } = values;

  type EstimateState =
    | { status: 'idle' }
    | { status: 'loading' }
    | { status: 'ok'; dto: MemoryEstimateDto }
    | { status: 'error' };

  const [estimate, setEstimate] = useState<EstimateState>({ status: 'idle' });
  const estimateTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Parsed numCtx — falls back to DEFAULT_NUM_CTX if the field is empty or
  // non-numeric so the estimate is always called with a valid positive number.
  const numCtxParsed = (() => {
    const n = parseInt(numCtxStr, 10);
    return Number.isFinite(n) && n > 0 ? n : DEFAULT_NUM_CTX;
  })();

  // Debounced estimate call whenever model/num_ctx changes — only when the panel
  // renders the num_ctx field (showNumCtx) and a model is present. The caller only
  // mounts this component when its disclosure is open, so there is no separate
  // "advancedOpen" guard here.
  useEffect(() => {
    if (!showNumCtx) return;
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
        endpoint,
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
  }, [showNumCtx, model, numCtxParsed, endpoint]);

  return (
    <div
      className="elmer-advanced-body get-key-advanced-body"
      data-testid={`${testIdPrefix}-body`}
    >
      {/* num_ctx — local/native tiles only */}
      {showNumCtx && (
        <div className="elmer-form-row" data-testid={`${testIdPrefix}-num-ctx-row`}>
          <label className="elmer-form-label" htmlFor={`${testIdPrefix}-num-ctx-input`}>
            Context length (num_ctx)
          </label>
          <div className="get-key-num-ctx-row">
            <input
              id={`${testIdPrefix}-num-ctx-input`}
              type="number"
              min={512}
              max={131072}
              step={512}
              className="elmer-form-input get-key-num-ctx-input"
              data-testid={`${testIdPrefix}-num-ctx`}
              value={numCtxStr}
              onChange={(e) => onChange({ ...values, numCtxStr: e.target.value })}
              spellCheck={false}
            />
            <span className="get-key-num-ctx-hint">
              Bigger = more room for tool schemas + history.
            </span>
          </div>
          {/* CPU-prefill speed cue */}
          <p className="get-key-advanced-cpu-hint" data-testid={`${testIdPrefix}-cpu-hint`}>
            Larger context is free to reserve but slow to fill on CPU.
          </p>
          {/* Memory estimate line */}
          {estimate.status === 'loading' && (
            <p className="elmer-advanced-loading" data-testid={`${testIdPrefix}-estimate-loading`}>
              Estimating memory…
            </p>
          )}
          {estimate.status === 'ok' && (
            <p className="get-key-estimate-line" data-testid={`${testIdPrefix}-estimate-line`}>
              <span className="get-key-estimate-kv">
                ≈ +{estimate.dto.kvCacheGb.toFixed(1)} GB
              </span>
              {' '}for this window (KV cache) · {model || '—'} ≈ {estimate.dto.weightsGb.toFixed(1)} GB →
              {' '}~{estimate.dto.totalGb.toFixed(1)} GB total ·{' '}
              {estimate.dto.fits ? (
                <span
                  className="get-key-estimate-fits"
                  data-testid={`${testIdPrefix}-estimate-fits`}
                >
                  fits, {estimate.dto.hostRamGb.toFixed(0)} GB host ✓
                </span>
              ) : (
                <span
                  className="get-key-estimate-exceeds"
                  data-testid={`${testIdPrefix}-estimate-exceeds`}
                >
                  exceeds {estimate.dto.hostRamGb.toFixed(0)} GB host
                </span>
              )}
            </p>
          )}
          {estimate.status === 'error' && (
            <p className="elmer-advanced-error" data-testid={`${testIdPrefix}-estimate-error`}>
              estimate unavailable
            </p>
          )}
        </div>
      )}

      {/* Temperature slider — all tiles */}
      <div className="elmer-form-row" data-testid={`${testIdPrefix}-temperature-row`}>
        <label className="elmer-form-label" htmlFor={`${testIdPrefix}-temperature-input`}>
          Temperature
        </label>
        <div className="get-key-temperature-row">
          <input
            id={`${testIdPrefix}-temperature-input`}
            type="range"
            min={0}
            max={1}
            step={0.05}
            className="get-key-temperature-slider"
            data-testid={`${testIdPrefix}-temperature`}
            value={temperature}
            onChange={(e) => onChange({ ...values, temperature: parseFloat(e.target.value) })}
          />
          <span
            className="get-key-temperature-value"
            data-testid={`${testIdPrefix}-temperature-value`}
          >
            {temperature.toFixed(2)}
          </span>
        </div>
      </div>

      {/* System prompt — all tiles */}
      <div className="elmer-form-row" data-testid={`${testIdPrefix}-system-prompt-row`}>
        <label className="elmer-form-label" htmlFor={`${testIdPrefix}-system-prompt-input`}>
          System prompt
        </label>
        <textarea
          id={`${testIdPrefix}-system-prompt-input`}
          className="elmer-form-input get-key-system-prompt"
          data-testid={`${testIdPrefix}-system-prompt`}
          rows={4}
          value={systemPrompt}
          onChange={(e) => onChange({ ...values, systemPrompt: e.target.value })}
          placeholder={defaultSystemPromptPlaceholder}
          spellCheck={false}
        />
        <button
          type="button"
          className="get-key-reset-prompt"
          data-testid={`${testIdPrefix}-reset-prompt`}
          onClick={() => onChange({ ...values, systemPrompt: '' })}
        >
          ↺ Reset to default
        </button>
      </div>
    </div>
  );
}
