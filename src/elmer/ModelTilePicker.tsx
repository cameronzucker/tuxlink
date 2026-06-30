/**
 * ModelTilePicker — Elmer's canonical model surface (Task 8a, F1-consume / F4).
 *
 * A cloud-first, price-tiered tile picker that replaces the dense `<select>`
 * form as the primary way an operator chooses a model. Tiles are grouped by
 * the `tier` field on each `ProviderPreset` into four sections:
 *   Free · no credit card   (gemini, groq)
 *   Pay-as-you-go · needs a card (openai, anthropic)
 *   On this computer        (localOllama)
 *   Other                   (openrouter, custom)
 *
 * Each tile is a real <button role="radio"> inside a per-tier role="radiogroup"
 * so the whole picker is keyboard-reachable (Tab to the group, arrows/Enter to
 * pick) — NOT a wall of clickable <div>s.
 *
 * State model: the picker owns a WORKING endpoint + model, seeded from
 * initialEndpoint/initialModel (the operator's SAVED config), so the
 * pre-selected tile shows the saved model rather than that tile's default.
 * Selecting a different cloud/local tile sets the working endpoint to the
 * preset's endpoint and uses `nextModelForPreset` (the single source of truth
 * shared with ModelForm.handlePresetChange) to decide the model — null = keep.
 *
 * The "Other" tier (openrouter + custom) renders the EXPORTED `ModelForm`
 * VERBATIM as its editor — the picker does not re-implement the custom-endpoint
 * flow (no second model-selection path). The guided "get a free key" flow and
 * the open-browser button are Task 9 — NOT built here.
 *
 * Security: `keyStatusByOrigin` is a PROP (statuses only — never key values).
 * The picker renders no key value anywhere. There is NO renderer-side fetch/XHR
 * to any provider endpoint (SSRF-1) — all egress goes through the onSave/onDetect
 * callbacks the parent wires to the vetted Tauri commands.
 */

import { useState } from 'react';
import {
  PRESETS,
  inferPreset,
  originOf,
  nextModelForPreset,
  type ProviderTier,
  type KeyStatusByOrigin,
  type KeyStatus,
  type SetKey,
  type KeySource,
} from './elmerModelConfig';
import { ModelForm } from './ElmerPane';
import { GetKeyCard } from './GetKeyCard';
import type { DetectState } from './useElmer';
import './ElmerPane.css';

export interface ModelTilePickerProps {
  onSave: (args: {
    agentEndpoint: string;
    agentModel: string;
    key: SetKey;
    agentTurnTimeoutSecs: number;
  }) => Promise<void>;
  onDetect: (args: { agentEndpoint: string; keySource: KeySource }) => Promise<void>;
  detectState: DetectState;
  /** Per-origin keyring status (statuses only, never values). Drives the per-tile key-saved badge. */
  keyStatusByOrigin: KeyStatusByOrigin;
  initialEndpoint: string;
  initialModel: string;
  initialKeyStatus: KeyStatus;
  initialTurnTimeoutSecs: number;
}

/** Tier section headers, in display order. */
const TIER_ORDER: { tier: ProviderTier; header: string }[] = [
  { tier: 'free', header: 'Free · no credit card' },
  { tier: 'paygo', header: 'Pay-as-you-go · needs a card' },
  { tier: 'local', header: 'On this computer' },
  { tier: 'other', header: 'Other' },
];

