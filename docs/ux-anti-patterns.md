# UX Anti-Patterns

> Load-bearing reference. Every task that touches tuxlink UI (app screens,
> wizard, menus, forms, message view, settings) MUST read this document
> first. Subagents reading it: this is not a style guide, it is a set of
> explicit failure modes we have already observed in Winlink Express and
> Pat and will NOT reproduce.

## The Two Incumbents and Why Users Suffer

Tuxlink is replacing (or offering a better path than) two products:

- **Winlink Express ("Winlink 2K")** — Windows-only, full-capability,
  native desktop feel but with 1990s Windows chrome and a catastrophic
  forms UX.
- **Pat** — cross-platform, web UI + CLI, smaller surface, config-via-YAML
  barrier that alienates the Winlink Express user population.

Tuxlink wins by giving the Winlink Express audience a desktop app that
feels like Windows Winlink Express improved, not like a web tool with a
native window wrapped around it.

## Anti-Patterns Observed in Winlink Express

The founder of tuxlink did a firsthand walk-through of Winlink Express's
forms feature on 2026-04-22. The user flow was:

1. Click "New Message"
2. "Select Template"
3. Archaic 2003-era node-based selector, un-sortable list
4. **Form opens in external web browser** (not embedded)
5. Browser may have CSS/extension conflicts that break the form
6. On submit: modal message says "click OK and close the open browser
   window. You will return to the new message window so you can post
   your message to the outbox"
7. In the founder's case: the draft was discarded, then "this message
   has not been posted, close anyway?" warning was thrown for no reason

### DO NOT reproduce any of the following:

- **NEVER open a form or any interactive surface in an external browser
  window.** All forms render inline in the message view or in an
  embedded webview pane within tuxlink's main window.
- **NEVER discard a draft on form-open, form-cancel, or any
  non-explicit-user-action.** The user's draft state survives form
  insertion, form cancellation, form error, form-submit-cancel, and
  app-background-resume. Explicit Send or Explicit Discard are the only
  draft-destroying actions.
- **NEVER show a modal that says "click OK and close the other
  window."** Window juggling is not a user interaction; it's an
  admission that the app architecture is broken.
- **NEVER show an "X could not be Y, do Z anyway?" warning where X
  didn't need to be Y in the first place.** If the save-to-outbox
  action has a reason to fail, explain it; if it doesn't, succeed
  silently. Warnings with no informational payload are noise.
- **NEVER present an un-sortable, un-searchable list of anything the
  user must pick from.** Templates, folders, gateways, forms, rig
  profiles, gateways: every picker is sortable, filterable, and
  type-to-find. "Scroll through 200 items to find the form" is a bug.
- **NEVER require the user to learn the software's internal
  categorization to find a feature.** If a feature is mentioned in
  documentation, a human searching for that phrase in the UI finds it
  within three clicks.

## Anti-Patterns Observed in Pat

Pat is a smaller surface and gets many things right. Its major migration
barrier for Winlink Express users:

- Config lives in `~/.config/pat/config.json` and is edited by hand.
- CLI-first; web UI assumes the user has already configured Pat via
  terminal.
