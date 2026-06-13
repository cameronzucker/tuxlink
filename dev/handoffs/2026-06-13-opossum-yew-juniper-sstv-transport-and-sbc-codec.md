# Session handoff — opossum-yew-juniper — 2026-06-13

Long, high-output session on SSTV inline images (`tuxlink-bcsy`). Shipped the **entire UV-Pro audio transport to `main`** (6 tasks, 2 PRs merged), then built the **SBC codec** (decode verified; encoder functional + wired) on a draft PR. The operator repeatedly steered toward sustained building (see the new memory [[feedback_no_premature_handoff_without_context_pressure]]).

## On `main` now (merged, CI-green both arches)

- **PR #668** (`f8bc840c`) — audio-transport foundation: `uvpro/audio/{framing,codec,transport}.rs` + `rfcomm.rs` audio-gateway SDP resolution. HDLC framing (byte-verified vs benlink), `SbcCodec` trait seam, `AudioTransport` TX/RX + **RADIO-1 disarm-on-abort**.
- **PR #671** (`ea8d6352`) — transport wiring: `uvpro/audio/keying.rs` (`c1` opcodes, GAIA command-group 3, RE'd from `v4/l1.java:630`), `UvproSession::open_audio`/`abort_audio`/`send_audio_pcm`/`finish_audio` (2nd RFCOMM socket to the same radio; does NOT re-acquire `UvproLinkLock`; `disconnect` aborts audio first). ~30 transport unit tests total.

## On PR #673 (draft, `bd-tuxlink-vgvn/sbc-codec`) — the SBC codec

Branch is off `main` @ #668 (before #671 — trivial `audio/mod.rs` merge: #671 adds `keying`, this adds `sbc`). Worktree: `worktrees/bd-tuxlink-vgvn-sbc-codec` (ACTIVE — do not dispose).

- **Decode** (`uvpro/audio/sbc.rs`, `decode_sbc`/`UvproSbcCodec::decode`): `mini_sbc` (pure-Rust, MIT/Apache, GPL-3 compatible). **Verified** against ffmpeg golden vectors — MAE 6.7 after the ~137-sample SBC synthesis-filterbank delay.
- **Encode** (`UvproSbcCodec::encode`): from-scratch pure-Rust port — analysis filterbank (proto_8_80 window) + Loudness bit-allocation (port of `mini_sbc::calculate_bits`) + quantize (inverse of mini_sbc dequant, `scale=4`=2^EXTRA_BITS) + bitstream pack + **CRC-8** (poly 0x1D, init 0x0F), frames padded to the 40-byte `frame_length`. **Verified in the standalone harness: 32/32 frames pass `mini_sbc`'s CRC-CHECKING decode** → produces valid, radio-decodable SBC.
- **`UvproSbcCodec`** implements the transport's `SbcCodec` trait with streaming state (filterbank + residual buffer via interior mutability). Ready to inject at `UvproSession::open_audio(Arc::new(UvproSbcCodec::new()))` (the UI does this — `tuxlink-yfyn`).
- Golden fixtures + `dev/tools/gen-sbc-golden-vectors.sh` + the standalone iteration crate `dev/tools/sbc-proto/` (builds in 3s, NOT CI-built — no root workspace; the place to iterate the encoder) + the plan `docs/superpowers/plans/2026-06-13-sbc-codec.md`, all committed.

### CI status (PR #673)
<!-- CI673 -->
**CI-GREEN** on `dc998c8f` — all 4 checks pass (build-linux + verify, amd64+arm64). Took 4 rounds: a `Default`-derive on `[f64;80]` (arrays >32 don't derive Default → manual impl), two `needless_range_loop`, two `useless_vec` in tests — all the `no_cold_cargo` tax (mechanical lints, not logic; the codec logic was proven in the standalone crate). Also merged `origin/main` twice to resolve conflicts (audio/mod.rs keying+sbc; then Cargo.lock for release 0.60.0 + #674). PR is `MERGEABLE`.

**Merge decision (operator):** the codec is dormant (no caller injects `UvproSbcCodec` until the UI/yfyn), so merging is safe — BUT the encoder is known-imperfect (MAE ~156) and the full-image quality gate + Codex adrev have NOT run. Recommend validating via the SSTV-codec full-image round-trip (st5n) + the adrev BEFORE merging this transmitted-audio encoder to main; merge-now is acceptable if unblocking st5n/yfyn is preferred (they can also branch off this PR).

## KNOWN-OPEN — encoder quality (not a bug, a refinement)

The encoder round-trips faithfully (decodable, CRC-valid) but a 1 kHz calibration tone reconstructs at **MAE ~156 / peak-err ~9850 vs ffmpeg's 6.7** — fine-grained quantization/scale-factor refinement (the analysis CONVENTION is confirmed correct via a variant scan). **The decisive quality gate is a full-image round-trip, not tone MAE**: SSTV decode is STFT (frequency-domain), expected robust to amplitude error. So:
- Next: build the **SSTV codec (`tuxlink-st5n`)** and run image → SSTV-encode → `UvproSbcCodec` → decode → SSTV-decode → image. If the image survives, the encoder is DONE; if not, the harness `dev/tools/sbc-proto/` is set up to refine it.

## Deferred

- **Codex adversarial review** of the transport (on `main`) + the codec — quota-blocked this session (reset ~1:49 PM); per `codex_quota_gotcha` do NOT substitute a Claude agent. Attack angles: RADIO-1 abort/runaway-TX, two-socket concurrency, wire correctness, the encoder's transmitted-audio correctness.
- **SSTV codec** (`tuxlink-st5n`, PCM↔image, HTCommander port) → **inline UI** (`tuxlink-yfyn`, deps on bcsy+vgvn+st5n) → the **whole-feature `wire-walk` gate**.
- Operator HCI snoop before any on-air run (audio channel#/UUID + Implicit-vs-`c1`-GAIA keying confirmation) — confirm-before-transmit, not a code blocker (ADR 0018).

## Worktree state
- `worktrees/bd-tuxlink-vgvn-sbc-codec` — ACTIVE, PR #673 draft. Gitignored on disk: `node_modules/` (docs linter), `target/`, `dev/scratch/sbc-proto/` (the scratch copy of the harness; the TRACKED copy is `dev/tools/sbc-proto/`). No at-risk untracked content.
- Two merged-dead worktrees (#668, #671) already disposed this session.

Agent: opossum-yew-juniper
