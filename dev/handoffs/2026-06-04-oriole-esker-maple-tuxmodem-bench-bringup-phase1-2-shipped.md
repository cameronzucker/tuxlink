# Handoff — oriole-esker-maple — tuxmodem bench bring-up Phases 1+1b+2 shipped, Phase 3 next

> **Date:** 2026-06-04 · **Agent:** `oriole-esker-maple` · **Machine:** pandora
>
> **Arc:** Long session opening with "wire un-wired modem modes in the sidebar" → six PRs shipped across two work-streams. Stream A (UI plumbing): P2P-VARA wire (#303), Print menu (#310), RMS-Relay foundation (#322, via #321→#322 rebase). Stream B (tuxmodem hardware bring-up — new umbrella): CM108-HID PTT (#355) — re-framed after operator clarified DRA-100 is FM-only — serial-RTS PTT for Digirig + G90 (#358, the actual HF Phase 1), then `tuxmodem-phy::audio_device` + bench play CLI (#362). All six PRs merged.
>
> **Status at handoff:** No open PRs from this session. Phase 3 (`tuxmodem-tx` — the **plug-into-radio milestone**) is filed at [`tuxlink-i3bz`](https://github.com/cameronzucker/tuxlink/issues?q=tuxlink-i3bz) and is the next session's primary target.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first, especially §5 (critical guidance) — there are
   TWO subtle-but-load-bearing operator-clarified framings from this
   session that future agents have already mis-applied twice.
2. Check the umbrella's progression: `bd show tuxlink-9ggl`.
3. Claim Phase 3: `bd update tuxlink-i3bz --claim` — that's the
   plug-into-radio milestone.
4. Read the existing layers in this order:
   - tuxmodem/crates/tux-rig-rts (Phase 1 — the PTT the operator
     actually uses; Digirig RTS, NOT CM108)
   - tuxmodem/crates/tuxmodem-phy/src/audio_device.rs (Phase 2 — the
     audio output)
   - tuxmodem/crates/tuxmodem-phy/src/bin/tuxmodem-audio-play.rs
     (Phase 2 CLI — reference for tuxmodem-tx's CLI shape)
   - The PHY's encoder entry points (modes.rs + phy_api.rs + ofdm_main/)
     to find the payload→AudioBuffer pipeline.
5. Build tuxmodem-tx per the tuxlink-i3bz spec; ship as a new
   feature-gated binary in tuxmodem-phy (mirrors tuxmodem-audio-play
   layout) OR a new crate if encoding lives elsewhere.
```

**Critical first read — DO NOT skip:** the operator clarified TWO subtle conflations during this session that future agents have already mis-applied. See §5 below.

---

## 1. Session arc (compressed)

1. **Stream A opened:** "we have a lot of modem modes in the left hand sidebar which are currently un-wired from the UI."
2. **PR #303 (`tuxlink-kb3s`):** Flipped p2p.vara-hf + p2p.vara-fm to `built: true` per PR #221's scope-bounded "CMS only" comment. Routing-fix on `AppShell.tsx:706` so P2P-VARA dispatches to MessageView not StubPanel. **Merged.**
3. **Stream A audit surfaced:** disabled menu items in File/Session/Tools per [tuxlink-dpf] convention. Print was the cheapest wire-up; Ctrl+P had an accelerator but no handler.
4. **PR #310 (`tuxlink-j0m3`):** Wired `menu:message:print` → `window.print()`. `tuxlink-zdfj` filed as follow-up for `@media print` stylesheet to drop chrome from the printed page. **Merged.**
5. **Operator pivot:** "I'm not sure why these are deferred: Radio-only / Post Office / Network PO..." Operator caught my framing — I'd called these "needs new backend, not in scope" when they ARE in scope per the WLE-parity closure plan (P2 🟢 ship, deep-dive at `dev/scratch/winlink-re/findings/client-of-rms-relay.md` complete).
6. **PR #321 → #322 (`tuxlink-kld3`):** RMS-Relay client FOUNDATION — `SessionIntent` enum (Cms / RadioOnly / PostOffice / Mesh / P2p) threaded through `ExchangeConfig`, `RoutingFlag` (C/R/L) char round-trip, byte-exact banner parser for relay self-identification. CI failure on #321 due to main moving while PR was open (PR #320 added new ExchangeConfig sites); closed #321 + opened #322 per operator decision (no force-push). **Merged.**
7. **Pivot to tuxmodem:** Operator: "Where are we with Tuxmodem?" → snapshot of program state (subsystems #1/#3/#4 shipped, #5-#8 not started). Operator: "I want to build and ship it, ideally. Let's get it to the point where we can plug it to a radio."
8. **Stream B opened:** Surveyed the gap — PHY/FEC/sim are DSP only; no real-time audio, no PTT. Filed `tuxlink-9ggl` umbrella for tuxmodem hardware bring-up.
9. **Wrong initial Phase 1 (`tuxlink-u1js`, PR #355):** Started with CM108-HID PTT (DRA-100 path) thinking it was "first hardware-touching code in the modem program." Operator caught: "Is PR 355 absolutely locked to a DRA-100? That wasn't approved." Then a sharper correction: "It's DRA-100 specific for FM. Not for HF. They are being conflated once again." DRA-100 is the FM bench rig (per `docs/hardware/modem-test-rig.md`). HF program targets G90 via Digirig. **CM108-HID is NOT the HF path.** PR #355 stayed open as one valid backend; it later merged.
10. **Actual Phase 1 (`tuxlink-mxyz`, PR #358):** `tux-rig-rts` — serial-RTS PTT primitive for Digirig + G90. RTS via `TIOCMBIS`/`TIOCMBIC` ioctls; `O_NOCTTY`, CRTSCTS clear, OpenClearBoth-first as the spurious-key-on-open defuse. Merge conflict on workspace Cargo.toml with #355 (both PRs added a crate); resolved via `git merge origin/main` into the branch (non-destructive — no force-push needed). **Merged.**
11. **Phase 2 (`tuxlink-h8pp`, PR #362):** `tuxmodem-phy::audio_device` module (feature-gated `audio-device`) — `list_output_devices`, `AudioOutput::open` (negotiates 48 kHz f32, prefers mono, falls back to stereo with sample-duplication), `play_blocking` (bounded recv timeout + 100ms tail-drain). New binary `tuxmodem-audio-play` with `--list` and `--sine HZ:SECS`. 21 new tests. **Merged.**

---

## 2. Six PRs shipped this session

| PR | Topic | bd | State |
|---|---|---|---|
| [#303](https://github.com/cameronzucker/tuxlink/pull/303) | P2P-VARA HF/FM sidebar wire | `tuxlink-kb3s` | **MERGED** |
| [#310](https://github.com/cameronzucker/tuxlink/pull/310) | `menu:message:print` Ctrl+P | `tuxlink-j0m3` | **MERGED** |
| [#322](https://github.com/cameronzucker/tuxlink/pull/322) | RMS-Relay foundation: `SessionIntent` + banner parser | `tuxlink-kld3` | **MERGED** (replaced #321 after CI/merge conflict; no force-push used) |
| [#355](https://github.com/cameronzucker/tuxlink/pull/355) | `tux-rig-cm108` CM108-HID PTT (DRA-100 / SignaLink class) | `tuxlink-u1js` | **MERGED** (NOT critical-path for HF — see §5) |
| [#358](https://github.com/cameronzucker/tuxlink/pull/358) | `tux-rig-rts` serial-RTS PTT (Digirig + G90) | `tuxlink-mxyz` | **MERGED** (actual HF Phase 1) |
| [#362](https://github.com/cameronzucker/tuxlink/pull/362) | `tuxmodem-phy::audio_device` + bench play CLI | `tuxlink-h8pp` | **MERGED** |

---

## 3. Open carry-over (bd issues this session filed or that remain open)

| Issue | Pri | What |
|---|---|---|
| **`tuxlink-i3bz`** | **P1** | **Phase 3 — `tuxmodem-tx` CLI: payload → PHY → PTT + audio. THE plug-into-radio milestone. Filed this session; ready for next.** |
| `tuxlink-9ggl` | P2 | UMBRELLA: tuxmodem hardware bring-up. Phases 1/1b/2 shipped; Phase 3 (i3bz) is the next blocking child. |
| `tuxlink-eaab` | P2 | UMBRELLA: RMS-Relay client (paths 1A/1B/1C). Foundation (kld3) shipped; per-path slices still open. |
| `tuxlink-zdfj` | P3 | `@media print` stylesheet (drop chrome from printed page). Depends-on `tuxlink-j0m3` (which merged). |
| `tuxlink-9ggl` Phase 1.5 (unnamed) | — | Watchdog daemon (SIGKILL-safe PTT release). Not filed as bd; spelled out in `tuxlink-9ggl` and PR #358 body. Optional safety upgrade; Phase 3 ships fine without it. |
| `tuxlink-9ggl` Phase 4 (unnamed) | — | `tuxmodem-rx` CLI: capture + demod + BER. Comes after Phase 3. |
| `tuxlink-9ggl` Phase 2 follow-up (unnamed) | — | WAV file playback for `tuxmodem-audio-play` (audio_io.rs already does WAV read; small follow-up). |

Also still open from prior sessions: `tuxlink-dr0x` (P2, floor-rate LDPC PEG construction — modem-internal, not blocking), 4 P2 carry-overs from plover-willow-basalt (`i9vn`, `ylra`, `hr8f`, `ztuv` — UI polish), and the various WLE-parity per-mode and per-transport issues.

---

## 4. Worktree + runtime state at handoff

**Worktrees alive in `/home/administrator/Code/tuxlink/worktrees/`** (all merged-dead — disposable per ADR 0009; none carry untracked content of concern):

- `bd-tuxlink-kb3s-p2p-vara-wire` — PR #303 merged
- `bd-tuxlink-j0m3-print-wire` — PR #310 merged
- `bd-tuxlink-kld3-rms-relay-foundation` — PR #321 closed (superseded); the v2 branch (`bd-tuxlink-kld3/rms-relay-foundation-v2`) is where #322 merged
- `bd-tuxlink-u1js-tux-rig-cm108` — PR #355 merged
- `bd-tuxlink-mxyz-tux-rig-rts` — PR #358 merged
- `bd-tuxlink-h8pp-audio-device` — PR #362 merged

(Plus many older worktrees from prior sessions — not enumerated here, all merged-dead.)

**Operator's `task-amd-main-ui` interactive rebase: still in progress, still untouched.** This session never went near it. The handoff at the top of session noted it; it remains operator state per the project's main-checkout-is-operator-state discipline.

**This handoff doc itself:** written as an untracked file in the main checkout's `dev/handoffs/` — matches the pattern of the two earlier untracked handoffs that have been sitting in the operator's main checkout since 2026-06-01 / 2026-06-02. Operator commits + pushes when convenient.

---

## 5. Critical guidance for next session — TWO load-bearing operator-clarified framings

### 5.1 The HF bench rig is **G90 + Digirig**, NOT DRA-100

Despite the `docs/hardware/modem-test-rig.md` doc being titled "VHF/UHF FM modem" and meticulously specifying the DRA-100 + Motorola-16 + CDM-1550LS+ chain, **that doc covers the FM bench rig only**. The tuxmodem program targets HF; the operator's HF radio is the Xiegu G90, driven through a Digirig Mobile interface. **Digirig PTT is RTS on the USB-serial port**, not CM108-HID. The CM108 backend (`tux-rig-cm108`, PR #355) is one of multiple `tux-rig` backends; the **operator-validatable HF path is `tux-rig-rts` (PR #358)**.

If a future agent reads `modem-test-rig.md` and concludes "the project's bench rig is DRA-100," they're conflating the FM rig with the HF rig. Same operator, two different rigs. Phase 3 (`tuxmodem-tx`) defaults to the Digirig + G90 path — the CM108 path is optional/alternative.

### 5.2 RMS Relay parsing is **client-side**, NOT hub implementation

PR #322 (RMS-Relay foundation) parses banner phrases that RMS Relay emits — to *understand* what the remote is. **Tuxlink is a Winlink-Express-class client; it dials INTO an RMS Relay, it does not implement one.** The deep-dive at `dev/scratch/winlink-re/findings/client-of-rms-relay.md` opens with this disclaimer. If a future agent reads the new `relay_banner` module and thinks "we're building RMS Relay" — they're not. Same parsing layer a browser uses to read HTTP headers without itself being a web server.

### 5.3 Other guidance

- **Non-destructive rebase-vs-conflict pattern:** when `origin/main` moves while your PR is open and conflicts arise, use `git merge origin/main` INTO your branch (append-only, no force-push). PR #321→#322 forced a close-and-reopen because I rebased instead of merged; PR #358 used the merge approach and stayed alive through the conflict. Per CLAUDE.md, force-push is banned; merge-into-branch is the right primitive.
- **Skip CSS for now on `@media print` (`tuxlink-zdfj`)** — operator hasn't asked, and Phase 3 + listener work are higher value. Defer until operator surfaces a need.
- **For Phase 3 (`tuxmodem-tx`), critical safety primitives** are codified in `tuxlink-i3bz`'s description: PTT lead-in (~100ms), bounded total airtime (default 30s, max 60s), --dry-run mode for non-RF validation, SIGINT/SIGTERM early-release, **agent does NOT run the binary against the real device** (RADIO-1: operator is the licensee).

---

## 6. Out-of-repo state changes this session

| Path | Change | Reversible? |
|---|---|---|
| Various `dev/adversarial/*-codex.md` | None — no Codex adrev rounds this session (all work was plumbing per `feedback_discipline_triage_rule`). | n/a |
| Memory (`~/.claude/projects/.../memory/`) | None added. | n/a |
| `~/.gstack/` | None touched. | n/a |
| The 6 worktrees listed in §4 | Created via `new_tuxlink_worktree.py`; each carries the merged feature branch. Disposable per ADR 0009. | Yes — disposal ritual at operator's convenience |

---

## 7. Untouched state (operator owns)

- `task-amd-main-ui` interactive rebase — still in progress; same as session start.
- 2 prior-session untracked handoffs in main checkout — still sitting.
- This handoff doc (untracked, will sit alongside the prior two).

---

## 8. Session totals

- **6 PRs shipped, all merged:** #303 / #310 / #322 (replacing #321) / #355 / #358 / #362.
- **2 work streams advanced end-to-end:**
  - Stream A (UI plumbing): VARA P2P + Print + RMS-Relay foundation.
  - Stream B (tuxmodem hardware bring-up — new umbrella): tux-rig-cm108 + tux-rig-rts + audio_device.
- **bd issues filed:** `tuxlink-kb3s`, `tuxlink-j0m3`, `tuxlink-zdfj`, `tuxlink-kld3`, `tuxlink-eaab` (umbrella), `tuxlink-u1js`, `tuxlink-mxyz`, `tuxlink-9ggl` (umbrella), `tuxlink-h8pp`, `tuxlink-i3bz` (Phase 3, ready to claim next session).
- **bd issues closed (via merge):** kb3s, j0m3, kld3 (via #322 merge), svsb (stale deep-dive), u1js, mxyz, h8pp.
- **0 Codex adversarial rounds** — all work qualified as plumbing per [[discipline-triage-rule]]; cross-provider adrev deferred to per-path slices (RMS-Relay 1A/1B/1C; tuxmodem-tx if operator wants it before first on-air run).
- **2 operator framings clarified the hard way** (DRA-100-vs-Digirig; RMS-Relay parsing vs hub) — both recorded in §5.

---

## 9. Next-session prompt (paste into a fresh session)

```
Resume tuxlink from the oriole-esker-maple 2026-06-04 tuxmodem-bench-bringup
handoff.

Handoff doc: dev/handoffs/2026-06-04-oriole-esker-maple-tuxmodem-bench-bringup-phase1-2-shipped.md
READ IT FIRST — especially §5 (TWO load-bearing operator-clarified framings):
  1. DRA-100 is the FM rig; HF is G90 via Digirig (NOT CM108-HID)
  2. tux-rig-rts (PR #358 merged) is the HF Phase 1, NOT tux-rig-cm108

Next work: Phase 3 of the tuxmodem hardware bring-up = the "plug it into a
radio" milestone. bd: tuxlink-i3bz (P1).

Pipeline to build: payload → tuxmodem-phy encoder → AudioBuffer →
tux-rig-rts PTT assert → tuxmodem-phy::audio_device::play_blocking → PTT
release. Critical safety primitives in the bd description (PTT lead-in,
bounded airtime, --dry-run, SIGINT-safe release; agent does NOT run the
binary against the real device per RADIO-1).

Reference files:
  - tuxmodem/crates/tux-rig-rts/src/bin/tux-rig-rts.rs (Phase 1 CLI shape)
  - tuxmodem/crates/tuxmodem-phy/src/bin/tuxmodem-audio-play.rs (Phase 2 CLI shape)
  - tuxmodem/crates/tuxmodem-phy/src/audio_device.rs (Phase 2 audio path)
  - tuxmodem/crates/tuxmodem-phy/src/{modes.rs,phy_api.rs,ofdm_main/} (PHY encoder)

Critical first action: `bd show tuxlink-i3bz` + `bd update tuxlink-i3bz --claim`.
Then read the four reference files above before writing Phase 3 code.

Untouched: operator's task-amd-main-ui rebase + prior untracked handoffs.
```

---

Agent: oriole-esker-maple
