# Station Intelligence — decode-engine delta: managed jt9 replaces the clean-room decoder

Status: v2 — REVIEWED. Five adversarial rounds applied 2026-07-10 (subprocess
lifecycle, audio/DSP/slot-timing, product integration, Codex, licensing/
packaging/appsec; raw transcripts local-only under `dev/adversarial/`). All
empirical claims below were verified against the installed wsjtx 2.7.0+repack-1
`/usr/bin/jt9` on the dev Pi and the committed SDR fixtures.
Amends: the approved 2026-07-05 office-hours design
(`~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260705-034957-passive-ft8-listener.md`)
Trigger: L0 spike NO-GO + operator decision 2026-07-07 (B+C) — see
`dev/handoffs/2026-07-07-bison-delta-birch-ft8-l0-nogo-jt9-fallback.md`.
Epic: tuxlink-b026z. Rewrites L1 (tuxlink-b026z.2); amends the L2 seam, the L3
state list, L4 schemas, L5's layer engine, and the .7 guard.

## What changed and why

The design chose Approach B (clean-room pure-Rust decoder). The L0 spike
produced decisive contrary evidence (1/5 + 0/2 message parity vs jt9's
5/5 + 2/2 on the same captures, root cause diagnosed as L-effort-class
weak-signal time sync), and the operator's recorded decision was to fall back
to jt9, keep `src-tauri/tuxlink-ft8` as a tested reference artifact, and
revisit only if the dependency proves problematic.

