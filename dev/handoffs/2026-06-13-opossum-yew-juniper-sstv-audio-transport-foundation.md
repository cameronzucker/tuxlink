# Session handoff — opossum-yew-juniper — 2026-06-13

Built the **UV-Pro audio-transport foundation** (SSTV component 1, `tuxlink-bcsy`) — framing + codec seam + SDP resolution + the `AudioTransport` with a RADIO-1 working abort. Four TDD tasks committed and pushed as **draft PR #668**. The transport design was NOT relitigated (operator correction mid-session: it's RE-locked in the bd-bcsy notes — confirmed, not re-brainstormed).

## ⚠️ Read first — critical context for the next session

- **Do NOT build SSTV on `bd-tuxlink-xygm/recover-handoffs`.** That branch is **1512 commits behind `origin/main`** and lacks the entire `uvpro/` backend. All SSTV work is in the worktree below, off fresh `origin/main`.
- **Worktree:** `worktrees/bd-tuxlink-bcsy-sstv-audio-transport`, branch `bd-tuxlink-bcsy/sstv-audio-transport` (off `origin/main` @ `d4cc07d7`). bd `tuxlink-bcsy` claimed + worktree-bound.
- **This handoff doc is UNCOMMITTED** in the main checkout's `dev/handoffs/` (the main checkout is contended — live sessions on recover-handoffs — so I could not commit here, and per `no_pr_for_handoffs` it must not ride PR #668). It joins the existing untracked-handoff pile for the recover-handoffs sweep.

## What's on PR #668 (draft) — built + unit-tested (TDD)

All in `worktrees/bd-tuxlink-bcsy-sstv-audio-transport`, 5 commits (`b8ade6ef` plan → `9e454f95` transport):

1. **`uvpro/audio/framing.rs`** (`37a65aea`) — `AudioMessage` Data/End/Ack/Unknown; `0x7e`-delimited, `0x7d`-escaped HDLC framing + streaming `AudioDeframer`. Byte-for-byte verified against benlink `protocol/audio.py`. 14 golden-vector/edge tests (escape, split frames, garbage resync, MAX_BUFFER bound, dangling escape).
2. **`uvpro/audio/codec.rs`** (`dff5fef8`) — `SbcCodec` trait seam (decouples transport from the codec sub-project) + `NullSbcCodec`/`RecordingSbcCodec` fakes.
3. **`ax25/rfcomm.rs`** (`922dd9f4`) — `parse_audio_channels`/`resolve_audio_channels`: SDP resolution targeting the audio-gateway service classes (`0x1112`/`0x111f`), mirroring the SPP resolver. Candidate-ranked; HCI snoop confirms which.
4. **`uvpro/audio/transport.rs`** (`9e454f95`) — `AudioTransport`: `send_pcm`/`finish` (TX), `pump_rx` (RX poll), and the **RADIO-1 working abort** (best-effort `AudioEnd` then *drop the link* = disarm-on-abort, the complete fix the KISS path's `tuxlink-0ja` note describes). `KeyingMode::Implicit` (default; benlink's working POC sends no `c1.TX_AUDIO`) / `Explicit` (injected GAIA `KeyFn`). 6 tests via in-memory `ByteLink` fakes.

**Plan:** `docs/superpowers/plans/2026-06-13-sstv-audio-transport.md` (on the branch).

## CI status (PR #668)

<!-- CI-STATUS -->
**CI-GREEN** — all 4 checks pass on PR #668: `build-linux` (amd64 11.0m / arm64 11.7m) + `verify` (amd64 9.0m / arm64 12.4m). `verify` runs `clippy --all-targets -D warnings` + full test suite, so the new framing/codec/SDP/transport modules compile clean and all unit tests (incl. the 20 new ones) pass on both arches. This is the verification substitute for the local cold-build that `no_cold_cargo_on_contended_pi` precludes.

## Deferred — next session (in order)

