# Station Intelligence L2 — live audio capture + slot-timing decode service

Status: v4 — IMPLEMENTED (tuxlink-b026z.3, plan
docs/superpowers/plans/2026-07-10-station-intel-l2-capture.md). Five
adversarial rounds applied 2026-07-10
(R1 audio/DSP/timing, R2 concurrency/lifecycle, R3 product/contract, R4
Codex cross-artifact/implementability, R5 fresh-eyes holistic; totals
16 P1 + 23 P2 + 20 P3, all dispositioned; raw reports local-only under
`dev/adversarial/`, consolidated dispositions at
`dev/adversarial/2026-07-10-station-intel-l2-r1-r3-consolidated.md` +
`…-r4-codex.md`; R5's lifecycle cluster resolved by the §Lifecycle
ownership consolidation). Session: esker-sorrel-redwood.
Issue: tuxlink-b026z.3 (epic tuxlink-b026z). Consumes: tuxlink-jt9 (L1,
shipped PR #1070). Resolves: tuxlink-gujnz (salvage-on-signal), dispositions
tuxlink-b026z.8 (grandchild pipe-holder leak bound).
Canonical design authority: `docs/design/2026-07-10-station-intel-jt9-engine-delta.md`
(the delta) — this spec implements its L2 seam section and records the
implementation decisions the delta left open. Where this spec amends the
delta or the types.rs contract, the PR carries the matching edits
(§Delta v3 notes, §types.rs edits); the delta stays canonical for design.

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

**Epic sequencing sanction (explicit):** L2 merges with no UI caller by
design — the commands exist for L3, autostart activates only after first use.
The epic (tuxlink-b026z) sanctions layer-wise landing: this PR closes
b026z.3 as a layer; the wire-walk gate runs when L3/L4 make FT8
user-reachable (delta wire-walk note; same sanction L1 shipped under).

Non-goals (delta §Non-goals plus L2-specific): no UI surface (L3), no MCP
tools (L4), no heat map (L5), no PipeWire host, no 44.1 kHz resampler path,
no TX of any kind, no persistent jt9 process, no bundling of wsjtx, no
revival of the clean-room decoder, no full radio-provisioning unification
(tuxlink-0nfe2 — L2's device identity is merely compatible with it), no
dsnoop/shared capture (exclusive-only; the base design's concurrency TBD is
answered).

## User flow (primary framing — the design is derived from this)

### First contact, nothing configured

1. Operator plugs the rig/interface in, opens Station Intelligence, turns the
   listener on.
2. Service lands in the first applicable blocked state. The common variant:
   `blocked(needs-device-selection)` — no device identity is persisted.
   **The snapshot carries `available_devices` whenever
   `config.ft8.device == None`, regardless of which blocked state won**, so
   the L3 picker renders (and `ft8_set_device` works) even when the service
   is simultaneously blocked on wsjtx. The wsjtx-absent variant of first
   contact (AppImage path, no Recommends metadata): operator sees both the
   package guidance and the device picker in one visit, fixes both without
   serialized round-trips.
3. The picker **always asks, even with one device present** (operator
   decision 2). Devices listed are filtered to capture-capable cards
   (a capture substream exists: `/proc/asound/card<N>/pcm*c`), by human name.
4. Operator picks once. The choice persists as a `StableAudioId`. Every later
   start is zero-question.
5. No CAT configured → health flag `cat-fixed-band`. The band chip the
   operator selects is a STATEMENT ("the radio is on 20 m"), and the snapshot
   carries the dial frequency the panel should instruct
   ("tune your dial to 14.074"). Band labels carry provenance
   (§Band provenance): until the operator clicks a chip or CAT confirms, the
   label is `default-unconfirmed` and downstream surfaces must render it as
   unconfirmed — the service never claims a band nobody asserted.
6. Decodes flow. Slot phase moves `waiting-first-slot → decoded | band-dead`.

### Configured radio

- `Config.rig` present → CAT available. The band chip becomes a QSY command:
  selecting a band tunes the radio to that band's FT8 dial (USB mode) via the
  existing spawn-tune-drop `ManagedRig` pattern (serial is never held while
  capturing — the FT-710 C-Media reset class of contention; the arbiter
  serializes ALL rig sessions it owns, §Arbitration).
- At listener start with CAT: one `ManagedRig` session reads the current dial
  to label the starting band (`cat-confirmed`); if the configured band's dial
  differs, tune to it (starting the listener is the consenting action;
  RX-only). Then drop the session (serial released).
- Sweep (opt-in): round-robin over the configured band list with a fixed
  dwell (§Sweep).
- Manual retune mid-listen makes the band label stale until the next
  QSY/resume/start; staleness is visible via `band_label_confirmed_utc_ms`.

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
- `state.rs` — listener state machine + N/k counters + sweep element
  (§State machine).
- `bands.rs` — FT8 band → dial-frequency table.

Dev-dependency on `tuxlink-jt9` (also std-only) so the writer↔preflight
round-trip is a unit test.

**Main-crate module `src/ft8/`** — everything that touches ALSA, tokio,
Tauri, tux-rig, or process lifecycle:

- `mod.rs` — `Ft8ListenerState` managed state.
- `alsa_source.rs` — the one ALSA touchpoint; implements `SampleSource`.
- `service.rs` — capture thread + decode thread + supervisor thread.
- `arbiter.rs` — modem yield/resume + rig-session serialization
  (§Arbitration).
- `sweep.rs` — dwell scheduler + QSY via tux-rig (driven by the arbiter).
- `clock.rs` — `ClockProbe` trait + `timedatectl` impl.
- `commands.rs` — Tauri commands.
- `events.rs` — `EventSink` impl (AprsState precedent) + event names.

### Testability traits (all injected; production impls are thin)

- `SampleSource`:
  `fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError>`.
  `ReadBatch { frames: usize, mono_ts: MonoTs, gap: Option<GapReport> }` —
  **time is data at this seam** (the assembler is pure; wall/monotonic time
  arrive as values, never read ambiently).
  `SourceError::{Busy, Absent, UnsupportedFormat, Suspended, Wedged,
  Io(String)}` — `Suspended` covers `-ESTRPIPE`; `Wedged` is the
  wait-timeout escalation (§ALSA read loop).
- `ClockProbe`: `fn ntp_synchronized(&self) -> ClockSync` with
  `ClockSync::{Synced, Unsynced, Unknown}`.
- `EventSink`: `emit_listening_change(..)`, `emit_slot(..)`.
- `DecodeEngine`: wraps `Jt9Runner` (`prewarm()`, `decode_slot(..)`);
  production impl delegates 1:1.

## Capture pipeline

### ALSA open (decision: `alsa` crate, not cpal; device string: `hw:`)

The `alsa` crate (alsa-rs) is used directly. Rationale over cpal: explicit hw
params control; errno-level discrimination at open; xruns and stream errors
surfaced as errno from `readi` for explicit handling; deterministic device
release on drop (the yield handshake depends on it). cpal hides all four and
brings no benefit since the delta pins Linux/ALSA only. New workspace
dependency: `alsa` (links system libasound; CI gains `libasound2-dev`, §CI).

**The device is opened as `hw:<card_index>,0` (numeric, live index) — NOT
`plughw:`, NOT `CARD=<id>`.** Two reasons. (1) The plug layer silently
satisfies any rate/format/channel request by converting (including the
linear resampling the delta bans), which would make
`blocked(unsupported-sample-rate)` unreachable and channel policy plug's
instead of ours; `hw:` makes parameter negotiation real. (2) `CARD=<id>`
names collide when two same-model USB codecs share a `card_id` (the existing
resolver's own duplicate-card fixture demonstrates this: it resolves a
specific card *index* yet returns an id-based name, `devices.rs:1147-1204`)
— on the FT-710+DRA-class multi-codec bench an id-based open can grab the
wrong card, defeating `StableAudioId`. The resolver therefore gains an
`alsa_hw: String` derived from the **freshly resolved `card_index`**
(`hw:<index>,0`), alongside the existing `alsa_plughw`
(`resolve_managed_device` region, `devices.rs:384`), with a named
duplicate-card-id resolution test.

Open parameters, negotiated on the hw device:

- Format S16_LE, rate **exactly 48000** (native only), channels 1 preferred;
  if 1 is rejected, channels 2 with channel-0 extraction inside
  `alsa_source` (deinterleave, keep left). Any other rejection →
  `blocked(unsupported-sample-rate)` carrying the ALSA diagnostic (the axis
  name is delta-pinned; the diagnostic distinguishes rate vs channel vs
  format). CM108-class codecs (Digirig/DRA) are natively S16_LE/48 k
  mono-or-stereo, so the happy path is unaffected.
- Period 4800 frames (100 ms), buffer 4 periods.

### ALSA read loop (bounded; never parks unboundedly)

`PCM::wait(200 ms)` + nonblocking `readi`, in a loop that checks the abort
flag every iteration (worst-case abort latency ≈ one wait timeout). Errno
handling:

- `-EPIPE` (overrun): `snd_pcm_recover`-equivalent prepare + restart.
  **EPIPE tells us THAT an overrun occurred, never how much was lost** —
  recover also discards unread ring contents. Gap size therefore comes from
  the monotonic expected-frame counter, evaluated at capture restart
  (§Slot assembly), never from ALSA.
- `-ESTRPIPE` (suspend): `SourceError::Suspended` → clock-anomaly discard
  path (§Slot assembly).
- `-ENODEV`/`-EBADFD`-class: `SourceError::Absent` → mid-run device-loss
  path (§Device loss).
- 10 consecutive wait-timeouts (2 s of a silent, non-erroring stream — the
  C-Media wedge class): `SourceError::Wedged` → treated as device loss.

### Decimator (48 k → 12 k, 4:1 polyphase FIR)

Delta pin: passband 0–4 kHz (jt9 decodes to 4007 Hz), stopband ≥ 8 kHz at
≥ 60 dB, Kaiser window, polyphase at the output rate. Implementation:

- **51 taps** (the Kaiser estimate for these edges is ~44; 51 buys real
  margin at the tested 8.0 kHz point instead of sitting on the estimate;
  ~0.66 M MAC/s — still trivial).
- Coefficients are a committed const f32 table with a committed generator
  note; a unit test numerically verifies the response so the table cannot
  rot: passband ripple ≤ ±0.5 dB across 0–3.8 kHz and ≤ ±1.0 dB across
  3.8–4.0 kHz (jt9's ceiling is 4007 Hz — the edge is verified, loosely);
  attenuation ≥ 60 dB at sampled points across 8.0–24 kHz, asserted
  explicitly AT 8.0 kHz.
- i16 in → i16 out; accumulate in f32, round-half-away, saturate.
- **Filter state persists across slot boundaries** (continuity model; both
  choices decode identically since 720,000 ≡ 0 mod 4, but the streaming-
  equivalence test needs one pinned answer). Group delay (25 input samples
  ≈ 520 µs at 51 taps) is a constant shift three orders of magnitude inside
  jt9's ±2 s DT tolerance — recorded as a verified non-issue.
- KATs: 9 kHz tone ≥ 60 dB down post-decimation (aliases to 3 kHz — the
  delta's named vector); 1 kHz passband level within 0.5 dB; DC and impulse
  sanity; streaming equivalence (chunked == one-shot) **including chunk
  lengths ≢ 0 (mod 4)** — gap fills are clock-sized and arbitrary-length, so
  input-phase tracking across odd chunks is load-bearing.

### Slot assembly (the wall-clock-true invariant; two clock domains, pinned)

The assembler is pure; it receives `(utc_now_ms, mono_now)` with every push
(threaded from `ReadBatch.mono_ts` + a UTC sample taken by the capture
loop). The two domains have disjoint jobs:

- **UTC (`SystemTime`)** labels slot identity only: sampled once at each
  boundary detection to stamp `slot_start_utc_ms` and to choose the next
  boundary (0/15/30/45 s, start within ±0.5 s — jt9 absorbs ±2 s DT).
- **Monotonic** drives everything inside a slot: a per-slot monotonic anchor
  is captured at the boundary; the expected-frame counter is
  `(mono_now − anchor) × 48000`. NTP steps and slews therefore cannot
  manufacture in-slot gaps.

Rules:

- **Gap fill (xrun):** on capture restart after `-EPIPE`, gap frames =
  expected − delivered (monotonic), zero-filled in place immediately after
  the last delivered frame. **Minimum fill threshold: 2400 frames (50 ms)**
  — deficits below it are scheduling jitter, not loss; filling them is pure
  signal damage. (Empirical basis for filling at all: 0.25 s time-shift = 0
  decodes; 0.25 s zero-filled = 13/14.)
- **Boundary close:** shortfall → zero-fill to exactly 720,000 input frames.
  **Surplus (fast soundcard) → DROPPED, never carried**: carryover would
  accumulate the card-vs-wall-clock skew without bound (+50 ppm ⇒ ~4.3 s/day
  ⇒ zero decodes after ~11 h — the delta's time-shift kill mechanism,
  self-inflicted). Dropped surplus is recorded as `boundary_skew_frames`
  provenance; at ≤ 50 ppm it lands in FT8's inter-slot guard interval and is
  harmless.
- **Clock-anomaly rule:** on `Suspended`, on any negative computed gap, on
  any single intra-slot gap > 1 s, or on UTC-vs-monotonic divergence > 1 s
  observed at a boundary (NTP step): **abandon the slot** — discard as a
  scheduled discard (class `clock-anomaly`, §Counter semantics), re-anchor
  at the next UTC boundary. This one rule uniformly covers suspend/resume,
  NTP step-forward/backward, and gross timing damage; a slot that survives
  it is trustworthy.
- Exactly 180,000 output frames per slot; per-slot re-anchor absorbs
  bounded drift.
- The partial first slot after start/resume is a scheduled discard
  (`waiting-first-slot` covers it).
- Provenance per slot: `lost_frames` (input-rate, filled), `boundary_skew_
  frames`, `clip_fraction` and `rms_dbfs` — both computed on the
  **post-extraction channel-0 i16 stream, delivered frames only**
  (denominator 720,000 − lost; zero-fill excluded so degraded slots don't
  read as quiet). **Drop the slot when `lost_frames` > 48,000 (1 s)** —
  counted toward N (types.rs contract: L2 drops fold into the degraded
  counter).
- Levels: i16 passthrough, never rescaled (delta pin). CM108-class mic AGC
  disable is documented at capture setup (docs task), not enforced in code.

### Device loss mid-run (pinned; the FT-710 C-Media reset class is routine)

`SourceError::Absent | Wedged` while `listening`: close the PCM, transition
`listening → blocked(device-absent)`, emit the change event. The supervisor
**retries device-absent every 5 s**: re-resolve the `StableAudioId` (the
card index can change on re-enumeration — never reuse a cached name),
re-probe, re-open; success re-enters `listening` with a scheduled first-slot
discard. `device-absent` is therefore self-healing (USB replug recovers the
listener without operator action); `needs-device-selection`, `wsjtx-absent`,
and `unsupported-sample-rate` remain command-gated (they need operator
input; retrying them is spin).

### Waterfall tap (delta pin: one path, three consumers)

The delta pins "Waterfall taps POST-resample at 12 kHz … One path, three
consumers (decoder, waterfall, ring), all downstream of the decimator"
(delta §L2 seam). L2 provides the tap; L3 provides the FFT/rendering:

- `WaterfallTap`: a bounded lossy ring (drop-oldest) of decimated 12 kHz
  i16 blocks, 1200 frames (100 ms) per block, capacity 32 blocks (3.2 s).
  The capture thread pushes every decimated block; when no subscriber has
  attached (L3 panel closed — the common state), pushes overwrite silently
  at zero cost. No FFT, no column cadence, no event traffic in L2 (the
  delta's stated-cadence waterfall channel and its budget are L3's exit
  gate); L2's contract is only "the 12 kHz sample stream is subscribable,
  bounded, and never backpressures capture."
- Named test: tap drops oldest under a stalled consumer; capture timing
  unaffected (fake source, assert no slot boundary slip).

### WAV writeout + tmpfs + wisdom

- Slot WAV: canonical 44-byte-header RIFF/WAVE, PCM16 mono 12 kHz, exactly
  180,000 frames — must pass `tuxlink_jt9::wav::preflight_slot_wav`
  (round-trip unit test).
- **Storage failure (tmpfs ENOSPC/permissions) is a defined outcome:** slot
  dir creation or WAV write failure → ring outcome
  `DroppedStorageError(diagnostic)`, counted toward N (a real failure, not a
  scheduled discard), `last_failure` set, best-effort dir cleanup, capture
  continues into the next slot. Named test: fake writer returning ENOSPC →
  outcome recorded, N incremented, no panic, no capture stall.
- Slot dir: `$XDG_RUNTIME_DIR/tuxlink/ft8/slot-<slot_utc_ms>-<seq>/` (tmpfs;
  ~2 GB/day must never hit the SD card). `<seq>` is a process-monotonic
  counter making dir names collision-proof under backward clock steps.
  Fallback when `XDG_RUNTIME_DIR` is unset: `/run/user/<uid>` if writable,
  else `std::env::temp_dir()` + a startup warning naming the SD-card risk.
- Slot-dir hygiene: deleted after decode returns (all outcomes) AND for
  every dropped/discarded slot at drop time; **at service start, stale
  `…/tuxlink/ft8/slot-*` dirs from crashed runs are swept**.
- jt9 FFTW wisdom data dir (the runner's `-a`): **one machine-wide dir**
  `<app local data dir>/jt9-wisdom/` — FFTW wisdom is keyed by FFT
  size/CPU, not by audio device; per-device dirs would re-pay planning per
  device for zero benefit.
- `Jt9Runner::prewarm()` runs **once per runner construction** (service
  start and any `set_device`-triggered restart — the runner is
  reconstructed, wisdom persists so re-prewarm is ~2.4 s warm), during
  `starting`, BEFORE the ALSA open (§Start sequence — prewarm must not run
  while holding the PCM).

## Service structure

### Threads (std threads, named; AprsState-pattern lifecycle)

- **capture thread** (`ft8-capture`): the ALSA read loop → gap accounting →
  decimate → slot assembler. On slot completion: write WAV, `try_send` the
  slot descriptor.
- **decode thread** (`ft8-decode`): `recv` slot descriptor →
  `DecodeEngine::decode_slot` (blocking; 12 s timeout + up to 2 s bounded
  drain ≈ 14 s worst case) → fold outcome into counters/state → push
  `SlotRecord` → `emit_slot` → delete slot dir.
- **supervisor thread** (`ft8-supervisor`): **the service's owner and the
  FIRST thread spawned** — `ft8_listener_start`/autostart spawns the
  supervisor; the supervisor executes the start sequence and spawns the two
  worker threads at its final step. It outlives every blocked state and
  ticks every 5 s. Duties: yielded-resume poll (§Arbitration),
  device-absent retry (§Device loss — possible precisely because the
  supervisor exists BEFORE the start sequence runs, so a start that blocks
  at device resolution still has a live retry owner), clock re-probe every
  20 slots, pipe-fd watermark every 100 slots (§b026z.8), sweep dwell
  bookkeeping + QSY execution (§Sweep), hold-latch TTL. Slot counts arrive
  via shared atomics incremented on the decode thread. (Neither worker
  thread can host these duties: capture is joined while yielded — precisely
  when the resume poll must run — and decode parks in `recv`.)
### Lifecycle ownership (single-owner rules; every axis transition has one writer)

| Action | Runs on | Threads alive after |
|---|---|---|
| start / autostart | `ft8_listener_start` handler (`spawn_blocking`) spawns the supervisor from `stopped` ONLY; **idempotent** — with a live supervisor it signals a sequence re-run instead (no runner reconstruction unless the device changed) | supervisor (+ capture + decode once `listening`) |
| stop | `ft8_listener_stop` handler (`spawn_blocking`) | none |
| pause (yield) | modem spawn path (`spawn_blocking` — pinned contract) | supervisor + decode (capture joined) |
| resume / device-absent retry | supervisor tick | supervisor + capture + decode |

- **Threads per state:** `stopped` — none (and `pause_for_modem` from
  `stopped` returns `Ok(())` immediately: no latch, no state change — a
  system that never enabled FT8 must never acquire phantom listener state).
  `blocked(*)` — supervisor only. `starting` — supervisor (executing the
  sequence; the PCM, if open, is held BY the supervisor until step 8 hands
  it to capture). `yielded` — supervisor + decode (parked in `recv`).
  `listening` — all three.
- **The decode thread and the channel survive yield and device loss.** The
  master `SyncSender` lives in `Ft8ListenerState` and is cloned into each
  capture thread; only `stop()` drops the master (decode's `recv` returns
  `Disconnected` — race-free; no stop sentinel exists in this design).
  **Resume and device-absent recovery re-run steps 1–7 and then spawn the
  capture thread only** (step 8′) — never a second decode thread or
  supervisor. (Including step 1 keeps jt9 discovery at "start + resume
  only", the delta's pinned probe timing.) Prewarm is skipped on resume
  (once per runner construction).
- **Stop protocol:** set stop-request (checked by the start sequence
  between every step, same points as the yield check) + abort → join
  capture if present (bounded 2 s via `is_finished()` poll; PCM closed on
  drop) → drop the master `Sender` → join decode if present (bounded 16 s,
  covering 14 s worst-case decode) → join supervisor (bounded 16 s — the
  supervisor may be inside an unabortable start step: prewarm is a blocking
  `decode_slot`; its tick sleep is abort-interruptible, `park_timeout`).
  Absent handles (already `take()`n by pause, or never spawned in a blocked
  state) are skipped, not errors. A join-bound overrun force-detaches with
  a warning AND transitions to **`blocked(capture-wedged)`** — a detached
  thread may still hold the PCM, so the state must say "this process can no
  longer arbitrate the card" rather than masquerade as self-healing
  `yielded`. Recovery from capture-wedged is app restart; the snapshot
  names it.
- **Axis writers:** pause writes `yielded`; the supervisor writes every
  other transition; the yield/stop request flags only tell the supervisor
  to abandon its sequence — they never write the axis themselves.
- **Supervisor cadences count slot BOUNDARIES (capture-side atomic), not
  decoded slots** — dropped/discarded slots never reach the decode thread,
  and the clock re-probe + pipe watermark must not freeze during exactly
  the degraded streaks they exist to observe.

### Lock discipline (pinned)

Thread handles live outside the state mutex and are `take()`n before any
join; the state mutex is leaf-level — never held across a join, an ALSA
call, a rig session, or an event emit; arbiter lock > state lock everywhere
both are taken. A named test drives `stop()` during an in-flight decode
(slow fake engine) and asserts completion without the force-detach path.

### Backpressure (types.rs contract; rendezvous, not queue-of-one)

One in-flight decode per audio source, enforced by **`sync_channel(0)`**
(rendezvous): `try_send` succeeds only when the decode thread is parked in
`recv` — genuinely idle. `sync_channel(1)` would let slot N+1 queue behind a
slow decode and only drop N+2, violating the delta pin "if slot N+1's WAV is
ready while slot N's decode is still alive, drop slot N+1; never queue." On
`try_send` failure the slot is dropped: dir deleted immediately, counted
toward N, ring-recorded (§Ring), logged with slot UTC. The named test
asserts **slot N+1 specifically** is the dropped slot.

### Decode budget fit

Slot ends at T; WAV write to tmpfs is sub-millisecond-class; decode ≤ 14 s
worst case (12 s timeout + 2 s bounded drain); next slot completes at T+15.
The rendezvous rule guarantees a busy decode causes a drop, never a late
decode of a stale slot.

## State machine (implemented pure in `tuxlink-capture::state`)

- **Service axis:** `stopped → starting → listening`,
  `yielded(device-busy)`,
  `blocked(device-absent | needs-device-selection | wsjtx-absent |
  unsupported-sample-rate | capture-wedged)`, `stopping`.
  New vs the delta: **`needs-device-selection`** (no persisted identity —
  distinct from `device-absent`, a persisted identity that no longer
  resolves) and **`capture-wedged`** (a force-detached thread may hold the
  PCM; arbitration is dead until app restart). Delta v3 notes required for
  both.
- **Health flags (orthogonal, coexist with `listening`):** `clock-unsynced`,
  `cat-fixed-band`, `jt9-degraded`.
- **Sweep element (named part of the machine, not a flag):**
  `Sweep::{Inactive, Active { band_idx, dwell_progress },
  FallbackHold { failures }}`. Runtime state only — `config.sweep.enabled`
  is never mutated by the machine; `FallbackHold` (entered after two
  consecutive QSY failures) re-arms to `Active` at the next start or resume.
  Delta v3 note (the delta's axis list lacks a sweep element).
- **Slot phase (within `listening`):** `waiting-first-slot → decoded |
  band-dead`.

### Counter semantics (N = 5, k = 20 from types.rs; L2 owns the counters)

- **N (jt9-degraded):** incremented by every `Failed(_)` outcome, every
  backpressure drop, every lost-frames drop, and every storage-error drop
  (§WAV). Cleared by any `Decoded`
  (including salvaged/partial — data flowed) and by `BandDead` (types.rs: N
  counts consecutive non-`Decoded`/non-`BandDead` outcomes; a clean
  zero-decode exit is a good slot).
- **k (band-dead phase):** incremented by `BandDead`, reset by `Decoded`.
  `Failed(_)` slots and all `Dropped*` outcomes neither increment nor reset
  k (neither failure nor a dropped slot is evidence about band quietness).
  k resets on band change (QSY) and on resume. Slot-phase-from-ring-recency
  treats `Dropped*`/`Discarded` records the same way: they are not evidence
  toward `decoded` or `band-dead`; the phase holds its last value.
- **Scheduled discards count toward NEITHER counter:** the partial first
  slot after start/resume, the QSY transition slot, and clock-anomaly
  abandonments. These are policy, not failures; folding them into N would
  degrade a healthy sweep (one transition slot per dwell) and punish every
  suspend/NTP-step recovery.
- **types.rs edit (in this PR, not just a delta note):** the pinned sentence
  at `types.rs:37-39` ("a slot L2 drops without ever calling decode_slot
  still counts as a non-Decoded outcome toward N") reads as covering
  scheduled discards. It is amended to: "…folds L2 backpressure,
  lost-frames, and storage-error drops … Scheduled discards (partial first
  slot after start/resume, QSY transition slot, clock-anomaly abandonment)
  count toward neither N nor k." (Storage-error drops are in this spec's
  own N definition above; the earlier revision of this quote omitted them —
  reconciled at T7 review.) types.rs is the cross-crate contract surface;
  the canonical statement lives there, the delta v3 note points at it.
- Mid-run jt9 disappearance surfaces as `Failed(SpawnFailed(..))` (and a
  vanished slot WAV as `Failed(BadWav("not found"))` — the stable-string
  contract). Both increment N; the snapshot carries the most recent
  failure's diagnostic so L3/L4 can name the cause once degraded.

## Device selection & persistence

- Config: `device: Option<StableAudioId>`. `None` →
  `blocked(needs-device-selection)`.
- `available_devices: Vec<{human_name, stable_id}>` is embedded in the
  snapshot **whenever `config.ft8.device == None` OR the service is
  `blocked(device-absent | needs-device-selection)`, regardless of the
  other axes** (§User flow — the picker must render while blocked on wsjtx,
  AND for the unplugged/replaced-device recovery path where a stale
  `Some(stable_id)` is persisted).
  Enumeration reuses `enumerate_audio_devices` (`devices.rs:323`) filtered
  to cards with a capture substream (`/proc/asound/card<N>/pcm*c` exists) —
  the existing filter is USB-presence only and would list playback-only
  cards.
- `ft8_set_device(stable_id)` persists (atomic config write under the ft8
  writer mutex, §Config) and, from any blocked state **except
  `capture-wedged`**, retriggers the start sequence. From `capture-wedged`,
  `set_device` and `ft8_listener_start` return a restart-required error: a
  detached thread may still hold the PCM, and starting a second capture
  path in a process that can no longer arbitrate the card is worse than
  refusing.
- Persisted-but-unresolvable at start → `blocked(device-absent)` naming the
  stored identity; self-healing via supervisor retry (§Device loss); re-pick
  always available.
- No ranking, no recommendation flag, no auto-pick (operator decision 2).

## Bands & CAT

### Band table (`tuxlink-capture::bands`)

Pinned FT8 dial frequencies (Hz), USB: 160 m 1 840 000; 80 m 3 573 000;
40 m 7 074 000; 30 m 10 136 000; 20 m 14 074 000; 17 m 18 100 000;
15 m 21 074 000; 12 m 24 915 000; 10 m 28 074 000.

### Band provenance (the "never claims a band it cannot know" invariant, made real)

`SlotRecord` and the snapshot carry
`band_source: cat-confirmed | operator-asserted | default-unconfirmed` plus
`band_label_confirmed_utc_ms: Option<u64>` (None until a CAT read or an
explicit operator chip click). The serde default `band: "20m"` is a
preselected chip, NOT an assertion: until confirmation, records are labeled
`default-unconfirmed` and L3/L4 must render them as such.

### Hold-band (default) behavior

- CAT present: at start, one arbiter-owned `ManagedRig` session — read dial,
  label band (nearest table entry within ±3 kHz, else `unknown`,
  `cat-confirmed`), tune to the configured band's dial if it differs, drop
  the session.
- CAT absent: `cat-fixed-band` flag; label = operator's chip statement
  (`operator-asserted`) or `default-unconfirmed`; snapshot carries the
  instructed dial.

### Sweep (opt-in round-robin; dwell decision recorded)

- Config: `sweep.enabled` (default false), `sweep.bands` (default
  80/40/20/15/10 m), `sweep.dwell_slots` (default **8** = 2 min/band; valid
  4–40). Dwell was the base design's open question; 8 balances meaningful
  per-band sampling against rotation latency (5-band default = 10 min).
- Requires CAT: `sweep.enabled` with `Config.rig` unset is a validation
  error at the command layer; `cat-fixed-band` implies `Sweep::Inactive`.
- Mechanics: dwell counted in decoded-or-band-dead slots (failures do not
  shrink a dwell; **under a persistent failure streak the dwell freezes —
  intended**: rotating a broken decode pipeline samples nothing, and
  `jt9-degraded` is the operator's signal). At each dwell boundary,
  immediately after a slot boundary, the **supervisor asks the arbiter** to
  run a spawn-tune-drop QSY to the next band's dial. The slot in progress
  during the QSY is the transition slot: a scheduled discard.
- QSY failure: log warn, retry at the next dwell boundary; two consecutive
  failures → `Sweep::FallbackHold` (surfaced in snapshot; config untouched;
  re-arms at next start/resume). **A failed QSY does NOT imply the radio
  stayed on the old band** — `ManagedRig::tune` sets frequency before mode
  (`tux-rig/src/managed.rs:92-96`), so a serial drop mid-tune can leave the
  dial moved with the error reported. After any QSY failure the band label
  downgrades: `band_source = default-unconfirmed`,
  `band_label_confirmed_utc_ms = None`, until the next successful CAT
  read/tune re-confirms. Named test: partial-tune failure → label
  downgraded, slots not attributed to the stale band.
- Sweep never fires while `yielded`, while a pause is in progress, or
  outside `listening`; dwell re-anchors on resume.
- RX-only; QSY never transmits. Sweep opt-in is the consent to move the
  dial (the Part-97 concern that removed auto-QSY-on-fail from the connect
  UI was about TX on unseen frequencies; a receive-only retune under
  explicit opt-in carries neither risk).

## Arbitration (yield/resume — positive hold token, single choke points)

Design principle (from adversarial round 2): resume decisions must not rest
on negative evidence alone (card-not-busy ∧ session-not-active sampled at
5 s); every yield **latches a hold** that the resume poll honors.

### The arbiter

`Ft8Arbiter` (main crate) owns: the pause/resume handshake, the hold latch,
and **all rig sessions the FT8 service creates** (start-labeling QSY, band
chip QSY, sweep QSY) — serializing them so a modem connect's pre-audio tune
can never overlap an FT8 rig session (the FT-710 dual-CAT-user contention
class).

### Yield — `pause_for_modem() -> Result<(), PauseError>`

Called from every modem path that will open the audio device
(**blocking-context-only contract** — all current call sites run under
`spawn_blocking`; the doc comment pins it):

- **ardopcf — a single choke wrapper `spawn_ardop_with_yield`** replaces the
  four reachable `make_transport` spawn sites (`dial_one_candidate`
  `modem_commands.rs:717`; legacy single-dial
  `modem_ardop_connect_post_consume_with_factory` `:475`; listen-only
  `start_modem_listen_only` `:822`; open-session `spawn_and_init_ardop_inner`
  `:918`). A test asserts no ardopcf spawn is reachable without it.
- **managed Dire Wolf** — `spawn_inner` (`managed_direwolf.rs:292`), before
  its Step-2 busy probe (which would otherwise abort on FT8's own hold).
- **VARA** — the tuxlink VARA session-open/connect command path (before the
  TCP connect that starts a session). The VARA open command is `async` and
  calls its inner synchronously (`vara/commands.rs` region) — **the pause
  call there is wrapped in `spawn_blocking`** to honor the
  blocking-context-only contract (a 2 s join + lsof poll must not park a
  tokio worker). This covers tuxlink-initiated VARA use. **Residual, disclosed:** VARA is an external process that may open
  its audio device at its own launch, before any tuxlink involvement; if the
  operator launches VARA while the listener holds the card, VARA's audio
  open fails with the error surfacing in VARA's UI, not tuxlink's. The
  listener must be stopped (or L3's pause affordance used) first. The
  delta's "the conflict is self-inflicted" premise is true for
  ardopcf/Dire Wolf only; this spec states the VARA exception rather than
  claiming uniform coverage.

Sequence (under the arbiter lock), by service axis:

- **`stopped`:** return `Ok(())` immediately — no latch, no state change
  (P1 of round 5: pause fires on EVERY modem spawn, including systems that
  never enabled FT8; those must acquire no phantom listener state).
- **`blocked(*)`:** latch the hold (the latch is a lazily-evaluated
  timestamp in the arbiter — it needs no supervisor to expire) but leave
  the blocked axis and reason untouched; return `Ok(())`.
- **`listening`:** cancel/await any in-flight rig session → latch → abort
  capture → join (bounded 2 s, `is_finished()` poll) → PCM closed by the
  capture thread's drop → write `yielded(device-busy)` (pause is this
  transition's single writer) →
  `confirm_audio_device_released(pcm_device_path, ..)` (`process.rs:286`)
  against `/dev/snd/pcmC<card>D<dev>c` → `Ok(())`.
- **`starting` (any sub-step):** cancel/await any in-flight rig session →
  latch → set the yield-request flag → write `yielded(device-busy)`. There
  is never a capture thread to join during `starting` (it is spawned only
  at step 8, which transitions to `listening`); if the supervisor is past
  step 7 it holds the PCM itself, and its between-step flag check **drops
  the PCM before abandoning the sequence** (the flag never writes the
  axis — pause already did). The trailing
  `confirm_audio_device_released` poll absorbs the milliseconds until that
  drop lands; steps 4–5 (the multi-second ones) hold no PCM, so pause
  never waits on them. → `Ok(())`.

`confirm_audio_device_released` timing out (something else still holds the
device path) returns `Err(PauseError::ReleaseTimeout)`; the modem seam
surfaces it as the same device-busy-class error as `CaptureWedged` and does
not proceed to a doomed spawn.

Join timeout (wedged capture thread — a hung USB device can park even the
wait-loop): transition to `blocked(capture-wedged)`, return
`Err(PauseError::CaptureWedged)`. The modem seam surfaces this as a clear
"audio device is wedged; restart Tuxlink" error and does NOT proceed into a
guaranteed-EBUSY spawn.

### Hold latch

Set by every successful pause. Cleared when the supervisor observes the card
transition to busy (the modem actually acquired it — positive evidence), or
after a **30 s TTL** (an aborted spawn must not wedge FT8). While latched,
the resume poll never fires. This closes the window where a resume could
steal the card between `pause_for_modem` returning and the modem's own
device open (the packet path never touches `ModemSession`, so session-state
alone gives it zero protection).

### Resume — supervisor poll (no teardown wiring)

While `yielded`, each 5 s tick resumes capture when ALL hold:

1. the hold latch is clear;
2. `probe_device_busy(plughw, card_index)` (`direwolf_probe.rs:345`) reads
   free;
3. the modem session is not active, defined **positively over the
   `ModemState` enum: resume-eligible states are `Stopped`, `Error`, and
   `SocketLost`** (a failed connect walk parks the session in `Error` with
   the card long since released — requiring `Stopped` would leave FT8
   yielded forever; `Idle` (listen-only, ardopcf holds the card) remains
   active).

Resume re-runs start steps 1–7 and spawns the capture thread only (step 8′,
§Lifecycle ownership — the decode thread and channel survive the yield;
prewarm is skipped, runner already constructed; step 1 keeps jt9 discovery
at the delta's pinned "start + resume only" timing). Device-absent recovery
uses the identical path. FT8 start while a modem is active lands in
`yielded` via its busy probe and auto-resumes later — safe and
self-healing.

## Clock probe

- `ClockProbe` production impl: subprocess
  `timedatectl show -p NTPSynchronized --value` (bounded 2 s, kill on
  overrun), parsed `yes`/`no`. This implements the delta's pinned
  `org.freedesktop.timedate1 NTPSynchronized` property via the timedatectl
  transport instead of a direct D-Bus client — same property, daemon-
  agnostic (chrony and timesyncd both drive it), no new crate dependency
  (zbus stays transitive); transport choice operator-reviewed (decision
  list item 3).
- `Unknown` (binary missing / timeout / unparseable): flag NOT set; a
  startup log records that clock sync is unverifiable. A false "decode
  unreliable" warning on every non-systemd system is worse than a missing
  warning on an exotic one.
- Cadence: at start, at resume, every 20 slots. Flag changes emit
  `ft8-listening:change`. (The in-slot timing model no longer depends on
  wall-clock stability — §Slot assembly — so the flag is purely operator
  information about decode-window alignment.)

## tuxlink-gujnz resolution — salvage-on-signal parity (L1 one-arm change)

**Decision: salvage.** In `tuxlink-jt9`'s runner, a signal-death (or nonzero
clean exit) with ≥ 1 parsed decode line returns `Decoded` with
**`partial = !saw_sentinel` — identical to the timeout arm** (a crash after
`<DecodeFinished>` yields complete records, `partial = false`); zero parsed
lines keeps `Failed(Signal)`. **Arm ordering pinned on ALL paths: the
`StderrEof` check runs BEFORE salvage** — signal-death + `EOF on input
file` + parsed lines is still `Failed(StderrEof)`; a capture bug must never
masquerade as decodes (theoretical on the signal path — EOF-on-input exits 0
empirically — but the ordering is pinned, not assumed).

To be unambiguous about current vs required behavior: **today's runner
returns `Failed(Signal)` unconditionally at `runner.rs:196-204` and
`Ft8Decode::partial`'s doc is timeout-only — the salvage arm, the EOF
ordering, and both doc edits are REQUIRED L1 changes implemented by this
PR**, not descriptions of existing behavior. Timeout-vs-signal tiebreak
under the change: verified safe — the timeout path returns before signal
classification is reachable (`runner.rs:181` vs `:194`), and post-salvage
both arms return `Decoded` for ≥ 1 line while zero-line outcomes stay
distinct (`Timeout` vs `Signal`).

Rationale (recorded for the delta v3 note): jt9's dominant real failure mode
IS decode-stream-then-SIGSEGV (kill at t=1 s had delivered 10/14 lines);
lines print only after jt9's internal CRC-14 accepts a candidate; the strict
parser guards corruption; the timeout path already trusts the identical
stream; discarding biases band intelligence against exactly the slots
proving the band alive. Counter effect: salvaged slots clear N; crash
frequency stays visible via outcome logs and ring provenance. Downstream
does NOT distinguish timeout-salvage from crash-salvage (stated; the
distinction buys nothing at L3/L4).

Changes (all in this PR): the runner classification arm; test inversions —
`signal_death_discards_prior_decodes_by_taxonomy` becomes
`signal_death_salvages_parsed_decodes` (its doc prose flips too), plus new
tests: zero-line signal death still `Failed(Signal)`,
signal-death-after-sentinel yields `partial = false`, EOF-beats-salvage on
the signal path; **`Ft8Decode::partial` doc comment** (currently "salvaged
from a timed-out run") reworded to "salvaged from an abnormally-terminated
run (timeout or signal/nonzero exit); false when the completeness sentinel
was seen"; delta v3 taxonomy note. tuxlink-gujnz closes with this PR.

## tuxlink-b026z.8 disposition — accept + observe

The residual bound (detached drain threads + pipe read-fds leak if a killed
or cleanly-exited jt9 left a pipe-holding grandchild) is accepted for v1:
jt9 does not fork in practice, and group-kill requires libc in a
deliberately std-only crate. L2 adds observability matched to the leak's
signature (2 pipe fds per event — a raw fd count would drown it in
Tauri/tokio fd noise): every 100 slots the supervisor counts **pipe-type
entries** in `/proc/self/fd` (readlink → `pipe:[...]`); a count exceeding
the service-start baseline by > 16 logs a warning naming tuxlink-b026z.8.
The issue closes as "accepted bound + pipe-fd watermark in the L2
supervisor"; a real observation reopens it with data.

## Ring, events, commands

### Ring

Bounded 240-slot `VecDeque` (1 h) of `SlotRecord` (SessionLogState pattern,
`session_log.rs:23`). **Every slot boundary yields a ring record — including
drops and discards** (L4's failure counters and honest recency need them):

```
SlotRecord {
  slot_utc_ms, band, dial_hz,
  band_source, band_label_confirmed_utc_ms,
  outcome: Decoded | BandDead | Failed(kind)
         | DroppedBackpressure | DroppedLostFrames
         | DroppedStorageError(diagnostic)
         | Discarded(first-slot | qsy-transition | clock-anomaly),
  decodes: Vec<Ft8Decode>,           // empty except Decoded
  partial_salvage: bool,             // = any(decode.partial)
  lost_frames, boundary_skew_frames, clip_fraction, rms_dbfs,
  dwell_slot_index: Option<u8>,
}
```

Slot phase is computed from ring recency (never resets to
`waiting-first-slot` on panel reopen — delta pin).

### Events (delta-named)

`ft8-decodes:slot` (one per slot, the `SlotRecord`) and
`ft8-listening:change` (snapshot summary: axis + flags + phase + band +
sweep). Emitted through `EventSink`; production sink `AppHandle::emit`
fire-and-forget (modem:status precedent).

### Snapshot (field-by-field — this is the L3/L4 contract; delta §L4 requires each)

```
Ft8Snapshot {
  service: ServiceAxis,                       // incl. blocked reason
  flags: { clock_unsynced, cat_fixed_band, jt9_degraded },
  slot_phase: WaitingFirstSlot | Decoded | BandDead,
  band: String, dial_hz: u64,
  band_source, band_label_confirmed_utc_ms,
  sweep: SweepStatus { mode: Inactive|Active|FallbackHold,
                       band_idx, dwell_progress },
  engine_version: Option<String>,             // Jt9Binary::engine_version
  n_consecutive: u8, k_consecutive: u8,       // live counter values
  last_slot_utc_ms: Option<u64>,
  last_failure: Option<String>,               // most recent diagnostic
  available_devices: Option<Vec<AudioDeviceChoice>>,
      // present when device==None OR blocked(device-absent |
      // needs-device-selection) — §Device selection is the one rule
  ring_tail: Vec<SlotRecord>,                 // bounded page
}
```

### Commands

`ft8_listener_start`, `ft8_listener_stop`, `ft8_listener_snapshot`,
`ft8_set_device(stable_id)`, `ft8_set_band(band)`, `ft8_set_sweep(enabled)`.

- `ft8_set_band`: validates against the band table BEFORE persisting
  (rejects out-of-table); while `listening` with CAT → QSY + relabel
  (`operator-asserted`/`cat-confirmed`) + reset k; while NOT `listening` →
  **persist-only, never touches the radio** (the consent framing: only a
  running listener the operator started moves the dial).
- All config-mutating ft8 commands serialize their read-modify-write through
  **one ft8 writer mutex** before `write_config_atomic` (atomic file replace
  does not make concurrent RMW cycles atomic; the existing config layer does
  not serialize writers).
- **Autostart:** setup hook starts the service when `config.ft8.enabled`
  alone — NOT gated on `device.is_some()`: a first-contact operator who
  enabled the listener and got interrupted mid-pick must find it in
  `blocked(needs-device-selection)` after restart (the state that resumes
  their flow), not silently `stopped`. `ft8_listener_start` sets
  `enabled = true`; `ft8_listener_stop` sets `enabled = false`.

## Config (`AprsConfig` pattern, serde defaults, no schema bump)

```rust
#[derive(Serialize, Deserialize, ...)]
#[serde(default)]
pub struct Ft8Config {
    pub enabled: bool,                    // default false
    pub device: Option<StableAudioId>,    // default None
    pub band: String,                     // default "20m" (preselected chip,
                                          //   NOT an assertion — §Band provenance)
    pub sweep: Ft8SweepConfig,            // { enabled: false,
                                          //   bands: ["80m","40m","20m","15m","10m"],
                                          //   dwell_slots: 8 }
}
```

Added to `Config` with `#[serde(default, skip_serializing_if = ...is_default)]`
(ElmerConfig precedent). `validate()`: band + sweep.bands ∈ table,
dwell_slots ∈ 4..=40, sweep.enabled ⇒ rig configured.

## Start sequence (pinned order; every arrow is a tested transition)

The sequence is executed BY the supervisor thread (spawned first by
`ft8_listener_start`/autostart — §Threads), so every blocked outcome below
has a live retry/tick owner. A yield-request from `pause_for_modem` is
checked between every step (§Arbitration).

`starting` →
1. discover jt9 (`discover_jt9(config override)`) — absent →
   `blocked(wsjtx-absent)` (snapshot still carries `available_devices` if
   device is unset — §Device selection).
2. resolve device: config `None` → `blocked(needs-device-selection)`;
   unresolvable → `blocked(device-absent)` (supervisor-retried).
3. clock probe → flag.
4. wisdom dir create + `prewarm()` (once per runner construction) —
   **before any PCM is held**; failure maps: spawn/not-found class →
   `blocked(wsjtx-absent)`; anything else logs + proceeds (a failed prewarm
   costs the first slots ~1.7 s planning; it does not block listening).
5. CAT presence (`Config.rig`) → `cat-fixed-band` flag if absent; else the
   one arbiter-owned start rig session (§Hold-band).
6. busy probe (`probe_device_busy`) — busy → `yielded(device-busy)`.
   **The hold latch is consulted here too: latched ⇒ treated as busy** (a
   fresh start command landing inside a pause-to-modem-open window must not
   steal the card the latch is protecting).
7. ALSA open (`hw:`) — `EBUSY` → `yielded`; absent-class →
   `blocked(device-absent)`; param rejection →
   `blocked(unsupported-sample-rate)`.
8. spawn capture + decode threads (the supervisor is already running — it
   is executing this sequence) → `listening` / `waiting-first-slot`.

Steps 4–5 (multi-second: prewarm, rig session) deliberately precede the PCM
open so the held-but-not-yieldable window shrinks to milliseconds — and the
pause hook covers `starting` past step 7 anyway (§Arbitration).

Blocked-state recovery matrix: `device-absent` — supervisor retry;
`needs-device-selection` / `wsjtx-absent` / `unsupported-sample-rate` —
command-gated (`set_device`, config change, or `ft8_listener_start` retry);
`capture-wedged` — app restart only. `yielded` — supervisor resume
(conditions in §Arbitration; resume re-runs steps 1–7 + the capture-only
spawn, skipping prewarm — §Lifecycle ownership is the canonical statement;
this line previously said "2–8", a leftover from before the R5 lifecycle
consolidation, caught at Gate E).

## Testing strategy

- **Leaf (`tuxlink-capture`, Pi TDD):** decimator response + KATs (incl.
  chunks ≢ 0 mod 4, state-across-slots pinning); assembler —
  gap fill + threshold, boundary shortfall fill, **surplus drop (fast-clock
  source, 1000 slots, slot-content-vs-UTC skew stays bounded < 1 period)**,
  clock-anomaly abandonment (negative gap, > 1 s gap, UTC-vs-mono
  divergence), lost-frames drop, carry-nothing invariant;
  writer→`preflight_slot_wav` round-trip; state machine — every axis
  transition, flag, sweep element transition (incl. FallbackHold entry +
  re-arm), and counter rule in §Counter semantics pinned by a named test.
- **L1 (`tuxlink-jt9`):** the gujnz arm — salvage, zero-line-still-Failed,
  after-sentinel `partial = false`, EOF-beats-salvage; doc-prose flips.
- **Main crate (`src/ft8/`, fakes for all four traits):** start sequence —
  one test per numbered arrow; backpressure (slow fake engine → **slot N+1
  specifically** dropped, dir deleted, N incremented, ring-recorded); yield
  handshake (pause joins capture, confirms release, latches hold; pause
  during `starting` converts to `yielded`; wedged join →
  `blocked(capture-wedged)` + `Err`); resume (all three conditions,
  including `Error`/`SocketLost` eligibility and hold-latch TTL); sweep
  (dwell counting, transition-slot discard, QSY-failure → FallbackHold,
  re-arm on resume, never-fires-while-yielded); device loss mid-run →
  `blocked(device-absent)` → supervisor recovery; `stop()` during in-flight
  decode (no force-detach); snapshot field completeness (every §Snapshot
  field asserted); config command validation + writer-mutex serialization;
  hold-latch positive clear on observed card-busy (not only the TTL path);
  pipe-fd watermark trip (fake /proc reader); autostart with
  `enabled = true, device = None` lands `blocked(needs-device-selection)`;
  pause from `stopped` is a stateless no-op; stop during `starting` (mid-
  prewarm fake) completes without capture-wedged.
- **E2E (CI, real jt9, both arches):** upsample a committed 12 kHz SDR
  fixture to 48 kHz by 4× sample repetition, feed through
  `SampleSource`-faked capture (synthetic time driving slot boundaries —
  time is injected data, so the fixture aligns exactly to one slot) →
  assembler → WAV → real jt9 decode; assert ≥ 90 % of the fixture's
  reference decode count. Validity argument (not "band-limited by
  construction", which is false for an SDR capture): ZOH images of ≤ 4 kHz
  content land ≥ 8 kHz (FIR stopband, ≥ 60 dB); 4–6 kHz baseband content
  stays above jt9's 4007 Hz ceiling; ZOH sinc droop at 4 kHz ≈ −0.10 dB —
  negligible.
- **On-air validation:** operator-run only (RADIO-1 posture; RX-only so no
  TX consent needed, but rig/audio bring-up is the operator's). The feature
  gate for user-reachability remains L3/L4 (delta wire-walk note).

## CI / packaging

- Add `libasound2-dev` to the apt step of every workflow that compiles the
  main crate (both arches) and to the release build images; `alsa` crate
  enters the workspace lockfile (Cargo.lock regenerated — never `--locked`
  masking).
- **Leaf-crate gates are explicit:** the workspace has
  `default-members = ["."]` (`src-tauri/Cargo.toml`), so a bare
  `cargo test` at the workspace root never runs `tuxlink-capture`'s tests.
  CI adds `cargo test -p tuxlink-capture` + `cargo clippy -p
  tuxlink-capture --all-targets -D warnings` alongside the existing
  `-p tuxlink-jt9` gates (mirroring how L1's crate is wired).
- No packaging metadata change (wsjtx Recommends shipped with L1).
- `.7` grep-guard: unaffected (no new jt9 spawn sites; the literal `"jt9"`
  stays confined to `tuxlink-jt9`).

## Contract edits carried by this PR (types.rs + delta v3)

types.rs (`tuxlink-jt9`):
1. `types.rs:37-39` counter-scoping sentence (§Counter semantics).
2. `Ft8Decode::partial` doc comment (§gujnz).
3. `SlotFailure::Signal` doc note: salvage-on-signal (≥ 1 parsed line →
   `Decoded`), zero-line only.

Delta v3 notes:
1. Taxonomy: salvage-on-signal parity (gujnz decision + rationale +
   sentinel semantics).
2. Service axis: `needs-device-selection` and `capture-wedged` added;
   `device-absent` narrowed to "persisted identity unresolvable" and made
   supervisor-retried; no-auto-pick pinned as a product rule.
3. Sweep element added to the state model (the delta's axes lack it).
4. Counter scoping: scheduled discards excluded (pointer to the types.rs
   canonical sentence).
5. Band-chip semantics under cat-absent: chip = operator statement +
   instructed dial + provenance; `default-unconfirmed` rendering duty.
6. Arbitration: VARA exception disclosed (the "self-inflicted conflict"
   premise holds for ardopcf/Dire Wolf only); hold-latch resume model
   supersedes the delta's bare "FT8 auto-resumes on modem shutdown".
