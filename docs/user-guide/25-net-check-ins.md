# Net check-ins

A Winlink net check-in is the standardised "I'm on the air and ready" message
an operator sends at the start of a scheduled net. Net Control collects the
check-ins, then directs traffic among the checked-in stations. Tuxlink supports
the check-in patterns the EmComm community has standardised on — ARES, Winlink
Wednesday, training nets — through the same compose + send mechanics any
Winlink message uses.

This topic covers the conventions for net check-ins, how to author them in
tuxlink, and the operating patterns that work across multiple nets.

## What a check-in carries

A check-in message typically includes:

- **Identification.** Callsign + name (and sometimes the station's grid
  square or position).
- **Net affiliation.** Which net is being checked in to (ARES district,
  Winlink Wednesday region, etc.).
- **Availability.** What kinds of traffic the operator can handle (HF
  reach, local VHF, attachments, etc.).
- **Status notes.** Anything operationally relevant — operating from
  battery, mobile, expected duration, special equipment.

The recipient is the net's controlling address — typically a callsign or
a group alias (`ARES-DIST7@winlink.org`, etc.) that the net's controller
publishes. Each net has its own conventions for the subject line.

## The Winlink Check-in form

The canonical Winlink check-in form is **Standard Forms → General Forms →
Winlink Check-in** in the Winlink Forms catalog. The form is HTML — the
operator fills it in via a browser-style surface in tuxlink, the form
serialises to a structured text body in the outbound message, and Net
Control's receiving station displays a uniformly-rendered check-in
regardless of which client the sender used.

The form's fields (per the canonical Winlink template):

- **Date / Time** — UTC and local.
- **Status** — real exercise, training scenario, net, or routine.
- **Band** — the operating band used (e.g., 40m).
- **Mode** — the digital mode used (e.g., VARA HF Standard).
- **To** — net control callsign (autofilled by recipient address).
- **Calling station call** — your callsign, including portable suffix
  if applicable (`OH8STN/P`, etc.).
- **Operator call** — phonetic spelling.
- **Operator** — the licensee on the controls, if filing for another
  station.
- **Location** — physical operating location (text).
- **GPS coordinates** — auto-filled if GPS is wired and the privacy
  setting allows (see [Position and privacy](26-position-and-privacy.md)).
- **Comments** — free text. Net controllers often specify what they
  want here in the net's pre-announce.

In tuxlink: **Compose → Use form → Winlink Check-in**. Fill, Submit, and
the form's serialised body lands in the compose window as the message
body. Save to Outbox, Connect, done.

## ARES nets

The Amateur Radio Emergency Service (ARES) runs the largest emcomm net
network in the US. ARES nets at the district or section level publish
their check-in protocol — the controlling callsign or alias, the
preferred subject line, and any extra fields expected in the form's
Comments section.

ARES nets typically use the **Winlink Check-in** form above. Some
districts use ICS-213 (the general-message form) for the check-in itself
when the local net has been organised that way; both work. The net's
pre-announce specifies which.

A representative ARES check-in flow:

1. Tuxlink's Compose → Use form → Winlink Check-in.
2. Fill **To** with the net's published address (e.g.,
   `ARES-NORCAL@winlink.org`).
3. Fill **Status** = "Net", **Band** = (your operating band),
   **Mode** = (your transport).
4. **Comments**: whatever the net's pre-announce asked for — sometimes
   "available for traffic," sometimes a specific protocol step.
5. Submit, Save to Outbox, Connect.

## Winlink Wednesday

Winlink Wednesday is a weekly informal exercise net run on Wednesdays.
Operators check in via Winlink (any transport) to demonstrate they can
reach the CMS that day. Useful for keeping skills sharp.

The check-in is simpler than a formal ARES net — usually a free-form
message to the Winlink Wednesday regional address, or a Winlink Check-in
form with **Status** = "Training". The regional coordinator publishes
the exact address and any net-specific expectations.

## SHARES and federal nets

