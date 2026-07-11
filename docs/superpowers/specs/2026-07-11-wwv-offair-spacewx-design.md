# Off-air WWV/WWVH space-weather decode — design spec

**Date:** 2026-07-11
**Status:** Design approved (operator, this session)
**bd issue:** tuxlink-xscum
**Agent:** gorge-fern-cedar
**Branch:** `bd-tuxlink-xscum/wwv-offair-spacewx`

## 1. Problem & value

Winlink Express has no off-air space-weather source. Tuxlink can decode the NOAA
SWPC geophysical alert that WWV/WWVH broadcast **by voice over HF**, giving the
propagation engine a real, internet-free solar input. This is an EmComm
capability gap WLE cannot close, and the operator's stated value: "excellent …
huge value added over WLE."

The feature's entire justification is **internet-free at runtime**. The obvious
"just pull SWPC/GIRO over HTTP" idea is worthless in the scenario that matters
(the internet is down); WWV works because the data rides HF and needs nothing
but a receiver.

## 2. Operator constraints (the filter — keep applied while building)

- **Internet-free at runtime.** First-run internet is permitted ONLY to acquire
  the STT model and validate the parser format. Once provisioned, the runtime
  path touches no network.
- **Operator-usable decision data, not science telemetry.** We ingest exactly
  the numbers the prediction engine uses (SFI → SSN) plus display context
  (A/K/storm state). WWV *Doppler* (TID science) and ionograms are explicitly
  out (§13).
- **Primary transceiver is first-class; no new hardware.** Reuse the existing
  rig-control and audio plumbing. SDR is a *supported optional* method, never
  required.
- **Occasional / on-demand / pre-flight; NEVER mid-session.** Missing a 3-hour
  cycle is fine.
- **RX only.** This feature never transmits. See §12.

## 3. Verified facts (2026-07-11)

- WWV voice-broadcasts the SWPC geophysical alert at **:18** past each hour;
  **WWVH at :45**. Segment is < 45 s, refreshed every 3 h
  (00/03/06/09/12/15/18/21 UTC). Carried over HF → receivable off-air.
- Rigid machine-generated template (`services.swpc.noaa.gov/text/wwv.txt`), the
  voice being that text read aloud. Closed vocabulary: numbers + the NOAA G/S/R
  scale phrases. Example wording (verified in-repo against `parse_wwv`):
  > "Solar flux 117 and estimated planetary A-index 6. The estimated planetary
  > K-index at 1200 UTC on 16 June was 1.33."
- **No digital space-weather subcarrier.** The 100 Hz BCD subcarrier carries
  TIME only. Therefore the decode is **voice STT**, bounded by the fixed grammar.

## 4. Key architectural finding — most of this already exists

Grounded against `origin/main` (NOT the ambient feature-branch checkout, which
predates the propagation + rig work):

| Pillar | Status on `origin/main` | Reference |
|---|---|---|
| Tune to WWV + save/restore VFO | **Exists** — `ManagedRig::tune(hz, mode)` + `status() -> RigStatus`; `release_serial()` for C-Media/DRA serial↔audio contention | `src-tauri/tux-rig/src/managed.rs:93,100,107` |
| Parse SFI/A/K from bulletin | **Exists** — pure, offline, tolerant substring parse | `src-tauri/src/propagation/solar.rs:77` (`parse_wwv`) |
| SFI → SSN for the engine | **Exists** — Covington F10.7↔SSN relation | `solar.rs:106` (`derive_ssn_from_sfi`) |
| Ingest WWV-derived SSN into VOACAP | **Exists + tested** — writes derived SSN to current month, preserving others | `solar_update.rs:104` (`apply_rf_solar_reply`) |
| Engine consumes SSN | **Exists** — VOACAP deck `SUNSPOT` field | `deck.rs:97` |
| Enumerate/pick audio device | **Exists** — `arecord -L` shell-out | `ui_commands.rs:4765` |
| WAV read | **Exists** — `hound` (used by `tuxlink-ft8`) | — |
| Offline STT | **Greenfield** — the one new subsystem | — |
| ~70 s capture | **Greenfield** — no live-capture path exists | — |

