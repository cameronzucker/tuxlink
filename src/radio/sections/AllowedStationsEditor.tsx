// src/radio/sections/AllowedStationsEditor.tsx
//
// Shared "Allowed stations" editor used inside each transport's Listen
// section (Telnet-P2P, Packet, ARDOP per spec §1.3). The editor is the
// only place the operator curates which inbound peers the listener
// accepts; all three transports share callsign-list semantics, while
// Telnet additionally has IP-pattern entries. Each transport's
// `*_allowed_stations_*` Tauri commands are passed in via callbacks so
// this component stays transport-agnostic.
//
// Affordances per the mock (Option A):
//   • Allow-any-peer toggle — when ON, every inbound peer is admitted;
//     the lists become advisory. Backend default is TRUE (flipped on
//     this branch in commit 5261f59) — operator-facing toggle default
//     mirrors that so a fresh install shows ON without ambiguity.
//   • Callsign chip-row: each entry is a removable pill; the trailing
//     "+ callsign" affordance opens an inline input (window.prompt for
//     v0.1 — mirrors the existing Peer Password pattern in
//     TelnetP2pRadioPanel.tsx:154).
//   • IP-pattern chip-row (Telnet only): same shape; the parent decides
//     whether to pass `ips`/`onAddIp`/`onRemoveIp`. If omitted, the IP
//     row is hidden — AX.25 has no IP layer and ARDOP has no IP-pattern
//     allowlist either.
//   • Help text describing the gate semantics. The match-logic copy is
//     specific to each transport, so it's passed in as a prop.

import { useState } from 'react';
import './ListenSection.css';

export interface AllowedStationsEditorProps {
  /** When TRUE, every inbound peer is admitted; the lists below are
   *  advisory only. Source of truth lives in the backend; the parent
   *  fetches it via `*_allowed_stations_get` on mount. */
  allowAll: boolean;
  /** Callsigns currently on the allow list. Normalized uppercase by
   *  convention. */
  callsigns: string[];
  /** IP patterns currently on the allow list (Telnet only). Pass
   *  undefined to hide the IP row entirely — Packet + ARDOP do that. */
  ips?: string[];
  /** Help-text string describing the match-logic for THIS transport.
   *  Per spec §1.3: Telnet uses "callsign OR IP," Packet says "no IP
   *  layer," ARDOP says "no station-password layer." */
  helpText: string;
  /** Fires when the operator toggles allow-any-peer. */
  onSetAllowAll: (enabled: boolean) => void;
  /** Fires when the operator adds a callsign (uppercased + trimmed by
   *  this component before invocation). */
  onAddCallsign: (callsign: string) => void;
  /** Fires when the operator removes a callsign chip. */
  onRemoveCallsign: (callsign: string) => void;
  /** Optional IP add/remove handlers — present for Telnet, omitted for
   *  Packet + ARDOP. */
  onAddIp?: (pattern: string) => void;
  onRemoveIp?: (pattern: string) => void;
  /** data-testid prefix so multiple instances on one page (impossible
   *  in production but useful for testing) don't collide. */
  testIdPrefix: string;
}

