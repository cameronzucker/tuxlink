# Handoff — 2026-05-22 — sorrel-moss-hemlock — AX.25 packet: spec + 4 plans (ready to build)

**Session outcome:** Brainstormed AX.25 1200-baud packet end-to-end (RE-grounded design, locked UI), wrote + committed the spec, and produced **all four implementation plans (P1–P4)**. **No production code yet.** Next session: subagent-driven build of P1→P4 in order.

**Branch / worktree:** all work on `bd-tuxlink-7fr/ax25-packet` (off `origin/main`), in `worktrees/bd-tuxlink-7fr-ax25-packet`. Pushed (tip `a93ec49`). bd epic **tuxlink-7fr** (in_progress, claims the worktree).

## ⚠️ Critical context the next session must NOT miss
1. **`origin/main` ALREADY contains the merged native client + v0.0.1 UI** (through PR #110, `d42af8d`). The `0ic` worktree and the May-20 handoffs are **STALE** — do NOT treat the native client as unmerged (I wasted time on that early this session). The `7fr` worktree (off origin/main) has the full `winlink/` stack + UI shell. **No Phase-0 integration is needed — build packet directly here.**
2. **Execute P1→P4 IN ORDER** (bd dep chain): **P1 `tuxlink-drh`** → P2 `tuxlink-wnd` → P3 `tuxlink-031` → P4 `tuxlink-5vx`. `bd ready` surfaces P1. Operator chose **subagent-driven-development**, run in the `7fr` worktree.
3. **RADIO-1:** the agent WRITES + tests via in-memory peer + loopback (no RF); the **operator** runs every on-air step (direct + digipeated connect, P2P call + answer). Never run anything that could transmit.

## Artifacts (all on `7fr`, pushed)
- **Spec:** `docs/design/2026-05-22-ax25-packet-v0.1-design.md`
- **RE findings:** `docs/design/ax25-packet-protocol-findings.md` (raw evidence in `dev/scratch/winlink-re/` is **gitignored, LOCAL TO PANDORA only** — extracted RMS install + decompiled `tnckiss/rms-express/md5lib` trees + the 3 agent findings docs)
- **Plans:** `docs/superpowers/plans/2026-05-22-ax25-packet-p{1,2,3,4}-*.md` — P1 wire codec (10 tasks) / P2 datalink+transports (14) / P3 winlink-integration (9) / P4 inline UI (14)
- **Future issue:** `tuxlink-5jb` — rig/frequency control plane (research; v0.1 ships note-only frequency)

## Design in one paragraph
Host-side AX.25 v2.x **connected-mode** (mod-8, **0–2 digipeaters — load-bearing for the operator's at-home gateway reach**) over **KISS** — the RMS Express model (wl2k-go delegates to kernel/AGWPE; we do **not**). Three KISS byte-pipes: TCP (Dire Wolf/SoundModem), USB-serial, Bluetooth-serial (treated as a serial device). Three modes: **dial gateway** (secure-login) + **dial peer** (no auth) + **listen/answer** (P2P; default-on idle listening). **Global sticky station SSID** (operate as `N7CPZ-7`); the B2F/Winlink identity stays the **base callsign** (`N7CPZ`). Inline UI in a Radio panel (no pop-up windows — operator pet peeve). Reuses `run_exchange` for dialing; adds an **Answer (master) role** for listening.

## Execution-time verification seams (flagged by the plan authors — do NOT skip)
- **Spec §9 verifications:** AX.25 control-byte/address bit layouts vs decompiled `TNCKissInterface` (`dev/scratch/winlink-re/decompiled/tnckiss/`, local) + AX.25 v2.2; FBB turn-order for both roles; KISS ACKMODE; SABM (mod-8), not SABME. The P1 tests encode expected values as fixtures, so a wrong layout fails loudly.
- **Abort-handle seam (P2→P3):** P2's `connect_link` returns `Box<dyn ByteLink>` with no abort hook; P3 wires abort via `Box<dyn AbortHandle>`. Reconcile (extend `connect_link` to also yield a handle, or downcast the `TcpStream`).
- **Tauri arg-key names (P3→P4):** P4 must verify the JS arg-object keys (`call`/`path`/`enabled`/`dto`) against P3's `#[tauri::command]` param names. The command **names** are fixed; the keys need confirming.
- **Packet handshake locator (P3):** P3 set the B2F `locator` empty (valid). Wire the real grid from config/position — coordinate with 686.
- **P2 cross-provider Codex round (P2 Task 14):** the connected-mode state machine is correctness-critical → run the Codex adrev (mind the daily quota).

## Coordination with `tuxlink-686` (position subsystem, in_progress, sibling worktree)
686 (manual grid inline-edit + gpsd source arbiter) and 7fr (packet) both edit `config.rs`, `winlink_backend.rs`, `ui_commands.rs`, `lib.rs`, and the `DashboardRibbon`.
- **P1 (codec) has ZERO overlap** → build it first, freely.
- `lib.rs` `generate_handler!` list = highest textual-conflict risk.
- 686 bumps `CONFIG_SCHEMA_VERSION` (`PrivacyConfig.position_source`); P3 rides past with `#[serde(default)]` (no bump).
- Ribbon: 686 owns Callsign/Grid/Position + the MANUAL/GPS chip; P4 owns the Connection/transport indicator — keep edits non-overlapping.
- **Rebase `7fr` onto `main` AFTER 686 lands** (non-interactive `git rebase main`), then resolve any `lib.rs` handler-list conflict.

## Worktree + working-tree state
- **`7fr` (this session):** clean, pushed. Brainstorm mocks in `.superpowers/brainstorm/…` (gitignored). bd issue tuxlink-7fr claims it.
- **Other worktrees present** (0ic native-winlink-client, 686 position, 882, 22l, etc.) — NOT touched this session; 0ic's content is already merged to main (stale leftover). Disposal out of scope this session.
- **`dev/scratch/winlink-re/`** — gitignored, **local to pandora**: extracted RMS install (`install/`), decompiled .NET trees (`decompiled/`), and `findings/0{1,2,3}-*.md`. The committed findings DOC is the portable synthesis; the raw trees won't exist on another machine.
- **`.dotnet` / `ilspycmd`** installed userland this session (`~/.dotnet`, roll-forward `DOTNET_ROLL_FORWARD=Major`) for decompiling — available for future RE.

## Pending operator decisions
- **None blocking P1.** (P3/P4 merge timing depends on 686 landing — coordination, not a build blocker.)