1. **Confirm CI-green** (above). The code is unverified-compiling until CI says so (`verification_before_completion`).
2. **Codex adversarial review** — DEFERRED: hit usage limit mid-run (reset ~1:49 PM local). Per `codex_quota_gotcha` this is a capacity-defer, **do NOT substitute a Claude agent**. Re-run the prompt at `dev/adversarial/2026-06-13-sstv-audio-transport-codex.md` (gitignored; only the prompt + quota stub are there now). Attack angles: RADIO-1 abort/runaway-TX, wire correctness vs benlink, deframer robustness, RX loop, keying state machine.
3. **Task 5 — `keying.rs`** (`c1` opcodes over GAIA): BLOCKED on extracting the GAIA `command_group`/`command_id` that carries the `c1` enum, from the decompile `dev/scratch/benshi-re/apk/jadx-out/sources/v4/g2.java` (around the `W0(c1.TX_AUDIO_STOP, ...)` call ~line 420) cross-checked with `message.rs`'s `header()`/BASIC=2 convention. OFF critical path — `Implicit` keying is the benlink-proven path; only needed if the HCI snoop shows the app keys via GAIA.
4. **Task 6 — `UvproSession::open_audio`/`abort_audio`**: resolve audio channel → connect 2nd `RfcommSocket` → construct `AudioTransport` (inject codec) → store behind the session mutex. **Confirm the `UvproLinkLock` model permits a 2nd RFCOMM channel to the SAME radio over the SAME ACL link** (it should — multiplexed, same operator intent — but document the decision; the Codex adrev will attack it).

## Sibling sub-projects (filed, in `bd ready`)

- **`tuxlink-vgvn`** (P2) — SBC codec (pure-Rust encode+decode), implements `SbcCodec`. **OPEN OPERATOR DECISION:** no mature pure-Rust SBC *encoder* on crates.io (`mini_sbc`=decode-only; `libsbc`=C-FFI). Port a pure-Rust encoder (matches repo's no-C-dep ethos; FFmpeg `sbcenc.c` LGPL→GPL-3 compatible reference) **vs** `libsbc` C-FFI fallback. Params pinned: 32 kHz mono s16le ⇄ SBC, bitpool=16, subbands=8, blocks=16, msbc=false.
- **`tuxlink-st5n`** (P2) — SSTV codec (PCM↔image), HTCommander C# port (Robot36 + a PD mode; STFT decode). Independent of SBC.
- **`tuxlink-yfyn`** (P2) — inline image UI in `AprsChatPanel`. Deps on `bcsy` + `vgvn` + `st5n`. The **whole-feature `wire-walk` gate** fires here (not before — partial transport is not "shipped").

## Operator-gated before any on-air (RADIO-1 / ADR 0018)

HCI snoop of a real vendor-app image-send (Android Dev Options → Bluetooth HCI snoop log → Wireshark; host-side/plaintext, NOT RF sniffing) to confirm: audio RFCOMM channel#/UUID, `AudioData` framing, SBC params, and **whether `c1.TX_AUDIO` keys via GAIA** (→ flip `KeyingMode` to Explicit) or keying is implicit (benlink's evidence). This is the gate before the operator's first on-air run; it is NOT a code blocker (agent has no radio; ADR 0018).

## Worktree state at handoff

- `worktrees/bd-tuxlink-bcsy-sstv-audio-transport` — branch `bd-tuxlink-bcsy/sstv-audio-transport`, PR #668 open (draft), pushed. Gitignored on disk: `node_modules/` (installed for the pre-push docs linter — `tsx`), `target/` (none yet — no local build), `dev/adversarial/` (Codex quota stub). No at-risk untracked content. Active worktree (work in progress) — do NOT dispose.
- The two prior merged-dead worktrees (`2f2n`, `ve3j`) from the taiga-marsh-kite handoff remain undisposed (separate cleanup; archive `benshi-re` first per ADR 0009).

Agent: opossum-yew-juniper
