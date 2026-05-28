# Session-type connection selector — design spec

> **Status:** design, pending operator review → `writing-plans`.
> **Date:** 2026-05-22 · **Agent:** redwood-cypress-spruce · **Brainstorm:** this session (visual companion).
> **Ground truth:** decompiled RMS Express (`dev/scratch/winlink-re/decompiled/rms-express/`, local-only) +
> [`ax25-packet-protocol-findings.md`](ax25-packet-protocol-findings.md).
> **Relates to:** `tuxlink-3o0` (PR #122 — user-switchable CMS host/transport) is *subsumed* by this design;
> its controls relocate from the SettingsPanel overlay into a connection pane (see §6).

## 1. Goal & context

Replace the would-be flat session selector (legacy RMS Express uses a single 26-row
"Open Session" dropdown — a flattened `protocol × role` cross-product, scan-heavy and
duplicative) with a **two-level accordion in the existing Connections sidebar**: the
operator expands a *session type* and picks a *protocol* beneath it; the existing rich
connection-management pane opens in the reading-pane slot for that selection.

This **evolves the current shell** (Mock B) — it does not introduce a new structure:
the sidebar already has a Connections section and the reading-pane already hosts an
inline `PacketConnectionPanel`. We restructure the Connections section into the
accordion and route its selections to the existing pane pattern.

## 2. Model (ground-truthed, operator-confirmed)

A Winlink connection is **(routing intent) × (transport)**. Routing intent — *not*
"connect vs listen", *not* auto-detected — is the primary axis. From the decompiled
client, intent is carried as a per-message **routing flag** + session class
([`B2Protocol.B2SessionType`](../../dev/scratch/winlink-re/decompiled/rms-express/RMS_Express/B2Protocol.cs), `B2CheckSendMessage` / inbound stamping):

| Session type | `B2SessionType` | Flag | Function |
|---|---|---|---|
| **Winlink (CMS)** | `CMS` / `Automatic` | `C` | Credentialed secure-login to the global Winlink system — via internet (Telnet→CMS) or RF (→an RMS gateway). Syncs the CMS-routed mailbox; needs internet at the far end. |
| **Radio-only** | `RadioOnly` | `R` | Separate message pool that rides the Winlink **Hybrid** network over RF only — never the internet. |
| **Post Office** | `PostOffice` | `L` | Separate "local" pool — store-and-forward at a **local RMS Relay** "post office". |
| **Peer-to-peer** | `P2P` | (none) | Different session class — direct to a named peer, **no creds**, no CMS. Connect *or* Listen. |
| **Network Post Office** | `MESH` | — | Telnet to a locally-run RMS Relay instance; MESH proper = Winlink over AREDN mesh. |

A message flagged `R` will not be sent in a `CMS` session and vice-versa — these are
genuinely different delivery semantics, which is why session type is the primary,
named choice. **All five intents are in scope** — "full-fat client" (operator,
2026-05-22). *Transports* phase in by build status (§5).

## 3. UI structure (operator-approved shape)

### 3.1 Connections sidebar = session-type accordion

The Connections section of [`FolderSidebar`](../../src/mailbox/FolderSidebar.tsx) becomes
an accordion. Each **session type** is an expandable row; expanding lists its
**protocols** beneath it. Selecting a protocol loads its management pane (§3.2). The
**current selection stays visible** in the sidebar at all times (this was the explicit
fix over an earlier "ephemeral settings window" shape — selection is persistent;
only the *detail pane* is per-selection).

```
Connections · session type
▾ Winlink (CMS)
    ● Telnet            ← selected
    ○ Packet (AX.25)
      VARA HF   soon
      VARA FM   soon
▸ Radio-only
▸ Post Office
▸ Peer-to-peer
▸ Network Post Office
```

- Expand/collapse per session type (chevron ▸/▾). Multiple may be expanded; one
  protocol is the active selection.
- Built protocols are live; unbuilt show a `soon` badge and are non-selectable
  (mirrors today's `v0.1` badge convention).
- A transport-state dot (green = listening/connected) rides the active protocol row,
  as the Packet entry does today.

### 3.2 The connection-management pane (reading-pane slot)

Selecting a protocol opens the **existing rich management pane** in the reading-pane
slot — *the placeholder in the mockup stands in for this*. It is the
`PacketConnectionPanel`-class content (Modem block, My-station/SSID, Status/Listen,
Connect), and for Winlink-CMS the host/transport/login controls from `tuxlink-3o0`
(§6). The pane is per-selection ("ephemeral" in that sense) — but the **selection
itself is persistent in the sidebar**, so the operator always knows the live state.

Connect / Listen are actions **inside** the pane. Listen appears only where it applies
(P2P answering); CMS is connect-only.

### 3.3 Session-type × protocol matrix (tuxlink)

Valid cells follow the legacy matrix, scoped to tuxlink's transports:

| | Telnet | Packet (AX.25) | VARA HF/FM | Pactor / ARDOP / RP / Iridium |
|---|:--:|:--:|:--:|:--:|
| **Winlink (CMS)** | ✅ built | ✅ built | planned | *open Q (§8)* |
| **Radio-only** | ✅* | ✅* | planned | *open Q* |
| **Post Office** | ✅* | ✅* | planned | *open Q* |
| **Peer-to-peer** | rare | ✅ built | planned | *open Q* |
| **Network PO** | ✅* (RMS Relay) | — | — | — |

✅ built = UI + backend exist today · ✅* = UI structure present, backend (R/L pools,
RMS Relay) lands later · planned = VARA modem roadmap.

## 4. Mapping to existing code

- [`FolderSidebar.tsx`](../../src/mailbox/FolderSidebar.tsx): the static `CONNECTION_ITEMS`
  + the single selectable Packet button become the accordion. `ConnectionKey` (today
  `'packet'`) generalizes to a `{ sessionType, protocol }` key.
- [`AppShell.tsx`](../../src/shell/AppShell.tsx): `selectedConnection` carries the new key;
  the reading-pane render branch (today `selectedConnection === 'packet' ? <PacketConnectionPanelContainer/> : <MessageView/>`)
  extends to dispatch the right pane per `{sessionType, protocol}`.
- [`PacketConnectionPanel.tsx`](../../src/packet/PacketConnectionPanel.tsx): reused as the
  Packet pane (under both CMS-gateway and P2P intents; intent parameterizes Connect
  vs Listen + secure-login).
- The `tuxlink-3o0` CMS host/transport/login controls become the **Winlink (CMS) →
  Telnet** pane (relocated from `SettingsPanel`).

## 5. v0.1 build scope vs. full structure

The **structure** (all five session types in the accordion) ships in v0.1, so the
client *reads* as full-fat. The **panes built now** are those with backend support:

- **Winlink (CMS) → Telnet** — the `tuxlink-3o0` controls (PR #122), relocated.
- **Winlink (CMS) → Packet** — gateway dial (existing packet B2F path).
- **Peer-to-peer → Packet** — existing `PacketConnectionPanel` (dial / Listen).

Session types whose backend isn't built (Radio-only `R` pool, Post Office / Network PO
RMS-Relay, VARA transports) appear in the accordion with their protocols badged `soon`
/ panes stubbed, until their backend lands. *(Proposed scope — confirm in §8.)*

## 6. Relationship to PR #122 (`tuxlink-3o0`)

PR #122 added a user-switchable CMS host + TLS·8773/Plaintext·8772 transport selector,
but placed it in the `SettingsPanel` overlay reachable only via *Tools → Settings →
GPS & Privacy…* (the "Connection" menu entry is disabled). This design **relocates**
those controls into the **Winlink (CMS) → Telnet** connection pane and removes the CMS
fieldset from `SettingsPanel`. PR #122's backend (`config_set_connect`, `resolve_cms_host`,
the connect-exercise test) is unchanged and reused. Decision needed: land #122 as-is
then refactor, or fold the relocation into #122 (§8).

## 7. Out of scope / deferred

- VARA HF/FM transports (modem roadmap); Pactor/ARDOP/Robust Packet/Iridium panes.
- The R-pool (Hybrid) and L-pool (Post Office) backend message-routing — UI structure
  only until the backend exists.
- Rig/frequency control (`tuxlink-5jb`).

## 8. Open questions for operator review

1. **Protocol scope** — beyond Telnet + Packet (built) and VARA (planned), do
   Pactor / ARDOP / Robust Packet / Iridium GO belong in the accordion at all for
   tuxlink, or are they permanently out?
2. **v0.1 pane build set (§5)** — agree to build Telnet-CMS, Packet-CMS-gateway,
   Packet-P2P now and stub the rest, or a different cut?
3. **PR #122** — land as-is then relocate in a follow-up, or fold the relocation in
   before merge?
4. **Labels** — "Winlink (CMS)" / "Radio-only" / "Post Office" / "Peer-to-peer" /
   "Network Post Office" — keep, or shorten (e.g. "Hybrid" for Radio-only)?
