# Handoff — 2026-06-29 — esker-larch-bluff

VARA feature-parity fixes shipped, r2-poe stood up as a VNC test box, and a frontend
design-system consolidation was brainstormed + planned (not yet executed).

## Shipped this session

- **VARA ↔ ARDOP parity (3 bugs) — merged, PR #951, `tuxlink-n95sr` closed.**
  1. Send/Receive failure now routes to the session log (dropped the inline `setActionError`), mirroring ARDOP.
  2. Connect-failure now frees the modem: `VaraExchangeOutcome` enum + extracted `finish_vara_b2f_exchange` (ConnectFailed → `vara_stop_session_inner` + drop transport; mid-exchange/success → re-install). Regression test asserts post-failure STATE. Mirrors ARDOP's B3 fix.
  3. Dashboard/radio control parity: VARA action row now mirrors ARDOP's state machine (Start ⇄ Send/Receive + Stop), plain label, no station relabel, no `flex:1` duplicate.
  CI green both arches; worktree disposed.

- **Frontend design-system plan — PR #953 OPEN (docs only), `tuxlink-9q6ly` in_progress.**
  Office-hours brainstorm (Codex cross-model validated) → spec + plan in the repo:
  - Spec: `docs/superpowers/specs/2026-06-29-frontend-cohesion-design-system-design.md`
  - Plan: `docs/superpowers/plans/2026-06-29-frontend-cohesion-design-system.md`
  Diagnosis: the "AI-generated" look = `.radio-panel-btn{flex:1}` (RadioPanel.css:323) + dash-* 9/10/12/13/14px font soup + no shared primitives/scattered tokens (+ Codex found wizard gradient/animation/radius soup, duplicated dialog chrome, sparkline gradients, sessionlog glyphs, fractional fonts). Approach: phased full design-system, hybrid control layer (tokens+CSS classes additive, thin React wrappers for big-3 later, stylelint guard). First chunk planned = Phase 0 (additive tokens/classes/stylelint-warn) + dashboard-ribbon pilot.

## r2-poe (x86 Ubuntu test box, Tailscale 100.127.88.9) — NOT in the repo

Stood up as a VNC-based test box for VARA-on-WINE / future on-air work. State (all on r2-poe disk, not version-controlled):
- **TigerVNC standalone**, systemd `tigervncserver@:1` (enabled, boots), GNOME-on-Xorg in Xvnc. Listens :5901. `~/.vnc/{xstartup,config,passwd,rsa.pem}`.
- **Encryption:** RSA-AES (RA2_256/RA2) offered for RealVNC Viewer + TLSVnc + VncAuth fallback; currently `SecurityTypes=VncAuth` (operator chose Tailscale-only for now — WireGuard encrypts the wire). RealVNC-Viewer RA2 interop is finicky (sub-auth) — documented in `~/.vnc/config` comments.
- **Snap launch fix (two layered bugs):** `~/.vnc/xstartup` now exports `XAUTHORITY=$HOME/.Xauthority` AND uses the systemd USER bus (`DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus`) instead of `dbus-run-session` — snaps (Firefox, App Center) need the user bus to create their cgroup scope. Verified Firefox launches.
- VARA HF runs under WINE on r2-poe (x86); box64-on-Pi blocks VARA TX, x86 WINE clears it (validated earlier this session-arc).

## Branch / worktree state

- Main checkout: on `bd-tuxlink-ant8s/ardop-connect-fixes` (operator state, stale — ~2495 commits behind origin/main at session start; reads via `git show origin/main`).
- Worktree `worktrees/bd-tuxlink-9q6ly-frontend-design-system` (branch `bd-tuxlink-9q6ly/frontend-design-system`, off origin/main): holds PR #953's plan+spec + this handoff. **Kept** (claimed by in_progress `tuxlink-9q6ly`) so the next session resumes execution here. Untracked: `node_modules/` (gitignored build cache) only.
- `tuxlink-n95sr` worktree: disposed (PR #951 merged).

## Pending / next

1. **Merge PR #953** after a glance (docs only; CI trivial). Then the spec/plan are on main.
2. **Execute the design-system plan** (`tuxlink-9q6ly`): Phase 0 (additive, zero-pixel) then the dashboard-ribbon pilot, via subagent-driven-development or executing-plans. Verify each via the WebKitGTK render harness (`pnpm dev` + `dev/render-harness/snapshot.py ?view=ribbon` — Task 4 adds the ribbon mount). Do NOT freeze the React wrapper API until ribbon + radio panes both pass screenshot review.
3. The gstack toolchain has a big upgrade available (0.15 → 1.58) — operator's call, independent of project work.

## Pending decisions

- Plan Task 5 flags one micro-decision: the 14px callsign — keep prominence via weight at `--type-body` (13px) or add a `--type-strong:14` step. Default in the plan: weight-only.