**Consequence:** the engine's *ingestion logic* for WWV-derived SSN is built and
unit-tested (`parse_wwv` → `derive_ssn_from_sfi` → `apply_rf_solar_reply` →
`ssn-forecast.json` → VOACAP `SUNSPOT`), and the predict path reads the forecast
**fresh per call** — so once *something* writes the forecast, the next
prediction uses it. That "something" does **not exist yet**:
`apply_rf_solar_reply` has no caller (only tests), and there is no update command
(internet or RF) and no frontend trigger/reader. So this feature reuses the
**engine chain** but must **build the command + frontend that drives it** (§5
scope note). There is **no new *engine* work** and **no information-overload
risk** — the architecture already separates the single engine input
(`ssn-forecast.json`, SSN from SFI) from display-only context
(`solar-snapshot.json`, A/K/provenance; explicitly "NOT a VOACAP input", per
`solar_update.rs:10-13`).

### Fidelity caveat (accepted, not papered over)

The internet path gives VOACAP its designed input — a **smoothed monthly SSN
forecast**. The WWV path gives a **single daily-SFI-derived instantaneous SSN**
written to the current month (the code calls this "the documented coarser
fallback"). Predictions from the WWV path are coarser. This is acceptable
because (a) it is the *same fidelity tuxlink already accepts* for its
Winlink-radio path — we introduce no new compromise — and (b) off-air the
alternative is **zero** space-weather data.

## 5. End-to-end pipeline

New components in **bold**; everything else reuses `origin/main`:

```
operator hits "Refresh off-air"  ← **new frontend button (§9) + new Tauri command**
  → scheduler computes nearest window (WWV :18 / WWVH :45), arms one-shot  ← **new: wwv_offair::schedule**
  → at window (ManagedRig spawned via reused rig_config_from(&config.rig)):
      ManagedRig::status()                 [save current VFO + mode; tux-rig]
      ManagedRig::tune(wwv_freq, Mode::Usb) [tux-rig]
      IF close_serial_sequencing (FT-710 class): release_serial() → rigctld STOPS  [tux-rig]
      **arecord capture ~70 s → 16 kHz mono WAV**   ← **new: wwv_offair::capture** (reuses arecord + hound)
      RESTORE: re-spawn ManagedRig if released, then tune(saved_freq, saved_mode)
               (DRA-100 path never releases, so no re-spawn) [tux-rig]
      **tuxlink-stt::transcribe(wav, DecodeMode::WwvBiased)**  ← **new crate**
      **normalize_spoken_numbers(transcript)**       ← **new: wwv_offair::normalize**
      parse_wwv(normalized) + SFI sanity bound [50,500]  [solar.rs, unchanged] — the grammar enforcement
      apply_rf_solar_reply(text, y, m, now, dir), snapshot source = "rf-wwv-voice"  [solar_update.rs, +1 tag]
  → predictions read ssn-forecast.json fresh per call, so the next predict uses it  [existing predict path]
  → **new frontend: conditions readout + "off-air WWV HH:18 UTC" provenance stamp**
```

> **Scope note (grounded 2026-07-11):** `apply_rf_solar_reply` and the whole
> `solar_update` module are **pure, unit-tested logic with no caller** — no
> update command (internet or RF) and no frontend trigger/reader exist yet (the
> internet "Update propagation data" feature, tuxlink-ot71, is also only
> planned). So the engine *ingestion* is proven and reused, but the **Tauri
> command and the frontend surface are NEW work this feature builds first**, not
> an existing surface. The engine-compatibility guarantee still holds: the
> predict path reads `ssn-forecast.json` fresh each call, so an off-air update
> drives real predictions.

## 6. Component design

### 6.1 `tuxlink-stt` crate (new, reusable)

A self-contained crate under `src-tauri/tuxlink-stt/`, mirroring how `tux-rig`
is an independent crate rather than glue inside a feature module. This is the
deliberate seam that lets Elmer voice-input reuse it later (operator's note).

```rust
pub enum DecodeMode {
    /// General open-vocabulary decode (Elmer voice-input, future).
    General,
    /// Decode biased toward the closed WWV vocabulary via `set_initial_prompt`.
    WwvBiased,
}

pub struct SttResult {
    pub text: String,
    /// Per-segment no-speech / avg-logprob so the caller can reject noise.
    pub confidence: SttConfidence,
}

pub fn transcribe(wav_path: &Path, mode: DecodeMode, model: &WhisperModel)
    -> Result<SttResult, SttError>;
```

- **Engine:** `whisper-rs` (Rust bindings to whisper.cpp). Native in-process —
  **no Python, no Docker, no sidecar** (rejects the Geographica microservice;
  avoids cross-repo coupling). Consistent with the Pat-sidecar removal (#175):
  native over external runtime.
- **Model:** `base.en`, quantized `q5_1` (~57 MB) ggml. `base` (not `tiny`)
  because it transcribes noisy HF voice better AND is the general-purpose model
  Elmer would reuse. Grammar constraint (below) recovers the accuracy a smaller
  model would lose on this vocabulary.
- **Vocabulary biasing (`WwvBiased`):** `whisper-rs`'s `FullParams` does **NOT**
  expose GBNF grammar-constrained decoding (verified against the binding — it
  offers `set_initial_prompt` + token-level probs, not hard grammar). So the
  "grammar" is achieved in two cooperating places instead of inside the decoder:
  (1) `set_initial_prompt` primes the decoder with the WWV vocabulary ("NOAA
  space weather: solar flux, planetary A-index, K-index, geomagnetic storm…") to
  bias output; (2) the closed grammar is **enforced after decode** by
  `parse_wwv`'s tolerant substring match + the existing SFI sanity bound
  [50,500] + retry-next-window on parse failure. Same robustness outcome,
  achieved post-decode. (If a future `whisper-rs` exposes grammar rules, it's a
  drop-in upgrade to place (1).)
- **Noise rejection:** port Geographica's tuned thresholds (config, ~5 lines,
  not the service): `no_speech_threshold=0.8`, `log_prob_threshold=-0.8`. On a
  low-SNR capture, return empty/low-confidence rather than a confident
  hallucination — the caller then retries the next window or shows the clip.
- **Model acquisition:** fetched at **first-run / setup** into
  `~/.local/share/tuxlink/models/` (respects the "first-run internet only"
  rule); runtime is off-air thereafter. A documented **manual-place** path
  supports genuinely air-gapped installs. Keeps the app installer lean (the
  operator's footprint concern — the ~57 MB never enters the shipped binary).

### 6.2 Capture (`wwv_offair::capture`)

- Shell out to `arecord` (pattern already in `ui_commands.rs`) against the
  configured capture device: `arecord -D <dev> -f S16_LE -c 1 -r 16000 -d 70 out.wav`.
  16 kHz mono is Whisper's native rate — no resample step.
- ~70 s window starting ~5 s before the segment tolerates a few seconds of
  system-clock error (airtime is free; RX only; margin is cheap).
- Read back via `hound` (already a dep). WAV written to a scratch temp path,
  deleted after transcription unless the operator asks to keep the clip
  (low-SNR confirm, §9).
- The capture device is single-owner. Because capture is pre-flight /
  never-mid-session, no VARA/ARDOP session holds it. If the device is busy, the
  attempt fails cleanly with a "close your modem session first" message.

### 6.3 Rig orchestration (`wwv_offair::capture_cycle`)

Ordering matters for the DRA-100 / C-Media class where opening the CAT serial
port while the audio codec is active can reset the codec:

1. `ManagedRig::spawn(rig_config_from(&config.rig)?)` — reuse the existing
   adapter (`crate::modem_commands::rig_config_from`).
2. `status()` → save `{freq_hz, mode}` (`mode` is `Option<Mode>`).
3. `tune(wwv_freq, Mode::Usb)`.
4. **If `config.rig.close_serial_sequencing`** (FT-710 / internal-codec class):
   `release_serial()` — this **stops rigctld**, freeing the CAT serial before
   the audio codec opens. On the DRA-100 path (`close_serial_sequencing ==
   false`) skip this — rigctld keeps running through capture.
5. `arecord` capture.
6. **Restore:** if `release_serial()` was called, `ManagedRig::spawn(...)` again
   (rigctld was stopped; `tune`/`status` fail until re-spawn — see the
   `release_serial` doc comment), then `tune(saved_freq_hz, saved_mode)`. On the
   DRA path, the same `ManagedRig` is still live, so just `tune(...)` to restore.

If rigctld/CAT is unavailable (`rig_config_from` returns `None` — no hamlib
model or blank CAT serial), degrade
to a **manual-tune** flow (mirrors the FT8 design's own fallback): prompt
"tune your radio to WWV 10 MHz USB, then Capture," skip the tune/restore steps,
and run capture → STT → parse directly.

**Frequency selection by time of day** (operator-overridable): 10 MHz as the
all-rounder default; 5 / 2.5 MHz at night; 15 / 20 MHz in daytime. WWV carries
2.5/5/10/15/20 MHz; WWVH 2.5/5/10/15. On no-copy, fall through to WWVH :45 or
the next cycle (§8).

### 6.4 Spoken-number normalizer (`wwv_offair::normalize`)

Pure function: maps residual spoken forms the grammar didn't already coerce to
digits ("one hundred seventeen" → "117", "one point three three" → "1.33") so
the existing `parse_wwv` substring matcher works unchanged. Fully unit-testable
against fixed transcripts. Kept deliberately small because the GBNF grammar does
most of the digit coercion upstream.

### 6.5 Parse & engine feed (reuse)

- `parse_wwv(normalized) -> Option<SolarIndices>` — unchanged.
- `apply_rf_solar_reply(text, year, month, now_ms, config_dir)` — reused. The
  only change: a **new provenance tag `"rf-wwv-voice"`** distinguishing an
  off-air voice decode from the Winlink-network `"rf-wwv"` catalog reply. This
  is an additive string; the snapshot's `source` field already carries it.

### 6.6 Scheduler / nearest-window (`wwv_offair::schedule`)

- Given "now" (UTC), compute the next WWV :18 and next WWVH :45; arm a
  **non-blocking one-shot** at whichever is sooner (typically ≤ 33 min).
- Deterministically unit-testable via injected `now_ms` (the propagation
  modules already follow this pattern).
- The operator keeps doing pre-flight while the one-shot waits; a small "armed
  for HH:MM UTC — cancel?" affordance is shown.

### 6.7 Optional-SDR seam (`CaptureSource` trait)

Designed now, **not built**:

```rust
trait CaptureSource {
    fn capture(&self, freq_hz: u64, dwell: Duration) -> Result<PathBuf, CaptureError>;
}
```

`PrimaryRigSource` (tune + arecord) is the only implementation this feature
ships. A future `SdrSource` drops in without touching the STT/normalize/parse/
engine chain. This keeps the wideband-SDR work (§13) cleanly separable.

## 7. Data-model changes

- `SolarSnapshot.source`: new documented value `"rf-wwv-voice"` alongside
  `"swpc"` and `"rf-wwv"`. Purely additive; existing snapshots still parse.
- No change to `ssn-forecast.json` shape, `SolarIndices`, or the engine deck.
- New config (additive `Option<...>` sub-struct, per the `modem_ardop` pattern):
  `WwvOffairConfig { capture_device, preferred_freqs_by_tod, model_path,
  auto_retry_next_window }`. Absent config → sensible defaults; no breaking
  change to existing `config.json`.

## 8. Error handling & failure modes

| Failure | Handling |
|---|---|
| No copy / low SNR (STT below confidence threshold) | Auto-retry the next window **once** (WWVH :45 after WWV :18, or the next 3 h cycle); then surface the saved clip for the operator to confirm/enter manually. Never write a hallucinated value. |
| `parse_wwv` returns `None` (no valid SFI, or SFI outside [50,500]) | Treat as no-copy; do not update the forecast; keep the prior value + its freshness stamp. |
| rigctld/CAT unavailable | Manual-tune fallback (§6.3). |
| Capture device busy (modem session live) | Clean error: "close your VARA/ARDOP session first." No forced device grab. |
| System clock badly off | Wider capture window (70 s) absorbs seconds of error; gross error → operator sees no-copy and can retry. (Future: self-sync to WWV minute tone; out of scope for v1.) |
| Model not yet provisioned | Prompt to download at setup, or point to the manual-place path; feature disabled until present, rest of app unaffected. |

## 9. UI surface

- Enhance the **existing station-finder conditions surface** (do not add a new
  window): `src/catalog/StationFinderControls.tsx` — the topbar actions cluster
  (`station-finder__actions`) has a code comment explicitly **reserving this row
  for the "Update propagation data" action** once it ships. Add a **"Refresh
  off-air"** button there (the internet update button does not exist yet either;
  ours is the first update control).
- Note: the conditions bar declares `sfi`/`kIndex` props but the parent never
  passes them today, so SFI/K currently never render — this feature also wires
  the off-air `SolarSnapshot` (SFI/A/K + provenance) into that readout (a new
  `invoke` reading the persisted snapshot; none exists today).
- Flow: click → "next WWV bulletin at HH:MM UTC (in N min) — arm capture?" →
  armed indicator → on completion, the conditions bar updates and stamps
  **"off-air WWV HH:18 UTC"** provenance next to the SFI/A/K readout.
- Low-SNR: a "couldn't copy — retry next cycle / play clip / enter manually"
  affordance. The clip player lets the operator verify by ear.
- Wire-walk (hard gate, §14) before any "shipped" claim.

## 10. Model acquisition summary

`base.en` q5_1 (~57 MB), fetched at first-run/setup to
`~/.local/share/tuxlink/models/`, verified by checksum, runtime off-air after.
Manual-place path documented for air-gapped installs. Not bundled in the
installer (footprint). If field-cold-install (no internet ever) becomes a
requirement, revisit bundling as an alternative.

## 11. Testing strategy

- **Pure units (no hardware):** `normalize_spoken_numbers` (transcript →
  digits), scheduler window math (injected `now_ms`), frequency-by-ToD
  selection, provenance-tag plumbing. `parse_wwv` / `derive_ssn_from_sfi` /
  `apply_rf_solar_reply` are already tested on `origin/main`.
- **STT box:** fixture WAVs — a clean synthesized WWV-style announcement and a
  noise-degraded variant — assert transcript → `parse_wwv` yields the expected
  `{sfi, a_index, k_index}`, and that the degraded/low-SNR fixture is rejected
  (empty) rather than hallucinated. Seed/validate the grammar against the real
  `wwv.txt` format (permitted first-run internet).
- **Orchestration:** mock `CaptureSource` + a fake rig to assert save → tune →
  release_serial → capture → restore ordering and the manual-tune fallback.
- **Integration (operator, real radio):** RX-only capture of a live WWV :18,
  end-to-end to a stamped conditions update. No transmission; no consent gate
  needed (§12). This is the wire-walk gate (§14).

## 12. Security / Part 97

**Receive-only.** This feature tunes the VFO and captures RX audio; it never
keys the transmitter. RADIO-1 / the live-transmission consent gate governs TX,
not RX, so it does not gate this feature. No new airtime, no VOX, no PTT path is
touched. `release_serial()` is used only to avoid the codec-reset hardware
quirk, not to key anything. STT runs locally on captured audio; no data leaves
the machine.

## 13. Out of scope (do NOT conflate)

- **Wideband-SDR multi-band beacon / MUF sensing** (WWV/CHU/broadcasters as
  always-on band-open beacons; highest-audible-freq ≈ MUF) — a separate,
  heavier, optional feature. Keeping it separate is what lets the primary-radio
  path drop the SDR requirement here. The `CaptureSource` seam (§6.7) is the
  only forward-hook.
- **WWV Doppler** (ionospheric motion / TIDs) — science derivative, not
  operator-actionable. Cut.
- **Chirpsounder / ionograms** — real but hardware-heavy, on the sounder's
  path. Cut.

## 14. Risks

- **WWV/WWVH continuation (external).** NIST has repeatedly floated defunding
  WWV/WWVH. The feature's value is contingent on continued broadcast; if the
  stations go dark the off-air source disappears (the internet path and Winlink
  `PROP_WWV` path are unaffected). Documented, not mitigable by us. The
  `CaptureSource` seam and the shared downstream mean an alternate off-air
  source (e.g., CHU) could be added later at low cost.
- **STT accuracy on noisy HF (the hard part).** Mitigated by the closed grammar
  (numbers + ~20 phrases, not open speech), grammar-constrained decode,
  noise-rejection thresholds, next-window retry, and the human-confirm clip.
- **SSN fidelity** — coarser than smoothed monthly SSN; accepted (§4), same as
  the existing RF path.

## 15. Reuse vs. new (build checklist)

**Reuse (origin/main, unchanged logic):** `tux-rig` (`ManagedRig::spawn/tune/
status/release_serial`, `Mode`, `RigConfig`) · `rig_config_from(&config.rig)`
adapter · `parse_wwv` · `derive_ssn_from_sfi` · `apply_rf_solar_reply`
(first caller; +1 provenance tag) · `arecord` shell-out · `hound` WAV · the
predict path's fresh `ssn-forecast.json` read.

**New (this feature builds it):** `tuxlink-stt` path-dep crate (whisper-rs,
base.en q5_1, `DecodeMode` — `set_initial_prompt` biasing, NO GBNF) ·
`wwv_offair` module (capture, capture_cycle orchestration with the
serial-sequencing branch, normalize, schedule) · `CaptureSource` trait (+
`PrimaryRigSource` impl) · `WwvOffairConfig` (additive `Option<>` on `Config`) ·
**the Tauri update command** (first command to call `apply_rf_solar_reply`;
no update command exists today) · **frontend** "Refresh off-air" button in the
reserved `station-finder__actions` row + snapshot `invoke` + conditions/
provenance readout + low-SNR confirm · model-acquisition (setup download +
manual-place).

## 16. Locked decisions

1. Engine: whisper-rs native in-process; **not** Geographica's service; **not**
   an LLM. Model `base.en` q5_1, data-dir, setup-download.
2. STT is a reusable crate (`tuxlink-stt`) with a decode-mode param — Elmer is a
   future consumer.
3. Transport-only change: reuse the entire shipped WWV→SSN→VOACAP chain via
   `apply_rf_solar_reply`; add provenance tag `"rf-wwv-voice"`.
4. Trigger: nearest-window (WWV :18 / WWVH :45), non-blocking one-shot arm.
5. Capture: `arecord` → 16 kHz mono WAV. Rig ordering: save → tune →
   release_serial → capture → restore. Manual-tune fallback when CAT absent.
6. RX-only; no TX; RADIO-1 does not gate.
7. Optional SDR is a designed seam (`CaptureSource`), not built here.
