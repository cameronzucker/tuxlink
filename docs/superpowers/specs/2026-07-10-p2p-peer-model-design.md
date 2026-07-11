# Design — the Peer model and P2P as a complete mode

Date: 2026-07-10
Author: kite-sandbar-vetch (design); harrier-glade-osprey (adversarial-review fold)
bd: tuxlink-c39af (VARA protocol), tuxlink-gbb05 (SSID path), tuxlink-m9kcd
(REGISTERED gate), tuxlink-sg5zw.8 (peer store — this document is the design
that issue's dangling "spec Part 2" pointer intended), tuxlink-sg5zw.2
(coordination: telnet_p2p agent-tool rebuild consumes the peer store)
Status: DRAFT — operator-approved section by section in-session (2026-07-10);
full 5-round adversarial review folded (R1 Codex, R2 security, R3 protocol, R4
data-model/integration, R5 Codex re-attack) 2026-07-10. Ready for BRF Step 3
(writing-plans). **AMENDED 2026-07-11:** the operator design pivot at the
Task-25 mock gate folds peers into Contacts and removes the agent telnet dial
— see the binding AMENDMENT section immediately below, which supersedes
Sections 1/2/4/5 where they conflict.

> **Review-fold note.** This revision incorporates ~45 findings across all five
> adversarial rounds (R5 re-attack confirmed the fold and added 10 refinements).
> Disposition ledger:
> `dev/adversarial/2026-07-10-p2p-design-consolidated-dispositions.md`
> (gitignored, local). Finding IDs are cited inline as `[R#-N]`. The fold GROWS
> the feature (identity merge/split, packet-P2P intent plumbing, engine-split
> protocol, curate_peer, agent-tool signature change, keyring migration,
> caps/quarantine). Per ADR 0018 the spec reflects the whole; a correct large
> spec beats a wrong small one.

## AMENDMENT — operator design pivot 2026-07-10/11: contacts are the superset (BINDING)

> Status of this amendment: **operator decisions, recorded at the Task-25 mock
> gate (2026-07-10, session oak-owl-taiga) and the follow-on design pass
> (2026-07-11, session bluff-alder-kestrel).** Where this section conflicts
> with Sections 1, 2, 4, 5, or Cross-store consistency below, **this section
> wins.** The superseded text is retained for the review-finding record (the
> `[R#-N]` security findings remain load-bearing where noted).

### The decision

**There is no separate peer entity and no `peers.json`. A peer is a contact.**
The user model is "the stations I talk to," and Contacts already is that
(2026-06-07 Contacts+Favorites design). The peer store re-implemented the
Favorites reachability model with an identity-management layer on top —
engineering parallelism, not a user need. Instead:

1. **`Contact` becomes the superset of added + observed stations** via a tier
   field: `confirmed` (operator added it — the curated address book, exactly
   as before) and `unconfirmed` (auto-created from P2P session events or a
   manual dial). Auto-creation NEVER lands in the curated tier, so the
   2026-06-07 design's "no silent pollution" guarantee holds; its literal
   "never auto-create" rule (§A.3) is amended to "never auto-create
   `confirmed`."
2. **Reachability lives ON the contact:** `channels` (RF: transport, exact
   SSID'd wire target, via path, freq, bandwidth, direction, ok/fail counts,
   last-seen) and `endpoints` (telnet: host, port, provenance, last-seen),
   plus `grid`. Same shapes as the superseded §2 `Channel`/`Endpoint`; only
   the parent entity changes. `contacts.json` schema_version bumps with
   migration: existing records → `confirmed`, empty reachability.
3. **"Verified" means curation, not authentication.** Pivot decision #2's
   rationale stands: anyone can transmit any callsign, so no UI or doc copy
   may imply identity verification. `confirmed` claims only "the operator
   confirmed this entry into the address book." The "unverified claimed
   identity" badge and the endpoint **promotion ceremony are removed**
   (`peer_endpoint_promote` dies). Endpoint `provenance` survives with one
   job: a stored password attaches to, and is only ever auto-sent to, an
   operator-entered endpoint (`[R2-S7]` unchanged in force).
4. **Identity merge machinery is deleted, not migrated:** `canonical_base` as
   a merge key, `presented_callsigns`, merge/split, `do_not_merge`, conflict
   records. Matching is **exact presented callsign only** — an observation
   attaches to a contact iff the callsign matches exactly; otherwise it
   creates its own `unconfirmed` contact. (The shared callsign module's
   grammar validation + display sanitizer remain; `canonical_base` remains a
   display/grouping helper where already used, never a merge key.)
5. **The agent telnet P2P dial is removed entirely** (supersedes §4 "Agent
   telnet dial"). Not for consent reasons — the armed egress gate is the
   consent mechanism and works. The reason is destination-trust: a telnet
   endpoint is a host:port that arming cannot vouch for, and the
   DNS-rebinding denylist + provenance stack existed only to prop that up.
   The tool, the denylist machinery, and the agent-path telnet observation
   guard all go. The **radio (VARA/ARDOP) agent P2P dial stays** — no host to
   distrust, the armed gate is complete (§4 RF-channel paragraph unchanged).
   The **operator's** manual telnet dial is unchanged.
6. **The agent curated read** (`find_peers`/`curate_peer`) re-reads from
   contacts and **never reveals telnet `host:port` to the agent** under any
   arm state (the agent cannot dial telnet, so it has no use for the
   address). All other curation rules (`[R2-S1][R2-S9][R2-S11][R4-9]`)
   carry over verbatim.
7. **UI: no roster editor, no settings surface** (supersedes §5's "+ Add
   peer" / "P2P Peers settings section"). ContactsPanel gains a **"Recent"**
   section below the curated list (vocabulary shared with Favorites'
   Recent). Within Recent, rows with a completed session carry the
   **"Heard"** distinction ("dialed into my station" / reached); rows
   without one read **"dialed · not reached yet"** — the row, not the
   section, makes the RF claim (honest-record idiom, 2026-06-07 §B.3).
   Contact detail shows live reachability rows with Connect (the Task-23a
   seam). A **manual "dial a callsign" affordance lives in the finder** and
   creates an `unconfirmed` contact. Promote = one-click add, same idiom as
   mailbox suggestions.
8. **Caps + limiter guard the `unconfirmed` tier only:** the inbound
   rate-limit/quarantine (`[R2-S6]`) and the auto-record LRU cap apply to
   auto-created `unconfirmed` contacts; `confirmed` contacts are never
   auto-created and never evicted.
9. **Cross-store consistency machinery dies with the second store** (the
   `contacts:changed` → `reconcile_contact_links` listener). The favorites
   bridge re-keys `Favorite.peer_id` → `Favorite.contact_id`.
10. **Target model, out of scope here:** Favorites is conceptually an
    elevated category of contacts; folding `stations.json` into contacts is
    deliberately NOT in this feature (none of the definition-of-done flows
    traverse it) and is filed as a follow-up bd issue.

## Problem

Tuxlink has no entity that represents a remote station a user connects to
directly. `find_stations` lists only RMS gateways; the same human peer exists
as up to four unlinked records (`Contact`, `Favorite.gateway` string,
`AllowedStations` entry, runtime `PeerId`) or as no record at all. An
accepted inbound P2P call leaves no durable trace anywhere — the recents log
records outbound dials only (`favorite_record_attempt`, frontend-driven).
VARA P2P is built (listen arming, answer-role B2F, dial path) but carries
protocol defects: HF-only commands sprayed at the VARA FM engine, no
session-type command, invalid compression vocabulary, an SSID-stripping dial
path in the catalog, and a REGISTERED readiness gate whose release token the
command parser cannot produce. P2P cannot function as a mode without peer
tracking (operator decision 2026-07-01, recorded in tuxlink-sg5zw.8).

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

Session-type wire behavior (the c39af gap), **VARA HF/SAT only**: WLE's
`VaraSession` sends `P2P SESSION` or `WINLINK SESSION` per dial, immediately
before `CONNECT` (`VaraSession.cs:3683/3688`). At HF init it sends `PUBLIC ON`,
`CWID ON/OFF`, `COMPRESSION`, `RETRIES 10` (P2P branch only), `MYCALL`, then
`LISTEN ON`. **VARA FM is a separate class** (`VaraFMSession.cs`) whose entire
TNC command set is `MYCALL`, `LISTEN ON/OFF`, `CONNECT src dst [VIA d1 [d2]]`,
`ABORT`, `DISCONNECT` — no SESSION, no COMPRESSION, no RETRIES, no PUBLIC, no
BW `[R3-1]`. This engine split is load-bearing for §7.

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
  A peer MAY link to a contact via `contact_id` (one-way; §Cross-store).
- **Favorite / recents** — the quick-dial axis (unchanged shape and store).
  Peer channels are starrable exactly as gateway channels are.
- **Gateway** — catalog entity (listings today, CMS channels API per
  tuxlink-hmoz8). Gateways never become peers; peers never appear in the
  gateway catalog.

APRS heard stations remain their own ephemeral in-memory family (separate
identity per the APRS config posture) with a "save as peer" promotion
affordance in the station popup (origin `Aprs`, §2 `[R4-12]`).

### Identity model — anchor is a dedup hint, not a wire source `[R4-6][R4-7][R1-C6]`

The naive "anchor on base callsign" over-merges operationally distinct
stations (club calls, reassigned vanity calls, two ops sharing a base) and
mis-normalizes portable/tactical forms. The identity model separates three
concerns:

- `canonical_base: String` — the **auto-dedup anchor only**. Derived by
  `canonical_base()`: uppercase, trim, take the substring before the first
  `/`, then strip a trailing SSID (`-0`..`-15`, `-T`, `-R`, WLE off-doc `-L`).
  Never used to derive a wire target.
- `presented_callsigns: Vec<String>` — every exact form observed or dialed
  (`W6ABC-7`, `W6ABC/P`, `W6ABC`), deduped, verbatim. The wire target for
  any dial is always the exact presented/SSID'd callsign of the chosen
  channel `[R3-9]`.
- `identity_kind: Individual | Tactical | Club | Unknown` (`#[serde(other)]
  Unknown`). Auto-created records default `Unknown`. `Tactical` records are
  NOT base-normalized-merged (a tactical call has no standard structure); the
  anchor for a `Tactical` peer is its full presented string.

Auto-tracking upserts by `canonical_base` **but never re-merges a record the
operator has split** (a `do_not_merge` marker survives on split records).
Manual **merge** and **split** affordances live in the peers settings UI. A
"two ops on one club call collapse to one record unless split" behavior is a
chosen default, pinned by a test `[R4-6]`.

**Identity key of a Peer record:** a stable ULID `id`. `canonical_base` is an
index for dedup, not the primary key — so a split produces two records with
distinct ids sharing a base.

## Section 2 — storage: a new first-class peer store

`peers.json` in the Tauri app-data dir beside `contacts.json` /
`stations.json`, same atomic-write JSON pattern, Rust source of truth
(`src-tauri/src/peers/store.rs`, new). NOT a section in the main TOML config
(`Config` is `deny_unknown_fields`; the peer roster is data, not
configuration).

### File shape + forward-compat `[R4-5][R1-C12][R3-11]`

```
PeersFile {
  schema_version: u32,           // = 1; hand-written Default like contacts/store.rs
  peers: Vec<Peer>,
}

Peer {
  id: String,                    // stable ULID — primary key
  canonical_base: String,        // dedup anchor (§1); NOT a wire source
  presented_callsigns: Vec<String>,
  identity_kind: IdentityKind,   // #[serde(other)] Unknown
  do_not_merge: bool,            // set when operator splits; suppresses auto-merge
  source: RecordSource,          // Auto | Manual | OperatorPinned  (#[serde(other)] Unknown)
  origin: Origin,                // Incoming | Outgoing | Manual | Aprs  (#[serde(other)] Unknown)
  contact_id: Option<String>,    // one-way link into contacts.json (§Cross-store)
  grid: Option<{ value: String, source: GridSource }>,  // GridSource: Contact|Aprs|Manual, #[serde(other)]
  note: String,                  // operator free-text; NEVER crosses the agent surface (§4)
  created_at / last_connected_at: RFC3339 local-offset (favorites ts_local convention),
  channels: Vec<Channel>,        // RF reachability observations
  endpoints: Vec<Endpoint>,      // network reachability (telnet P2P)
}

Channel {
  transport: TransportKind,      // packet | ardop | vara-hf | vara-fm  (#[serde(other)])
  target_callsign: String,       // EXACT SSID'd callsign for the wire (e.g. N0DAJ-7)
  via: Vec<String>,              // digipeater path, max 2 (packet/FM); empty = direct  [R3-6]
  freq_hz: Option<u64>,          // center frequency, catalog semantics (#1064); exact Hz, not rounded
  bandwidth: Option<Bandwidth>,  // HF: Hz enum; FM: Wide|Narrow  [R3-7]  (#[serde(other)])
  direction: Direction,          // most recent  (#[serde(other)])
  counts: { ok: u32, fail: u32 },// saturating
  last_seen: ts,
}

Endpoint {
  id: String,                    // stable ULID — keyring key component
  host: String, port: u16,
  provenance: Provenance,        // Operator | ObservedIncoming  (#[serde(other)] Unknown)
  last_seen: ts,
}
```

Telnet peer passwords go in the OS keyring, keyed by
`p2p-endpoint:<peer_id>:<endpoint_id>` (both ULIDs), NOT by callsign
`[R2-S7][R1-C7][R2-S10]`. The password is never serialized to the file, never
auto-sent to an `ObservedIncoming` endpoint, and is cascade-cleared on
peer/endpoint delete.

**Legacy keyring migration is conservative** `[R5-5]`: a legacy
`p2p-peer:<CALLSIGN>` secret is auto-re-keyed only when it maps
**unambiguously** — exactly one matching peer and exactly one `Operator`
endpoint. Any ambiguity (multiple peers on that base, multiple endpoints, or
no Operator endpoint) surfaces a manual-reassignment prompt in the peers
settings UI rather than guessing. The legacy secret is deleted only after the
new-key write succeeds (no window where both keys disagree or the secret is
lost).

**Load path (infallible-open):** mirror `contacts/store.rs` — a missing file
starts empty; a corrupt or parse-failing file is renamed to
`peers.json.corrupt-<ts>` (quarantine) and the store starts empty, never
panicking. Every enum carries `#[serde(other)] Unknown` so a variant written
by a future binary quarantines that one row's field, not the whole roster
`[R4-5]`.

### Dedup — per-transport keys `[R4-11][R1-C8][R3-6]`

Upsert Peer by `canonical_base` **unless a split has occurred** `[R5-4]`: once
any record carries `do_not_merge`, new auto-observations for that base are
routed by **exact presented callsign + channel match** to the specific split
record, never by base alone. An observation that matches no split record's
presented callsigns is held as a conflict-marked record for manual
association (it does not silently update the wrong twin). Then:

- **Channel dedup key:** `(transport, target_callsign, via, freq_hz exact,
  bandwidth)`. The prior `freq rounded 0.1 kHz` rounding is dropped (too
  lossy across transports; telnet has no freq, packet/FM need `via`). Repeat
  sessions on the same channel update counts/last_seen; a different `via`,
  bandwidth, or exact freq is a distinct channel.
- **Endpoint dedup key:** `(host_normalized, port, provenance)`. Provenance
  is **monotonic**: `Operator` is sticky and NEVER downgraded by an
  observation; an inbound observation may never create or mutate an
  `Operator`-provenance endpoint; `ObservedIncoming` never auto-upgrades — only
  the explicit settings-UI promotion sets `Operator` `[R4-4][R2-S8]`.

### Growth caps — "bounded by reality" fails against an adversary `[R2-S6][R1-C9]`

`allow_all` defaults TRUE, so an attacker can loop handshakes with rotating
spoofed callsigns. Therefore:

- `source: Auto | Manual | OperatorPinned` distinguishes provenance of the
  record itself. `Manual`/`OperatorPinned` records are never evicted.
- Auto-created records have a soft cap (default 1000); over-cap eviction is
  LRU among `Auto`-only records.
- Inbound auto-create is rate-limited, but the limiter must not silently drop
  a legitimate exercise (many distinct stations calling in a short window)
  `[R5-9]`. It distinguishes **accepted, authorized inbound exchanges**
  (allowlist-passed, B2F-completed) from **unauthorized/failed bursts**
  (rejected, auth-failed, or handshake-abandoned). Accepted exchanges get a
  high threshold (a real net); unauthorized/failed bursts get a low one and
  increment a bounded quarantine counter (logged **visibly** to the operator,
  not persisted to the roster). Thresholds and window are per-transport,
  configurable, and surfaced with a review path — a spoofing loop hits the
  counter; a busy field day does not lose roster observations.

## Section 3 — auto-tracking (both directions, all transports)

**There is no single chokepoint** `[R4-1][R1-C3][R3-11]`. `run_exchange_with_role`
(`session/mod.rs:271`) is deliberately transport-agnostic — `ExchangeConfig`
carries only `intent/mycall/targetcall/locator`, not transport/freq/bandwidth/
endpoint — so it cannot build a `Channel`. And two transports bypass
`WinlinkBackend` entirely (`ui_commands.rs:7564,7645`). The design is a
**shared recorder function called at each transport's attempt-conclusion
site**:

```
record_peer_observation(ctx: P2pObservationContext)
  ctx = { transport, engine, direction, presented_target, canonical_base,
          via, freq_hz, bandwidth, endpoint: Option, outcome, phase }
```

`outcome`/`phase` classify the attempt: `dial_attempted → connected →
(login_failed | b2f_started → b2f_ok | b2f_fail) | accepted | rejected |
aborted/wedged`. The recorder maps these to `counts.ok`/`counts.fail` or to
**no record** (rejected/unauthorized inbound must never populate the roster —
an attacker knocking is not a peer). Recording is placed in a drop-guard /
finally so a mid-exchange-wedged or aborted session still records a `fail`
(the ARDOP ARQTimeout-no-host-backstop lesson applies to VARA too) `[R3-11]`.

**Enumerated record sites (8), because "one funnel" would silently miss
telnet and packet:**

| Transport | Outbound (dial) | Inbound (answer) |
|---|---|---|
| VARA HF/FM | `run_vara_b2f_exchange` conclusion + `ConnectFailed` pre-exchange site (`commands.rs:2528`) | `run_vara_b2f_answer` (`winlink_backend.rs:3594/3606`) |
| ARDOP | `run_ardop_b2f_exchange` conclusion **+ the outer connect-fail site — ARQ `ConnectFailed` returns pre-exchange** (`modem_commands.rs:1836-1850`), so the recorder guard sits at `run_ardop_connect_b2f_with_transport`, not only inside the exchange `[R5-2]` | `run_ardop_b2f_answer` (`winlink_backend.rs:3460/3472`) |
| Packet | `native_packet_connect` (**requires P2P-intent plumbing**, below) | `native_packet_exchange` answer (**same**) |
| Telnet | `telnet_p2p_connect` (`ui_commands.rs:7564`) incl. resolve/login-fail (`telnet_p2p.rs:107`) | `telnet_listen` answer completion |

Two sites feed connect-failure (pre-exchange) and exchange-conclusion into
the same recorder `[R3-11]`. `freq_hz` for an `origin: incoming` row has no
wire source (VARA `CONNECTED` carries bandwidth, not frequency) — it comes
from rig/CAT state if available, else `None`; stated explicitly so it isn't
silently fabricated `[R3-11]`.

**Packet P2P intent plumbing (in scope).** `native_packet_exchange` currently
builds `ExchangeConfig { intent: SessionIntent::Cms }` for both directions
(`winlink_backend.rs`), so an intent filter can never classify a packet
session as P2P and packet peers would silently never track `[R4-3][R1-C15]`.
Plumbing only `ExchangeConfig.intent` is insufficient `[R5-3]`: packet has no
intent in `TransportConfig`/`PacketConnectCtx` and hand-builds its outbound
proposals *before* the exchange config (`winlink_backend.rs:2416-2440`). The
design threads `SessionIntent` through `packet_connect`
(`ui_commands.rs:4862`) → `PacketConnectCtx`/`TransportConfig` → the
**intent-aware outbound-proposal builder** (replacing the hand-built path),
with `Cms` as the default for every existing caller. Tests pin both
directions: CMS packet behavior unchanged, and a P2P packet session is not
classified as CMS. Packet stays a peer transport (WLE `Packet Peer Stations`
ground truth).

Backend placement (not frontend) means agent/MCP dials and headless inbound
accepts populate the roster with no UI mounted. Gateway/CMS/RadioOnly/
PostOffice sessions never create peer records.

Plain-language vocabulary everywhere: **incoming / outgoing / added** — no
ham parlance ("worked") in UI strings or field names.

## Section 4 — trust boundary

A peer record confers NO network trust and its RF-sourced fields are
attacker-controllable (`parse_peer_call` upper-cases + trims but applies no
charset filter — `listener.rs:94-110`; `allow_all` defaults TRUE). Two
invariants:

> **I1. A telnet endpoint is agent-dialable only if `provenance: Operator`.**
> **I2. Every peer-derived string is validated at write and escaped at render.**

### `curate_peer` — mandatory boundary before the agent surface `[R2-S1][R2-S9][R2-S11][R1-C5]`

`find_stations` is agent-safe only because `curate_gateway`
(`mcp_ports.rs:2311-2344`) drops free-text, shape-validates callsigns, and
strips control chars. `find_peers` MUST mirror that **curation**, not the DTO
shape. `curate_peer`:

- validates every callsign — but a single regex is both too loose and too
  tight `[R5-10]`. Two distinct checks: a **broad display/injection sanitizer**
  (reject control chars, `:`, path separators, whitespace, angle brackets —
  the XSS + keyring-account-string safety floor, applied to everything crossing
  the boundary) and **transport-aware grammar validation** (VARA/AX.25 rules
  that preserve legitimate `/P`, `-T`, `-R`, `-L` presented forms). A record
  failing the sanitizer is dropped from the agent DTO; a malformed inbound
  callsign is dropped/quarantined at the write boundary before it can be
  exported;
- drops `note` and any free-text `[R2-S11]`;
- drops contact names/notes reached via `contact_id` `[R4-9][R1-C13]`;
- clamps peer `grid` to the operator's configured precision (4-char default)
  — a third party's location is protected at least as well as the operator's
  own `[R2-S9]`;
- redacts endpoint `host:port` unless `provenance: Operator` **and** the
  egress arm is active;
- strips control chars and caps result size.

`find_peers` is additionally **gated behind egress-arm state** — the roster is
the operator's private social graph (who they talk to, locations, internal LAN
IPs), not public catalog data `[R2-S5][R1-C5]`.

### Write-boundary + render-boundary validation `[R2-S2][R2-S10]`

Charset-validate callsigns at `parse_peer_call` / record-write; a
non-conforming inbound callsign creates no record (closes DOM-XSS and keyring
account-string injection at the source). Every peer-derived string rendered in
Leaflet `divIcon`/`bindPopup` uses `textContent`/escaped interpolation, never
a raw HTML string; a hostile-callsign render test pins it.

### Agent telnet dial — no raw host `[R2-S3][R2-S4][R1-C4][R1-C16]`

The existing `telnet_p2p_connect` accepts a raw `host`/`port`
(`ui_commands.rs:7473`); that path stays **UI-only** (operator click =
consent). The agent egress tool takes `(peer_id, endpoint_id)` only and
resolves `(host, port)` from `Operator`-provenance endpoints — the SSRF
closure sg5zw.8 requires; the agent never supplies a raw host. The egress
denylist must be **DNS-rebinding-safe** `[R5-6]`: the current dialer resolves
inside `connect_stream` and iterates all returned addresses
(`telnet_p2p.rs:107-126`), so denylisting a single "the resolved IP" leaves a
TOCTOU hole. The agent tool resolves **once**, applies the denylist to **every
candidate address** (loopback, RFC1918, link-local `169.254/16`, ULA
`fc00::/7`, cloud metadata `169.254.169.254`, plus IPv6 loopback/link-local
and IPv4-mapped-IPv6 private ranges), then connects to a vetted concrete
`SocketAddr` with **no second lookup** `[R2-S4]`.

### Provenance + inbound endpoints `[R2-S3][R1-C16]`

An `ObservedIncoming` endpoint (learned because a station connected to us) is
recorded but **agent-non-dialable**; it is operator-dialable in the UI (the
click is consent), badged **"unverified claimed identity"** (the callsign is
spoofable), and **never auto-promotable**. Promotion to `Operator` is an
explicit operator edit that acknowledges out-of-band verification, and is a
**stable-id in-place mutation** of the existing endpoint (it does not mint a
new endpoint id) so the keyring secret is not orphaned `[R5-5]`. The stored
keyring password is never auto-sent to an `ObservedIncoming` endpoint
`[R2-S7]`.

RF channels (callsign + frequency) carry no network-egress risk and are
unrestricted on both surfaces. RADIO-1 consent continues to gate the actual
transmission at the Connect click / consent-token flow, unchanged. Inbound
accept policy is unchanged: `AllowedStations` (allow_all default true) +
listener arms decide acceptance; the peer store observes, it does not
authorize.

## Section 5 — Find a Station: one dialog, type-filtered

There is no finder split. The existing finder surface (map + station rail,
band/mode multi-select, callsign search, radius, reachability ramp,
distance/bearing, star, Use →) becomes **"Find a Station" globally**, with a
new **station type** filter dimension: **Gateway / Peer** (both on by
default).

**Peers do NOT reuse `aggregateStations()`** `[R4-8]`: that function drops
gridless rows (`stationModel.ts:53-61 if (!grid) continue;`) and keys on
`(base, grid)`. Peers are frequently gridless (telnet-only) and key on
`canonical_base` alone. The design specifies a **distinct peer aggregation**
that (a) keys on `canonical_base`, (b) tolerates `grid: None`, (c) renders
gridless/telnet-only peers in the rail (untiered) even when they cannot be
map-placed. Peer rows bring:

- origin metadata (incoming / outgoing / added / APRS) and last-connected,
- telnet endpoint rows (with provenance badge) alongside RF channel rows,
- Connect actions feeding the existing per-mode connect flows with intent
  `p2p`, target = the channel's SSID'd callsign, `via` prefilled where
  present, frequency prefilled (center semantics).

Reachability prediction applies to peers exactly as to gateways when a grid
is known; peers without grid or with only telnet reachability render
untiered. A "+ Add peer" affordance lives on the finder; the full roster
editor (endpoints, provenance promotion, keyring password, contact link,
merge/split) is the inline "P2P Peers" settings section per tuxlink-sg5zw.8
(high-fidelity mock before build).

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

All peer-derived strings in `divIcon`/popup are escaped per §4 (I2).

## Section 7 — VARA protocol completeness (c39af, gbb05, m9kcd) — engine-split

Grounded in the EA5HVK "VARA Protocol Native TNC Commands" doc (Feb 2022) and
the WLE decompile (`VaraSession.cs`, `VaraFMSession.cs`). **The command plan
branches on engine** `[R3-1][R1-C1]` — HF/SAT and FM are separate WLE classes
with different command sets.

### VARA HF / SAT command plan

Open order: `MYCALL` → **readiness gate** → `PUBLIC ON` → optional `CWID` →
`SessionType` → `COMPRESSION TEXT` → `RETRIES 10` → optional `BW` → optional
`LISTEN ON`. Dial: `SessionType` → `CONNECT <mycall> <target> [VIA …]`, with
`SessionType` re-sent **inside `send_connect_and_wait`** immediately before
each CONNECT (per-candidate, so a multi-candidate QSY walk never dials a later
candidate in a stale session mode) `[R3-9-placement]`.

1. **`OutboundCommand::SessionType(VaraSessionType)`** — renders `P2P SESSION`
   or `WINLINK SESSION` (HF/SAT only per the doc). Mapping: `P2p → P2P
   SESSION`; all other intents → `WINLINK SESSION`. Sets the 4.6 s (P2P) vs
   4.0 s (RMS) retry cycle. ARDOP and packet have no wire equivalent.
2. **Compression vocabulary** — `Compression` enum becomes `Off | Text |
   Files` (doc-exact; `Binary`/`Auto` draw `WRONG`). `TEXT` is the
   doc-"Recommended for Winlink" mode. Sent HF/SAT only.
3. **`RETRIES 10`** — undocumented-but-WLE-used; WLE sends it on the P2P
   branch only, never to FM `[R3-4]`. Send HF-only. **Verify it is a TCP
   command vs `VARA.ini`-only** before relying on the runtime send; if
   ini-only, provision via `VARA.ini` (§8 wire-tap confirms "RETRIES
   accepted?").
4. **`PUBLIC ON` + CWID** `[R3-5]` — WLE sends `PUBLIC ON` at every HF open
   (may gate inbound accept → protects bench step 3). Send `PUBLIC ON` at HF
   open; CWID owned-vs-ini is an operator/regulatory call recorded in the
   plan. Neither goes to FM.
5. **REGISTERED readiness gate (m9kcd)** `[R3-2][R1-C2]` — CONFIRMED bug: the
   parser recognizes only `LINK REGISTERED` (`command.rs:249`), never the bare
   `REGISTERED <call>` the gate waits for. Fix:
   - Add `InboundCommand::Registered(Option<String>)`, disambiguated from
     `LinkRegistered`, accepting **any** `REGISTERED` line (bare = unregistered
     tier, or SSID'd — no callsign match required; an unregistered VARA is
     fully functional and the project's common case).
   - The gate is **latched once per transport-open**, NOT per-dial (REGISTERED
     does not repeat per dial → a per-dial gate deadlocks dial #2).
   - Behavior: wait ≤ `T_max` (default 5 s) for any readiness token, but always
     honor a `T_min` settle (default 600 ms) that defeats the 464 ms m9kcd
     race whether or not the token arrives. On arrival after `T_min`, proceed.
     On `T_max` expiry, **fail open** — proceed with a "modem readiness
     unconfirmed" warning, never a hard "modem not ready" error, never a
     wedge. (Anti-wedge posture per the ARDOP ARQTimeout lesson.)
6. **SSID end-to-end (gbb05)** `[R3-3-echo]` — CONFIRMED collision: the dial
   target must be the channel's full SSID'd callsign, but VARA's `CONNECTED`
   echo is the **bare** callsign (`listener.rs:8-9`), and the outbound success
   check `peer.eq_ignore_ascii_case(target)` (`commands.rs:2762`) then rejects
   a successful SSID'd dial as `unexpected CONNECTED peer`. Fix: compare the
   echo on `canonical_base` (strip SSID both sides); the wire dial string stays
   SSID'd. Pinned catalog → finder → favorites prefill → `CONNECT` wire string,
   and echo-match, by tests. MYCALL passes an SSID'd identity through
   unmangled; add wire-grammar validation (base 3-7 A-Z0-9; SSID `-1..-15`,
   `-T`, `-R`; reject 8-char base / `-16`) `[R3-9]`.

### VARA FM command plan `[R3-1]`

FM's entire WLE command set is `MYCALL`, `LISTEN ON/OFF`, `CONNECT src dst
[VIA d1 [d2]]`, `ABORT`, `DISCONNECT`. Therefore FM:

- Open: `MYCALL` → readiness settle (`T_min` only; no `REGISTERED` wait, though
  FM does emit REGISTERED — it is log-only) → optional `LISTEN ON`.
- Dial: bare `CONNECT <mycall> <target> [VIA …]` — **no `SessionType`
  prefix**, no `COMPRESSION`, no `RETRIES`, no `PUBLIC`, no `BW`.
- FM `CONNECTED` bandwidth token is `WIDE`/`NARROW`, not Hz `[R3-7]`; the peer
  observation stores a `Bandwidth::{Wide,Narrow}` enum for FM, and the parser
  must not `tokens[2].parse::<u32>()` (silently `None`) nor drop via-digis.

### Setter response handling — WRONG is non-fatal `[R3-3]` (keystone)

WLE hardcodes a `WRONG`-suppression list for CWID / COMPRESSION / WINLINK
SESSION / P2P SESSION (`VaraSession.cs:4452`) — hard evidence these draw
`WRONG` across engine/version drift. Therefore a `WRONG` reply to
`SessionType`/`COMPRESSION`/`RETRIES`/`PUBLIC`/`CWID` is **logged, never
fatal**; only CONNECT-path failure is dial-fatal. Additionally, parse a bare
`WRONG` during dial as a distinct inbound so a rejected/malformed CONNECT
fails fast instead of eating the full `VARA_CONNECT_DEADLINE` `[R3-6-wrong]`.

### LISTEN sequencing `[R3-8]`

`LISTEN ON/OFF` force-disconnects an active link if received mid-connection.
The LISTEN setter is sent only when the ARQ link is confirmed down (at open,
or strictly after `DISCONNECTED`); post-exchange re-arm is sequenced after
teardown. Ordering test against the mock server.

Command order at open (HF/SAT): `MYCALL` → wait readiness → `PUBLIC ON` →
[CWID] → `SessionType` → `COMPRESSION TEXT` → `RETRIES 10` → [BW] → [LISTEN
ON]. Dial (HF/SAT): `SessionType` → `CONNECT`. FM as above.

**Recorded boundary (not a deferral):** WLE's `-T`/`-L` MYCALL suffixes
implement radio-only / post-office **gateway sub-mode addressing** — a
different feature from P2P, out of scope. Recorded so the boundary is
explicit.

### Section 7.5 — interop matrix (replaces the single "off-wire" claim) `[R2/R1-interop]`

The peer store itself never appears on any wire, but several changes are
wire- or operator-visible. Per-surface:

| Surface | Wire/operator-visible change | WLE-compat direction |
|---|---|---|
| VARA HF/SAT TNC stream | +SessionType, +COMPRESSION (valid vocab), +RETRIES, +PUBLIC; fixes P2P retry cycle (4.6 s vs 4.0 s RMS) | Increases conformance; host-local to Tuxlink's own modem |
| VARA FM TNC stream | No session-type/compression/retries (matches WLE FM); +via digi support | Matches WLE FM exactly |
| ARDOP | No change (no session-type wire concept) | Neutral |
| Packet | +P2P intent plumbing (off-wire classification); via/relay already on wire | Neutral off-wire; on-wire unchanged |
| Telnet P2P login | Unchanged | Neutral |
| ConReq / AX.25 addressing | Dials use the channel's exact SSID'd callsign; base anchor is aggregation-only, nothing wire-facing derives from it | Correct |
| Inbound answer recording | New (off-wire); no wire effect | Neutral |

Two non-wire caveats, neither created by the divergence: cross-mode
aggregation is an inference (mitigated because every channel row keeps its own
callsign/freq/last-seen/via evidence, and the identity model allows split);
and Winlink P2P requires mutual intent (a WLE far end answers only from an open
P2P session window — operational fact in the §8 runbook).

## Section 8 — two-rig bench verification (operator-executed, RADIO-1)

The home lab is a complete P2P bench: no CMS, no WDT dependency. Rig plan:
Tuxlink (R2, VARA instance 1) ↔ WLE under WINE (R2, VARA instance 2), the
existing `~/.wine-wle` install as the interop far end.

**Step 0 — bench topology (must precede any traffic)** `[R3-10]`: both VARA
instances default to cmd 8300 / data 8301 on the same host and would collide.
Assign an explicit port map (instance 1 = 8300/8301, instance 2 = 8310/8311,
socat tap on a distinct port), a per-instance audio device, and confirm which
rig pairs with which VARA instance — the FT-710 crashes the operator's VARA;
**G90 is the validated VARA pairing** (memory `rig-test-path`). The pre-#1064
`/tmp/corrected_dials.py` scripts are retired (the app now expects CENTER
frequencies).

1. **Wire tap:** socat hex tap on the Tuxlink↔VARA cmd socket to capture
   `P2P SESSION`, `REGISTERED` (and whether it re-arrives on TCP reconnect),
   `COMPRESSION`, `RETRIES` (accepted, or `WRONG`?), `PUBLIC` on the wire (also
   serves the tuxlink-i3dg9 evidence gap).
2. **Outgoing (HF):** Tuxlink dials WLE peer (P2P intent), B2F both directions,
   clean disconnect. Verify the peer record materializes: origin `outgoing`,
   correct channel (SSID, center freq). WLE far end must have its **Vara P2P
   Session** window open (mutual-intent requirement).
3. **Incoming (HF):** Tuxlink armed listen (P2P auto-arm); WLE dials Tuxlink.
   Verify accept path, answer-role B2F, and the peer record with origin
   `incoming` — the live test of the inbound-tracking gap. Confirm `PUBLIC ON`
   did not block acceptance.
4. **SSID variant:** repeat (2) with an SSID'd MYCALL (e.g. `-7`); confirm
   ConReq addressing, the CONNECTED-echo base-match fix (gbb05), and B2F carry
   the SSID.
5. **FM leg (if an FM-capable pairing is available):** confirm the FM command
   set (no SessionType/COMPRESSION/RETRIES) and `WIDE`/`NARROW` CONNECTED
   parse; else record FM as bench-deferred with the HF leg proving the shared
   record path.
6. Every dial is operator-initiated with clear-channel check; consent = the
   Connect click per RADIO-1.

## Integration wire-walk matrix (ADR 0018 — must land together) `[R4-10][R1-C10]`

The feature is a stub unless every link in the chain lands together. Any row
whose UI/action is not fully wired is **hidden**, not shipped disabled:

| # | Piece | Feeds |
|---|---|---|
| 1 | `peers/store.rs` (PeersFile, dedup, caps, quarantine, keyring re-key) | 2,3,7 |
| 2 | `record_peer_observation` + 8 record sites + packet P2P intent plumbing | 1 |
| 3 | peer read command (Tauri) + distinct peer aggregation | 5 |
| 4 | `find_peers` (curated DTO) + `curate_peer` + egress-arm gate | agent |
| 5 | finder type filter (Gateway/Peer) + peer rows | 6 |
| 6 | map peer shape (circle) wired to peers.json, escaped render | — |
| 7 | agent `telnet_p2p_connect(peer_id, endpoint_id)` + egress denylist | agent |
| 8 | peers settings section (endpoints, provenance promotion, merge/split, keyring, contact link) | 1 |
| 9 | **VARA protocol core + engine-aware RF egress** — §7 command/parser work AND removing the MCP `VaraHf` hard-pin (`mcp_ports.rs:1269-1301`) so an agent VARA-FM peer action dispatches on the channel's engine, not always HF `[R5-1]` | 2,4,7 |
| 10 | **favorites schema/DTO/command** — `Favorite` gains `peer_id` (`src/favorites/types.ts`); peer-recorder is authoritative for recents, with ONE explicit bridge from peer observations to the favorites attempt log to avoid double-counting the existing frontend `favorite_record_attempt` (`connectDispatch.ts:48-57`) `[R5-7]` | 1,5 |

"Hide unimplemented rows" is a **mechanism, not a promise** `[R5-8]`: each
matrix row exposes a backend capability bit; the finder/map/settings query the
bit and a peer row/action/symbol is absent (not disabled or stubbed) unless
its capability is present, with UI tests proving absence. The wire-walk gate
traces flows verbatim at done-time; the greenfield flows are captured at build
start.

## Cross-store consistency `[R4-9][R1-C13]`

- `contact_id` is a **one-way** link. Contact edits never rewrite peer
  identity/grid. On contact delete there is no peer-side hook, so the peer
  resolves `contact_id` lazily and treats a miss as unlinked; a
  `grid.source: Contact` value whose contact is gone is cleared (not shown as
  authoritative — §1 "grid never guessed").
- A starred peer channel writes a `stations.json` Favorite; it carries the
  `peer_id` back-link so a peer rename/delete does not silently orphan the
  star `[R4-12]`.
- `find_peers` never exposes contact notes/names (§4 curate_peer).

## Testing (TDD, no on-air)

- **VARA wire rendering, engine-split:** HF/SAT vs FM command-set tests
  (FM sends no SessionType/COMPRESSION/RETRIES/PUBLIC/BW); `SessionType`,
  `Compression` (new vocab), `RETRIES` exact-string; open/dial command-order
  against a scripted mock VARA server, including the readiness gate (latched
  per-open, arrival release, `T_max` fail-open, `T_min` settle) and the
  `WRONG`-to-setter non-fatal path.
- **CONNECTED parse shapes** `[R3-7]`: HF numeric BW, FM `WIDE`/`NARROW`, FM
  via-digi; peer observation stores the FM bandwidth enum and the via path.
- **REGISTERED parse** `[R3-2]`: bare `REGISTERED`, `REGISTERED <call>`, and
  disambiguation from `LINK REGISTERED`.
- **SSID pipeline** `[R3-3-echo][R3-9]`: catalog SSID'd channel → finder row →
  dial target → `CONNECT` wire string carries `CALL-N` at every hop; CONNECTED
  echo base-match; MYCALL/CONNECT grammar validation (reject 8-char base,
  `-16`).
- **LISTEN sequencing** `[R3-8]`: setter only when link down; post-exchange
  re-arm after DISCONNECTED.
- **Peer store:** upsert/dedup (canonical_base anchor, per-transport channel
  key incl. via/bandwidth, endpoint key + monotonic provenance), both-direction
  recording from all 8 sites (answer-role via the mock listener path),
  rejected-inbound records nothing, wedged/aborted records a fail, atomic-write
  round-trip, `#[serde(other)]` forward-compat + corrupt-file quarantine,
  auto-cap LRU + inbound rate-limit quarantine, keyring re-key migration +
  cascade-delete.
- **Identity** `[R4-6][R4-7]`: canonical_base normalization (portable `/P`,
  tactical, `-T`/`-R`); club-call collapse-unless-split; auto-create never
  re-merges a split record.
- **Trust boundary:** `curate_peer` drops note/free-text/contact-names,
  validates callsign, clamps grid, redacts endpoints unless Operator+armed;
  `find_peers` gated behind egress-arm; agent `telnet_p2p_connect` refuses
  `ObservedIncoming` and raw host, egress denylist rejects loopback/RFC1918/
  metadata; provenance monotonic (Operator never downgraded); keyring never
  serialized nor auto-sent to ObservedIncoming; hostile-callsign render test
  (DOM-XSS).
- **Finder/map:** peer aggregation renders gridless/telnet peers untiered in
  the rail; type filter; peer rows (origin badges, endpoint provenance badge);
  map divIcon shape classes for both maps (pin via `L.divIcon` +
  eventHandlers per the react-leaflet false-green pitfall).

## Scope (ADR 0018 — built whole)

"P2P as a working, discoverable, verifiable mode": peer store (identity model,
per-transport dedup, caps/quarantine, keyring re-key) + auto-tracking at 8
sites incl. packet-P2P intent plumbing + trust boundary (curate_peer, egress
denylist, provenance monotonicity, agent-tool signature) + finder type filter
+ peer aggregation + map symbology + engine-split VARA protocol completeness +
bench runbook. Nothing in that list is sliced or deferred. The recorded
boundaries — WLE `-T`/`-L` sub-mode addressing, the tuxlink-hmoz8 CMS channels
API ingest, and FM-digi wire dialing if the FM leg proves out of reach on the
bench — are different features or explicitly-recorded limits, not slices of
this one. Coordination: tuxlink-sg5zw.2's telnet_p2p agent-tool rebuild
consumes the peer store built here (bd dep edge via sg5zw.8; sg5zw.8's build
lands with this design).
