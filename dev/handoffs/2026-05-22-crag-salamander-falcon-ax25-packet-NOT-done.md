# Handoff — 2026-05-22 — crag-salamander-falcon — AX.25 packet: backend built, integration NOT done

**Read this, not the rosy `*-p1-p4-complete.md` handoff from earlier this session.** That one declared "done" prematurely. The truth: a lot of code exists and unit-tests pass, but **the packet feature has never run end-to-end** — not against a real KISS modem, not on hardware, not on-air. The operator ran the app and found the UI shipped as a non-functional mockup. Several basic-UX defects were fixed this session, but the feature is **not demonstrably working**.

**Operator's standing directives (2026-05-22, hard):**
- **BUILD THE FEATURE. NO MOCK HARNESSES.** Mocks here get wired to prod wrong or aren't useful. Build the real integration chain.
- **Validate by RUNNING it, not by declaring done.** Every defect this session was found by the operator running the app; unit tests passed the whole time. Do not hand the operator "go test it" until the thing actually works when you run it.
- The test ladder is: unit/scripted (done) → **real KISS modem on localhost (TCP/Dire Wolf — no RF)** → hardware (USB/BT) → on-air (RADIO-1). We are NOT past rung 1 in any proven sense. Stop jumping to "test on-air."

## Branch / state
`bd-tuxlink-7fr/ax25-packet` @ `406fe5d`, pushed. PR **#115** open against `main` — but **CONFLICTING** (origin/main moved to `420a16e`; needs a `git merge origin/main` conflict resolution to land — rebase is banned, branch is pushed). Worktree `worktrees/bd-tuxlink-7fr-ax25-packet`. Epic `tuxlink-7fr` OPEN.

## What exists (committed/pushed; unit/loopback-tested ONLY)
- **P1 codec** (`winlink/ax25/{frame,kiss}.rs`) — bit layouts verified vs decompiled TNCKissInterface.
- **P2 datalink state machine + transports** — got an opus review + a cross-provider Codex round that found **9 real ARQ bugs, all fixed** (mod-8 retransmit/ack wrap, foreign-frame routing, RNR backpressure, inbound DISC, P/F, bounded teardown, etc.). 65 ax25 tests.
- **P3 Winlink integration** — `ExchangeRole`, `[packet]` config, `TransportConfig::Packet`, `native_packet_exchange/connect`, Tauri commands.
- **P4 UI + this session's fixes (PR #115):** controlled modem inputs (fixed the `127.0.0.1`-into-USB/BT leak); honest status (no false "Listening"); device picker **classified** by kind (USB/Bluetooth/UART, labeled, separate tabs); digipeater dup removed; **real Listen mode** — `packet_list_serial_devices`, `packet_listen` command, link-close abort (`recv_frame` Ok(0)→ConnectionAborted + `connect_link_with_abort` + abort_handle wiring), honest armed-state UI control.

## What is NOT built / broken / unproven — THE INTEGRATION CHAIN TO BUILD
1. **`bd` P1 — the real end-to-end chain:** nothing has completed a connect or a listen against any KISS modem. Make it actually work (start with TCP — the most-complete transport, the only one where Stop works). This is the headline remaining work.
2. **Real Bluetooth/USB connect flow (`bd` P2):** tuxlink does NOT meaningfully connect a BT TNC — the pair/`rfcomm bind` step is unsupported manual OS work; the app only scans `/dev` for an already-bound node. `serialBaud` is meaningless for RFCOMM yet passed to `serialport::open()` (untested). USB open never run on hardware either. Decide + build a real flow.
3. **Live status feed (`bd` P2):** ribbon/panel show static state; the Listen "armed" is frontend-local only. No backend→UI feed of real connect/listen activity. Build it.
4. **Serial/BT Stop (filed):** abort only closes the link over TCP; serial/BT Stop is a no-op.
5. **Spec §9 / earlier carry-forwards:** the `read()` Ok(0) contract was changed this session (now errors on close — re-check P4 consumers); REJ-per-gap dedup; `tuxlink-e13` (trunk integration tests don't compile after 686's `position_source`).

## Hard environment constraints (these bit us repeatedly — see memories)
- **Disk:** `CARGO_TARGET_DIR=/home/administrator/.cache/tuxlink-cargo-target` on EVERY cargo command. NO full/`--all-targets`/clippy-all builds; scope to `--lib <filter>`. Operator watches `btop`, reclaims space manually, escalated 3× this session. Read memory `feedback_shared_cargo_target_dir` IN FULL.
- **Port 1420:** every worktree's `tauri dev` binds Vite :1420 (strictPort) → **one build at a time machine-wide**. "Feature missing/broken" is often the operator on a DIFFERENT worktree's build. Memory `project_worktree_dev_port_collision`.
- **RADIO-1:** build + commit the transmit path; NEVER run it. Operator runs builds + all transmission.
- Don't edit files under the operator's running dev server without coordination; don't screenshot their screen unprompted.

## Working tree
Clean, pushed. Local-only scratch: `dev/scratch/winlink-re/` (decompiled RMS Express, gitignored), `dev/scratch/*.png` (screenshots), `dev/adversarial/*-codex.md` (the P2 Codex transcript, gitignored). Other worktrees (39b, dj6, 686, etc.) are other sessions — not touched.