export function AllowedStationsEditor({
  allowAll,
  callsigns,
  ips,
  helpText,
  onSetAllowAll,
  onAddCallsign,
  onRemoveCallsign,
  onAddIp,
  onRemoveIp,
  testIdPrefix,
}: AllowedStationsEditorProps) {
  // Inline-input state for the "+ callsign" / "+ IP pattern" rows.
  // null = chip-mode (showing the "+ add" button); string = input-mode.
  // Tracking these inline (rather than via window.prompt) keeps the
  // operator's flow uninterrupted and lets the test suite drive add
  // operations via fireEvent.change + .keyDown.
  const [addingCallsign, setAddingCallsign] = useState<string | null>(null);
  const [addingIp, setAddingIp] = useState<string | null>(null);

  const commitCallsign = () => {
    if (addingCallsign === null) return;
    const trimmed = addingCallsign.trim().toUpperCase();
    if (trimmed !== '') onAddCallsign(trimmed);
    setAddingCallsign(null);
  };

  const commitIp = () => {
    if (addingIp === null) return;
    const trimmed = addingIp.trim();
    if (trimmed !== '') onAddIp?.(trimmed);
    setAddingIp(null);
  };

  return (
    <div className="expander-body" data-testid={`${testIdPrefix}-allowed-body`}>
      {/* Allow-any-peer toggle — default ON post-flip per project memory
          [allowed-stations-default-true]. When OFF, only the lists below
          are accepted. */}
      <label
        className="listen-allow-all-row"
        data-testid={`${testIdPrefix}-allow-all-row`}
      >
        <input
          type="checkbox"
          data-testid={`${testIdPrefix}-allow-all-toggle`}
          checked={allowAll}
          onChange={(e) => onSetAllowAll(e.target.checked)}
        />
        <span>Allow any peer (matches WLE default)</span>
      </label>
      <p className="radio-panel-help">
        {allowAll
          ? 'Allow-any is ON — every inbound peer is admitted. Toggle OFF to restrict to the lists below.'
          : 'Allow-any is OFF — only peers on the lists below are admitted.'}
      </p>

      {/* Callsign chip row. */}
      <div className="radio-panel-chip-row" data-testid={`${testIdPrefix}-callsign-row`}>
        {callsigns.map((c) => (
          <span key={c} className="radio-panel-chip" data-testid={`${testIdPrefix}-callsign-${c}`}>
            {c}
            <button
              type="button"
              className="radio-panel-chip-x"
              data-testid={`${testIdPrefix}-callsign-remove-${c}`}
              aria-label={`Remove callsign ${c}`}
              onClick={() => onRemoveCallsign(c)}
            >
              ×
            </button>
          </span>
        ))}
        {addingCallsign === null ? (
          <button
            type="button"
            className="radio-panel-chip radio-panel-chip-add"
            data-testid={`${testIdPrefix}-callsign-add-btn`}
            onClick={() => setAddingCallsign('')}
          >
            + callsign
          </button>
        ) : (
          <input
            type="text"
            className="radio-panel-input"
            data-testid={`${testIdPrefix}-callsign-add-input`}
            autoFocus
            value={addingCallsign}
            placeholder="W7AUX"
            spellCheck={false}
            autoCapitalize="characters"
            autoCorrect="off"
            style={{ width: 110 }}
            onChange={(e) => setAddingCallsign(e.target.value)}
            onBlur={commitCallsign}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                commitCallsign();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                setAddingCallsign(null);
              }
            }}
          />
        )}
      </div>

      {/* IP-pattern chip row — present only when the parent passes IP
          handlers (Telnet only). */}
      {ips !== undefined && onAddIp !== undefined && onRemoveIp !== undefined && (
        <div className="radio-panel-chip-row" data-testid={`${testIdPrefix}-ip-row`}>
          {ips.map((p) => (
            <span key={p} className="radio-panel-chip" data-testid={`${testIdPrefix}-ip-${p}`}>
              {p}
              <button
                type="button"
                className="radio-panel-chip-x"
                data-testid={`${testIdPrefix}-ip-remove-${p}`}
                aria-label={`Remove IP pattern ${p}`}
                onClick={() => onRemoveIp(p)}
              >
                ×
              </button>
            </span>
          ))}
          {addingIp === null ? (
            <button
              type="button"
              className="radio-panel-chip radio-panel-chip-add"
              data-testid={`${testIdPrefix}-ip-add-btn`}
              onClick={() => setAddingIp('')}
            >
              + IP pattern
            </button>
          ) : (
            <input
              type="text"
              className="radio-panel-input"
              data-testid={`${testIdPrefix}-ip-add-input`}
              autoFocus
              value={addingIp}
              placeholder="192.168.1.*"
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              style={{ width: 140 }}
              onChange={(e) => setAddingIp(e.target.value)}
              onBlur={commitIp}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  commitIp();
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  setAddingIp(null);
                }
              }}
            />
          )}
        </div>
      )}

      <p className="radio-panel-help" data-testid={`${testIdPrefix}-help`}>
        {helpText}
      </p>
    </div>
  );
}