The wedge survives: "agent-legible RF truth over MCP" depends on structured
in-process decode state, not on who demodulates. **Amended success criterion**
(supersedes the base design's "with no WSJT-X installed" line): the operator
gets the feature off the already-configured rig with wsjtx as a
package-manager-managed optional dependency; `wsjtx-absent` is a designed,
actionable state, not an error.

## Grounded facts (all verified on the dev Pi, 2026-07-10)

- `/usr/bin/jt9` ships in the Debian/RaspiOS `wsjtx` package; EmComm Tools also
  ships WSJT-X (ECT target safe). FT8 file-mode: `-8`, depth `-d N`, data dir
  `-a`, temp dir `-t`, T/R period `-p`, FFTW patience `-w`.
- **jt9 has no version interface** (`--version`/`-v` → "unrecognised option",
  exit 0). Version comes from the sibling `wsjtx_app_version -v` → `WSJT-X 2.7.0`
  (the `-v` is load-bearing; bare invocation prints nothing).
- **Failure mode is signal death, not exit codes:** missing input file and
  corrupt WAV both produce a Fortran runtime error on stderr followed by
  SIGSEGV (exit 139). Diagnostics are on **stderr**; stdout stays parseable.
- **jt9 ignores the WAV sample-rate header** (a 48 kHz-stamped file with 12 kHz
  PCM decodes identically) and **exits 0 with zero decodes on truncated input**
  (stderr `EOF on input file`) — capture-layer bugs are indistinguishable from
  a quiet band without host-side WAV validation.
- **File placement (split-path verified):** `-a` receives `jt9_wisdom.dat`
  (FFTW wisdom, written only on successful completion) + `timer.out`; `-t`
  receives `decoded.txt`. The stray `decoded.txt`/`timer.out` in the repo root
  are fallout from M3 oracle runs without `-a`/`-t`.
- **Timing (this Pi, `-8 -d 3 -w 1`):** cold (no wisdom) 4.2 s, warm 2.4 s
  (ordinary fixture); crowded fixture 4.7–6.2 s warm. `-w 4` ran > 60 s —
  patience must be pinned low. Decodes stream to stdout incrementally; a kill
  at t=1 s on the crowded fixture had already received 10 of 14 lines;
  `<DecodeFinished>` is the completeness sentinel.
- **Depth:** committed fixture refs were generated at default depth
  (`jt9 -8`, `-d 1` — 10 decodes on crowded); `-d 3` yields 14. The refs are
  depth-1 artifacts, not "the L0 oracle settings."
- **Output is locale-stable** (byte-identical under other locales) and the
  fixed-format line grammar (split on the first `~`) holds for live stdout.
  The leading timestamp column is `000000` for non-WSJT-X-named files —
  `slot_utc` must come from the host slot scheduler, never from jt9 output.
- **Level-agnostic:** −30 dB gain and +18 dB hard clip both decode ≈ full;
  −48 dB hits the quantization floor. No normalization needed.
- **Time-alignment tolerance:** full decode across effective DT −2.0…+2.0 s;
  cliff beyond ±2.5 s. Exact boundary alignment is not required (committed
  fixtures sit at DT ≈ −1.0 and decode fully).
- **Sample-loss sensitivity (the sharpest finding):** deleting 0.25 s
  mid-capture (time-shifting the tail) kills **100 %** of decodes in the slot;
  the same 0.25 s **zero-filled in place** preserves 13/14. Slot timelines
  must be wall-clock-true; gaps are zeros.
- The rig-audio reality: **no in-process audio capture exists anywhere in the
  codebase** (no cpal dependency; ardopcf/Dire Wolf/VARA own the device as
  external processes). cpal has no PipeWire host on Linux (ALSA + JACK only).
- Existing device-arbitration primitives: `probe_device_busy` runs before
  modem spawn (`managed_direwolf.rs:304`) and
  `ManagedModem::confirm_audio_device_released` (`process.rs:286`).

## Revised L1 — decode service on managed jt9 (tuxlink-b026z.2)

A backend service in the main `tuxlink` crate (`tuxlink-ft8` stays frozen as
reference; its line grammar is **lifted, not imported** — `parse_reference_log`
returns normalized message strings and discards exactly the SNR/DT/freq
metadata the product needs).

- **Input:** one 15 s slot of mono 16-bit PCM at 12 kHz from L2, written as a
  WAV of **exactly 180,000 frames** into a per-slot temp dir on tmpfs
  (`$XDG_RUNTIME_DIR`; ~2 GB/day of slot writes must never hit the SD card).
  WAV deleted after its decode returns.
- **Preflight (before spawn):** validate the slot WAV host-side — exists,
  readable, mono, 16-bit, 12000 Hz header, exactly 180,000 frames. Anything
  else is `jt9-failed{reason}`, never a spawn (jt9 cannot be trusted to
  reject bad input — it segfaults or silently under-decodes).
- **Invocation:** `jt9 -8 -d 3 -p 15 -w 1 -a <data> -t <slot-tmp> <slot.wav>`
  with `current_dir = <slot-tmp>`; stdout AND stderr captured.
  - `<data>` is a **persistent per-audio-source data dir** (survives slots and
    restarts) so FFTW wisdom accrues; wiped/killed runs never write wisdom, so
    a per-slot data dir would re-pay ~1.7 s planning forever (timeout
    death-spiral). At service start, **pre-warm** by decoding a bundled 15 s
    silence WAV to completion before the slot loop.
  - Depth is fixed `-d 3`, AP off; not user-tunable in v1. The committed
    fixture reference logs are **regenerated at `-8 -d 3 -w 1`** (README
    recipe updated) so tests anchor to the production flag set.
- **Process discipline (mechanism, not just signal):** own the
  `tokio::process::Child`, spawn with `kill_on_drop(true)`, timeout via
  `tokio::time::timeout(child.wait())` at **12 s**; on overrun
  `child.kill().await` (kill + reap in one step). Never `libc::kill` on a
  stored PID, never `pkill`. On timeout, **parse the partial stdout already
  received**: if decode lines exist without `<DecodeFinished>`, emit them
  flagged `partial: true`; discard only zero-output overruns.
- **Backpressure:** one in-flight jt9 per audio source. If slot N+1's WAV is
  ready while slot N's decode is still alive, **drop slot N+1** (count it,
  fold into the degraded counter). Never queue; the slot loop never blocks on
  a kill.
- **Failure taxonomy** (per-slot events feeding the `jt9-degraded` health
  flag, N consecutive → degraded, first good slot clears):
  `not_found` / `permission` (preflight), `signal` (died by signal — the
  common real mode, detected via `ExitStatus::signal()`, stderr captured for
  the log), `timeout`, `stderr-eof` (jt9's `EOF on input file` — a capture
  bug, NOT a quiet band), `parse_error` (slot-level only if zero lines parse).
  Unparseable individual lines are skipped and counted, never void a slot.
  Zero decodes with clean exit = `band-dead` input, not failure.
