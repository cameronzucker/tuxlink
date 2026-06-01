# Subsystem #5 — Link / MAC layer

> **Status: Canonical.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview.md](2026-05-31-clean-sheet-modem-overview.md).
> Incorporates overview §5.A.2 (payload-size-aware routing — MAC decides
> which PHY family handles outgoing frames based on size + channel
> conditions, in concert with subsystem #7 link adaptation).

## §1.A Payload-size routing role (per overview §5.A.2)

The MAC layer is the **routing decision point** between PHY families. For
each outgoing frame, MAC consults (current channel quality estimate +
payload size) and decides:

- **Long messages (typical mail exchange):** route to the bit-adaptive
  OFDM family at the link-adaptation-chosen mode. ARQ applies.
- **Short critical payloads (status / position / ICS-213-class) under
  degraded channel conditions:** route to the FSK weak-signal floor mode.
  No ARQ; retransmit-the-whole-message semantics (FT8-pattern).
- **Short messages under good channel conditions:** stay in the OFDM
  family — there's no SNR-floor advantage to dropping down.

Specific size/SNR thresholds for the routing decision are an open
question (see §4 below); the *policy mechanism* (MAC routes; subsystem #7
provides the channel-quality input) is settled.

## §1. Role

The link / MAC subsystem owns **the frame format** that PHY transmits +
receives, and the **link state machine** that governs connection
establishment, identification, addressing, and ordered delivery.

PHY (#3) produces and consumes raw modulated symbols; MAC produces and
consumes structured frames. ARQ (#6) sits above MAC for reliable
delivery; MAC's job is to make ARQ's job legible (correctly-framed
input, addressed correctly, sequence-numbered).

## §2. What the subsystem is NOT

- **Not the application protocol.** Application-level message exchange
  (Winlink B2F, HTML forms, etc.) lives above this layer in the host
  protocol (#8) and the consuming client (tuxlink).
- **Not retransmission policy.** ARQ (#6) owns that. MAC carries the
  sequence numbers; ARQ decides what to do with them.
- **Not link adaptation.** Link adaptation (#7) selects which PHY mode
  to use; MAC is mode-agnostic.

## §3. Forcing functions

1. **Station identification.** Per Part 97, every transmission identifies
   the licensed station. Either MAC carries an explicit callsign field
   in every frame (clean, verifiable, follows AX.25 pattern), or station
   ID is interleaved at standard intervals (less clean, harder to
   verify). The choice is an open question.
2. **Frame header overhead** vs. payload size. Bigger headers = more
   overhead per frame; smaller headers = less expressiveness. Header
   overhead is recoverable: the header gets transmitted *once* per
   frame, and the frame can contain many bytes of payload.
3. **Variable vs. fixed-size frames.** Fixed frames simplify PHY frame
   detection (the receiver knows how many symbols to expect). Variable
   frames are more flexible at the cost of a length field + recovery
   on length corruption.
4. **CRC for frame integrity.** Standard. Polynomial choice matters but
   is well-studied (CRC-16-CCITT, CRC-32, etc.).
5. **Connection-oriented vs. connectionless framing.** AX.25 supports
   both; ARDOP is connection-oriented. tuxmodem will likely support both,
   with connection-oriented (ARQ-protected) being the common case and
   connectionless (bare frames, no ARQ) for broadcast / beaconing.
6. **Addressing scheme.** Source + destination callsign minimum;
   AX.25-style "digipeater path" support is more complex but enables
   relayed delivery (relevant for tuxmodem's potential MAC role in
   non-CMS networks).
7. **No examination of VARA's MAC** (ADR 0014). Less of a temptation
   here than for PHY/FEC, but the rule stands.

## §4. Open design questions

| # | Question | Notes |
|---|---|---|
| §5.Q1 | Frame layout — header / payload / trailer composition? | First architectural choice. |
| §5.Q2 | Station identification — explicit field per frame, or interval-based? | Affects header overhead + Part 97 compliance verification. |
| §5.Q3 | Addressing scheme — callsign-only, callsign+SSID (AX.25 style), or arbitrary opaque addresses? | tuxmodem may need to interop with non-amateur uses long-term; opaque addresses give more flexibility. |
| §5.Q4 | Fixed vs. variable frame size? | Tradeoff per §3.3. |
| §5.Q5 | CRC polynomial and length? | CRC-16-CCITT is the AX.25 default; CRC-32 is stronger. |
| §5.Q6 | Connection-oriented vs. connectionless framing — both supported? | Probably yes; details in subsystem-canonical spec. |
| §5.Q7 | Digipeater / relay path support? | Adds complexity; useful for non-CMS networks. |
| §5.Q8 | Sequence number width — 7 bits (AX.25 v2.0 default), longer? | Wider sequence space allows larger ARQ windows. |

## §5. Citations from foundation doc

- §4: ARQ literature (Lin/Costello, Bertsekas/Gallager — frame structure +
  sequence numbering interact with ARQ design).
- §6.2: ARDOP — connection-oriented framing + sequence number reference.
- §6.3: AX.25 v2.0 vs. v2.2 distinction — useful primitive for "spec your
  sequence width up-front, the wider option is easy to pre-include but
  hard to retrofit later" (the 22-year v2.0 → v2.2 adoption gap shows
  this).

## §6. Dependencies

- **Upstream:** subsystem #3 (PHY frame structure constrains MAC
  framing).
- **Downstream:** subsystem #6 (ARQ uses sequence numbers MAC provides),
  subsystem #8 (host protocol exposes MAC-level operations to clients).

## §7. No-implementation-choice markers

No specific framing, addressing scheme, CRC polynomial, sequence width,
or connection model designated here.

## §8. Watched failure modes

- **Identification framing skipped.** Forgetting Part 97 ID in early
  prototypes can ship into operator-smoke testing — the rule says
  identification has to be there *before* on-air transmission per the
  RADIO-1 pitfall. MAC layer is the right enforcement point.
- **Sequence width underspecified.** Too-narrow sequence space hits
  wraparound under high-throughput conditions and silently corrupts
  ARQ state. The AX.25 v2.0 → v2.2 transition is the warning shot.
- **Frame-size choice that bakes in payload assumptions.** Choosing a
  payload size that "feels right" for current use cases (e.g., "256
  bytes is enough for any Winlink message") creates a hard cap that
  bites later. Build in length flexibility from the start.

Agent: mink-swallow-kite
