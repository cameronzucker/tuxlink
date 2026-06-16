# Handoff — 2026-06-16 — atoll-towhee-juniper

**One-line:** ARDOP went from "the creator can't even configure it" to **validated transmitting correct ARDOP on real hardware**, plus three real bugs root-caused and **shipped to `main`** (UI freeze/abort, status-bar Connect, Outbox-needs-auth). Still a long way from ARDOP "done" — chipping steadily.

## Session framing
Started from the operator connecting a **Digirig + Xiegu G90** and being unable to configure ARDOP. Worked outward from there into hardware config, a TX-validation harness, and a string of bugs the testing surfaced. The operator was running parallel sessions (bl01 / hw-webgl / hoi1) the whole time.

## Shipped to `origin/main` (merged this session)
| PR | Issue | What |
|---|---|---|
| #728 | tuxlink-ab9h | **ARDOP connect/disconnect/b2f → async + `spawn_blocking`.** The UI froze for the whole TX and the Stop button only appeared after the handshake failed — i.e. **no working abort during transmission**. Root cause: synchronous `#[tauri::command]`s ran blocking I/O on the WebKitGTK main thread. |
| #742 | tuxlink-vu97 (P0) | **Status-bar Connect now one-click fires the last-selected session of ANY mode** (ARDOP/VARA/packet, not just Telnet-CMS), pane closed, dialing the configured target (persisted to localStorage). Per-mode Abort wired. `src/connections/connectDispatch.ts` + AppShell rewire. |
| #743 | tuxlink-spbw (P1) | **Outbox queue works without authentication.** `send_message` required an authenticated active identity to derive From; an RF-only/offline operator (callsign set, no CMS password) couldn't even queue a draft. Now falls back to `config.identity.active_full`. **Closed GH #691.** |
| #730 | tuxlink-0kew | Collapsible "Radio" config group (reclaim ARDOP panel real estate). |
| #731/#734/#723 | dependabot | dirs / toml / reqwest bumps. |

## Dependency triage (operator directives)
- **#732 (libheif-rs 1.1.0) CLOSED** — would raise the system `libheif ≥ 1.18` floor above ECT/Ubuntu-24.04's 1.17.6 → breaks the `.deb` build. New durable rule in memory: **ECT is a hard support target; reject system-C-lib version floors above ECT's base.** (See `project_ect_hard_support_target.md`.)
- **#733 (zip 2→8) CLOSED** — routine 6-major jump, no security driver; pure churn risk mid-alpha (operator: N).
- **tuxlink-qag3** filed — flaky test `managed_direwolf::spawn_device_busy_short_circuits` hardcodes a TCP port → port-bind race; reddens CI intermittently. Not yet fixed.

## ARDOP — the bigger arc (NOT done)
- **TX validated for real:** ardopcf builds + keys the G90; the transmitted waveform decodes as **correct ARDOP** — confirmed by the operator on **SDR Console V3** (RTL-SDR **V3**, direct-sampling/quadrature, USB, 20m @ 14.225). Dummy load, 1 W, supervised.
- **Config that works (Digirig + G90):** Binary = full path to `ardopcf` (the spike build at `/home/administrator/Code/ardopcf-spike/build/linux/ardopcf` — **not on $PATH**; a bundled sidecar is the real productization fix, not yet done); Capture+Playback = `plughw:CARD=Device,DEV=0` (the Digirig C-Media card, **same device both directions**); PTT = `/dev/ttyUSB0` (Digirig CP210x, RTS keying); cmd 8515; WebGUI blank.
- **Pi-side SDR decode harness** (`dev/scratch/ardop-rx-sdr.py` GNU Radio direct-sampling USB demod + `dev/scratch/ardop-rx-monitor.sh` 2nd ardopcf in RXO): detects the ARDOP **leader** reliably but **frame-decode failed** on the Pi (likely 8-bit ADC overload from the strong bench signal, or demod tuning). TX validation ultimately succeeded via SDR Console instead. The harness is reusable (operator noted it suits the **Sonde** project's TX validation — do that from a session rooted in the sonde repo).

## Pending — operator on-air smokes (CI proved the code; these are runtime/RF)
1. **#728 abort:** confirm Stop halts an ARDOP TX **mid-connect** on a converged build (the real RADIO-1 working-abort check).
2. **#742 status-bar one-click ARDOP:** the ribbon runs `modem_ardop_connect` then `modem_ardop_b2f_exchange` **back-to-back** vs the panel's two spaced clicks — verify the timing on-air (may need a readiness wait between).
3. **#743 Outbox:** Post-to-Outbox with a configured callsign and **no CMS password** should now queue cleanly.

## Pending — design/feature still owed
- **The ARDOP config UI redesign brainstorm** (the original "I made this and can't configure it"). The collapsible group (#730) was a pre-redesign quick win; the actual redesign (auto-detect interface, hide ardopcf internals like Binary/cmd-port/webgui, single audio-device pick, a Test affordance) has **not** been started. This wants the brainstorming skill + visual companion.
- **ardopcf sidecar bundling** so the Binary field isn't a hand-typed absolute path.

## Repo / worktree state
- Main checkout on `bd-tuxlink-xygm/recover-handoffs` (its own line; NOT on main). Working tree: pre-existing untracked handoffs/mocks/bug-hunts + this session's `dev/scratch/` additions (ardop-rx-sdr.py, ardop-rx-monitor.sh, 691-error.png).
- **Disposed-pending worktrees:** `worktrees/bd-tuxlink-vu97-*` and `worktrees/bd-tuxlink-spbw-*` — both on **merged/dead branches**, inert. Their ADR-0009 disposal (rm -rf + prune + branch -d) was **hook-blocked** because the operator's parallel sessions hold the main-checkout lease. Harmless; clean up when the lease frees. No untracked/dirty content in either (verified).
- Operator's own active worktrees (4pdu/hw-webgl, bl01 ×2, hoi1) are NOT mine — leave them.

## Memory written this session
- `project_ect_hard_support_target.md` — ECT is a hard support target.
- `project_statusbar_connect_fires_last_session.md` — the status-bar Connect product requirement.
- (Reinforced) RADIO-1 governs **agent** behavior, not the app/operator — corrected a memory that had mis-framed it as an app consent gate.