- **Output:** parsed records
  `{slot_utc (host scheduler), snr_db, dt_s, freq_hz, message, from_call?,
  to_call?, grid?, partial?}` via a new strict `Jt9DecodeLine` parser (KATs:
  live stdout lines, `<DecodeFinished>`, zero-decode, stderr interleave,
  malformed lines) plus an FT8 message-grammar field extractor (CQ / grid /
  report / RR73 — exists nowhere yet; explicit L1 work). Records feed the
  in-memory ring consumed by L3/L4.
- **Hashed-callsign regression (accepted, surfaced):** per-slot spawn is
  amnesiac — jt9's 12/22-bit hash table is in-memory only (verified: no hash
  state persists in `-a`), so `<...>` messages never resolve across slots the
  way WSJT-X (persistent jt9) or the M3 crate (T3.1a) could. Disposition:
  `from_call = None` for unresolved hashes; excluded from `ft8_who_can_i_hear`;
  rendered distinctly (e.g. "‹hashed›") in the decode rail; disclosed in the
  MCP tool descriptions. Two of four committed fixtures contain `<...>`
  traffic — this is common, not corner-case.
- **Discovery/probe:** config override > PATH probe for `jt9`; version from
  the sibling `wsjtx_app_version -v`, falling back to `"jt9 (version
  unknown)"`. Absence at service start → `blocked(wsjtx-absent)` naming the
  package; mid-run disappearance surfaces as consecutive `not_found` failures
  → `jt9-degraded` (probe timing: start + resume only).

## Service state machine (three orthogonal axes; supersedes the flat lists)

- **Service axis (mutually exclusive):** `stopped → starting → listening`,
  `yielded(device-busy)`, `blocked(device-absent | wsjtx-absent |
  unsupported-sample-rate)`, `stopping`. (The base design's `no-device` alias
  is dead; the name is `device-absent`.)
- **Health flags (orthogonal; coexist with `listening`):** `clock-unsynced`,
  `cat-fixed-band` (no CAT → single-band, no sweep), `jt9-degraded`
  (N consecutive failed/timed-out/dropped slots; N pinned in the plan; clears
  on first good slot).
- **Slot phase (within `listening` only):** `waiting-first-slot →
  decoded(n>0) | band-dead(k consecutive zero-decode slots; k pinned)`.

Every axis value gets exactly one UI treatment and one MCP representation.
`clock-unsynced + band-dead` is now legible (flag + phase, not two competing
states); `wsjtx-absent + decoding` is unrepresentable.

## L2 seam — corrected: a NEW capture subsystem, not an amendment

L2 is the codebase's **first in-process PCM capture** (greenfield; nothing
"gains a resample stage"). Decisions pinned:

- **Open path: direct ALSA** (not the PipeWire ALSA plugin, whose node-suspend
  timeout (~5 s) delays device release and races the modems' pre-spawn busy
  probe; cpal has no native PipeWire host anyway).
- **Rate: request 48000 Hz explicitly at stream open.** If the device won't do
  48 k → `blocked(unsupported-sample-rate)` (CM108-class codecs — Digirig/DRA
  — all support 48 k; a 147:40 polyphase for 44.1 k is out of v1). Never rely
  on ALSA `plug`-layer resampling (linear, no anti-alias).
- **Decimation 48 k → 12 k (4:1):** FIR spec — passband 0–4 kHz (jt9 decodes
  to 4007 Hz), stopband ≥ 8 kHz at ≥ 60 dB, Kaiser ≈ 45 taps, polyphase at the
  output rate (~0.5 M MAC/s — trivial). Test vector: 9 kHz tone ≥ 60 dB down
  post-decimation.
- **Slot invariant (design invariant, not a plan detail):** the slot WAV
  timeline is wall-clock-true. Keep an expected-frame counter anchored to the
  slot-start timestamp; zero-fill any stream discontinuity (xrun) in place;
  every slot WAV is exactly 180,000 frames. Annotate `lost_frames` in slot
  provenance; drop the slot above 1 s lost. (Empirical basis: 0.25 s
  time-shift = 0 decodes; 0.25 s zero-filled = 13/14.)
