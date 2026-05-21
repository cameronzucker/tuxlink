# Winlink client registration request — "tuxlink"

**Why:** Winlink's production CMS servers only accept client software whose
identifier (SID) name is on their allowlist. Tuxlink presents the SID name
`tuxlink`, which isn't registered yet, so `server.winlink.org` rejects it:

> `*** Unknown client types are not allowed on production servers -- use cms-z.winlink.org - Disconnecting`

Registering the name `tuxlink` with the Winlink Development Team adds it to the
production allowlist. This is the one remaining step before tuxlink can connect
to the production CMS (over plaintext **and** TLS — both are otherwise working).

**Where to send it:** the Winlink Development Team. The usual channels are the
Winlink "Programs / Developers" group on groups.io, or the developer contact via
winlink.org. (Route to whichever the team currently uses; the message below is
channel-agnostic.)

---

## Ready-to-send message

**Subject:** Client registration request — new client software "tuxlink"

Hello Winlink Development Team,

I'm requesting that a new Winlink client be added to the production CMS
client allowlist.

- **Client name (SID identifier):** `tuxlink`
- **Example SID presented during the handshake:** `[tuxlink-0.0.1-B2FHM$]`
  (name `tuxlink`, version, then the protocol codes `B2FHM$`)
- **Protocol support advertised:** `B2` (B2 Forwarding Protocol / compressed v2),
  `F` (FBB basic), `H` (hierarchical location designators), `M` (message
  identifier), `$` (BID).
- **What it is:** Tuxlink is a Linux-native, open-source Winlink client written
  in Rust. It speaks the standard B2 Forwarding Protocol directly — telnet and
  TLS-wrapped telnet (port 8772 / 8773) to the CMS — with a native FBB/lzhuf
  codec, secure login, and the standard message exchange. (AX.25/packet and a
  VARA-class HF modem are planned later milestones.)
- **Developer / operator call sign:** N7CPZ
- **Source:** https://github.com/cameronzucker/tuxlink

The client connects and authenticates correctly against `cms-z.winlink.org`
already (full secure-login + message exchange verified); it is rejected on the
production servers only by the client-type allowlist. Please let me know if you
need any additional detail, a test connection, or a different identifier format.

Thank you,
Cameron Zucker — N7CPZ

---

## Notes

- The allowlisted item is the **name** before the first dash in the SID
  (`tuxlink`), not the version — version bumps won't need re-registration.
- The SID name is set by `APP_NAME` in `src-tauri/src/winlink/handshake.rs`
  (currently `"tuxlink"`). If the team prefers a different exact string (e.g.
  capitalized `Tuxlink`), change that one constant (and its handshake test) to
  match what gets registered.
