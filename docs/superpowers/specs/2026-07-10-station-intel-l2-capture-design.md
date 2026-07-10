# Station Intelligence L2 — live audio capture + slot-timing decode service

Status: v1 — DRAFT for adversarial review. Session: esker-sorrel-redwood.
Issue: tuxlink-b026z.3 (epic tuxlink-b026z). Consumes: tuxlink-jt9 (L1, shipped
PR #1070). Resolves: tuxlink-gujnz (salvage-on-signal), dispositions
tuxlink-b026z.8 (grandchild pipe-holder leak bound).
Canonical design authority: `docs/design/2026-07-10-station-intel-jt9-engine-delta.md`
(the delta) — this spec implements its L2 seam section and records the
implementation decisions the delta left open. Where this spec amends the delta
(taxonomy, state axis), the PR carries matching delta v3 notes; the delta stays
canonical.

Operator decisions recorded 2026-07-10 (esker-sorrel-redwood session):

1. Station Intelligence is the front door. The listener must work with ZERO
   prior modem/radio configuration; deriving the capture device from modem
   config is rejected (discovery precedes modem commitment).
2. No device auto-selection, ever. Reference hardware (FT-710 + DRA-100)
   presents at least two USB codecs and only the operator knows which is the
   radio's audio path. First start always asks; the choice persists.
3. The remaining defaults (salvage-on-signal, dwell, sweep list, poll-resume,
   timedatectl probe, alsa-crate-not-cpal, leaf-crate split, b026z.8
   accept-plus-observe) were reviewed with the operator and stand.

## Scope

L2 delivers the persistent backend listening service: ALSA capture off the
operator-selected USB codec, 48 kHz → 12 kHz decimation, wall-clock-true 15 s
UTC slot assembly, slot WAV writeout to tmpfs, decode via the L1 `Jt9Runner`,
the full service state machine with health counters, the decode ring, Tauri
events + snapshot/control commands, modem yield/resume arbitration, and
opt-in CAT band sweep. Decoding runs independent of any window.

Non-goals (delta §Non-goals plus L2-specific): no UI surface (L3), no MCP
tools (L4), no heat map (L5), no PipeWire host, no 44.1 kHz resampler path,
no TX of any kind, no persistent jt9 process, no bundling of wsjtx, no
revival of the clean-room decoder, no full radio-provisioning unification
(tuxlink-0nfe2 — L2's device identity is merely compatible with it).

## User flow (primary framing — the design is derived from this)

### First contact, nothing configured

1. Operator plugs the rig/interface in, opens Station Intelligence, turns the
   listener on.
2. Service enters `blocked(needs-device-selection)`: the snapshot carries the
   enumerated capture-capable devices by human name. The L3 panel renders a
   picker. **Always asks, even with one device present** (operator decision 2).
3. Operator picks once. The choice persists as a `StableAudioId`. Every later
   start is zero-question.
4. No CAT configured → health flag `cat-fixed-band`. The band chip the
   operator selects is a STATEMENT ("the radio is on 20 m"), and the snapshot
   carries the dial frequency the panel should instruct
   ("tune your dial to 14.074"). The service never claims a band it cannot
   know; the label is the operator's assertion.
5. Decodes flow. Slot phase moves `waiting-first-slot → decoded | band-dead`.

### Configured radio

- `Config.rig` present → CAT available. The band chip becomes a QSY command:
  selecting a band tunes the radio to that band's FT8 dial (USB mode) via the
  existing spawn-tune-drop `ManagedRig` pattern (serial is never held while
  capturing — the FT-710 C-Media reset class of contention).
- At listener start with CAT: one `ManagedRig` session reads the current dial
  to label the starting band; if the configured band's dial differs, tune to
  it (starting the listener is the consenting action; RX-only). Then drop the
  rig session (serial released).
- Sweep (opt-in): round-robin over the configured band list with a fixed
  dwell. Detailed in §Sweep.
- Manual retune mid-listen makes the band label stale until the next
  QSY/resume/start. Accepted v1 limitation; recorded in slot provenance as
  `band_label_confirmed_utc_ms`.

### Later modem configuration

The listener's proven device choice becomes the default suggestion when the
operator later configures ARDOP/packet audio (forward derivation). Wiring
that suggestion into the modem panels is out of L2 scope; L2 just persists
the identity in a shape (`StableAudioId`) those flows can read.

## Architecture

### Crate placement (follows the L1 convention: Pi-testable leaf + main-crate wiring)

**New std-only leaf workspace crate `tuxlink-capture`** — pure logic,
compiles and TDDs on the dev Pi in seconds:

- `decimator.rs` — 48 k → 12 k polyphase FIR (§Decimator).
- `slot.rs` — wall-clock-true slot assembler (§Slot assembly).
- `wavwrite.rs` — canonical slot-WAV writer (§WAV).
- `state.rs` — listener state machine + N/k counters (§State machine).
- `bands.rs` — FT8 band → dial-frequency table (§Bands).

Dev-dependency on `tuxlink-jt9` (also std-only) so the writer↔preflight
round-trip is a unit test.

**Main-crate module `src/ft8/`** — everything that touches ALSA, tokio,
Tauri, tux-rig, or process lifecycle:

- `mod.rs` — `Ft8ListenerState` managed state (AprsState pattern:
  `Mutex<Option<Handle>>` + `Arc<AtomicBool>` abort + snapshot).
- `alsa_source.rs` — the one ALSA touchpoint; implements `SampleSource`.
- `service.rs` — capture thread + decode thread + supervisor tick.
- `arbiter.rs` — modem yield/resume (§Arbitration).
- `sweep.rs` — dwell scheduler + QSY via tux-rig.
- `clock.rs` — `ClockProbe` trait + `timedatectl` impl.
- `commands.rs` — Tauri commands.
- `events.rs` — `EventSink` impl (AprsState precedent) + event names.

### Testability traits (all injected; production impls are thin)

- `SampleSource`: `fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError>`
  where `ReadBatch` carries frames read plus an xrun/gap report;
  `SourceError` distinguishes `Busy | Absent | UnsupportedFormat | Io(String)`.
- `ClockProbe`: `fn ntp_synchronized(&self) -> ClockSync` with
  `ClockSync::{Synced, Unsynced, Unknown}`.
- `EventSink`: `emit_listening_change(..)`, `emit_slot(..)` (mirrors
  `aprs/engine.rs:186`).
- `DecodeEngine`: wraps `Jt9Runner` (`prewarm()`, `decode_slot(..)`) so
  service tests inject fakes; the production impl delegates 1:1.

## Capture pipeline

### ALSA open (decision: `alsa` crate, not cpal)

The `alsa` crate (alsa-rs) is used directly. Rationale over cpal: explicit hw
params control; errno-level discrimination at open (`EBUSY` → yielded,
`ENOENT`/`ENODEV` → device-absent, param rejection → unsupported-sample-rate);
xrun surfaced as `EPIPE` from `readi` with recoverable `snd_pcm_recover`
semantics for frame accounting; deterministic device release on drop (the
yield handshake depends on it). cpal hides all four behind a callback
abstraction and brings no benefit since the delta already pins Linux/ALSA
only. New workspace dependency: `alsa` (links system libasound; CI gains
`libasound2-dev`, §CI).

Open parameters, in order of attempt:

- Device: the resolved `plughw:CARD=<id>,DEV=0` name from the persisted
  `StableAudioId` (resolution reuses `resolve_managed_device`,
  `src/winlink/ax25/devices.rs:384` region). Direct ALSA device only — never
  a PipeWire compat PCM (delta pin: node-suspend timeout races the modems'
  busy probe).
- Format: S16_LE, rate **exactly 48000** (no `plug` resampling — set params
  on the hw device; rate must be native), channels 1; if 1 rejected,
  channels 2 with channel-0 extraction in the source impl. Any other
  rejection → `blocked(unsupported-sample-rate)` carrying the ALSA
  diagnostic (the axis name is delta-pinned; the diagnostic string
  distinguishes rate vs channel vs format for the operator).
- Period: 4800 frames (100 ms) — bounds abort-check latency; buffer 4 periods.

### Decimator (48 k → 12 k, 4:1 polyphase FIR)

Per the delta pin: passband 0–4 kHz (jt9 decodes to 4007 Hz), stopband
≥ 8 kHz at ≥ 60 dB, Kaiser window, ~45 taps, polyphase evaluated at the
output rate (~0.5 M MAC/s). Implementation decisions:

- Coefficients are a **committed const table** (f32) with a committed
  generator note; a unit test numerically verifies the response (passband
  ripple ≤ ±0.5 dB across 0–3.8 kHz, ≥ 60 dB attenuation at 8–24 kHz sampled
  points) so the table cannot silently rot.
- i16 in → i16 out; accumulate in f32, round-half-away, saturate to i16.
- KATs: 9 kHz tone ≥ 60 dB down post-decimation (the delta's named vector);
  1 kHz tone passband level within 0.5 dB; DC and impulse sanity; streaming
  equivalence (chunked calls == one-shot call for identical input).

### Slot assembly (the wall-clock-true invariant, delta-pinned as a design invariant)

- The assembler is anchored per-slot to the UTC boundary: at each 15 s
  boundary (UTC 0/15/30/45) it computes `slot_start_utc_ms` and expects
  exactly **720,000 input frames** (48 k) for the slot, producing exactly
  **180,000 output frames** (12 k).
- An expected-frame counter runs against the slot-start anchor at the input
  rate. Any discontinuity (xrun gap, device re-open gap) is **zero-filled in
  place at the input side** before decimation, sized by the gap the source
  reports (frames the wall clock says should exist minus frames delivered).
  Empirical basis (delta): 0.25 s time-shift = 0 decodes; 0.25 s zero-filled
  = 13/14.
- Per-slot re-anchoring absorbs soundcard-vs-wall-clock drift (≤ ~50 ppm ≈
  36 frames/slot); overflow frames at a boundary carry into the next slot,
  shortfall is zero-filled at boundary close.
- Provenance per slot: `lost_frames` (input-rate), `clip_fraction` and
  `rms_dbfs` computed on the raw i16 input pre-decimation (true ADC-side
  levels). **Drop the slot when `lost_frames` > 48,000 (1 s)** — counted as a
  dropped slot toward the degraded counter (types.rs contract: L2 drops fold
  into N).
- The partial first slot after start/resume is discarded and is NOT counted
  (scheduled discard; `waiting-first-slot` covers it).
- Levels: i16 passthrough, never rescaled (delta pin: −30 dB and +18 dB clip
  both decode; no normalization). CM108-class mic AGC disable is documented
  at capture setup (docs task), not enforced in code.

### WAV writeout + tmpfs + wisdom

- Slot WAV: canonical 44-byte-header RIFF/WAVE, PCM16 mono 12 kHz,
  exactly 180,000 frames — the writer's output must pass
  `tuxlink_jt9::wav::preflight_slot_wav` (round-trip unit test).
- Slot dir: `$XDG_RUNTIME_DIR/tuxlink/ft8/slot-<slot_utc_ms>/` (tmpfs; the
  delta's ~2 GB/day must never hit the SD card). Fallback when
  `XDG_RUNTIME_DIR` is unset: `/run/user/<uid>` if writable, else
  `std::env::temp_dir()` + a startup log warning naming the SD-card risk.
  The slot dir is deleted after its decode returns (all outcomes).
- jt9 FFTW wisdom data dir (the runner's `-a`): persistent per audio source —
  `<app local data dir>/jt9-wisdom/<stable-id-slug>/`, created at service
  start. Survives restarts by design (wiped wisdom re-pays ~1.7 s planning
  per slot).
- `Jt9Runner::prewarm()` runs once per service process lifetime, during
  `starting`, before the slot loop (delta pin; failure → the start sequence
  maps the returned `SlotFailure` to a blocked/degraded outcome per
  §Start sequence).

## Service structure

### Threads (std threads, named; AprsState-pattern lifecycle)

- **capture thread** (`ft8-capture`): blocking `SampleSource::read` of 100 ms
  periods → gap accounting → decimate → slot assembler. On slot completion:
  write WAV, `try_send` the slot descriptor over a
  `std::sync::mpsc::sync_channel(1)`.
- **decode thread** (`ft8-decode`): `recv` slot descriptor → `DecodeEngine::
  decode_slot` (blocking, up to 12 s) → fold outcome into counters/state →
  push `SlotRecord` to ring → `emit_slot` → delete slot dir.
- **supervisor tick** (inside the service run-loop, every 5 s): resume poll
  while yielded (§Arbitration), clock re-probe every 20 slots (§Clock),
  fd-watermark observability every 100 slots (§b026z.8), sweep dwell
  bookkeeping (§Sweep).
- Abort: one `Arc<AtomicBool>`; capture thread observes it between period
  reads (≤ 100 ms), decode thread via a sentinel message after the in-flight
  decode returns (bounded by the 12 s timeout). `stop()` joins both with a
  15 s bound and force-detaches with a logged warning past it.

### Backpressure (types.rs contract)

One in-flight decode per audio source. `sync_channel(1)` + `try_send`; on
`Full`, the slot is dropped: WAV deleted immediately, counted toward N as a
non-Decoded outcome (`types.rs:37-39` pins that L2 drops fold into the
degraded counter), logged with the slot UTC. Never queue, never block the
capture thread on the channel.

### Decode budget fit

Slot ends at T; WAV write to tmpfs is sub-millisecond-class; decode bounded
by `SLOT_DECODE_TIMEOUT_SECS = 12`; next slot completes at T+15. The single
in-flight rule plus the 12 s bound guarantees the decode thread is idle when
slot N+1 lands except under overrun, which backpressure handles.

## State machine (implemented pure in `tuxlink-capture::state`)

Axes per the delta (three orthogonal axes), with one L2 addition:

- **Service axis:** `stopped → starting → listening`,
  `yielded(device-busy)`,
  `blocked(device-absent | needs-device-selection | wsjtx-absent |
  unsupported-sample-rate)`, `stopping`.
  **`needs-device-selection` is new** (operator decision 2): no persisted
  device identity exists — distinct from `device-absent` (a persisted
  identity no longer resolves). Delta v3 note required.
- **Health flags (orthogonal, coexist with `listening`):** `clock-unsynced`,
  `cat-fixed-band`, `jt9-degraded`.
- **Slot phase (within `listening`):** `waiting-first-slot → decoded |
  band-dead`.

### Counter semantics (N = 5, k = 20 from types.rs; L2 owns the counters)

- **N (jt9-degraded):** incremented by every `Failed(_)` outcome, every
  backpressure drop, and every lost-frames drop. Cleared by any `Decoded`
  (including salvaged/partial — data flowed) and by `BandDead`: per types.rs
  N counts consecutive non-`Decoded`/non-`BandDead` outcomes, and a clean
  zero-decode exit is a *good* slot, so `BandDead` clears N.
- **k (band-dead phase):** incremented by `BandDead`, reset by `Decoded`.
  `Failed(_)` slots neither increment nor reset k (a failure is not evidence
  of a quiet band). k resets on band change (QSY) and on resume.
- **Scheduled discards count toward NEITHER counter:** the partial first
  slot after start/resume, and the QSY transition slot (§Sweep). These are
  policy, not failures; folding them into N would degrade a healthy sweep.
  Delta v3 note: the types.rs sentence "a slot L2 drops without ever calling
  decode_slot still counts" is scoped to backpressure/lost-frames drops, not
  scheduled discards.
- Mid-run jt9 disappearance surfaces as `Failed(SpawnFailed(..))` outcomes
  (and a vanished slot WAV as `Failed(BadWav("not found"))` — the types.rs
  stable-string contract). Both increment N like any `Failed(_)`; the
  snapshot carries the most recent failure's diagnostic so L3/L4 can name
  the cause once N degrades.

## Device selection & persistence

- Config: `device: Option<StableAudioId>` (§Config). `None` →
  `blocked(needs-device-selection)`; snapshot embeds
  `available_devices: Vec<{human_name, stable_id}>` from the existing
  enumeration (`enumerate_audio_devices`, `devices.rs:323`).
- `ft8_set_device(stable_id)` persists the choice (atomic config write) and,
  if the service is in a blocked state, retriggers the start sequence.
- Persisted-but-unresolvable at start → `blocked(device-absent)` naming the
  stored identity; re-pick via the same command.
- No ranking, no recommendation flag, no auto-pick (operator decision 2).

## Bands & CAT

### Band table (`tuxlink-capture::bands`)

Pinned FT8 dial frequencies (Hz), USB: 160 m 1 840 000; 80 m 3 573 000;
40 m 7 074 000; 30 m 10 136 000; 20 m 14 074 000; 17 m 18 100 000;
15 m 21 074 000; 12 m 24 915 000; 10 m 28 074 000.

### Hold-band (default) behavior

- CAT present: at start, one `ManagedRig` session — read dial, label band
  (nearest table entry within ±3 kHz of a dial, else `unknown`), tune to the
  configured band's dial if it differs, drop the session. Serial is never
  held while capturing (FT-710 contention class).
- CAT absent: `cat-fixed-band` flag; band label = the operator's chip
  statement; snapshot carries the instructed dial for the panel to display.

### Sweep (opt-in round-robin; dwell decision recorded)

- Config: `sweep.enabled` (default false), `sweep.bands` (default
  80/40/20/15/10 m), `sweep.dwell_slots` (default **8** = 2 min/band;
  valid 4–40). Dwell was the base design's open question; 8 balances
  meaningful per-band sampling against rotation latency (5-band default
  rotation = 10 min).
- Requires CAT; sweep with `Config.rig` unset is a config validation error
  surfaced at command level, and the flag `cat-fixed-band` implies sweep
  inactive.
- Mechanics: at each dwell boundary (counted in *decoded-or-band-dead*
  slots, so failures do not silently shrink a dwell), immediately after a
  slot boundary: spawn-tune-drop to the next band's dial. The slot in
  progress during the QSY is the **transition slot: discarded, scheduled,
  counted toward neither counter**. QSY failure: stay on the confirmed band,
  log warn, retry at the next dwell boundary; two consecutive QSY failures
  clear `sweep-active` back to hold (surfaced in snapshot; not a service-axis
  change).
- Sweep pauses while `yielded` and re-anchors its dwell on resume.
- RX-only; QSY never transmits. Sweep opt-in is the consent to move the dial
  (Part-97 TX concerns that removed auto-QSY-on-fail from the connect UI do
  not apply to a receive-only retune, and the opt-in covers the
  operator-surprise concern).

## Arbitration (yield/resume)

### Yield — synchronous pre-spawn hook (delta pin), two seams

`Ft8Arbiter::pause_for_modem()` is called from both modem-spawn seams:

- ardopcf: `dial_one_candidate` (`src/modem_commands.rs:691` region), before
  the `make_transport` spawn.
- managed Dire Wolf: `spawn_inner` (`managed_direwolf.rs:292`), before its
  Step-2 busy probe (otherwise that probe would abort on FT8's own hold).

Sequence: set `yielded(device-busy)` → signal capture stop → join capture
thread (bounded 2 s; ALSA PCM closed on drop) →
`ManagedModem::confirm_audio_device_released(pcm_device_path, ..)`
(`process.rs:286`) against `/dev/snd/pcmC<card>D<dev>c` → return. The hook is
a no-op when the listener is not `listening`.

### Resume — supervisor poll (no teardown wiring)

While `yielded`, the 5 s supervisor tick resumes capture when BOTH hold:

1. `probe_device_busy(plughw, card_index)` (`direwolf_probe.rs:345`) reports
   free (proc-status read, no device open), AND
2. the modem session is not active (ModemSession snapshot state is
   Stopped/None).

Rationale: poll-resume needs no hooks in any modem teardown path and
uniformly covers **VARA**, which tuxlink never spawns (external TCP peer
holding its own device): FT8 start or resume while VARA runs sees the card
busy → stays `yielded` until VARA exits. The base design's TBD ("can FT8 and
a modem read the card concurrently?") is answered: **exclusive-only**; no
dsnoop sharing in v1.

FT8 start while a modem is active: the start sequence's busy probe lands in
`yielded(device-busy)` and auto-resumes later — starting the listener during
a Winlink session is safe and self-healing.

## Clock probe

- `ClockProbe` production impl: subprocess
  `timedatectl show -p NTPSynchronized --value` (bounded 2 s, kill on
  overrun), parsed `yes`/`no`. Reads systemd-timedated's view of the kernel
  sync flag — daemon-agnostic (chrony and systemd-timesyncd both drive it),
  no new crate dependency (zbus stays transitive).
- `Unknown` (binary missing / timeout / unparseable): the `clock-unsynced`
  flag is NOT set; a startup log line records that clock sync is unverifiable
  on this system. A false "unreliable decode" warning on every non-systemd
  system is worse than a missing warning on an exotic one.
- Cadence: probe at start, at resume, and every 20 slots (5 min) from the
  supervisor. Flag sets/clears accordingly; flag changes emit
  `ft8-listening:change`.

## tuxlink-gujnz resolution — salvage-on-signal parity (L1 one-arm change)

**Decision: salvage.** In `tuxlink-jt9`'s runner, a signal-death (or nonzero
clean exit) with ≥ 1 parsed decode line returns `Decoded` with every record
`partial = true` (sentinel-aware exactly like the timeout arm: salvage after
`<DecodeFinished>` yields complete records); zero parsed lines keeps
`Failed(Signal)`. `StderrEof` retains its priority over decode lines on the
clean-exit path: jt9's EOF-on-input abort happens before decode output in
practice, and a capture bug must never masquerade as decodes.

Rationale (recorded for the delta v3 note): jt9's dominant real failure mode
IS decode-stream-then-SIGSEGV (delta grounded fact: kill at t=1 s had already
delivered 10/14 lines); lines are printed only after jt9's internal CRC-14
accepts a candidate, and the strict `parse_stdout_line` grammar guards
corrupted output; the timeout path already trusts the identical stream;
discarding biases band intelligence against exactly the slots that prove the
band is alive. Counter effect: a salvaged slot is `Decoded` → clears N (a
crashing-but-producing jt9 is degraded in fact but productive in output;
crash frequency remains visible via the per-slot outcome log and ring
provenance).

Changes: one classification arm in `runner.rs`, invert
`signal_death_discards_prior_decodes_by_taxonomy` to
`signal_death_salvages_parsed_decodes` (+ zero-line signal death still
`Failed(Signal)` test), types.rs doc sentence, delta v3 taxonomy note.
tuxlink-gujnz closes with this PR.

## tuxlink-b026z.8 disposition — accept + observe

The residual bound (detached drain threads + pipe read-fds leak if a killed
or cleanly-exited jt9 left a pipe-holding grandchild) is accepted for v1:
jt9 does not fork in practice, and group-kill requires libc/nix in a crate
that is deliberately std-only. L2 adds observability instead of mechanism:
every 100 slots the supervisor counts `/proc/self/fd` entries; a count
exceeding the service-start baseline by > 64 logs a warning naming
tuxlink-b026z.8. The issue closes as "accepted bound + watermark
observability in the L2 supervisor"; a real observation reopens it with
data.

## Ring, events, commands

- **Ring:** bounded 240-slot `VecDeque` (1 h) of
  `SlotRecord { slot_utc_ms, band, dial_hz, outcome: SlotOutcomeKind,
  decodes: Vec<Ft8Decode>, lost_frames, clip_fraction, rms_dbfs,
  dwell_slot_index, band_label_confirmed_utc_ms, partial_salvage: bool }`
  (SessionLogState pattern, `session_log.rs:23`). Decode ring IS the L3/L4
  hydration source; slot phase is computed from ring recency (never resets
  to `waiting-first-slot` on panel reopen — delta pin).
- **Events** (delta-named): `ft8-decodes:slot` (one per slot, the
  `SlotRecord`), `ft8-listening:change` (service axis + health flags + slot
  phase + band/dial + sweep status). Emitted through `EventSink`; production
  sink uses `AppHandle::emit` fire-and-forget (modem:status precedent).
- **Commands:** `ft8_listener_start`, `ft8_listener_stop`,
  `ft8_listener_snapshot` (full state + ring tail + `available_devices` when
  selection is needed), `ft8_set_device(stable_id)`, `ft8_set_band(band)`
  (QSY when CAT, relabel when not; resets k), `ft8_set_sweep(enabled)`.
  Config-mutating commands persist via `write_config_atomic`.
- **Autostart:** `lib.rs` setup hook starts the service when
  `config.ft8.enabled && config.ft8.device.is_some()` (broadcast pattern
  precedent, `lib.rs:1205` region). `ft8_listener_start` sets
  `enabled = true`; `ft8_listener_stop` sets `enabled = false` (the toggle IS
  the persistence; the listener is a persistent service, not a panel
  lifetime).

## Config (`AprsConfig` pattern, serde defaults, no schema bump)

```rust
#[derive(Serialize, Deserialize, ...)]
#[serde(default)]
pub struct Ft8Config {
    pub enabled: bool,                    // default false
    pub device: Option<StableAudioId>,    // default None
    pub band: String,                     // default "20m" (preselected chip)
    pub sweep: Ft8SweepConfig,            // { enabled: false,
                                          //   bands: ["80m","40m","20m","15m","10m"],
                                          //   dwell_slots: 8 }
}
```

Added to `Config` with `#[serde(default, skip_serializing_if = ...is_default)]`
(ElmerConfig precedent) — old config files stay byte-identical until first
FT8 use. `validate()`: band + sweep.bands ∈ table, dwell_slots ∈ 4..=40.

## Start sequence (pinned order; every arrow is a tested transition)

`starting` →
1. discover jt9 (`discover_jt9(config override)`) — absent →
   `blocked(wsjtx-absent)` naming the package.
2. resolve device: config `None` → `blocked(needs-device-selection)` (+
   enumeration in snapshot); unresolvable → `blocked(device-absent)`.
3. busy probe (`probe_device_busy`) — busy → `yielded(device-busy)`
   (supervisor resumes later).
4. ALSA open — `EBUSY` → `yielded`; absent-class → `blocked(device-absent)`;
   param rejection → `blocked(unsupported-sample-rate)`.
5. clock probe → flag.
6. CAT presence (`Config.rig`) → flag `cat-fixed-band` if absent; else the
   one start rig session (§Hold-band).
7. wisdom dir create + `prewarm()` once per process — `Err(SlotFailure)`
   maps: spawn/not-found class → `blocked(wsjtx-absent)` (binary vanished
   between 1 and 7); anything else logs + proceeds (a failed prewarm costs
   the first slots ~1.7 s planning, it does not block listening).
8. spawn capture + decode threads → `listening` / `waiting-first-slot`.

`blocked(*)` states are terminal until a command mutates the input that
blocked them (set_device, config change) or `ft8_listener_start` retries;
`yielded` is self-healing via the supervisor.

## Testing strategy

- **Leaf (`tuxlink-capture`, Pi TDD):** decimator response + KATs;
  assembler gap/zero-fill/boundary/drop/carryover; writer→`preflight_slot_wav`
  round-trip; state machine — every axis transition, flag set/clear, counter
  rule in §Counter semantics pinned by a named test (including
  scheduled-discard exclusion and BandDead-clears-N).
- **L1 (`tuxlink-jt9`):** the gujnz arm — salvage test + zero-line-still-
  Failed test + sentinel-complete salvage test.
- **Main crate (`src/ft8/`, fakes for all four traits):** start sequence —
  one test per numbered arrow above; backpressure drop (slow fake engine +
  fast fake source → drop counted, WAV deleted); yield handshake (pause
  joins capture + confirms release); resume poll (busy→free with modem
  stopped); sweep dwell + transition-slot discard + QSY-failure fallback;
  snapshot correctness incl. `available_devices` in needs-device-selection;
  config round-trip + validation.
- **E2E (CI, real jt9, both arches):** upsample a committed 12 kHz SDR
  fixture to 48 kHz by 4× sample repetition (zero-order hold; the pipeline's
  own FIR removes the ZOH images at ≥ 8 kHz, and the fixture content is
  band-limited below 4 kHz by construction), run it through
  `SampleSource`-faked capture → assembler → WAV → real jt9 decode;
  assert ≥ 90 % of the fixture's reference decode count. This proves the
  decimator+assembler+writer chain never degrades a known-good signal.
- **On-air validation:** operator-run only (RADIO-1 posture; RX-only so no
  TX consent needed, but the rig/audio bring-up is the operator's). The
  feature gate for user-reachability remains L3/L4 (delta wire-walk note).

## CI / packaging

- Add `libasound2-dev` to the apt step of every workflow that compiles the
  main crate (both arches) and to the release build images; `alsa` crate
  enters the workspace lockfile (Cargo.lock regenerated — the project rule:
  never `--locked` masking).
- No packaging metadata change (wsjtx Recommends shipped with L1).
- `.7` grep-guard: unaffected (no new jt9 spawn sites; the literal `"jt9"`
  stays confined to `tuxlink-jt9`).

## Delta v3 notes carried by this PR

1. Taxonomy: salvage-on-signal parity (gujnz decision + rationale).
2. Service axis: `needs-device-selection` added; `device-absent` narrowed to
   "persisted identity unresolvable"; no-auto-pick pinned as a product rule.
3. Counter scoping: scheduled discards (first partial slot, QSY transition)
   excluded from N; the types.rs "drops fold into N" sentence scoped to
   backpressure/lost-frames drops.
4. Band-chip semantics under cat-absent: chip = operator statement +
   instructed dial, never a claim.