SHARES (Shared Resources high-frequency program) is the federal
emergency-management amateur radio program — voluntary, but operated
under federal coordination. SHARES participation requires registration;
unregistered operators cannot check into SHARES nets.

SHARES nets use Winlink with the protocols their net controllers
publish. Tuxlink's Winlink Forms catalog includes the standard Winlink
Check-in form that most non-traffic SHARES check-ins use. Net-specific
SHARES forms (when a particular net uses one) ship through the same
forms catalog as ARES and Winlink Wednesday forms.

For SHARES operators: the same Compose → Use form → Submit → Connect
workflow applies. Net-specific protocols come from the net's published
documentation, not from tuxlink.

## Position reports

Some nets — particularly mobile / portable nets where stations move
between operating positions — call for a position report in the check-in.
A position report includes the operator's grid square (4 or 6 character
Maidenhead) or precise GPS coordinates.

> [!NOTE]
> **Position precision is operator-controlled.** Tuxlink defaults to
> broadcasting a 4-character Maidenhead grid (resolution ~100 km), which
> identifies the operator's general area without revealing a precise
> home address. Operators who specifically want higher precision (e.g.
> a mobile station whose location is operationally relevant to the net)
> opt in to 6-character or precise GPS via Settings. See
> [Position and privacy](26-position-and-privacy.md) for the privacy
> tradeoffs.

## Net Control operating

If running the net (acting as Net Control), the workflow differs:

1. **Open the net.** Send a pre-announce message to the regional address
   announcing the net is starting.
2. **Solicit check-ins.** Wait for a check-in interval (typically 5–10
   minutes), then connect to gather all received check-ins.
3. **Roll-call.** Read out the checked-in stations from the Inbox.
4. **Take traffic.** Connect periodically through the net to receive
   inbound traffic; relay or respond as appropriate.
5. **Close the net.** Send a close-out message to the regional address.

Net Control's workflow benefits from a [user folder](22-user-folders.md)
per net — the night's check-ins, traffic, and outbound messages live in
one place for the post-net log.

## On-air check-in

> [!WARNING]
> **Submitting a check-in over RF is on-air transmission.** Once the
> form is in the Outbox and the operator presses Connect with an RF
> transport selected, the radio is keyed under the operator's callsign.
> Net-check-in traffic is short — most check-ins fit in one B2F frame
> exchange — but the Part 97 consent applies: pressing Connect is the
> per-session licensee gate. A Telnet check-in does not transmit on air.

## Frequency and mode discipline

Nets that take place on RF (most ARES, SHARES, regional nets) specify
both the frequency and the Winlink mode in their net announcements. The
operator's job is to:

- Verify the announced frequency is correct in the operator's catalog (a
  catalog refresh before the net catches stale data).
- Pre-tune the radio to that frequency before the net starts.
- Pick the announced mode in tuxlink's radio panel.
- Be ready to fall back (different mode, different RMS) if the primary
  path fails.

## Common check-in failures

| Symptom | Cause |
|---|---|
| Check-in not in NCS's Inbox | Check-in still in operator's Outbox — Connect didn't run or failed |
| NCS replies "not received" but tuxlink's Sent folder shows the message | Message was delivered to CMS but lost in transit to NCS — rare; usually a NCS-side issue |
| Multiple check-ins for the same net | Two Connects sent the same Outbox message before the first ack came back — Outbox state is consistent; the duplicate sends are an at-most-once-per-CMS-session property the protocol does not provide |
| Bandwidth mismatch on the announced mode | The net announced VARA Standard but operator's tier is Free — Standard is the same as Free for VARA, so this should work; if not, check VARA's installed tier |

## Where next

- [Emcomm and ICS](24-emcomm-and-ics.md) — the broader ICS context.
- [Position and privacy](26-position-and-privacy.md) — what the check-in broadcasts.
- [HTML Forms](20-html-forms.md) — the ICS-213 form mechanics.
- [Catalog requests](23-catalog-requests.md) — refreshing the gateway list before a net.
