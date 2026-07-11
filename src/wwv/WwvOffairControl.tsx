// Self-contained "Refresh off-air" control (Task 15, wwv offair spec). Owns
// its own useWwvOffair() hook instance — no props threaded through the large
// presentational StationFinderControls — and mounts directly into the
// station-finder's reserved actions row. Kicks a background snapshot read on
// mount so a prior off-air (or SWPC/RF) capture's provenance shows up
// immediately without requiring an operator click first; that read hits
// Tauri's `invoke`, which throws outside a real Tauri webview (e.g. `pnpm
// vitest run` / a plain browser tab), so the effect swallows the rejection —
// a failed background prefetch must never crash the host station finder.
//
// Completion pass (spec §9 low-SNR clip + manual entry, §6.3 manual-tune
// prompt): the mount effect also fetches whether the operator's rig is
// CAT-controlled (same swallow-errors shape). When it isn't, an armed
// capture can't auto-tune the rig, so the armed state swaps its note for a
// manual-tune reminder. A no-copy capture gets a richer affordance: play
// back the captured clip (if the backend kept one) to verify by ear, or key
// in the solar indices directly when the auto-decode couldn't parse them.

import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useWwvOffair } from './useWwvOffair';
import { readClip } from './wwvApi';

export function WwvOffairControl() {
  const {
    status,
    windowLabel,
    snapshot,
    wavPath,
    catConfigured,
    arm,
    cancel,
    refreshSnapshot,
    refreshCat,
    manualIngest,
  } = useWwvOffair();

  useEffect(() => {
    void refreshSnapshot().catch(() => {});
    void refreshCat().catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Object URL for the last-fetched clip (raw WAV bytes -> Blob -> URL). Kept
  // as local component state rather than in the hook: it's a browser-side
  // playback affordance, not domain state the rest of the panel needs. Every
  // URL this component ever creates gets revoked exactly once — either when
  // replaced by a fresh fetch or on unmount — via the ref below, since
  // setState's updater form isn't a safe place to run a side effect like
  // URL.revokeObjectURL under React's strict-mode double-invoke.
  const [clipUrl, setClipUrl] = useState<string | null>(null);
  const clipUrlRef = useRef<string | null>(null);
  useEffect(() => {
    clipUrlRef.current = clipUrl;
  }, [clipUrl]);
  useEffect(
    () => () => {
      if (clipUrlRef.current) {
        URL.revokeObjectURL(clipUrlRef.current);
      }
    },
    [],
  );

  const [sfiInput, setSfiInput] = useState('');
  const [aInput, setAInput] = useState('');
  const [kInput, setKInput] = useState('');

  const capturing = status === 'capturing';
  const armed = status === 'armed';
  // Narrowed together so TS carries the non-null `indices` through the JSX
  // below without a non-null assertion. A manual entry lands the same
  // rf-wwv-manual-tagged snapshot the backend's manual-ingest command
  // produces, so it earns the same provenance stamp as a decoded voice
  // bulletin.
  const offairIndices =
    snapshot != null &&
    (snapshot.source === 'rf-wwv-voice' || snapshot.source === 'rf-wwv-manual') &&
    snapshot.indices != null
      ? snapshot.indices
      : null;

  const handlePlayClip = () => {
    if (!wavPath) return;
    void readClip(wavPath)
      .then((bytes) => {
        // readClip's Uint8Array return type carries an ArrayBufferLike
        // (rather than ArrayBuffer) backing type in this TS/lib.dom
        // combination, which BlobPart doesn't structurally accept. Re-wrap
        // through the array-like constructor overload (same pattern as
        // Ics309FormV2's PDF download) to get a fresh ArrayBuffer-backed copy.
        const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'audio/wav' }));
        // Update the ref SYNCHRONOUSLY here (not just via the clipUrl effect
        // below) so a rapid double-click on "Play clip" — a second
        // handlePlayClip resolving before the first render/effect cycle
        // completes — sees the just-created URL on clipUrlRef.current and
        // revokes it instead of orphaning it. Relying solely on the effect
        // left a window where clipUrlRef.current was still stale (null or
        // the URL-before-last) when the second call's revoke check ran.
        if (clipUrlRef.current) {
          URL.revokeObjectURL(clipUrlRef.current);
        }
        clipUrlRef.current = url;
        setClipUrl(url);
      })
      .catch(() => {});
  };

  const handleManualSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const sfi = Number(sfiInput);
    if (sfiInput.trim() === '' || Number.isNaN(sfi)) return;

    const aTrimmed = aInput.trim();
    const kTrimmed = kInput.trim();
    const aIndex = aTrimmed === '' ? null : Number(aTrimmed);
    const kIndex = kTrimmed === '' ? null : Number(kTrimmed);
    if ((aIndex != null && Number.isNaN(aIndex)) || (kIndex != null && Number.isNaN(kIndex))) return;

    void manualIngest(sfi, aIndex, kIndex);
  };

  return (
    <>
      <button
        type="button"
        className="station-finder__refresh-offair"
        disabled={capturing || armed}
        onClick={() => arm(Date.now())}
      >
        {capturing ? 'Capturing…' : 'Refresh off-air'}
      </button>
      {armed && catConfigured === false && (
        <>
          <span className="station-finder__offair-note" data-testid="wwv-offair-manual-tune">
            Tune your radio to WWV (e.g. 10 MHz USB) before {windowLabel} UTC
          </span>
          <button
            type="button"
            className="station-finder__cancel-offair"
            data-testid="wwv-offair-cancel"
            onClick={() => cancel()}
          >
            Cancel
          </button>
        </>
      )}
      {armed && catConfigured !== false && (
        <>
          <span className="station-finder__offair-note" data-testid="wwv-offair-armed">
            Armed for {windowLabel} UTC
          </span>
          <button
            type="button"
            className="station-finder__cancel-offair"
            data-testid="wwv-offair-cancel"
            onClick={() => cancel()}
          >
            Cancel
          </button>
        </>
      )}
      {offairIndices && snapshot && (
        <span
          className="station-finder__offair"
          data-testid="wwv-offair-provenance"
          title={`off-air WWV ${new Date(snapshot.updated_at_ms).toISOString()}`}
        >
          {snapshot.source === 'rf-wwv-manual' ? 'off-air WWV (manual)' : 'off-air WWV'} · SFI{' '}
          <b>{offairIndices.sfi}</b>
          {offairIndices.k_index != null && (
            <>
              {' '}
              · K <b>{offairIndices.k_index}</b>
            </>
          )}
        </span>
      )}
      {status === 'nocopy' && (
        <div className="station-finder__offair-nocopy" data-testid="wwv-offair-nocopy">
          <span>Couldn't copy — will retry next cycle, or verify by ear / enter manually:</span>
          {wavPath && (
            <button
              type="button"
              className="station-finder__refresh-offair"
              data-testid="wwv-offair-play"
              onClick={handlePlayClip}
            >
              Play clip
            </button>
          )}
          {clipUrl && <audio controls src={clipUrl} className="station-finder__offair-clip" />}
          <form className="station-finder__offair-manual" onSubmit={handleManualSubmit}>
            <input
              type="number"
              inputMode="decimal"
              required
              placeholder="SFI"
              aria-label="SFI"
              className="station-finder__offair-field"
              data-testid="wwv-sfi-input"
              value={sfiInput}
              onChange={(event) => setSfiInput(event.target.value)}
            />
            <input
              type="number"
              inputMode="decimal"
              placeholder="A"
              aria-label="A-index"
              className="station-finder__offair-field"
              data-testid="wwv-a-input"
              value={aInput}
              onChange={(event) => setAInput(event.target.value)}
            />
            <input
              type="number"
              inputMode="decimal"
              step="any"
              placeholder="K"
              aria-label="K-index"
              className="station-finder__offair-field"
              data-testid="wwv-k-input"
              value={kInput}
              onChange={(event) => setKInput(event.target.value)}
            />
            <button type="submit" className="station-finder__refresh-offair" data-testid="wwv-manual-save">
              Save
            </button>
          </form>
        </div>
      )}
      {status === 'error' && (
        <span className="station-finder__offair-note" data-testid="wwv-offair-error">
          off-air refresh failed
        </span>
      )}
    </>
  );
}
