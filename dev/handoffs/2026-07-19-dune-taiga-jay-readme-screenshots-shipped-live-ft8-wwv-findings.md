# 2026-07-19 — dune-taiga-jay: README screenshots shipped (#1163 + #1165), live FT8 capture, WWV field findings

Session scope: finish tuxlink-d8f3l task 4 (screenshots on the R2 bench) and
task 6 (ship), then an operator-driven follow-up round: default-scheme
recapture, live FT8 waterfall, WWV off-air receive testing with the freshly
restored antenna.

## Shipped

- **PR #1163** (first screenshot set, merged by the operator): five new
  captures + five stale recaptures + alt-text updates; tuxlink-d8f3l CLOSED.
- **PR #1165** (merged by the operator; branch bd-tuxlink-y6sof per
  ADR 0017): seven screenshots recaptured in Default (dark) after the
  operator caught that round one inherited his Repository Dark scheme, plus
  a LIVE FT8 capture; tuxlink-y6sof CLOSED.
- Both worktrees disposed per ADR 0009; archives (fact ledger, Codex adrev
  transcript, SDD briefs included) at
  .claude/worktree-archives/bd-tuxlink-d8f3l-...-20260719T052431Z.tar.gz and
  bd-tuxlink-y6sof-...-20260719T055444Z.tar.gz.

## The theme incident (transferable lesson)

The release binary's tauri:// origin has its OWN WebKit localStorage,
separate from the dev origin (localhost:1420). Both carried the operator's
tuxlink.colorScheme=github-dark, so capture instances rendered Repository
Dark (blue accents), not default (orange). Diagnosed by launching a fresh
instance and comparing accent colors. Recaptured: hero, elmer,
routines-designer, ft8-waterfall, vara-setup, mailbox, ardop-hf. Already
correct: request-center, first-run-wizard (fresh-XDG instance = true
default), color-night-red, color-daylight. END-STATE: release-origin scheme
restored to Repository Dark (operator preference); dev origin never touched.

## Live FT8 (the highlight)

With the exterior antenna restored and the CAT port corrected, the listener
pulled real 30m FT8: 57 decodes/min, 34 grids heard, CAT-confirmed dial
10.136, live waterfall, decodes out to HB9CXZ (5,799 mi). That capture is
the shipped tuxlink-ft8-waterfall.png.

## Hardware finding: CAT serial re-enumerated

The FT-710 no longer answers CAT on the configured /dev/ttyUSB0 (CP2102N);
it answers on /dev/ttyUSB1 (CP2105 if00) — likely re-enumeration during the
antenna work. rig.cat_serial_path was corrected to /dev/ttyUSB1 via the UI
and DELIBERATELY kept (the one intentional config delta vs the pre-session
snapshot). Recommend migrating cat_serial_path to the stable
/dev/serial/by-id/ path the way audio devices already use stable ids.

## WWV off-air testing (operator-participating; full detail in tuxlink-76y11)

- STT model installed via scripts/fetch-stt-model.sh.
- Attempt 1 (WWVH :45, 04:45Z): capture ran (70.0s wav, real audio, window
  covers the bulletin — premature-QSY not supported by the recording;
  operator likely heard the QSY-back at :46:05) but decode returned NoCopy
  at the confidence gate (no_speech_prob/avg_logprob, tuxlink-stt
  is_confident) with NO durable UI surfacing.
- Attempt 2 (WWV :18): never fired — the one-shot arm is LOST when the
  Station Intelligence overlay closes.
- Attempt 3 (WWVH :45, 05:45Z, overlay held open): arm persistence
  confirmed; capture fired; decode NoCopy again on 5 MHz noise.
- Retained wavs for playback/manual entry: r2:/tmp/wwv-235598-*.wav (two).
- Frequency bug found: freq_for_utc_hour never selects 10 MHz despite its
  own comment; operator freq override unimplemented.
- The rig is temporarily reserved by the operator for manual clear-channel
  STT listening — do not touch FT-710/CAT/audio paths until he releases it.

## bd filings this session

- tuxlink-76y11 (P2): WWV arm loss, silent NoCopy, freq rotation gaps —
  with full diagnosis + fix directions in notes.
- tuxlink-caxy6 (P2): FT8 device picker offers loopback/non-USB capture
  cards the resolver can never resolve (picker=capture enumeration,
  resolver=packet USB-only filter); level meters "meter unavailable" on R2.
- tuxlink-y6sof (P1): the recapture follow-up — closed with PR #1165.

## R2 state at close

- config.json vs pre-session snapshot (/tmp/fidelity/config-pre-dtj.json):
  byte-identical EXCEPT the deliberate rig.cat_serial_path=/dev/ttyUSB1.
  elmer block restored (twin-bramble + qwen3-coder-next — NOTE the endpoint
  now serves qwen35-122b-nvfp4, so agent_model is stale regardless;
  operator may want to update it. Qwen3.5-122B also looped docs_search 15x
  where Claude Haiku one-passed the same question — Elmer model-eval data.)
- Audio defaults restored; FT8 listener stopped; popouts docked back;
  Repository Dark active; my instances killed; operator's debug instance
  (PID 143559) untouched except twice iconified/remapped for hero captures.
- Outbox note: the earlier "2 to send"→1 was the app soft-deleting the
  owl-moraine-sycamore session's "Observability probe" (AKQHQ5KF7FR7) at
  02:40Z — recoverable in Deleted; nothing transmitted.
- Left in place deliberately: shipped-.deb sidecars staged in
  r2:~/Code/tuxlink/src-tauri/target/release/ (voacapl+itshfbc, ardopcf,
  pmtiles, rigctl(d), tuxlink-mcp) + extraction at /tmp/fidelity/deb-extract
  + capture tooling (drive2/drive3/arrange/rootclick/scroll/combo .py) at
  /tmp/fidelity/ + retained WWV wavs.

## Open items

- tuxlink-76y11, tuxlink-caxy6 (new, above).
- tuxlink-of8ee (ui_commands config-path race, P2) — pre-existing.
- tuxlink-w9vof (quality gates, tabled) — awaits operator decisions.
- Old repo-wide stashes (7, other sessions') — untouched.

Agent: dune-taiga-jay
