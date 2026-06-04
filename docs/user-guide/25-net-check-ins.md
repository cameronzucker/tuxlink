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

## ARES check-in

The Amateur Radio Emergency Service runs the largest emcomm nets in the US.
ARES check-ins use the standard ICS-213 form (see
[Emcomm and ICS](24-emcomm-and-ics.md)) with the net's specific subject line.

A typical ARES check-in:

```
To: ARES-NORCAL@winlink.org
Subject: Check-in 2026-06-15 evening net
Body:
Net: NorCal ARES Evening
Station: WA1XYZ, Cameron, CN85qe
Mode: VARA HF Standard, 40m
Status: Home station, mains power, available for traffic
Time: 1830 PDT
```

In tuxlink: **Compose → Use form → ICS-213**. The form prefills the
sender's callsign and grid from the wizard. The operator fills in net,
status, and time, then Sends. The message lands in the Outbox; the next
Connect to a CMS-reachable transport sends it.

## Winlink Wednesday

Winlink Wednesday is a weekly informal exercise net run on the Wednesday
of every week. Operators check in via Winlink (any transport) to
demonstrate they can reach the CMS that day. Useful for keeping skills
sharp.

The check-in is simpler than ARES — typically a free-form message to the
Winlink Wednesday regional address with a short body confirming reach.

In tuxlink: free-form compose to the regional WW address (published by
the regional coordinator), Send, Connect.

## SHARES and military nets

SHARES (Shared Resources) is the federal emergency-management amateur
radio net. Its check-in form is more structured than ARES and uses a
specific SHARES catalog form rather than ICS-213. The tuxlink
HTML Forms catalog includes the SHARES check-in form for operators
participating in SHARES nets.

For SHARES operators: the check-in workflow is the same shape — Compose,
pick the SHARES check-in form, fill, Send, Connect. The form-validation
is stricter (some fields are required and the receiver validates
mechanically).

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