- **Slot start:** within ±0.5 s of the UTC 0/15/30/45 boundary (jt9 absorbs
  ±2 s DT comfortably); the partial first slot is discarded
  (`waiting-first-slot` covers it).
- **Levels:** pass i16 through unscaled; log clip-fraction + RMS per slot;
  document disabling CM108 mic AGC at capture setup.
- **Clock probe:** daemon-agnostic — `org.freedesktop.timedate1
  NTPSynchronized` (set by chrony AND systemd-timesyncd; stock RaspiOS ships
  timesyncd, not chrony), behind an injected trait so `clock-unsynced` is
  unit-testable.
- **Yield is a synchronous pre-spawn hook, not a busy-error reaction:** the
  conflict is self-inflicted (Tuxlink spawns the modems). Modem-spawn sequence
  becomes: stop FT8 capture → `confirm_audio_device_released` (existing
  primitive) → spawn modem; FT8 auto-resumes on modem shutdown. Without this,
  Dire Wolf's own pre-spawn `probe_device_busy` would abort modem start
  because FT8 holds the card.
- **Waterfall taps POST-resample at 12 kHz** (not pre-resample as v1 said):
  display band is 0–3 kHz; a 12 k tap gives the same display for a 4× smaller
  FFT and inherits the FIR's cleanup. One path, three consumers (decoder,
  waterfall, ring), all downstream of the decimator.

## L4 — MCP surface (naming + provenance corrected)

- **Tools (snake_case — all 59 existing tools are; dots appear nowhere):**
  `ft8_band_intel`, `ft8_decodes`, `ft8_who_can_i_hear`, plus **`ft8_status`**
  (service axis + health flags + slot phase, current band + dwell policy,
  jt9 version, failure counters, last-slot UTC). Classified with the existing
  "Station intelligence (inert reads; no taint, no gate)" block.
- **Every result embeds `service_state` + `as_of_utc`** in addition to the
  per-datum recency + dwell + `engine` annotations. Non-listening states
  return the stale ring WITH the explicit state (never an error, never
  silently-stale data).
- **Description text is the schema surface** (this codebase exposes input
  schemas only): each tool's description names `captured_at_utc` / `dwell_s` /
  `engine`, states "data reflects only the current dwell," and discloses the
  hashed-callsign limitation.
- **Grid provenance:** many FT8 messages carry no grid (reports, RR73).
  Station aggregation carries `grid_source` (this-decode | prior-decode | none)
  + TTL; no map placement or distance for grid-unknown stations. The override
  invariant stands: the jt9 binary override is config-file/settings-UI only and
  **must never appear in an MCP write DTO**.

## L3 — panel integration (corrections to the base wiring)

- **Rail:** tabs `Station` (current detail; default; auto-activated on marker
  select) | `Live decodes`. The base design's "gateway list is one tab"
  mischaracterized StationRail (it is the selected-station detail pane that
  *replaced* the old list); the marker-click → Station tab → Use→ QSY flow is
  the unbroken primary flow.
- **Waterfall strip budget:** collapsed by default (~28 px title bar);
  expanded height fixed 160 px, subtracted from the body's 540 px minimum at
  the same breakpoints as the FZ-M1 compact rules; compact mode is
  collapse-only.
- **Events:** one `ft8-decodes:slot` Tauri event per slot carrying the decode
  array (jt9 makes batching free — decodes arrive at process exit); waterfall
  columns on their own stated-cadence channel (L3 exit-gate budget stands);
  panel open/reopen hydrates from a snapshot command reading the backend ring
  (slot phase computed from ring recency — never resets to
  `waiting-first-slot` on reopen). Precedent: the per-event APRS emit caused
  the "drunk map" CPU storm; the snapshot-then-pull pattern exists in
  `logging/env_probes`.
- **Ribbon badge:** shell-level `ft8-listening:change` event + a
  dependency-light hook in `src/shell/` importing nothing from `src/catalog/`
  (the badge is in the cold-start bundle; the panel is lazy). Four visual
  states: listening / off / **yielded (device-busy)** / blocked.
- **Band chips:** openness dots render on both selected AND unselected chips
  (an off chip going hot is exactly the signal that matters).
- **Menu/ids:** rename label to "Station Intelligence…" but **keep the
  compat-frozen id `menu:tools:find_gateway`** (accelerators key on it);
  sweep or explicitly retain the `catalogBuilderOpen` state name.
