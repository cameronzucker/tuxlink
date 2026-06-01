# Troubleshooting

Quick diagnostics for the most common issues, plus where to look when
things are not working.

## "Not configured" in the message list

The backend has no callsign / grid / transport. Either re-run the wizard
or delete the config file:

```
rm ~/.local/share/com.tuxlink.app/config.json
```

The wizard re-runs on next launch.

## Connect button does nothing

- Check the selected transport in the folder sidebar — the highlighted
  connection is the one Connect uses.
- Watch the session log inside the radio panel; backend errors land
  there.
- For Packet: confirm the modem (e.g. Dire Wolf) is listening on the
  configured KISS TCP port.
- For ARDOP: confirm `ardopcf` is running and the configured ALSA
  capture / playback devices exist (`aplay -l`, `arecord -l`).

## CMS times out

- Try Telnet first — internet is the simplest failure mode.
- The default CMS endpoint can be slow; consult
  https://winlink.org/CMSStatus for global CMS health.
- For Packet / ARDOP: the local gateway must be running.

## GPS shows nothing

- Tools → Settings → GPS state must be `Broadcast at precision` or
  `Local display only` (not `Off`).
- A `gpsd` instance must be running on the host; Tuxlink reads from
  `gpsd` over TCP (default `localhost:2947`).

## Theme looks wrong

- Use View → Color Scheme to verify the active scheme.
- If switching from a custom theme leaves stale color: pick the Default
  preset to clear the inline override, then pick the desired theme.

## Compose window will not open

- The compose window is a separate Tauri webview; webview creation can
  fail if WebKitGTK is not installed. On Debian / Ubuntu:
  ```
  sudo apt install libwebkit2gtk-4.1-0
  ```
- The native title-bar Close on the compose window does NOT save in some
  early builds — confirm via Drafts that the in-progress text persisted.

## Reporting a bug

The Help → Report Issue menu opens the GitHub issue tracker in the
operator's default browser. Include:

- Tuxlink version (Help → About Tuxlink, or the Mailbox bar's right end).
- Transport (Telnet / Packet / ARDOP).
- The line(s) from the radio panel's session log around the failure.
- Steps to reproduce, if possible.

## Where next

- [Settings](07-settings.md) — every preference's effect.
- [Connections](02-connections.md) — what each transport needs.
- [Getting started](01-getting-started.md) — wizard recovery.
