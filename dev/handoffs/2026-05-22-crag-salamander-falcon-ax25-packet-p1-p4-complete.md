# Handoff — 2026-05-22 — crag-salamander-falcon — AX.25 packet P1–P4 built end-to-end

**Session outcome:** Executed all four AX.25 packet plans (P1→P4) via subagent-driven-development in the `7fr` worktree. The feature is **code-complete and pushed**, validated by in-memory/loopback tests only. **Pending: operator browser smoke + all on-air steps (RADIO-1), then PR merge.**

**Branch / worktree:** `bd-tuxlink-7fr/ax25-packet` @ `3c2371c`, pushed, clean tree. `worktrees/bd-tuxlink-7fr-ax25-packet`. Built on `origin/main` (fork `d42af8d`) with `origin/main` (9ac3c50 — incl. tuxlink-686 position subsystem) **merged in** (not rebased — branch was pushed, so merge per the destructive-git ban; ADR-0010 merge-commit model).

## What's complete (all pushed)

| Plan | bd issue | Commits | Tests | Notes |
|---|---|---|---|---|
| **P1** wire codec (frames+KISS) | `tuxlink-drh` ✓closed | 10 TDD + 1 fix (`ef72711`) | 22 ax25 | Bit layouts verified vs decompiled `TNCKissInterface`. Quality review found 2 debug-panic bugs (overflow on non-ASCII / nr>7) — fixed. |
| **P2** datalink SM + transports | `tuxlink-wnd` ✓closed | 13 TDD + 1 fix (`60372e7`) | 63 ax25 | **Cross-provider Codex round + opus review found 9 real ARQ defects — ALL fixed via TDD** (see below). |
| **686 merge** | — | merge + test fix (`2e52eeb`) | — | Added `position_source` to PrivacyConfig test literals (trunk was broken — see `tuxlink-e13`). |
| **P3** Winlink integration | `tuxlink-031` ✓closed | 8 TDD (`5a59b09`) | 262 backend | ExchangeRole{Dial,Answer}, `[packet]` config, TransportConfig::Packet, native_packet_exchange/connect, 4 Tauri commands. Locator wired via `cms_locator` (686-aware). |
| **P4** inline UI | `tuxlink-5vx` (open) | 8 TDD (`3c2371c`) | 395 frontend | PacketConnectionPanel (inline, no window), sidebar entry, ribbon/status indicator, session-log packet lines. Vitest-only (no cargo/tauri-dev — disk constraint). |

### P2 — the 9 ARQ defects the Codex round + opus review caught and fixed (`60372e7`)
Critical correctness, several reachable with a *well-behaved* peer (the verbatim plan was a happy-path sketch):
A retransmit_from mod-8 wrap dropped frames · B ack_through out-of-window N(R) guard · C await_ack resent wrong frame on wrap + retransmitted before T1 · D **frames accepted from any source** (foreign-station corruption on shared RF) → src==peer filter · E write() window-stall never retransmitted (+ `attempts as u8` hang) · F maxframe>7 clamp · G P/F final-bit on polled inbound I · H disconnect/Drop blocked past T1 on real link + ignored inbound DISC/DM (root cause: 60s LINK_TIMEOUT incompatible with poll/T1 model → 200ms) · I RNR remote-busy backpressure. Cross-checked against the official client (`Connection.cs` ErrorRecovery/remoteBusy, `Frame.cs`). Raw transcript: `dev/adversarial/2026-05-22-ax25-datalink-codex.md` (gitignored, local).

## Pending — OPERATOR only (RADIO-1 + disk-supervision)
1. **Browser smoke** (`pnpm tauri dev` under btop; agent did NOT run it — disk constraint). Walk the P4 flow: select "Packet (AX.25)" sidebar → inline panel renders (no window); TCP/USB/BT segment swap; SSID → "Operating as N7CPZ-N"; Listen toggle; ≤2 relay chips (add-button hides at cap); Connect; ribbon `Listening · Packet 1200`; status bar `Packet 1200 · Listening as …`; session log stays full-width.
2. **On-air (licensee-run, per-invocation consent):** direct (0-relay) connect to a gateway · digipeated (≥1-relay) connect · P2P dial · P2P **answer** (second station). Agent built + tested in-memory/loopback only; "on-air testing will tell the full story."
3. **PR merge** after 1–2 pass.

## Follow-ups (bd issues + unfiled notes)
- **`tuxlink-e13`** (filed, P2): trunk `cargo test` doesn't compile — 686 added `PrivacyConfig.position_source` but missed 5 integration-test literals (fixed on THIS branch; trunk needs the same + a CI gap closed).
- **Read Ok(0)/EOF contract** (filed): `Ax25Stream::read` returns `Ok(0)` for "no data yet" — P4 wiring of `run_exchange` must not treat it as EOF (documented on `read()`).
- **REJ-per-gap dedup** (filed, minor).
- **Abort-handle (P2→P3) — UNCONFIRMED, needs verification:** P2 left abort to P3; the P3 subagent's commit didn't clearly state whether it generalized `abort_handle` to `Box<dyn AbortHandle>` or scoped abort to TCP-only. **Verify in `winlink_backend.rs` + `winlink/ax25/link.rs`; file a follow-up for serial-link abort if scoped.** (Not a hang risk — P2's 200ms read timeout means a stuck packet connect fails within N2×T1.)
- **P4 v0.1 seams (documented):** ribbon/status-bar show SSID hardcoded `0` (panel shows the real SSID) — needs a packet-status IPC feed; sidebar dot uses the shared CMS `tone` rather than a dedicated packet-listen state.
- **Lint-sweep scope creep (accepted):** P3's `clippy --all-targets -D warnings` gate swept pre-existing lints in `lzhuf.rs/message.rs/telnet.rs/pat_process.rs/build.rs` — verified mechanical/behavior-preserving (Error::other, div_ceil, Default for Message, an #[allow]).

## Disk discipline (load-bearing this session)
Shared `CARGO_TARGET_DIR=/home/administrator/.cache/tuxlink-cargo-target` for ALL cargo. Operator escalated twice over disk burn; P4 ran vitest-only (zero cargo). The shared target dir holds ~15 GB of regenerable artifacts — reclaimable by the operator. `pnpm tauri dev` is operator-run under btop. See memory `feedback_shared_cargo_target_dir`.

## Epic
`tuxlink-7fr` (the feature epic) — **left OPEN**: code complete, but close only after operator on-air validation + PR merge.
