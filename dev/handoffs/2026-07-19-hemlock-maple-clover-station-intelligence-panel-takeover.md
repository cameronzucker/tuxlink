# Handoff — Station Intelligence panel is unusable (setup hijacks the body); fix in the morning, operator-supervised

- **Agent:** hemlock-maple-clover
- **Date:** 2026-07-19 (~02:00 local, operator stopping for the night)
- **Session scope this segment:** operator live-tested v0.94.0 and found two
  defects. Diagnosis only — NO code changed, NO fix applied. Operator will
  supervise the fix in the morning.
- **bd issue:** `tuxlink-6i0ie` (P1) — full root cause is in the issue body.

## The critical gate for the next session

**DO NOT autonomously implement.** The operator explicitly wants to
supervise the setup-presentation redesign. First action in the morning is
to **brainstorm the fix WITH the operator** (standing preference: brainstorm
UI changes, launch the visual companion). The root cause is already pinned
below — no re-investigation needed; go straight to design options.

## What is broken (confirmed, read from origin/main)

"Station Intelligence" = **`src/catalog/StationFinderPanel.tsx`**, opened
from the dashboard ribbon (a first-class feature). It is currently unusable
in any capacity:

- **The panel body is a hard either/or** (`StationFinderPanel.tsx:505`):
  `{setupActive && ft8.snapshot ? (<FT-8 setup body>) : (<map + station rail + live strip>)}`.
  When `setupActive` is true the else-branch — the ACTUAL Station
  Intelligence content (map + station list) — **never mounts**. The code
  says so: comment at :506 "map+rail and the live strip do not render
  underneath"; :575 "setup IS the body".
- **It auto-takes-over.** `setupActive` (:217) =
  `forceSetup || (needsSetup && !setupDismissed)`; `needsSetup` (:213) =
  `ft8.uiState.state === 'needs-setup'`, which promotes the instant FT-8
  mode has no resolved capture device. So opening the panel with no FT-8
  device configured throws away the map + station list and shows an FT-8
  soundcard setup screen instead — automatically, unasked.
- **The chrome persists around the hole.** The band-filter header and
  `AntennaControl` render ABOVE the ternary (~:485-502), outside the
  switch, so they stay on screen while the useful content is gone. That is
  the "malformed interface / removes the stations and map but keeps the
  rest" the operator described.

## Secondary defects

- **Window/panel not sized for the real content** (map + rail + strip).
  Mechanism UNCONFIRMED — could be the secondary-window geometry
  (`src-tauri/src/secondary_window.rs`) or the panel CSS. **Pin the exact
  cause before touching it; do not guess.**
- **The FT-8 audio picker is a RED HERRING — it works.** `Ft8SetupSurface`
  + `DeviceList` (`src/ft8ui/Ft8SetupSurface.tsx`) render a full
  multi-device selectable list (`DeviceList` maps over every device, each
  row has a "Use this device" button that persists via `ft8_set_device`);
  backend enumeration returns all capture-capable cards. The
  "single device / can't select" symptom was an artifact of testing on the
  dev Pi, which has exactly one capture-capable card (an ALSA loopback);
  the onboard HDMI/headphone cards are correctly filtered as playback-only.
  The picker is simply **mounted in the wrong place** — as a full-body
  takeover of a different feature. It is not itself defective. Do not spend
  time "fixing" the picker.
  - Note: FT-8 in tuxlink is **receive/decode only** — there is no TX /
    playback-device concept in the `ft8` module. One capture device is the
    correct device count for the feature; the defect is purely the takeover
    + reachability + window sizing, not the device model.

## Fix direction (to brainstorm, not to execute yet)

Setup must stop being a body-replacement. Candidate shape (for discussion):
map + station list stay mounted **always**; FT-8 device setup becomes a
contained overlay or a section inside the FT-8 live-band strip; it stops
auto-promoting over the whole panel (a "needs setup" nudge in the strip
that the operator opts into, rather than a hijack). Then size the
window/panel to hold the real content. The exact presentation is the
operator's call — that is what the morning brainstorm decides.

## State at close

- **No worktrees own code work for this** (this handoff worktree is
  disposed after push). Main checkout is leased by another live session
  (an overwatch/handoff session merged #1171); this handoff was written
  from a worktree and pushed direct to main.
- **A broader "what is Station Intelligence / what else is defective"
  investigation subagent was dispatched and may not have completed before
  wrap-up.** Its findings are not incorporated here; the panel-takeover
  root cause above stands on a direct code read and is authoritative for
  the reported symptom. If the morning session wants a wider defect sweep
  of the Station Intelligence data/scoring itself (beyond the layout
  takeover), re-run it.
- Spark unchanged from earlier today (serving `qwen3-coder-next`).
- Nothing else in flight; v0.94.0 is the promoted Latest release.

## Next session

1. Read this handoff + `bd show tuxlink-6i0ie`.
2. **Brainstorm the setup-presentation redesign WITH the operator** (they
   are supervising) — launch the visual companion. Do not start coding
   before that.
3. Then implement the agreed design in a worktree; pin the window-sizing
   cause with evidence before changing geometry.
