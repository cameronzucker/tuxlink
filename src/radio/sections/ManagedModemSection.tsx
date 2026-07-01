// src/radio/sections/ManagedModemSection.tsx
//
// Managed-modem link section for the packet panel (tuxlink-yq3l P7). The
// ACCESSIBILITY PAYOFF of the managed Dire Wolf path: the operator picks a
// sound card + PTT from dropdowns and never authors a Dire Wolf `.conf`.
//
// Renders, for `linkKind: 'Managed'`:
//   - a sound-card picker (dropdown of packet_list_audio_devices() results,
//     shown by humanName; selecting one persists its stableId),
//   - a PTT picker scoped to the chosen device's ranked pttCandidates (default
//     = the first/ranked candidate; an override dropdown shows the resolved
//     choice in plain terms — CM108 HID vs serial RTS),
//   - the station callsign (read from identity upstream — display only; MYCALL
//     comes from identity, the operator does not type it here).
//
// On a card/PTT change the parent persists `linkKind:'Managed'` +
// `managedAudioDevice` (the stableId) + `managedPtt` (the chosen PttChoice).
//
// Empty device list → a helpful "plug in your interface and Refresh" affordance
// + a Refresh button re-calling the list command (no dead end). Soft-failure on
// the invoke matches ModemLinkSection (undefined / reject → empty list).

