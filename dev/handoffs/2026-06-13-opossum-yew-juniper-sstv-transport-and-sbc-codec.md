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

**Merge decision (operator, 2026-06-13):** Codex was quota-blocked, so a **self-adrev** was run instead (fresh-context reviewer agent over `sbc.rs` + the mini_sbc reference) and #673 merged on green-with-no-major-findings. The review found **one P0** — `decode` panicked on a truncated/garbage SBC frame body (`for block in frame` used mini_sbc's `Iterator` impl, which `.unwrap()`s a mid-frame read error), violating the "one corrupt RX frame can't kill the receive loop" contract on the lossy-RF path. **Fixed** (commit `190fbba4`: fallible `frame.next()` + break; regression test; verified no-panic in the standalone harness). Allocation port, CRC, quantize, framing, streaming all audited clean. One P2 (odd-byte PCM input silently dropped — out-of-contract, the transport hands whole samples) left as a documented minor. Encoder amplitude refinement (MAE ~156) remains separately tracked, gated on the full-image round-trip.

## KNOWN-OPEN — encoder quality (not a bug, a refinement)

The encoder round-trips faithfully (decodable, CRC-valid) but a 1 kHz calibration tone reconstructs at **MAE ~156 / peak-err ~9850 vs ffmpeg's 6.7** — fine-grained quantization/scale-factor refinement (the analysis CONVENTION is confirmed correct via a variant scan). **The decisive quality gate is a full-image round-trip, not tone MAE**: SSTV decode is STFT (frequency-domain), expected robust to amplitude error. So:
- Next: build the **SSTV codec (`tuxlink-st5n`)** and run image → SSTV-encode → `UvproSbcCodec` → decode → SSTV-decode → image. If the image survives, the encoder is DONE; if not, the harness `dev/tools/sbc-proto/` is set up to refine it.

## ▶ START HERE next session — SSTV codec (`tuxlink-st5n`)

The transport + SBC codec are done/merged. The next build piece is the **SSTV codec**, which is *also the quality gate that closes out the SBC encoder*.

1. **Build `tuxlink-st5n`** — pure-Rust PCM↔image SSTV. Port from HTCommander C# (`dev/scratch/benshi-re/HTCommander/src/SSTV/`: `Encoder.cs`, `Robot_72_Color`, `ShortTimeFourierTransform.cs`; `docs/SSTV.md`). Encode at least **Robot36 + one PD mode**; decode via **STFT**. 32 kHz mono PCM (matches the audio path). Golden-vector + round-trip tested. Use the standalone-fast-iteration-crate pattern (`dev/tools/sbc-proto/` is the template) if DSP iteration is needed under `no_cold_cargo`.
2. **Full-image quality gate (decisive):** image → SSTV-encode → `UvproSbcCodec::encode` → `decode` → SSTV-decode → image. **This determines whether the SBC encoder's MAE-156 matters.** If the image is clean, the encoder is DONE; if corrupted, refine the encoder in `dev/tools/sbc-proto/` (analysis-filterbank fixed-point precision / scale).
3. Then **inline UI** (`tuxlink-yfyn`, deps bcsy+vgvn+st5n) — composer attach + inbound thumbnail in `AprsChatPanel`; **inject the codec** at `UvproSession::open_audio(Arc::new(UvproSbcCodec::new()))`. The **whole-feature `wire-walk` gate** fires here.

## Deferred (gates, not build pieces)

- **Codex adversarial review** of the *transport* (on `main`) — still quota-deferred. The *codec* was self-adrev'd this session (P0 fixed, see merge note); the transport adrev is lower-urgency (structurally simpler, abort logic). Re-run when Codex quota is available; per `codex_quota_gotcha` don't substitute Claude unless the operator directs it (they did for the codec).
- Operator **HCI snoop** before any on-air run (audio channel#/UUID + Implicit-vs-`c1`-GAIA keying confirmation) — confirm-before-transmit, not a code blocker (ADR 0018).

## Worktree state
- `worktrees/bd-tuxlink-vgvn-sbc-codec` — ACTIVE, PR #673 draft. Gitignored on disk: `node_modules/` (docs linter), `target/`, `dev/scratch/sbc-proto/` (the scratch copy of the harness; the TRACKED copy is `dev/tools/sbc-proto/`). No at-risk untracked content.
- Two merged-dead worktrees (#668, #671) already disposed this session.

Agent: opossum-yew-juniper
