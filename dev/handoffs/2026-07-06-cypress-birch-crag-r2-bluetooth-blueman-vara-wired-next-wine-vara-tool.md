# Handoff — 2026-07-06 — cypress-birch-crag

**Session was R2 infra + shipping-state investigation (no tuxlink source changes).**
Working tree has pre-existing dirt (89 files) from other sessions — not this session's.
A concurrent worktree session (`bd-tuxlink-xnenf/ctx-meter`, the Elmer context-meter work)
is live; do not touch its lane.

---

## NEXT-SESSION BUILD TARGET (operator-requested)

**Stand up a NEW, separate, PUBLIC repo (local + remote) for a WINE-VARA auto-deploy tool.**
- Automates installing + configuring **VARA HF under WINE on Linux** (the fiddly `.wine-vara`
  prefix dance the operator did by hand on r2-poe). "High-leverage, people will love it."
- Framing: a **reusable community primitive** (open-source, helps many hams) that **Tuxlink
  can also consume** — Tuxlink deliberately does NOT manage VARA (treats it as a third-party
  external process; see below), so this tool fills that gap without changing that posture.
- Operator wants ~**1 hour, semi-autonomous** build while they do other things.
- **DISCIPLINE:** brainstorm/scope FIRST (project rule: brainstorm before creative work) —
  what it installs, how it detects/handles the VARA installer + wine prefix + registration,
  idempotency, how Tuxlink would call it — THEN build. It's a NEW repo, so **root the session
  in the new repo's directory** (sibling-repo rule: beside-tuxlink work is rooted THERE, not
  in the tuxlink checkout).

---

## This session's findings (shipping state, verified vs origin/main)

### R2 (r2-poe) Bluetooth over VNC — RESOLVED
- Root cause: GNOME blanks its built-in Bluetooth/WiFi panels in **remote/seatless** sessions
  (TigerVNC `:1` = `Remote=yes`, no seat). Upstream GNOME behavior, not a defect (the Pi works
  over VNC because it runs labwc, not GNOME's seat-gated panel).
- WRONG turn (cost hours): forcing a local seated session (autologin + Xorg + x11vnc on `:0`)
  made GNOME's panel appear but tanked VNC perf (mirroring a real 1080p framebuffer, `-noshm`).
- RIGHT answer: **use blueman** — talks to BlueZ directly, NOT seat-gated, works in the fast
  TigerVNC session. **Reverted** to TigerVNC `:1` (port 5901, pw `vncbench`) + blueman.
  Saved to memory: `project_r2_poe_bluetooth_over_vnc_blueman`.

### Tuxlink CLI — does NOT exist
- GUI-only Tauri app; only CLI hook is `uninstall_cleanup::handle_cli` (packaging).
- Dev probes exist: **`native_cms_probe`** = headless real CMS **Telnet** session driver
  (defers all inbound, prints B2F wire log — RADIO-1-free, internet not RF); **`vara_tcp_probe`**
  = VARA TCP codec/plumbing only (no CONNECT); `tuxlink-gps-fix` = GPS util.
- Catch: dev bins need `cargo run`; R2's rustc reportedly too old to build the workspace.

### VARA → CMS — WIRED end-to-end (not the Phase-2 dead-end a stale comment implies)
- `VaraRadioPanel` Send/Receive → `modem_vara_b2f_exchange` (registered `lib.rs:2047`, also MCP)
  → `run_vara_b2f_with_transport` (`commands.rs:1797`): walks dial candidates, CAT-tunes rig,
  issues `CONNECT`, waits `CONNECTED` (bounded by `VARA_CONNECT_DEADLINE`), runs B2F over data
  socket, disconnects. Operator flow: **Open Session first**, then Send/Receive.
- "Likely to work OTA?" caveats: **never run OTA**; recent handoffs flag **"flaky CONNECT is
  the wall"** (expect CONNECT to be make-or-break); **no per-invocation consent token** (test
  `modem_vara_b2f_exchange_signature_has_no_consent_token`) — asymmetric with ARDOP's gated
  connect, worth a conscious call for the "approved" build; R2 (x86 WINE) is the RIGHT host
  (box64 VARA-TX block is Pi-only).
- **VARA-via-WINE is manual host setup, NOT a Tuxlink feature** — this is what motivates the
  new auto-deploy tool above.

---

## Operator's own plan (context; not my work items)
1. Run the CMS acceptance battery from the R2 against real gateways — see what artifacts
   actually export, and **first-ever VARA OTA**.
2. Finish all in-flight work (Elmer via the ctx-meter worktree agent; Telnet P2P/peering is
   unfinished per stale parallel sessions — operator won't ship "approved" with that hole).
3. Cut **0.82.0** and tag it as the named CMS-acceptance build for the maintainers to reference.

## Git / tracker state
- Branch `bd-tuxlink-ant8s/ardop-connect-fixes` (this handoff landed server-side on a handoff
  branch to avoid main-checkout contention). Working tree dirt is other sessions' — not mine.
- bd (from earlier this conversation): `tuxlink-o98yl` (rmcp 2.0, P2), `tuxlink-vdzbs` (pbf 5, P3),
  `tuxlink-qg5im` (corosync recovery, P2). Dependabot #960/#875 still open (real major-break migrations).
- r2-poe: TigerVNC `:1` + blueman working; relocated/relocatable to the radio bench.

Agent: cypress-birch-crag