import { useCallback, useEffect, useState } from 'react';
import type { ChangeEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type {
  ManagedAudioDeviceDto,
  PttChoice,
  StableAudioId,
} from '../../packet/packetTypes';
import { Button } from '../../controls';
import './ModemLinkSection.css';

export interface ManagedModemSectionProps {
  /** Currently-persisted audio-device stable id (null when none chosen yet). */
  audioDevice?: StableAudioId | null;
  /** Currently-persisted PTT choice (null when none chosen yet). */
  ptt?: PttChoice | null;
  /** Effective station call (`<base>-<ssid>`) shown read-only; '' until identity loads. */
  effectiveCall: string;
  /** Emit the chosen device + PTT after every change. The parent persists
   *  `linkKind:'Managed'` + these two fields via packet_config_set. */
  onChange: (device: StableAudioId, ptt: PttChoice) => void;
}

/** Stable-id equality — two ids match when kind AND value agree. */
function sameId(a: StableAudioId | null | undefined, b: StableAudioId): boolean {
  return !!a && a.kind === b.kind && a.value === b.value;
}

/** Serialize a PttChoice to a stable <option> value for the override dropdown. */
function pttKey(p: PttChoice): string {
  return p.kind === 'cm108Hid' ? `cm108Hid:${p.hidrawPath}` : `serialRts:${p.tty}`;
}

/** Plain-language label for a PTT choice — CM108 HID vs serial RTS, with the
 *  resolved device path so the operator can confirm the right line. */
function pttLabel(p: PttChoice): string {
  return p.kind === 'cm108Hid'
    ? `CM108 HID — ${p.hidrawPath}`
    : `Serial RTS — ${p.tty}`;
}

export function ManagedModemSection({
  audioDevice,
  ptt,
  effectiveCall,
  onChange,
}: ManagedModemSectionProps) {
  const [devices, setDevices] = useState<ManagedAudioDeviceDto[]>([]);

  const loadDevices = useCallback(() => {
    // Defend against the Tauri invoke resolving to undefined (a mock that
    // doesn't define this command), mirroring ModemLinkSection's guard.
    void invoke<ManagedAudioDeviceDto[]>('packet_list_audio_devices')
      .then((list) => setDevices(Array.isArray(list) ? list : []))
      .catch(() => setDevices([]));
  }, []);

  // Load the device list on mount (the section only renders when Managed is the
  // active connection, so this fires when the operator picks Managed).
  useEffect(() => {
    loadDevices();
  }, [loadDevices]);

  // The device whose stableId is persisted, resolved against the live list (so
  // we can show its pttCandidates). Null when the persisted device isn't in the
  // current list (unplugged) or nothing is persisted yet.
  const selected = devices.find((d) => sameId(audioDevice, d.stableId)) ?? null;

  // The PTT candidates to choose from come from the selected device. The
  // resolved/active PTT is the persisted one when it's still a candidate,
  // otherwise the device's ranked default (first candidate).
  const candidates = selected?.pttCandidates ?? [];
  const activePtt: PttChoice | null =
    (ptt && candidates.find((c) => pttKey(c) === pttKey(ptt))) ??
    candidates[0] ??
    null;

  const onPickDevice = (e: ChangeEvent<HTMLSelectElement>) => {
    const value = e.target.value;
    const dev = devices.find((d) => `${d.stableId.kind}:${d.stableId.value}` === value);
    if (!dev) return;
    // Default the PTT to the device's ranked first candidate on a device switch.
    const defaultPtt = dev.pttCandidates[0];
    if (!defaultPtt) return; // a device with no PTT line can't be persisted as Managed
    onChange(dev.stableId, defaultPtt);
  };

  const onPickPtt = (e: ChangeEvent<HTMLSelectElement>) => {
    if (!selected) return;
    const next = candidates.find((c) => pttKey(c) === e.target.value);
    if (!next) return;
    onChange(selected.stableId, next);
  };

  return (
    <section className="radio-panel-sec" data-testid="managed-modem-section">
      <h5>Sound card</h5>

      {devices.length === 0 ? (
        <>
          <p
            className="modem-link-help"
            data-testid="managed-no-devices"
          >
            No sound card detected. Plug in your interface (DigiRig / DRA-100),
            then Refresh.
          </p>
          <Button
            tone="neutral" emphasis="outline" size="xs"
            data-testid="managed-refresh"
            onClick={loadDevices}
            aria-label="Refresh sound card list"
          >
            ↻ Refresh
          </Button>
        </>
      ) : (
        <>
          <label className="radio-panel-input-row">
            <span>Card</span>
            <select
              className="radio-panel-input"
              data-testid="managed-device-select"
              value={selected ? `${selected.stableId.kind}:${selected.stableId.value}` : ''}
              onChange={onPickDevice}
            >
              <option value="" disabled>
                Choose sound card…
              </option>
              {devices.map((d) => (
                <option
                  key={`${d.stableId.kind}:${d.stableId.value}`}
                  value={`${d.stableId.kind}:${d.stableId.value}`}
                >
                  {d.humanName}
                </option>
              ))}
            </select>
            <Button
              tone="neutral" emphasis="outline" size="xs"
              data-testid="managed-refresh"
              onClick={loadDevices}
              aria-label="Refresh sound card list"
            >
              ↻
            </Button>
          </label>

          <label className="radio-panel-input-row">
            <span>PTT</span>
            <select
              className="radio-panel-input"
              data-testid="managed-ptt-select"
              value={activePtt ? pttKey(activePtt) : ''}
              disabled={!selected || candidates.length === 0}
              onChange={onPickPtt}
            >
              {candidates.length === 0 ? (
                <option value="" disabled>
                  {selected ? 'No PTT line found for this card' : 'Choose a card first'}
                </option>
              ) : (
                candidates.map((c) => (
                  <option key={pttKey(c)} value={pttKey(c)}>
                    {pttLabel(c)}
                  </option>
                ))
              )}
            </select>
          </label>
          <p className="modem-link-help" data-testid="managed-ptt-help">
            The recommended PTT line is selected automatically; override it if
            your interface keys differently.
          </p>
        </>
      )}

      <label className="radio-panel-input-row">
        <span>Call</span>
        <input
          type="text"
          className="radio-panel-input"
          data-testid="managed-effective-call"
          value={effectiveCall}
          readOnly
        />
      </label>
    </section>
  );
}
