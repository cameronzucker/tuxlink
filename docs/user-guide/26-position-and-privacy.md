# Position and privacy

Every Winlink message can carry the sender's geographic position. Operators
use position data for situational awareness in emcomm nets, for mobile /
portable operating, and for the position-report list. The privacy implication
matters: a precise position broadcast on a public network exposes the
operator's location, and "public network" includes more parties than just
the intended recipient.

Tuxlink takes a deliberate stance on this: **GPS is enabled by default,
broadcast precision is reduced by default to a 4-character Maidenhead grid
square (resolution ~100 km), and the operator opts in to higher precision.**
This topic covers what that means, how to change it, and what the privacy
tradeoff looks like in practice.

## The amateur no-encryption rule and Open Message Viewer

Two facts shape every privacy decision on amateur Winlink:

1. **Amateur Winlink messages cannot be encrypted.** Part 97 (US;
   equivalents elsewhere) prohibits encrypted content on amateur
   frequencies. Every byte that crosses an amateur RF link is sent in the
   clear. This is intentional — the rule exists to keep amateur radio
   open to monitoring and to prevent commercial / hidden use of the
   amateur allocations. It is not optional.
2. **Every amateur Winlink message is publicly retrievable.** Winlink
   operates the **Open Message Viewer** — a public web tool that lets
   anyone search and read messages that traversed amateur RF links.
   The retention window covers months of recent traffic. Search by
   callsign, recipient, or date range surfaces sent and received
   amateur-side messages.

The combined implication: any message an operator sends or receives over
an amateur Winlink path — VARA, ARDOP, Packet — is effectively a public
record. The CMS (running on internet infrastructure) is not the
attacker; OMV is operated by Winlink itself as transparency
infrastructure consistent with the no-encryption rule.

**Operating consequences:**

- **Do not send personally-sensitive content over amateur Winlink.**
  Medical details, financial details, addresses of vulnerable
  individuals, anything that would harm the recipient if widely seen —
  these belong on a different medium. Email over commercial internet,
  encrypted messaging apps, or any non-amateur channel is the right
  carrier for that traffic.
- **EmComm protocol drafts standardize on this.** Forms like the ICS-213
  general message and the Winlink Check-in form are designed for
  callsign / location / status traffic that is comfortable being public.
  When operating EmComm, the protocol-designed forms have already done
  the privacy thinking.
- **Position is a special case** — it's a header field that gets
  indexed, displayed, and retained even for messages whose body content
  doesn't mention position. That's why the rest of this topic focuses
  on the position-precision default specifically.

Whether messages routed entirely over Telnet (internet transport, no
amateur RF in the path) appear in OMV is a Winlink-side policy question
that operators should verify against current Winlink documentation
before relying on it. The conservative posture: treat any message sent
under an amateur callsign as potentially publicly visible regardless of
transport.

## The two settings

Two independent settings control position behaviour:

| Setting | What it controls | Default |
|---|---|---|
| **GPS state** | Whether tuxlink reads from a connected GPS at all | On (read GPS when available) |
| **Broadcast precision** | What level of precision is included in outbound messages | 4-character Maidenhead grid |

The two are independent: GPS can be on (so the operator sees their precise
location on the local UI) while broadcast precision is low (the wire only
carries the rounded grid).

## Maidenhead grid precision levels

The Maidenhead Locator System encodes position into letter / number tokens
of varying precision:

| Precision | Format example | Resolution at mid-latitudes |
|---|---|---|
| 4-character | `CN85` | ~110 km north–south × ~150–200 km east–west — a small county or two |
| 6-character | `CN85qe` | ~5 km × ~7–10 km — a small town's footprint |
| 8-character | `CN85qe72` | ~460 m × ~700–900 m — a few city blocks |
| Full GPS | `37.7749, -122.4194` | sub-meter, depending on receiver |

The east–west dimension shrinks with latitude (it's based on degrees of
longitude, which converge at the poles), so a 4-character grid covers
less ground at 60° N than at 30° N.

A 4-character grid identifies the operator's general region — a county
or two. A 6-character grid identifies the operator's town. An 8-character
grid identifies a few city blocks around the operator. Full GPS
identifies the operator's chair.

Tuxlink broadcasts only the 4- or 6-character grid. The 8-character and
full-GPS rows above explain the Maidenhead system; they are not selectable
broadcast options.

