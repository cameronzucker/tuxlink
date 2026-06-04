# The B2F protocol

B2F — Block Forwarding v2 — is the application protocol that carries every
Winlink session. The name comes from the original FBB packet bulletin board
network from which Winlink inherited its framing; the "v2" refers to the
modern revision the Common Message Server speaks.

Every Winlink exchange — Telnet to the CMS, ARDOP to an RMS, VARA HF to an
RMS, Packet to an RMS, peer-to-peer between two operators — runs B2F. The
transport changes (TCP, packet frames, ARDOP ARQ, VARA ARQ); the B2F
exchange on top is the same.

This topic exists so the session log makes sense when something goes wrong.
The wire framing is not authoring detail the operator usually needs; the
shape of an exchange is.

## Line framing

B2F is a text protocol. Lines end with a carriage return (`\r`). Some
implementations send CR-LF; the leading LF on the next line is tolerated and
trimmed. Some servers pad with stray null bytes; those are stripped at the
client. There is no length prefix, no framing escape character — just lines.

A few example lines from a real session:

```
[WL2K-5.0-B2FWIHJM$]
;PQ: 12345678
CMS>
```

These are the kind of lines that appear in the session log. The first is
the server's identification banner. The second is a `;`-prefixed
informational line (the `PQ` here is the operator's password challenge,
discussed below). The third is the server's prompt asking for the next
command.

## The handshake

A B2F session begins with each side declaring who it is and what features
it supports. The exact sequence varies by transport (ARDOP and VARA
front-load the connect with their own ARQ negotiation before B2F starts),
but the B2F handshake itself is:

1. The server (CMS or RMS) sends its banner: `[WL2K-5.0-B2FWIHJM$]`.
   The text in brackets advertises the version and the supported feature
   character set. Tuxlink reads the banner to learn whether the server
   accepts compressed mode `C`, gzip mode `D`, etc.
2. The server sends a password challenge: `;PQ: <8-digit-number>`.
3. The client responds with its identification — callsign, features it
   speaks — and an MD5-derived response to the password challenge.
4. The server replies with its prompt: `CMS>` or `RMS>`.

At this point both sides have authenticated and the exchange phase begins.

## The exchange: proposals and answers

The interesting part of B2F is the **proposal-answer dance**. The side that
has the prompt offers messages to the other; the other side answers each
proposal accept / reject / defer.

Each proposal is a single line of the form:

```
F<code> <type> <mid> <size> <compressed-size> 0
```

- `<code>` — the format. `C` = standard Winlink compressed (LZHUF + B2F's
  framing), `D` = gzip compressed.
- `<type>` — usually `EM` (encapsulated message). Some session types use
  `CM` for catalog requests.
- `<mid>` — the unique message ID, an opaque 10–12 character string.
- `<size>` — uncompressed size in bytes.
- `<compressed-size>` — what actually moves on the wire.
- The trailing `0` is an offset field, reserved for partial-transfer resume.

A batch ends with a checksum line `F> <hex>` so the receiver can detect a
line-noise corruption before answering.

The receiver answers with `FS <one-character-per-proposal>`:

- `Y` — yes, send it.
- `N` — no, reject (already have it, do not want).
- `=` — defer (have it but accept for re-delivery elsewhere — rarely seen
  in tuxlink sessions).

Accepted messages then transfer, one after the other, in the order proposed.

## End of exchange

Either side ends the conversation with one of:

- `FF` — finished, normal close. The other side answers `FF` and the
  connection closes cleanly.
- `FQ` — quit, abnormal close. Used when the session needs to terminate
  immediately (e.g. the operator hit Abort).

A clean B2F session closes with `FF` from both sides followed by the
transport closing. An abrupt RF dropout or a TCP reset terminates the
session without a closing `FF` — tuxlink logs this as "disconnected mid-
exchange" and offers a retry path.

## Compression

The default format `C` uses LZHUF compression — the same compression
scheme used by the original 1990s-era FBB packet bulletin boards. Tuxlink
ships its own LZHUF implementation (visible at
`src-tauri/src/winlink/testdata/lzhuf/` for development reference).

The newer format `D` is gzip. Servers that advertise `D` in their banner
prefer it for smaller messages; the LZHUF format is still required for
backward compatibility with older RMS gateways.

The compressed size in the proposal lets the receiver budget — on a 200 Hz
ARDOP channel with a 30-second propagation reservation, a 50 KB compressed
message is borderline. The session log calls this out.

## What can go wrong

| Session log line | What it means |
|---|---|
| `disconnected while reading a line` | The transport closed before a full `\r`-terminated line arrived. RF dropout, TCP reset, or remote close. |
| `unexpected proposal format <X>` | The other side offered a format tuxlink doesn't know. Usually an aging RMS sending a non-standard variant; the message gets `N`-rejected and the session continues. |
| `checksum mismatch on proposal batch` | The `F>` line didn't match — line noise corrupted the proposal. Tuxlink reports the error; the other side typically re-proposes. |
| `authentication failed` | The password challenge response was wrong. Wizard credentials are stale. |

The session log is verbose by design — every line the wire carries gets
echoed in the panel. Reading top-to-bottom is the fastest diagnostic on a
failed session.

## Where next

- [Mailbox model](07-mailbox-model.md) — how the exchange's accepted messages land in folders.
- [The Winlink ecosystem](04-the-winlink-ecosystem.md) — who runs the CMS and RMSes B2F talks to.
- [Picking a transport](08-picking-a-transport.md) — which B2F transport when.
- [Troubleshooting](29-troubleshooting.md) — how to read session-log failures.
