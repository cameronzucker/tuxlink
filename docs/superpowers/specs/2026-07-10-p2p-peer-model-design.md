# Design — the Peer model and P2P as a complete mode

Date: 2026-07-10
Author: kite-sandbar-vetch
bd: tuxlink-c39af (VARA protocol), tuxlink-gbb05 (SSID path), tuxlink-m9kcd
(REGISTERED gate), tuxlink-sg5zw.8 (peer store — this document is the design
that issue's dangling "spec Part 2" pointer intended), tuxlink-sg5zw.2
(coordination: telnet_p2p agent-tool rebuild consumes the peer store)
Status: DRAFT — operator-approved section by section in-session (2026-07-10);
pending Codex adversarial review (build-robust-features step 2)

## Problem

Tuxlink has no entity that represents a remote station a user connects to
directly. `find_stations` lists only RMS gateways; the same human peer exists
as up to four unlinked records (`Contact`, `Favorite.gateway` string,
`AllowedStations` entry, runtime `PeerId`) or as no record at all. An
accepted inbound P2P call leaves no durable trace anywhere — the recents log
records outbound dials only (`favorite_record_attempt`, frontend-driven).
VARA P2P is built (listen arming, answer-role B2F, dial path) but carries
three protocol defects: no session-type command, invalid compression
vocabulary, and an SSID-stripping dial path in the catalog. P2P cannot
function as a mode without peer tracking (operator decision 2026-07-01,
recorded in tuxlink-sg5zw.8).

## Ground truth — how Winlink Express handles this (decompile-verified)

WLE has **no unified station entity**. Four disjoint stores that never join
on callsign:

- Radio P2P targets: one flat pipe-delimited file per mode
  (`Ardop Peer Stations.dat` = callsign|freq;
  `Packet Peer Stations.dat` = callsign|relays|baud|freq). Rows are
  auto-appended on outbound connect (`ArdopSession.cs:2526-2557`) plus
  manual dialog entry.
- Telnet P2P targets: `Telnet P2P Favorites.dat`
  (callsign|IP|port|password|…) — entirely manual; no directory/CMS lookup
  of peer addresses exists anywhere in WLE.
- Inbound callers: never persisted. An accepted inbound P2P logon writes one
  session-log line (`TelnetP2PSession.cs:1319`) and nothing else.
- Contacts (`Contacts.txt`): email addressing only; no P2P path reads or
  writes it. `StationStatistics.dat` keeps per-callsign+frequency quality
  stats, but only to bias gateway channel selection.

Session-type wire behavior (the c39af gap): WLE sends `P2P SESSION` or
`WINLINK SESSION` per dial, immediately before `CONNECT`
(`VaraSession.cs:3683/3688`). At init it sends `PUBLIC ON`, `CWID ON/OFF`,
`COMPRESSION ON/OFF`, `RETRIES 10`, `MYCALL`, then `LISTEN ON`.

Tuxlink adopts WLE's *behavior* (working a station creates the record;
telnet addresses are operator-supplied) and intentionally diverges from its
*architecture* (per the features-yes-UX-no rule): one entity, both
directions tracked, linked rather than fragmented.

## Section 1 — the entity model

**Tuxlink models the station, not the transport.** Four entities, three
relationships:

- **Peer** — a station this station has connected to directly, been called
  by, or intends to reach. Auto-tracked (below) plus manual add. The hub.
- **Contact** — the who-you-know axis (mail addressing, unchanged shape).
  A peer MAY link to a contact via `contact_id`.
- **Favorite / recents** — the quick-dial axis (unchanged shape and store).
  Peer channels are starrable exactly as gateway channels are.
- **Gateway** — catalog entity (listings today, CMS channels API per
  tuxlink-hmoz8). Gateways never become peers; peers never appear in the
  gateway catalog.

APRS heard stations remain their own ephemeral in-memory family (separate
identity per the APRS config posture) with a "save as peer" promotion
affordance in the station popup.

**Identity key:** the peer record anchors on the **base callsign**
(canonical uppercase). Every per-channel observation stores the **exact
SSID'd callsign** dialed or observed. This mirrors the catalog's
station/channel aggregation (`stationModel.ts` aggregation key vs
`Channel.ssid`), matches reality (the same operator is `W6ABC-7` on packet
and `W6ABC` on VARA HF), and structurally prevents the gbb05 bug class: the
dial target is always the channel's SSID'd callsign, never the pin's base
callsign.

## Section 2 — storage: a new first-class peer store

