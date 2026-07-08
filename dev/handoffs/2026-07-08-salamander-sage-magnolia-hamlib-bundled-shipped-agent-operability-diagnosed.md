# Handoff — 2026-07-08 — salamander-sage-magnolia

**Two things happened this session: (1) shipped bundled hamlib end-to-end (0.84.0 → 0.85.0), (2) diagnosed — but did NOT build — a cluster of agent-operability gaps that a live VARA HF agent-mode alpha test exposed.** The build of that cluster is the next session's job, and it needs the operator's design input first.

---

## SHIPPED THIS SESSION (done, merged, cut, verified)

- **0.84.0** — bundle hamlib `rigctl`/`rigctld` as Tauri sidecars (`tuxlink-rigctld`/`tuxlink-rigctl`), **drop the conflicting system hamlib dependency**. Fixes the R2 install failure (0.83.0 refused to install where hamlib already existed). PR **#1044** (bd tuxlink-a9ip3, closed). Went through 5-round adrev + empirical arm64 static build; CI harness bugs found+fixed (ldd loader-path, libusb pkg name, rigctl banner in smoke). Verified on R2: installs clean, `rig_status configured:true`.
- **0.85.0** — bump bundled hamlib **4.6.2 → 4.7.2** + add **FTX-1 (model 1051)** to the CI `--dump-caps` capability gate. Closes issue **#1046** (Jerry WF5W, needs 4.7+ for the Yaesu FTX-1). PRs **#1047** (bump, typed `build:` so it didn't trigger release-please) + **#1048** (`feat:` release-trigger). bd tuxlink-13pno (closed). Cut + provenance-verified (`.deb` ships `tuxlink-rigctld`, reports `Hamlib 4.7.2`, no `libhamlib-utils`).
- **GitHub Discussions enabled** (`has_discussions=true`) for Jerry's community question.
- **Lesson banked:** release-please only cuts on `feat:`/`fix:` commit types — a user-facing change typed `build:`/`chore:`/`docs:` will NOT auto-propose a version (that's why #1047 needed the #1048 trigger).

## THE NEXT-SESSION WORK — agent-operability cluster (diagnosed, not built)

The operator ran an ambitious agent-mode VARA HF test on R2 (Elmer in-app assistant, backed by Opus-4.8 API, armed 1h under supervised Part 97). It **failed**, and mapped four real gaps in the agent's `perceive → configure → operate → report` loop. Evidence: R2 `~/Documents/tuxlink_0.85.0_VARA_HF_agent_mode-plumbing bug.pdf` (Winlink msg ID ALH7GW0Q47D7).

### tuxlink-7ppfq (P1) — agent is blind to the LIVE VARA (the blocker)

**Root cause (code + runtime, cross-validated — read the bd `--notes`):**
- There is **ONE** shared `Arc<VaraSession>` (`lib.rs:792`); the in-process MCP server reads the *same* object the UI mutates. **Not** a two-object silo.
- `vara_status` reports "does Tuxlink hold an *open cmd-port session* right now?" — which is `Closed` at rest (normal between/after exchanges). It does **not** mean "is a VARA modem running." (`mcp_ports.rs:213-232`)
- `vara_engine_available` reports "is the vendored **installer** bundled in this build?" (`resolve_engine().is_ok()`, `install.rs:127`) — a provisioning probe that never touches the network. False-negative for "is VARA running."
- `modem_get_status.kind` is **hardcoded `"ardop"`** (`mcp_ports.rs:207`).
- SSH-verified on R2: `VARA.exe` live at `~/.wine-vara/drive_c/VARA/VARA.exe`, TNC **8300 (cmd) + 8301 (data) LISTENING**. So the operator's ground truth ("VARA was configured, connected, reading FT-710 audio, ready to send") is correct; all three tool outputs were false-negatives.

**⚠️ CRITICAL SCOPE CONSTRAINT (operator, explicit): the fix must be ADDITIVE. Do NOT redefine `vara_engine_available` into a liveness probe** — it is the **CONFIGURE gate** ("is VARA present / can the agent provision it?", test step 5). Support **both** functions:
- **Configure path (keep):** `vara_engine_available` = installer/provisioning present.
- **Send path (add):** a NEW `vara_reachable`/`vara_tnc_status` = "a VARA TNC answers on `host:cmd_port`" (TCP probe `127.0.0.1:8300`, **read-only, must NOT acquire the session mutex** at `vara/commands.rs:1577` or it races open/close, **transmits nothing → no RADIO-1 gate**). `vara_status` gains a `reachable` field ALONGSIDE (not replacing) `connected`.
- Also fix `modem_get_status.kind` hardcode via an active-modem source of truth.

### tuxlink-z2nwx (P2) — no agent print/print-to-file tool

Elmer had no filesystem-write/PDF tool (only `message_attachment_save`, which extracts an existing attachment). It couldn't produce the report step 9 wanted; it staged an unsent Winlink message instead. Add an agent-invokable write-report-to-file/print MCP tool (format md/txt vs PDF, sandboxed path, surface path back — all TBD with operator).

### tuxlink-77seh (P2) — agent can't disambiguate audio / guide setup

Test step 6 (deliberate, a common "how do I set this up?" ask) failed: the agent's audio surface was too coarse ("all USB PnP Sound Device"), so it fell back to proposing a manual unplug test. The disambiguating detail exists and is cheap (SSH-verified): USB `VID:PID` (`0d8c:013a` vs `0d8c:0013`), USB port path, capture/playback in-use via `/proc/asound` + `fuser`. **Operator: do NOT ID devices for them — the AGENT must be able to.** Give the agent a rich audio-device surface + a disambiguation method so it can advise VARA Input/Output (same full-duplex card). Operator knows their own devices; this is for new users.

### Design calls to SETTLE WITH THE OPERATOR before writing code (brainstorm first)

The operator was actively shaping design (the additive correction; "agent must do the audio") — do NOT skip straight to building.
1. `vara_reachable` contract: connect-only vs a cmd handshake; timeout; reading `host:cmd_port` from config.
2. active-modem SoT: managed vs persisted; running-vs-selected semantics (the frontend `AppShell.tsx` write-point is the one place a fix regresses if missed).
3. print tool: format, sandbox path, path surfacing.
4. audio surface: how much detail; whether it ships a guided disambiguation method.

**Build order:** 7ppfq first (unblocks the send test — agent finally sees the live VARA), then z2nwx + 77seh. Each through build-robust-features (adrev + CI), each preserving the configure path.

## R2 BENCH STATE (live, ready for the next test)

Operator confirmed R2 is "still configured like that." SSH `administrator@r2-poe` (key `~/.ssh/id_ed25519_r2poe`). Read-only inspection is fine; **transmit is operator-only** (armed, supervised).
- `tuxlink` 0.85.0 running (pid was 5671); bundled `tuxlink-rigctld` = Hamlib 4.7.2, rig control works.
- `VARA.exe` live under WINE at `~/.wine-vara`; TNC 8300/8301 LISTENING (no persistent client attached at rest).
- FT-710 on 40m/20m, low Delta Loop, 50W (Phoenix/July heat-limited). DRA-100 = audio; FT-710 native USB = CAT/RTS PTT.
- Two USB audio cards, distinguishable: card1 `0d8c:013a` "USB PnP Sound Device" (usb path -3); card2 `0d8c:0013` "USB Audio Device" (usb path -7.2). PipeWire (pid 1401) routes; verify VARA Input AND Output = the DRA-100 (RX worked, TX untested; PipeWire showed capture on card1 / playback on card2).

## GIT / WORKTREE / TRACKER STATE

- Branches merged-dead (PRs merged): `bd-tuxlink-a9ip3/hamlib-bundled-sidecar`, `bd-tuxlink-13pno/hamlib-472-bump`, `bd-tuxlink-13pno/release-085-trigger`. Local worktrees for these are **disposal candidates** (ADR 0009 ritual — `git worktree remove` is hook-banned). Enumerated here per ADR 0009; next session or operator can dispose. Untracked/gitignored-stateful content in them: `node_modules/` (a9ip3, 13pno-bump, this handoff wt had `pnpm install`), the usual `target/` build caches. Nothing else at risk.
- This handoff lands on `bd-tuxlink-7ppfq/session-handoff` (off main). The next session building 7ppfq can continue on it or branch fresh.
- bd: a9ip3 + 13pno closed. Open for next session: **7ppfq (P1), z2nwx (P2), 77seh (P2)** — this cluster. Also open (pre-existing, unrelated): the earlier hamlib-sidecar follow-ups (model-# migration / CI cache / license provenance), Elmer items (0mudm, gag8u), P2P (sg5zw.8).

## PENDING DECISIONS

- Jerry's #1046 Discussions/Q&A reply — operator's to post (I offered to draft; not posted).
- Whether `vara_reachable` + the audio surface become one PR or separate (recommend: 7ppfq alone first).

Agent: salamander-sage-magnolia
