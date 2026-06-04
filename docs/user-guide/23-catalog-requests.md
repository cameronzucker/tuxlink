# Catalog requests

A catalog request is a special Winlink message that asks the CMS for a piece
of catalog data — the RMS gateway list, the weather catalog, the position
report list, or one of several others. The CMS answers asynchronously with
a regular Winlink message containing the catalog response, which lands in
the local Inbox like any other received message.

This topic covers what catalog requests are, the ones tuxlink uses, and how
the operator drives them.

## What a catalog request looks like

A catalog request is a Winlink message addressed to a special address —
typically `query@winlink.org` or similar — with a specific subject line
that names the requested catalog. The body usually carries query
parameters (filter to a region, restrict to a band, etc.).

When the CMS receives a catalog request, it queues a response. The next
time the operator's tuxlink station connects (via any transport), the
response arrives in the Inbox.

Catalog requests are inherently **two-pass**:

1. The operator composes and sends the request (one session).
2. The operator connects again to receive the response (a later session).

For Telnet, both can happen in quick succession — minutes apart. For RF,
the gap is usually longer; the operator has to come back and connect again
later.

## Catalogs tuxlink uses

### RMS gateway list

Subject: `request RMS_LIST` (the exact incantation varies; tuxlink's
catalog request form fills it in for the operator).

Returns: the canonical list of every RMS gateway worldwide — callsign,
frequency, mode, bandwidth, position, last-heard time. This is the data
that drives the [Picking a gateway](05-cms-and-rms.md) decision.

Tuxlink stores the returned list and exposes it via the radio panel's
gateway picker. A periodic refresh — operator-initiated, no automated
catalog request — keeps the list current.

### Weather catalog

Subject: `request WX_<region>`. Returns forecast and observation data for
the requested region.

This is operationally relevant for emcomm responders making movement
decisions, marine mobile operators planning routes, expedition operators
choosing band conditions.

### Position report (PR_) and Catalog of Catalogs

Other catalogs include the position-report list (who has reported in
recently from where), the catalog of catalogs (an index of all available
catalog queries), and ad-hoc query types added by the CMS administrators.

## Driving a catalog request

**Tools → Catalog request** opens a small form:

1. **Pick a catalog** — dropdown lists the catalogs tuxlink knows about.
2. **Region / parameter** — text field, prefilled with sensible defaults
   for the catalog (your callsign's grid square, the current weather
   region, etc.).
3. **Send via** — pick a transport. Telnet is the fastest for catalog
   requests because the request itself is small and the response is
   not enormous.

The form generates a properly-formatted Winlink message, drops it in the
Outbox, and the next Connect sends it.

## The response

Catalog responses arrive in the Inbox like any other message. The body is
plain text (or, for some catalogs, formatted with a known structure
tuxlink's parser recognises). The RMS gateway list response, specifically,
is parsed by tuxlink — selecting it from the Inbox triggers an "Update
catalog" affordance that pulls the parsed list into the local cache.

A response that doesn't get parsed (or that the operator dismisses) still
sits in the Inbox as a readable message — no data is lost.

## How often to refresh

The RMS gateway list changes slowly — gateways come and go, but the
quarterly turnover is small. A monthly refresh is sufficient for a station
that operates regularly. For a station that is dormant for months and
then comes back for an emcomm event, a refresh just before the event is
the right call.

The weather catalog is the opposite — it's perishable. A 12-hour-old
weather catalog response is operationally useless. The pattern is fetch
just before you need it.

## Size and bandwidth

| Catalog | Typical response size | Practical transport |
|---|---|---|
| RMS gateway list (global) | 200–400 KB | Telnet — too big for HF radio |
| RMS gateway list (regional filter) | 30–100 KB | VARA HF Standard works |
| Weather catalog (region) | 10–50 KB | VARA HF, ARDOP 1000 Hz |
| Position report list (region) | 5–30 KB | Any HF transport |

A global RMS gateway list pulled over Packet would tie up the channel for
30+ minutes. The right answer for HF refresh is a regional filter — the
request form supports this.

## When catalog requests are inappropriate

Catalog requests are NOT appropriate during an active emcomm event when
operating time is tight and the gateway list / weather is already
sufficient. They are appropriate during the **pre-event preparation**
phase, when the operator has time and a good Telnet path.

For routine non-emcomm operating, catalog requests are background work —
slot them between traffic.

## Where next

- [The Winlink ecosystem](04-the-winlink-ecosystem.md) — what catalogs the CMS serves.
- [CMS and RMS gateways](05-cms-and-rms.md) — what the gateway list is for.
- [Picking a transport](08-picking-a-transport.md) — which transport for which catalog size.
- [The mailbox](18-the-mailbox.md) — where catalog responses land.