`peers.json` in the Tauri app-data dir beside `contacts.json` /
`stations.json`, same atomic-write JSON pattern, Rust source of truth
(`src-tauri/src/peers/store.rs`, new). NOT a section in the main TOML config
(`Config` is `deny_unknown_fields`; the peer roster is data, not
configuration). Telnet peer passwords go in the OS keyring, keyed by peer
callsign; never in the file.

```
Peer {
  id: String,                    // stable ULID
  callsign: String,              // base callsign, canonical uppercase — dedup anchor
  contact_id: Option<String>,    // link into contacts.json
  grid: Option<{ value: String, source: Contact|Aprs|Manual }>,  // never guessed
  origin: Incoming | Outgoing | Manual,   // how the record was born
  note: String,
  created_at / last_connected_at: RFC3339 local-offset (per the favorites
                                  ts_local convention),
  channels: [ {                  // RF reachability observations
    transport: TransportKind,    // packet | ardop | vara-hf | vara-fm
    target_callsign: String,     // EXACT SSID'd callsign for the wire (e.g. N0DAJ-7)
    freq_khz: Option<f64>,       // center frequency, catalog semantics (#1064)
    bandwidth_hz: Option<u32>,
    direction: Incoming | Outgoing,   // most recent
    counts: { ok: u32, fail: u32 },
    last_seen: ts,
  } ],
  endpoints: [ {                 // network reachability (telnet P2P)
    host: String, port: u16,
    provenance: Operator | ObservedIncoming,
    last_seen: ts,
  } ],
}
```

Growth is bounded by reality (records are created by actual RF/telnet
sessions or deliberate adds); no LRU cap. Dedup: upsert by base callsign;
channel observations dedup by (transport, target_callsign, freq_khz rounded
to 0.1 kHz) — repeat sessions on the same channel update counts/last_seen
rather than appending rows.

## Section 3 — auto-tracking (both directions)

One chokepoint in the Rust session layer: `record_peer_observation()` fires
where a P2P-intent exchange concludes, for BOTH exchange roles:

- **Outgoing:** every dial with `SessionIntent::P2p`, any transport —
  packet (`packet_connect`), telnet (`telnet_p2p_connect`), ARDOP/VARA B2F
  dials with intent p2p. Success AND failure are recorded (a failed dial is
  reachability intelligence, mirroring the favorites attempt log).
- **Incoming:** every ACCEPTED inbound answer session. The answer paths
  already hardcode `SessionIntent::P2p`
  (`winlink_backend.rs:3460/3472` ARDOP, `:3594/3606` VARA), so the same
  intent test covers both directions. Rejected/unauthorized inbounds do NOT
  create peers (an attacker knocking must not populate the roster).

Backend placement (not frontend) means agent/MCP dials and headless inbound
accepts populate the roster with no UI mounted. Gateway/CMS/RadioOnly/
PostOffice sessions never create peer records.

Plain-language vocabulary everywhere: **incoming / outgoing / added** — no
ham parlance ("worked") in UI strings or field names.

## Section 4 — trust boundary

A peer record does NOT confer network trust. The single invariant:

> **A telnet endpoint is agent-dialable only if `provenance: Operator`.**

- `find_peers` (new agent read tool, mirrors `find_stations`, does not
  taint) returns peers with all channels; endpoints marked with provenance.
- `telnet_p2p_connect` on the agent surface resolves `(host, port)` from
  operator-provenance endpoints only — this is the SSRF closure
  tuxlink-sg5zw.8 requires; the agent selects among vetted peers and never
  supplies a raw host.
- An `ObservedIncoming` endpoint (learned because a station connected to
  us) is dialable by the operator in the UI — the click is consent — and is
  visibly badged with its provenance. Promotion to `Operator` provenance is
  an explicit edit in the peers settings UI.
- RF channels (callsign + frequency) carry no network-egress risk and are
  unrestricted on both surfaces. RADIO-1 consent continues to gate the
  actual transmission at the Connect click / consent-token flow, unchanged.
- Inbound accept policy is unchanged: `AllowedStations` (allow_all default
  true) + listener arms record decide acceptance; the peer store observes,
  it does not authorize.

## Section 5 — Find a Station: one dialog, type-filtered

There is no finder split. The existing finder surface (map + station rail,
band/mode multi-select, callsign search, radius, reachability ramp,
distance/bearing, star, Use →) becomes **"Find a Station" globally**, with a
new **station type** filter dimension: **Gateway / Peer** (both on by
default). Peers flow through the same aggregation shape as catalog stations
(base-callsign pin, per-channel rows), bringing:

