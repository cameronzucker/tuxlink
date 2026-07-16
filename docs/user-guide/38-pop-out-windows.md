# Pop-out windows

Three surfaces — Routines, Tac Map, and APRS Chat — can pop out of the main
window into their own OS window. This suits a second-screen station: the Tac
Map on a side monitor while the mailbox stays in front, or a Routines
dashboard left running in its own window during a net. The default
single-laptop bag deployment works exactly as before; popping out is opt-in,
never automatic.

## The three controls

- **↗ Pop out.** A text-labeled control in the surface's own header (the
  Routines dashboard, the Tac Map toggle, the APRS Chat panel) moves that
  surface to a new window. The main window's menu item for a popped surface
  relabels to show the ↗ marker (for example, "Routines ↗") and clicking it
  focuses the popped window instead of reopening the pane inline.
- **⇤ Dock back.** The popped window's title bar carries a ⇤ control that
  returns the surface to the main window and brings it to the front there —
  the inline pane you land on is exactly where you left off.
- **✕ (the window's own close button, or Ctrl+W).** Closing a popped window
  does not discard anything and does not disturb the mailbox. It returns the
  surface to the main window in the background — available again from its
  usual menu or panel entry, but without jumping in front of whatever the
  operator is reading or composing. The difference from ⇤ is presentation
  only: ⇤ brings the surface forward; ✕ puts it away quietly.

Docked or popped, a surface reads and writes the same underlying state —
runs, positions, chat history. Nothing about what a surface is doing changes
when it moves between window and pane.

## Layouts persist

Each surface remembers whether it was popped out or docked, and the popped
window's size and position, across a full quit and relaunch. A monitor that
is disconnected before the next launch does not strand a window off-screen:
the window system places the restored window back on a connected display.

## Memory cost

Each additional popped-out window is a separate WebKitGTK web process, and
that costs real memory — tuxlink does not pretend otherwise. Measured on the
reference hardware (Raspberry Pi 5, 16 GB, software GL rendering), an extra
webview window runs in the **~30 MiB class** with dashboard-grade content.
Content-heavier surfaces cost more — a popped Tac Map carries its tile cache
on top of the base window cost.

On a 4 GB machine running the full graphical station, that is well under 1%
of available memory per extra window. Windows spawn only when the operator
pops a surface out and the process is reclaimed the moment it docks back —
tuxlink never pre-renders a hidden window to make a future pop-out feel
instant.

## Where next

- [The mailbox](18-the-mailbox.md) — what a popped surface never interrupts.
- [Settings](27-settings.md) — application preferences.
- [Troubleshooting](29-troubleshooting.md) — what to check if a popped window
  does not appear on a monitor as expected.