- New render state joining the L3 list: `wsjtx-absent` (actionable: names the
  package), plus the axis model above replaces the flat state list.

## L5 — heat layer engine (base design's API does not exist here)

`L.heatLayer` is the leaflet.heat plugin — not installed, unmaintained,
canvas-based (untestable under the project's jsdom + SVG-renderer discipline).
**Engine decision: a hand-rolled Maidenhead-cell density layer** — per-cell
`L.rectangle` fills on the existing `L.svg()` renderer via
`useLeafletLayerGroup()`, color by decode density/SNR band, unit-inspectable
like every other layer in the codebase. No new dependency. (Leaflet overlay
pane z-400 sits under markers at z-600 — gateway markers stay on top.)

## GPL boundary and packaging (hardened)

- **Doctrine, stated precisely:** exec-with-argv + stdout is the FSF
  "separate programs at arm's length" case; not shipping wsjtx in our
  artifacts means §5 aggregation is never even engaged. jt9's `-s/--shmem`
  shared-memory mode is the FSF's named boundary-crosser and is **BANNED** —
  WAV-file + argv + stdout only. (This also closes the "persistent jt9 would
  fix hash amnesia and wisdom cost" optimization: driving shmem mode requires
  replicating WSJT-X's shared-memory struct layout, impossible without
  reading GPL source.)
- **Not bundled, with the rationale recorded:** the ardopcf precedent (GPL-3
  binary injected via `release.yml`'s `externalBin` jq step) creates live
  pressure to bundle jt9 the same way. Rejected: wsjtx's §6
  corresponding-source duty would attach to the entire Qt5/boost/hamlib/fftw
  dep chain, and apt keeps wsjtx patched. (ardopcf's own §6 offer gap is filed
  separately: tuxlink-y0z5h.)
- **Packaging:** `deb.recommends: "wsjtx (>= 2.5)"` AND
  `rpm.recommends: "wsjtx >= 2.5"` (mirrors the direwolf entries at
  `tauri.conf.json:63-65,86-88`); AppImage has no dependency metadata — the
  `wsjtx-absent` message + `docs/install.md` §prerequisites are the AppImage
  path, stated in docs. Fix `bundle.license` from the stale
  `"GPL-3.0-or-later"` to `"AGPL-3.0-or-later"` in the same PR that touches
  `tauri.conf.json`. Note: deb-install-test CI will auto-install the
  Recommends chain (~job-time/disk increase — expected, and it validates the
  Recommends resolves).
- **Version floor:** wsjtx ≥ 2.5 pinned in Recommends; the stdout format is
  verified on 2.7.0 only — the plan includes either a 2.5-era output fixture
  or an explicit verified-identical note before ship.
- **`.7` grep-guard (rescoped, concrete deny-patterns; structural, so the
  reference crate's prose mentions of jt9/wsjtr can't false-positive):**
  1. No GPL source files: `git ls-files` must not match
     `\.(f90|f95)$|(^|/)(parity|generator)\.dat$`.
  2. No dependency edge: no `^\s*(wsjtr|ft8core)\s*=` in any tracked
     `Cargo.toml`.
  3. No FFI: no `#[link]`/`extern "C"` in any `.rs` that mentions `wsjt`.
  4. No bundling: `tauri.conf.json` `.bundle.externalBin // [] + .bundle.resources`
     must never match `jt9|wsjt`; `release.yml` must not contain
     `binaries/(jt9|wsjt)` nor add `jt9|wsjtx` in the externalBin jq-inject.
  5. Subprocess confinement: the literal `"jt9"` in spawn position only inside
     the one decode-service module; `-s|--shmem` never in its arg builder.

## Non-goals (restated for subagents)

- No TX, no PSKReporter, no VOACAP fusion, no WSJT-X GUI automation, no
  jt9 shmem mode, no bundling of jt9/wsjtx in any artifact.
- No revival of the clean-room decode path inside this epic; `tuxlink-ft8` is
  read-only reference (its fixtures get regenerated refs + README recipe
  update — the only in-crate change).
- Nothing in the existing gateway finder is removed or reflowed; the Use→ QSY
  flow stays one click from marker select.