- origin metadata (incoming / outgoing / added) and last-connected,
- telnet endpoint rows (with provenance badge) alongside RF channel rows,
- Connect actions that feed the existing per-mode connect flows with
  intent `p2p`, target = the channel's SSID'd callsign, frequency prefilled
  (center semantics).

Reachability prediction applies to peers exactly as to gateways when a grid
is known (contact link, APRS, or manual); peers without grid or with only
telnet reachability render untiered. A "+ Add peer" affordance lives on
the finder; the full roster editor (endpoints, provenance promotion,
keyring password, contact link) is the inline "P2P Peers" settings section
per tuxlink-sg5zw.8 (high-fidelity mock before build).

## Section 6 — map symbology: shape encodes entity, color keeps its meaning

Color is already semantically loaded (reachability ramp in the finder;
session-outcome tiers on tac chat) and MUST NOT encode entity type.

- **Shape** is the entity discriminator everywhere: **diamond = gateway**
  (unchanged), **circle = peer** (new), **authentic sprite = APRS heard**
  (unchanged).
- Finder map: both shapes take the existing six-step reachability ramp;
  peers without prediction render dashed-outline in the untiered grey.
- Tac-chat map: both shapes take the existing outcome colors (reached
  green / failed orange / stale translucent / live halo). A never-connected
  manual peer renders dashed.
- A station that is both a saved peer and live APRS traffic shows the APRS
  sprite (live RF truth wins) with a neutral dashed ring linking it to the
  roster. No identity hue anywhere.

## Section 7 — VARA protocol completeness (c39af, gbb05, m9kcd)

Grounded in the VARA Protocol Native TNC Commands doc (EA5HVK) and the WLE
decompile:

1. **`OutboundCommand::SessionType`** — new variant:
   `SessionType(VaraSessionType)` rendering `P2P SESSION` or
   `WINLINK SESSION`. Sent (a) at `vara_open_session` based on
   `SessionIntent`, and (b) again immediately before every `CONNECT` (WLE
   sends it per-dial at exactly that point; explicit-over-default protects
   against a modem left in the wrong mode by another host app). Mapping:
   `P2p → P2P SESSION`; all other intents → `WINLINK SESSION`. The spec
   semantics: P2P SESSION sets the 4.6 s retry cycle required for
   peer-to-peer timing; WINLINK SESSION the 4.0 s RMS cycle. ARDOP and
   packet have no wire equivalent — this is VARA-specific plumbing behind
   the same intent; no cross-mode abstraction is invented for it.
2. **Compression vocabulary fix** — `Compression` enum becomes
   `Off | Text | Files` (spec vocabulary; `BINARY`/`AUTO` would draw
   `WRONG`). A send site is added: explicit `COMPRESSION TEXT` at session
   open (the spec-recommended Winlink mode; WLE sends an equivalent at
   init).
3. **Own `RETRIES 10`** — sent over the cmd socket at open rather than
   inherited from VARA.ini (WLE does this; the R2 ini currently carries a
   campaign-debugging value of 30 — exactly the drift this prevents).
4. **REGISTERED gate (m9kcd)** — after `MYCALL`, `CONNECT` is held until
   `REGISTERED <call>` arrives (bounded readiness window, actionable
   "modem not ready" error on expiry). Kills the 464 ms race that silently
   killed dials after fresh VARA starts.
5. **SSID end-to-end (gbb05)** — the VARA modem path already passes SSIDs
   intact (only `.trim().to_uppercase()` between target and wire;
   validation accepts hyphens). The strip is in the catalog:
   `stationModel.ts` dials the base callsign for HF channels. Fix: the dial
   target for every mode is the channel's full SSID'd callsign whenever the
   catalog carries one, pinned by tests from catalog → finder → favorites
   prefill → `CONNECT` wire string. MYCALL likewise passes an SSID'd
   configured identity through unmangled (test-pinned; the VARA spec allows
   `-1..-15, -T, -R`).

Order of commands at open becomes: `MYCALL` → wait `REGISTERED` →
`SessionType` → `COMPRESSION TEXT` → `RETRIES 10` → optional `BW` →
optional `LISTEN ON`. Dial: `SessionType` → `CONNECT <mycall> <target>`.

**Recorded boundary (not a deferral):** WLE's `-T`/`-L` MYCALL suffixes
implement radio-only / post-office **gateway sub-mode addressing** — a
different feature from P2P, out of this design's scope. It is recorded here
so the boundary is explicit; if radio-only-over-RF interop with WLE
gateways is pursued, it needs its own design.

### Interop analysis — the divergence from WLE's per-mode stores is off-wire

