# Handoff — ARDOP on-air bring-up: shipped (bundle / logging / ARQBW), diagnosed modem-good, on-air RF unresolved (operator/bench)

**Agent:** canyon-salamander-taiga · **Date:** 2026-06-23

## One-sentence frame
A long FT-710/G90 on-air ARDOP bring-up: the **tuxlink software is proven working** (modem connects, RX decodes DX, the destination call is in our transmission); we **shipped the real packaging/observability fixes** (ardopcf built from pinned source, stdout logging, ARQBW syntax); and the remaining blocker — **the transmitter not getting out on the air** — is a **station/bench RF problem, not software.** Several radio-UI defects are deferred with plans; **nvye needs an operator UX decision before coding.**

## Shipped this session (merged to main)
- **PR #892 — `tuxlink-4zs4`** (in 0.75.2): bundle ardopcf **built from pinned `pflarue/ardop` develop `cb2c4c1`** per-arch in CI (native `make`). No tagged release has all three features tuxlink needs together (CODEC + `--webgui` + `--ptt`); only the develop line does. Supersedes the 1.0.4.1.3 (#889) and 2.0.3.2.1 (#890) bundles.
- **PR #894 — `tuxlink-c119`**: `ManagedModem::spawn` now pipes ardopcf stdout/stderr into the `tuxlink::winlink::modem::ardopcf` tracing target (drops Jack noise) + logs the full spawn args. This is the fix for the blindness that cost most of the day — ardopcf's PTT/audio/decode/`SendARQConnectRequest TARGET` lines now land in the `.jsonl` instead of `/dev/null`.
- **PR #896 — `tuxlink-87uc`** (OPEN, CI running → cut **0.75.3**): ARQBW sent as a single token (`2000FORCED`), not `2000 FORCED` (space faulted "Syntax Err", aborting init whenever a bandwidth was set).

## Diagnosis established (so the next session doesn't re-chase it)
- **Modem works:** two ardopcf instances completed a full ARQ connect over an `snd-aloop` loopback at decode **Quality 100**, including **cb2c4c1 ↔ stable 2.0.3.2.1 interop**. The shipped modem is field-protocol-compatible. (See memory `project_ardop_modem_loopback_validated_code_works`.)
- **RX works end-to-end:** on 20 m, `jt9` decoded ~12 FT8 stations/cycle incl. EU DX through G90→DigiRig→Pi. The receive chain is healthy **in U-D mode** (plain USB gives silence — wrong mode).
- **We DO transmit the destination call:** generated our `ARQCALL N0DAJ` to a WAV off-air and decoded it back — the ConReq frame literally contains `N7CPZ N0DAJ`. Cameron's logs never showed it only because tuxlink discarded ardopcf stdout (now fixed, c119).
- **PTT on the DigiRig is CM108 GPIO**, config string `CM108:/dev/hidraw0` (bare `/dev/hidraw0` is parsed as a serial port and faults). Operator is in `audio` group; the chip PID `0x0013` is on ardopcf's CM108 list.
- **RTL-SDR (V3) HF works only on direct-sampling Q-branch (`direct_samp=2`)**; the bundled `librtlsdr` lacks `rtlsdr_set_dithering` so pyrtlsdr fails — use the ctypes wrapper in `/tmp` (or rebuild). It needs an actual HF antenna/stub; bare attach is deaf.

## OPEN — pending operator (the real blocker)
**The transmitter is not radiating on the air.** Symptoms: FT-710 *and* G90, multiple antennas (Delta Loop + ground long-wire), brand-new double-choked 25 ft coax, ATU tunes fine — yet **no remote SDR (even near Phoenix) sees anything**, on TUNE/MOX. The radio's own PO meter shows ~50 W on USB with sustained hard audio (mic gain 70).
- **SSB caveat that explains much of it:** on USB, output tracks audio — MOX/key with no/low audio = no carrier = nothing to see. The radio *does* make ~50 W on sustained audio, so it is not a dead PA.
- **Reciprocity argument:** an antenna that receives DX *must* radiate 50 W. So either (a) RF isn't reaching the antenna despite the meter, or (b) it IS radiating and the **remote-SDR verification is the flaw** (frequency, the SSB **sideband offset** = dial+audio, real-time/sustained, a known-good SDR).
- **Next steps (operator/bench):** near-field RTL-SDR sniffer at the SO-239 (low power, 5–10 W — don't fry the dongle) to confirm RF leaves the jack; a **steady carrier in RTTY/CW key-down** to read true power without SSB peak ambiguity; re-verify a remote SDR with the sideband offset in mind. Memory `project_ft710_usb_audio_rfi_reset_on_tx` is **superseded/uncertain** — the "USB resets on TX" was a confounded test; do not treat RFI as settled.

## Deferred radio-UI issues (with plans)
- **`tuxlink-nvye` (P1) — Find-a-Station 'Use' → mode routing. NEEDS OPERATOR UX DECISION FIRST.** `channelToDial` (`src/catalog/channelGrouping.ts`) builds the dial from the clicked channel's mode; `handleStationUse` (`AppShell.tsx:1415`) routes to it. Decide: when a station offers ARDOP+VARA+packet, should 'Use' target the clicked channel, the operator's last-used mode, or prompt? Also a **separate VARA-panel bug**: it doesn't consume the gateway prefill (`emitGatewayPrefill`). Do not code before the UX call.
- **`tuxlink-5xxq` (P1)** — failed ARDOP connect (3 dials exhausted / REJ) must self-terminate + log honestly, not sit in REJ. Backend connect-state work in `winlink/modem/ardop/session.rs`.
- **`tuxlink-zrmy` (P2)** — PTT picker should enumerate CM108 HID devices (scan `/dev/hidraw*` for VID `0x0D8C` + the CM108 PID list) and offer them `CM108:`-prefixed, so the operator stops hand-typing. Additive (low regression risk) — good next pickup.
- **`tuxlink-5016` (P1)** — Find-a-Station favorites/save affordance.
- **`tuxlink-p6n7` (P2, docs)** — write the CMS-Z prod→replica propagation-lag troubleshooting note.

## Worktree / tree state
- Working tree: the main checkout (`bd-tuxlink-xygm/recover-handoffs`) carries many pre-existing untracked dev artifacts (bug-hunts, mocks, PNGs) from prior sessions — not mine; left as-is.
- In-flight worktree: `worktrees/bd-tuxlink-nvye-radio-ui-fixes` (branch `bd-tuxlink-nvye/radio-ui-fixes`) — carries **only the 87uc commit** (PR #896). Dispose after #896 merges. Stale merged-PR worktrees from this session (`bd-tuxlink-4zs4-*`, `bd-tuxlink-c119-*`, `bd-tuxlink-y061-*`) should be disposed per ADR 0009 if still present.
- After #896 is green: merge, then `gh workflow run release-merge.yml` to cut **0.75.3** (bundles c119 + 87uc).
