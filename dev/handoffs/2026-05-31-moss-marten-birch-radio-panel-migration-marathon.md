# Handoff — moss-marten-birch — radio-panel migration marathon (P1 → P2 → P3+P4)

> **Date:** 2026-05-31 → 2026-06-01 · **Agent:** `moss-marten-birch` · **Machine:** pandora (Pi 5)
>
> **Arc:** started as a single-phase P1 execution; expanded into a multi-hour marathon covering P1 (merged) → P2 (merged) → P3+P4 (DRAFT PR #185, multiple smoke rounds in). Operator's main checkout was on a long-stale `task-amd-main-ui` branch; significant trust-recovery work mid-session. Ended on a low-context wall mid-fourth-smoke-round.
>
> **Status:** P1 + P2 shipped + operator-validated. P3+P4 on DRAFT PR #185 with 4 smoke rounds applied (~22 commits). Two **operator-flagged smoke findings remain open** (filed as bd issues, not yet fixed).

---

## 0. Critical first action — next session

```
1. Read THIS handoff first; do NOT skim.
2. Open PR #185 (bd-tuxlink-p4bl/radio-panel-packet-ardop) — DRAFT, 22+ commits.
3. Fix the two open bd issues from smoke round 4:
     - tuxlink-i63g (SSID dropdown — should show -N only, not full callsign;
                     2-digit SSIDs get clipped by the scroll bar)
     - tuxlink-jmfm (ARDOP controls misplaced under Settings → GPS/Privacy)
4. After fixing tuxlink-i63g, surface to operator for round-5 smoke.
   tuxlink-jmfm is P2; can defer to a follow-up PR.
5. Do NOT auto-merge PR #185 — operator decides via 4+ rounds of smoke now.
6. Do NOT introduce P5 (vocab cleanup) until P3+P4 lands.
```

Paste-ready next-session prompt at the bottom of this doc.

---

## 1. PR state

| PR | Scope | State |
|---|---|---|
| **#173** | P1 — RadioPanel shell + bottom-strip removal | **Merged** to main 2026-05-31 (commit `2404c6d`) |
| **#176** | P2 — TelnetRadioPanel + SessionLogSection + Codex R1 + R2 + operator smoke fixes | **Merged** to main 2026-05-31 (commit `88a8949`) |
| **#185** | **P3 + P4 bundled** — PacketRadioPanel + ArdopRadioPanel + SignalSection + PINGACK Quality + 4 rounds of operator-smoke fixes | **DRAFT — operator-deferred merge** |

PR #185 branch: `bd-tuxlink-p4bl/radio-panel-packet-ardop`. Worktree: `worktrees/bd-tuxlink-p4bl-radio-panel-packet-ardop/`. Operator has been the merge gate per their explicit "DRAFT, no auto-merge" instruction.

---

## 2. P3+P4 commit topology (top 22 on branch)

```
cc82bf4 fix(radio): clamp ARDOP panel content inside the 360px width
d045c58 fix(radio): ARDOP WebGUI button gates on running + adds webgui_port override
64ab42f fix(radio): selects use appearance-none + chevron so they don't read as disabled
022c09d fix(shell): ribbon SSID is single click-to-edit callsign select  ← OPERATOR REJECTED THIS DESIGN (see §4 open issues)
507b32b fix(radio): Clear log drains backend buffer so cleared lines don't reappear
db82383 fix(shell): DashboardRibbon SSID options render bare integer
94bccfe fix(radio): ARDOP Open WebGUI uses tauri-plugin-shell instead of window.open
4c88618 fix(radio): add Radio section to ARDOP panel (audio + PTT inline editor)
f8cb08e fix(radio): bump ARDOP UI font sizes between Signal and log pane
5f27150 fix(radio): add Clear control to session log (panel-local reset)
df09be8 fix(radio): rename Packet Connect button to Start for vocab consistency
a82f620 fix(shell): SSID propagates to ribbon callsign + inline-edit from status pane
4ed69ee fix(packet): AX.25 serial-link baud default 9600→1200 + editable ladder
20ab2b6 fix(radio): restore listenDefault preference + ARQ bandwidth dropdown (Codex R1)
2508789 refactor(shell): delete legacy panels + simplify reading-pane (P3.4 + P4.7)
82cea5a feat(shell): route ARDOP HF to ArdopRadioPanel; remove dual-mount (P4.6)
6c084c1 feat(radio): ArdopRadioPanel — replaces ArdopDock + ArdopHfStub (P4.5)
069245d feat(radio): SignalSection — Quality + S/N trend + frames (P4.4)
ae90839 feat(modem): parse PINGACK + PING events for Quality (P4.3; closes tuxlink-1637)
55dd337 feat(radio): FrameRibbon (P4.2)
171c981 feat(radio): Sparkline (P4.1)
33d1ef6 feat(shell): route Packet → PacketRadioPanel (P3.3)
4876ec2 feat(radio): PacketRadioPanel (P3.2)
e2ca267 feat(radio): ModemLinkSection (P3.1)
```

---

## 3. What's actually working in PR #185

Operator validated smoke rounds 1, 2, 3 confirmed each round of fixes. The current state of the panels:

**TelnetRadioPanel** (from P2 already merged):
- Editable host + quick-pick chips
- Transport radio (TLS · 8773 / Plaintext · 8772)
- Live session log
- Start / Stop actions
- Status bar correctly transitions Connecting → Connected (1.5s held) → Disconnected
- Event-driven backend_status updates

**PacketRadioPanel** (P3):
- ModemLinkSection (TCP/USB/BT picker; baud now defaults 1200)
- SSID picker 0..15
- Listen section (with `Auto-arm Listen at startup` checkbox)
- Connect/Start action (Telnet-vocab parity)
- Live log

**ArdopRadioPanel** (P4):
- Connect section (Target + ARQ bandwidth dropdown)
- Radio section (Audio capture/playback/PTT — inline editor; persists to ardop config)
- Live + Signal + Session log + ARQ state
- Open WebGUI button (gated on running modem; uses Tauri shell plugin)
- WebGUI port now a persisted override on `ArdopUiConfig.webgui_port`; both spawn `-G` and frontend URL go through `resolved_webgui_port()`

**SessionLogSection** (shared):
- Live tail via `useSessionLog` hook (subscribes to `session_log:line` events; snapshot-merge on mount with seq-dedup)
- Clear button that ALSO drains the backend `SessionLogState` buffer via new `session_log_clear` Tauri command

**DashboardRibbon SSID inline edit**:
- 022c09d collapsed callsign+SSID into a single `<select>` whose options render full call (`W7CPZ-0` ... `W7CPZ-15`)
- **OPERATOR REJECTED THIS DESIGN** — see §4

---

## 4. Open issues (filed as bd, NOT YET FIXED)

### `tuxlink-i63g` (P1) — SSID dropdown is wrong shape

Operator quote: *"The SSID dropdown selector still isn't it. The dropdown should JUST be for the -N SSID, and as-built, the scroll bar covers the second digit of double-digit SSIDs such that they can't be read."*

**Diagnosis:** my commit `022c09d` collapsed callsign and SSID into one element where each option shows full `W7CPZ-N`. Operator wants two separate surfaces: a bare callsign chip + an adjacent dropdown that shows ONLY `-0` through `-15`. PacketRadioPanel's panel-side SSID select already follows this pattern (`-${n}` option text); the ribbon should mirror.

Additional rendering issue: at 2-digit SSIDs (`-10` ... `-15`) the dropdown scroll bar visually covers the second digit. The dropdown needs to be wider OR the scroll bar needs styling so it doesn't overlay text.

**Fix scope (next session):**
- Revert `022c09d`'s "one select with full-call options" approach
- Restore the pattern from commit `a82f620` (callsign chip + adjacent SSID select with `-${n}` option text)
- Style the SSID select so 2-digit values display fully (give it explicit min-width; use `appearance: none` + custom chevron so we control the chrome width; OR style its scrollbar narrower)
- Verify the operator's previous correction ("we should not display two SSIDs") doesn't re-emerge — make sure the callsign chip does NOT also format with `-${ssid}`; the ssid display lives ONLY in the picker.

### `tuxlink-jmfm` (P2) — ARDOP controls in wrong Settings section

Operator quote: *"We have a bunch of ARDOP controls under settings → GPS and Privacy? That's not where those belong, if they belong there at all."*

**Pre-existing**, not introduced by P3/P4. The `SettingsPanel.tsx` 'GPS and Privacy' section has nested ARDOP fields. PR #185 moved them inline into the ARDOP panel's Radio section, but the Settings copy is still there in the wrong place.

**Two options for the next-session agent:**
- (a) Delete the ARDOP block from Settings entirely (panel is the primary surface)
- (b) Promote to its own Settings section ('Radio' or 'ARDOP', peer of GPS/Privacy)

Defer until P3+P4 lands; not blocking.

---

## 5. Session-arc highlights — context-recovery for next agent

This session went 100+ messages. Critical inflection points the next agent should know:

1. **P1 was framed as a "complete unit" merge** — but P1 alone is an ugly intermediate state (bottom session-log strip gone, ARDOP dual-mounts placeholder + dock). Operator was surprised post-smoke; we'd documented this in the PR body but didn't EMPHASIZE the regression. Lesson recorded as part of why we then bundled P3+P4 into one PR.

2. **Operator's main checkout was on a SCAFFOLD-ERA branch** (`task-amd-main-ui`) — they branched it before tuxlink ever had real UI. Their `pnpm tauri dev` from `/home/administrator/Code/tuxlink` was running the create-tauri-app scaffold for hours; they thought the entire app was lost. Recovered by `git checkout --ignore-other-worktrees main` + `pnpm install` + cargo rebuild. The "task-amd-main-ui" name lies; do NOT assume it's parity with main.

3. **RADIO-1 governs TX, not UI** — operator pushback when Codex flagged "Send/Receive without separate consent modal" as a P1. The Part 97 consent is the click on Send/Receive; insisting on a wrapper modal is UX ceremony, not legal compliance. Memory saved as `feedback_radio1_governs_tx_not_ui.md` so this doesn't keep coming up.

4. **The CMS-Z exchange is sub-second** — the 5s status poll missed the "Connected" green window entirely. Two fixes landed:
   - Event-driven backend_status (broadcast channel + Tauri emit) — pushed in commit `9d3c2cd` (P2)
   - 1.5s hold after exchange complete before calling disconnect (`2a5a0af`, P2)
   Operator validated the result.

5. **The DashboardRibbon SSID inline-edit feature** was a NEW addition per operator request — was NOT in the original P3/P4 spec. Took two iterations to get right (the second is still wrong per `tuxlink-i63g`).

---

## 6. Worktree inventory

The session created two worktrees that remain on disk:

| Worktree | Branch | Disposition |
|---|---|---|
| `worktrees/bd-tuxlink-m0eh-radio-panel-shell/` | `bd-tuxlink-m0eh/radio-panel-shell` (merged + remote-deleted) | **STALE** — dispose at convenience per ADR 0009 ritual (P1 worktree, content merged) |
| `worktrees/bd-tuxlink-yq55-radio-panel-telnet/` | `bd-tuxlink-yq55/radio-panel-telnet` (merged + remote-deleted) | **STALE** — dispose (P2 worktree, content merged) |
| `worktrees/bd-tuxlink-p4bl-radio-panel-packet-ardop/` | `bd-tuxlink-p4bl/radio-panel-packet-ardop` | **ACTIVE** — PR #185 lives here; do not dispose until merged |

All have committed work pushed to origin. No untracked content of concern beyond `dev/adversarial/*-codex.md` transcripts (gitignored per CLAUDE.md).

---

## 7. Gates state at session-end

PR #185 latest commit (`cc82bf4`):
- ✅ `pnpm vitest run` — 709 tests across 82 files
- ✅ `pnpm exec tsc --noEmit` — clean
- ✅ `cargo build --bin tuxlink` — clean
- ✅ `cargo test --lib` — 591 passing

Binary at `worktrees/bd-tuxlink-p4bl-radio-panel-packet-ardop/src-tauri/target/debug/tuxlink` is current (built ~22:48 UTC; size 243 MB). Next agent's smoke should be fast.

---

## 8. Closed bd issues this session

| Issue | Closed via |
|---|---|
| `tuxlink-m0eh` | P1 merged (#173) |
| `tuxlink-yq55` | P2 merged (#176) |
| `tuxlink-o7d4` | False alarm; was resolved by an unrelated PR upstream — confirmed by P4 implementer (552 cargo tests pass) |
| `tuxlink-p4bl` | Will close when #185 merges |

Cascade closures **pending** on #185 merge: `tuxlink-mnk4`, `tuxlink-ed51`, `tuxlink-mzr7`, `tuxlink-1637`.

---

## 9. Next-session paste-ready prompt

```
PR #185 (radio-panel P3+P4 bundled) is DRAFT, multi-smoke-rounded.
Handoff doc: dev/handoffs/2026-05-31-moss-marten-birch-radio-panel-migration-marathon.md

Two operator-flagged smoke findings remain OPEN. Fix tuxlink-i63g
FIRST (SSID dropdown should be -N only, not full callsign; 2-digit
SSIDs get clipped). tuxlink-jmfm (Settings ARDOP misplacement) is
P2 and can defer to a follow-up PR.

DO NOT mark #185 ready or merge — operator decides. Surface for
round-5 smoke once tuxlink-i63g is fixed.

Read the handoff doc above before touching code. The session arc
spans ~22 commits across 4 smoke rounds; the operator has been
through every UX iteration and will recognize regression.
```

Agent: moss-marten-birch