- No forms support (Issue #135 open since 2018).
- Keyboard shortcuts and desktop-app affordances are absent.

### DO NOT reproduce any of the following:

- **NEVER require the user to edit a config file.** Every piece of
  configuration is settable via the Settings UI. Config files may
  exist as storage (JSON, TOML) but the Settings UI is authoritative
  and the user never opens the file.
- **NEVER require terminal commands during normal use.** After install,
  tuxlink runs from the desktop launcher. No `rigctld -m 1049 -r
  /dev/ttyUSB1 ...` in another tab. No shell commands in any quickstart
  except the install one-liner.
- **NEVER treat CLI as the primary interface.** A CLI may exist for
  power users and CI. The primary interface is the Tauri GUI. The CLI
  never has features the GUI lacks.
- **NEVER require the user to understand that tuxlink manages Pat.**
  The Pat daemon is an implementation detail. The user does not know
  Pat exists unless they open About -> Credits.

## Desktop-App Migration Commitments (to win the Winlink Express audience)

Tauri on Linux uses WebKitGTK for the content area but allows the window
chrome to be fully native. Use that capability.

### Required elements (not negotiable for v0.1)

- **Native menu bar at the top of the window**, populated via
  `tauri::menu`. Categories, in order (subject to tuning per menu item):
  - **File**: New Message, Open Draft, Save Draft, Post to Outbox, Quit
  - **Message**: Reply, Reply All, Forward, Print, Properties
  - **Session**: Connect (Telnet/AX.25/VARA HF/VARA FM/...), Disconnect,
    Session Log, Statistics
  - **Mailbox**: Inbox, Outbox, Sent, Archive, Empty Trash
  - **View**: Show/Hide Session Log, Show/Hide Status Bar, Theme
  - **Tools**: Templates, Rig Control, Operator Info, Radio Tuning,
    Preferences, Diagnostics
  - **Help**: About, Documentation, Winlink Website, Report Issue
- **System tray icon** with: Show/Hide window, New Message, Quit. Clicking
  the window close button HIDES the window to the tray; it does NOT quit
  the app. Quit only happens from File -> Quit, tray -> Quit, or the
  shortcut.
- **Keyboard shortcuts** matching Winlink Express where documented.
  Starting set: Ctrl+N (new), Ctrl+R (reply), Ctrl+Shift+R (reply all),
  Ctrl+P (print), Ctrl+Q (quit), F5 (connect/receive), F6 (send all
  pending).
- **Status bar at the bottom** showing: connection state (idle /
  connecting / in-session), active protocol, session time remaining,
  message counts pending.
- **Familiar terminology from Winlink Express**: "Mailbox," "Outbox,"
  "Posted," "Templates," "Session," "Gateway," "Propagation" — do not
  re-invent with fresh marketing language. The Winlink Express user
  knows these words; tuxlink honors that vocabulary.
- **Multi-window / dialog patterns where they make sense.** Composing a
  message may open in a separate window. Preferences is a dialog, not a
  slide-out drawer. Form submission never spawns an external browser.

### Forbidden elements (v0.0.1 through v0.1+)

- **NO hamburger menus.** This is a desktop app; the menu bar is at the
  top and categorized.
- **NO slide-out drawers / sidebars that auto-hide.** A persistent
  folder tree on the left is fine. Auto-hiding it because the window
  got narrow is mobile-first thinking.
- **NO mobile-first / responsive layout below 1024px.** Tuxlink is a
  desktop application; a laptop minimum is fine, a phone is out of scope.
- **NO "single-page app" layouts where the whole UI swaps on a click.**
  Message selection updates the reading pane; it does not swap the
  entire view.
- **NO in-content toolbars that duplicate menu items.** If something is
  in the menu bar, it should not also be a floating button in the
  message view, unless (a) it is a verb that operates on the currently
  selected message and (b) it is the most-common action.
- **NO "Try our new X" UI nudges, notification toasts for non-urgent
  events, or onboarding takeovers after first-run.** The first-run
  wizard is the onboarding; after that, the app is silent unless the
  user needs to act.

## For Subagents Implementing UI Tasks

If your task touches any of these surfaces (wizard, menus, compose,
inbox, sent, forms, settings, session log), your task preamble MUST
include:

> Read `docs/ux-anti-patterns.md` before starting. Your implementation
> must not introduce any listed anti-pattern. If the task description
> accidentally describes an anti-pattern, stop and surface it.

Every PR or commit that touches UI must include `ANTI-PATTERN REVIEW:
none` or a specific note about which anti-pattern was considered and why
the chosen implementation avoids it.

## Source Incidents

- Winlink Express forms UX — observed 2026-04-22 (founder's personal
  Windows install).
- Pat's config-via-YAML migration barrier — observed across ham radio
  community discussions and Pat Issue #93 ("missing features
  checklist").
- Pat forms gap — [Issue #135](https://github.com/la5nta/pat/issues/135)
  open since 2018.
- "I look forward to the day a well-supported Open Source project fixes
  this situation" — Alan, [Turbid Plaque,
  2025-10-21](https://turbidplaque.com/wp/2025/10/21/winlink-and-vara-on-linux-surprisingly-straightforward/).