export function ModelTilePicker({
  onSave,
  onDetect,
  detectState,
  keyStatusByOrigin,
  initialEndpoint,
  initialModel,
  initialKeyStatus,
  initialTurnTimeoutSecs,
}: ModelTilePickerProps) {
  // Working endpoint + model, seeded from the SAVED config (not a tile default)
  // so the pre-selected tile shows the operator's model.
  const [endpoint, setEndpoint] = useState(initialEndpoint);
  const [model, setModel] = useState(initialModel);

  // Which preset id is currently selected, derived from the working endpoint's
  // origin so a hand-edited path on a known origin still maps to its tile.
  const selectedId = inferPreset(endpoint);
  const selectedPreset = PRESETS.find((p) => p.id === selectedId);
  // The "Other" tier (openrouter, custom) edits via the reused ModelForm.
  const otherSelected = selectedPreset?.tier === 'other';

  function handleTileSelect(presetId: string) {
    const preset = PRESETS.find((p) => p.id === presetId);
    if (!preset) return;
    // Decide the next model BEFORE moving the endpoint (nextModelForPreset reads
    // the OUTGOING endpoint to detect an untouched-vs-hand-edited model).
    const nextModel = nextModelForPreset(endpoint, model, presetId);
    setEndpoint(preset.endpoint);
    if (nextModel !== null) setModel(nextModel);
  }

  function handleSave() {
    void onSave({
      agentEndpoint: endpoint,
      agentModel: model,
      key: { action: 'keep' },
      agentTurnTimeoutSecs: initialTurnTimeoutSecs,
    });
  }

  return (
    <div className="elmer-tile-picker" data-testid="elmer-tile-picker">
      {TIER_ORDER.map(({ tier, header }) => {
        const tilesInTier = PRESETS.filter((p) => p.tier === tier);
        if (tilesInTier.length === 0) return null;
        return (
          <section className="elmer-tier" key={tier} data-tier={tier}>
            <h3 className="elmer-tier-header">{header}</h3>
            <div className="elmer-tier-tiles" role="radiogroup" aria-label={header}>
              {tilesInTier.map((preset) => {
                const isSelected = preset.id === selectedId;
                const origin = originOf(preset.endpoint);
                const keySaved = origin !== '' && keyStatusByOrigin[origin] === 'present';
                return (
                  <button
                    key={preset.id}
                    type="button"
                    role="radio"
                    aria-checked={isSelected}
                    className={`elmer-tile${isSelected ? ' elmer-tile--selected' : ''}`}
                    data-testid={`elmer-tile-${preset.id}`}
                    onClick={() => handleTileSelect(preset.id)}
                  >
                    <span className="elmer-tile-label">{preset.label}</span>
                    {preset.id === 'gemini' && (
                      <span className="elmer-tile-badge elmer-tile-badge--recommended">
                        RECOMMENDED
                      </span>
                    )}
                    {keySaved && (
                      <span
                        className="elmer-tile-badge elmer-tile-badge--keysaved"
                        data-testid={`elmer-tile-keysaved-${preset.id}`}
                      >
                        ✓ key saved
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          </section>
        );
      })}

      {/* Editor for the selected tile.
          - "Other" tier (openrouter, custom) reuses ModelForm verbatim.
          - Cloud/paygo tiles with a keyPageUrl get the guided GetKeyCard flow (Task 9).
          - Local/other tiles without a keyPageUrl get the lightweight model+Save summary. */}
      <div className="elmer-tile-editor" data-testid="elmer-tile-editor">
        {otherSelected ? (
          <ModelForm
            onSave={onSave}
            onDetect={onDetect}
            detectState={detectState}
            initialEndpoint={endpoint}
            initialModel={model}
            initialKeyStatus={initialKeyStatus}
            initialTurnTimeoutSecs={initialTurnTimeoutSecs}
          />
        ) : selectedPreset?.keyPageUrl ? (
          <GetKeyCard
            preset={selectedPreset}
            onSave={onSave}
            agentModel={model}
            agentTurnTimeoutSecs={initialTurnTimeoutSecs}
          />
        ) : (
          <div className="elmer-tile-summary">
            <div className="elmer-form-row">
              <label className="elmer-form-label" htmlFor="elmer-tile-endpoint">
                Endpoint
              </label>
              <span
                id="elmer-tile-endpoint"
                className="elmer-tile-endpoint elmer-form-input--mono"
                data-testid="elmer-tile-endpoint"
              >
                {endpoint}
              </span>
            </div>
            <div className="elmer-form-row">
              <label className="elmer-form-label" htmlFor="elmer-tile-model-input">
                Model
              </label>
              <input
                id="elmer-tile-model-input"
                type="text"
                className="elmer-form-input elmer-form-input--mono"
                data-testid="elmer-tile-model-input"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                spellCheck={false}
                autoComplete="off"
              />
            </div>
            <div className="elmer-form-save-row">
              <button
                type="button"
                className="elmer-save-btn"
                data-testid="elmer-tile-save"
                onClick={handleSave}
              >
                Save &amp; use
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