The peer store never appears on any wire. The interop surfaces are: the VARA
TNC command stream (host-local to Tuxlink's own modem; this design increases
WLE conformance there — session-type command, valid compression vocabulary,
owned RETRIES — and fixes the one far-end-observable nonconformance, dialing
peers on the 4.0 s RMS retry cycle instead of the 4.6 s P2P cycle); ConReq/
AX.25 addressing (dials always use the channel's exact SSID'd callsign — the
base-callsign anchor is aggregation-only and nothing wire-facing derives
from it); B2F semantics (already modeled by SessionIntent/RoutingFlag/
ExchangeRole independent of storage); and the telnet P2P login exchange
(unchanged). Two non-wire caveats, neither created by the divergence:
cross-mode aggregation is an inference (same licensee, possibly different
physical stations — mitigated because every channel row retains its own
callsign/frequency/last-seen evidence), and Winlink P2P requires mutual
intent (a WLE far end answers only from an open P2P session window — an
operational fact recorded in the bench runbook below).

## Section 8 — two-rig bench verification (operator-executed, RADIO-1)

The home lab is a complete P2P bench: no CMS, no WDT dependency. Rig plan:
Tuxlink (R2, VARA instance 1, FT-710) ↔ WLE under WINE (R2, VARA instance
2, G90) — the existing `~/.wine-wle` install is the interop far end. The
cross-rig path was proven at 2300 Hz by the self-decode rig.

Runbook (full command detail to be carried in the implementation plan;
R2 relaunch per the 2026-07-09 handoff §machine-state; the pre-#1064
`/tmp/corrected_dials.py` scripts are retired — the app now expects CENTER
frequencies):

1. **Wire tap first:** socat hex tap on the Tuxlink↔VARA cmd socket to
   capture `P2P SESSION`, `REGISTERED`, `COMPRESSION`, `RETRIES` on the
   wire (also serves the tuxlink-i3dg9 evidence gap).
2. **Outgoing:** Tuxlink dials WLE peer (P2P intent), B2F message moves
   both directions, clean disconnect. Verify the peer record materializes
   with origin `outgoing`, correct channel (SSID, center freq). The WLE far
   end must have its **Vara P2P Session** window open (P2P requires mutual
   intent; a WL2K-session window will not answer a P2P call).
3. **Incoming:** Tuxlink armed listen (P2P auto-arm); WLE dials Tuxlink.
   Verify accept path, answer-role B2F, and that the peer record
   materializes with origin `incoming` — the live test of the
   inbound-tracking gap this design closes.
4. **SSID variant:** repeat (2) with an SSID'd MYCALL (e.g. `-7`) on one
   end; confirm ConReq addressing and B2F handshake carry the SSID.
5. Every dial is operator-initiated with clear-channel check; consent =
   the Connect click per RADIO-1.

## Testing (TDD, no on-air)

- Wire rendering: `SessionType`, `Compression` (new vocab), `RETRIES`
  exact-string tests; open/dial command-order test against a scripted mock
  VARA server, including the REGISTERED gate (CONNECT held, released, and
  the expiry error path).
- SSID pipeline: catalog fixture with SSID'd channel → finder row → dial
  target → `CONNECT` wire string carries `CALL-N` at every hop; MYCALL
  SSID pass-through.
- Peer store: upsert/dedup semantics (base-callsign anchor, channel dedup),
  both-direction recording from the session-layer chokepoint (answer-role
  test drives the existing mock listener path), rejected-inbound records
  nothing, atomic-write round-trip, schema-forward-compat (unknown fields
  preserved or versioned per the config-clobber lesson in tuxlink-ulrz).
- Trust boundary: agent `telnet_p2p_connect` refuses `ObservedIncoming`
  endpoints; `find_peers` marks provenance; keyring never serialized.
- UI: finder type filter, peer rows (origin badges, endpoint provenance
  badge), map divIcon shape classes for both maps (pin tests via
  `L.divIcon` + eventHandlers per the react-leaflet false-green pitfall).

## Scope (ADR 0018 — built whole)

The feature is "P2P as a working, discoverable, verifiable mode": peer
store + auto-tracking + trust boundary + finder type filter + map
symbology + VARA protocol completeness + bench runbook. Nothing in that
list is sliced or deferred. The two recorded boundaries — WLE `-T`/`-L`
sub-mode addressing and the tuxlink-hmoz8 CMS channels API ingest — are
different features (gateway addressing and gateway catalog sourcing), not
slices of this one; hmoz8 additionally has its own issue and design notes.
Coordination: tuxlink-sg5zw.2's telnet_p2p agent-tool rebuild consumes the
peer store built here (bd dep edge already exists via sg5zw.8; sg5zw.8's
build lands with this design).