Tuxlink defaults to 4-character because the privacy / utility curve is
asymmetric: 4-character is sufficient for most operating uses (gateway
selection, regional propagation, net check-ins), and the marginal utility
of finer precision is small relative to the marginal privacy loss.

## Why GPS-on, broadcast-reduced

GPS-on makes the operator's local situational awareness work — the
dashboard ribbon shows precise position, the radio panel can suggest
gateways by distance, and any features that need real-time position (mobile
operating, expedition tracking) work out of the box.

Broadcast-reduced protects what goes over the wire. A Winlink message is
not end-to-end encrypted — it traverses the CMS, possibly an RMS gateway,
possibly internet SMTP relays. Any of those parties can read the position
header. Reducing the broadcast precision to a grid square limits exposure
without losing the operational utility.

The default is **the operator's recommended posture**: the privacy
implications of precise broadcast accumulate over time and are hard to
reverse, while increasing precision when an operator decides they need it
is a single setting toggle.

## When to broadcast more precision

Some operating contexts justify higher precision broadcasts:

- **Mobile / portable operating.** Backpacking, marine mobile, expedition,
  bike-mobile. The operator is on the move and reports their position so
  recipients can plan accordingly. 6-character is typically enough.
- **Emcomm at a fixed assignment.** Operating from a shelter, an EOC, or a
  staging area. The position is publicly-known (the shelter has a
  published address), and net coordinators benefit from knowing where the
  operator is. 6-character — the finest Tuxlink broadcasts — is sufficient.
- **Search and rescue / SHARES.** Some SAR-adjacent nets call for
  precise positions. Operator's call; the net's stated requirements apply.

## When to broadcast less

- **Home station, daily operating.** The operator's home address is
  personal. 4-character (or no position at all) is the right default.
- **Hidden mobile.** The operator is mobile but doesn't want the route
  known. Switch to grid square only or disable broadcast.

## Changing the settings

<!-- screenshot-needed: docs/user-guide/images/26-position-and-privacy/dashboard-grid-display.png
     Show: the dashboard ribbon showing the operator's grid square (CN85
     or similar 4-character) displayed alongside callsign + connection
     state. Dashboard ribbon crop, ~800x80 (full width, ribbon only). -->


**Tools → Settings → GPS & Privacy** opens the inline panel. The settings:

- **GPS state.** Three options: **Broadcast at precision** (default — GPS is
  read and may be broadcast on air at the precision below), **Local display
  only** (GPS is read for the local UI, but outbound traffic uses the
  configured grid), and **Off** (GPS is not read; the configured grid is used).
- **Broadcast precision.** Dropdown: **4-character grid** (~1°, default) and
  **6-character grid** (~5 km). Tuxlink does not broadcast finer than
  6-character.

Changes take effect immediately. The next outbound message uses the new
settings.

## How tuxlink reads GPS

Tuxlink reads from any GPS source available on the system. The two
canonical options:

- **gpsd.** The Linux GPS daemon. If `gpsd` is running and connected to
  a GPS receiver (USB, serial, Bluetooth NMEA), tuxlink reads from it
  automatically.
- **Embedded GPS.** Some interfaces (the IC-705's built-in GPS, the
  Mobilinkd TNC's optional GPS) provide GPS data over their existing
  serial connections. Tuxlink reads these when configured.

If no GPS source is reachable, tuxlink uses the manual grid entered in
the wizard as the broadcast position. The dashboard ribbon clearly shows
whether the position is GPS-derived or manual.

## What goes in the wire

The position information embedded in an outbound Winlink message is a
header field, not message body content. The recipient sees it next to
the From line; FTS5 search in the recipient's mailbox indexes it.

```
From: WA1XYZ
Grid: CN85
Subject: ARES check-in
```

The grid in the example above is the 4-character default. With 6-character
broadcast it would be `CN85qe`. Tuxlink does not broadcast finer than
6-character.

## Auditing what you broadcast

Tuxlink's compose preview window (see [Composing](19-composing.md)) shows
exactly what will be sent on Send — including the position header. The
operator can verify the precision is what they intend before transmission.

For messages already sent, the Sent folder retains the as-sent copy
including the position header. Reviewing it is a one-click navigation.

## Where next

- [Settings](27-settings.md) — the GPS & Privacy panel.
- [Net check-ins](25-net-check-ins.md) — when broadcast position is operationally relevant.
- [Composing](19-composing.md) — the per-message preview.
- [The mailbox](18-the-mailbox.md) — auditing what was sent.
