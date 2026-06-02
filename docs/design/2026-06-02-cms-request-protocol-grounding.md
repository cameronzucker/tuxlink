# CMS-Request protocol grounding (tuxlink-v8ee, sprint tuxlink-2u4n)

**Date:** 2026-06-02
**Author:** bluff-birch-cove session
**Status:** Research → operator decision required before sprint code starts
**bd:** tuxlink-v8ee (closes this doc) · sprint tuxlink-2u4n

## Why this doc exists

The sprint as originally framed (`tuxlink-2u4n`) assumed WLE's "Request" features
(catalog, station list, bulletins, GRIB) all used a uniform CMS-side mechanism
that could be added on top of our shipped outgoing-message rails (`tuxlink-l55l`).
That assumption is incorrect. Each request type has a different backend:

- **GRIB files** are NOT a Winlink-CMS feature — they're served by a third-party
  SMTP service (Saildocs). Winlink is just the mail transport.
- **Catalog inquiries** ARE in-band Winlink messages, sent with `Type: Inquiry`
  to a specific service address per the B2F spec.
- **RMS station list** has TWO valid mechanisms: (a) HTTPS pull from
  `api.winlink.org/gateway/status.json` (Pat's approach, online-only) or (b) an
  in-band catalog inquiry (legacy WLE-RF approach).
- **Bulletins** use `Type: Inquiry` to subscribe / pull specific bulletin IDs.

This doc captures the protocol-grounding research done against open-source
references + Winlink published docs, identifies what's confirmed vs. unknown,
and surfaces three operator decisions needed before any code lands.

## Reference sources consulted

| Source | URL | Use |
|---|---|---|
| Winlink B2F spec | https://winlink.org/B2F | Type taxonomy, header fields, addressing rules |
| la5nta/wl2k-go (depth-1 clone) | https://github.com/la5nta/wl2k-go | `catalog/position_report.go` — only concrete in-band implementation in OSS |
| la5nta/pat (depth-1 clone) | https://github.com/la5nta/pat | `internal/cmsapi/` — HTTPS REST against `api.winlink.org`; does NOT implement in-band requests beyond position |
| Saildocs GRIB docs | https://saildocs.com/gribinfo | GRIB body syntax (canonical for the Saildocs service) |
| Tuxlink WLE inventory doc | `docs/design/2026-05-29-winlink-express-feature-inventory.md` | Prior agent's mapping of WLE menus → tuxlink targets; already calls out `query@winlink.org` as the catalog address |
| Winlink Catalog Requests Drill (GAARES) | https://groups.io/g/gaares/attachment/356/0/CatalogRequestExerciseCopyPaste.pdf | Paywalled — couldn't fetch |

Open-source coverage of the in-band protocol is **thin**: only position reports
are implemented end-to-end in `wl2k-go/catalog/`. Pat sidesteps the issue by
using HTTPS REST. The literal catalog inquiry body strings WLE pre-populates
are not in any greppable source I found.

## What's confirmed

### Message-type taxonomy (B2F spec)

The `Type:` header on a Winlink message can be one of:

| Type | Purpose |
|---|---|
| `Bulletin` | Sent to CMS for the WL2K catalog system |
| `Private` | End-user to end-user (the default for outgoing mail) |
| `Service` | To a station operator (sysop) |
| `Inquiry` | **Requests downloading of specific bulletins / catalog items** — THIS is the in-band request mechanism |
| `Position Report` | User reports their location |
| `Position Request` | User requests another user's location |
| `Option` | Modify station parameters in the Winlink system |
| `System` | RMS↔client internal |

Confirmed by both the B2F spec page and `wl2k-go/fbb/message.go` (`MsgType`
const block).

### Addressing rules (B2F spec)

For in-band service messages:
- `To: SYSOP` or `To: Service` → sysop of the station first receiving the message.
- `To: SYSOP.` (with trailing dot) → cross-station sysop addressing.
- `To: CMBO` → Central Message Board Operator.
- `To: QTH` → wl2k-go's `PositionReport` uses this as the destination (see
  `catalog/position_report.go`).
- `To: query@winlink.org` → per the existing inventory doc, this is the catalog
  request address. **Not verified end-to-end against a live CMS in this
  research, but cited by the prior agent.**

### Position Report (the one concrete OSS reference)

`wl2k-go/catalog/position_report.go` — full implementation. The pattern is:

```
Type:    Position Report (as MsgType)
Subject: POSITION REPORT
To:      QTH
Body (key:value lines):
    DATE: <yyyy/mm/dd hh:mm UTC>
    LATITUDE: <ddd-mm.mmN/S>
    LONGITUDE: <ddd-mm.mmE/W>
    SPEED: <decimal, units unspecified per source comment>
    COURSE: <NNN[T|M]>     // T = true, M = magnetic
    COMMENT: <up to 80 chars>
```

This is the **canonical pattern for in-band requests**: a typed message with
specific Subject + To + key:value body. Catalog, bulletins, and Inquiry
requests follow the same shape.

### GRIB request via Saildocs

This is NOT a Winlink-CMS feature. WLE's "GRIB file request" menu item composes
a regular outgoing message addressed to a third-party SMTP service.

- **Recipient:** `query@saildocs.com`
- **Subject:** anything (Saildocs ignores it; convention is a brief label)
- **Body — single line, no leading whitespace:**
  ```
  send gfs:LAT0,LAT1,LON0,LON1|dlat,dlon|VTs|Params
  ```
- **Defaults** (when fields omitted): `2,2` grid spacing · `24,48,72` forecast
  hours · `PRESS,WIND` parameters.
- **Region format:** whole degrees + N/S/E/W (`40N,60N,140W,120W`).
- **Forecast hours:** comma-separated or range (`6,12..96` = 6, 12, 18, …, 96).
- **Params:** any subset of `PRMSL`, `WIND`, `HGT`, `SEATMP`, `AIRTMP`, `WAVES`.
- **Sub/send:** `send` = one-shot; `sub` = recurring schedule
  (`sub ... days=30 time=18:00`).
- **Response:** GRIB-1 binary as a MIME-attached file; renderable in OpenCPN,
  zyGrib, Expedition, etc.

Source: https://saildocs.com/gribinfo — the canonical service-side spec.

### Saildocs supports more than GRIB

Same syntax over `query@saildocs.com` reaches: weather faxes, surface
analyses, NWS text bulletins, etc. The `send <category>:<args>` grammar is
uniform. List of categories at https://saildocs.com/gribmodels (not fetched
in this research round).

### Station list — TWO mechanisms

**A. Pat-style (HTTPS REST):**
- Endpoint: `https://api.winlink.org/gateway/status.json`
- Access key: `1880278F11684B358F36845615BD039A` (issued by WDT to Pat in 2017
  per the comment in `pat/internal/cmsapi/api.go`).
- Returns JSON list of every RMS gateway with mode/band/coords/status.
- Pat caches this in `gateway_status.json.gz` (vendored fallback).
- **Online-only.** Useless to an RF-only operator.

**B. WLE legacy (in-band catalog inquiry):**
- Send an Inquiry message requesting the station list as a "catalog item."
- Response comes back as a regular message containing the gateway list as
  body text or attachment.
- **Works over RF** (it's just a Winlink message round-trip).
- Body syntax: presumed similar to GRIB/Saildocs (`send <category>` keyword)
  but the literal category name is not in any OSS source I found.

The two mechanisms are not mutually exclusive — WLE actually offers both. The
right tuxlink answer is probably: do (A) when online (fast), fall back to (B)
when not. But that's a future-iteration concern.

## What's NOT confirmed

The following are NOT in any open-source reference I could find. They likely
exist in WLE's binary, in the paywalled training-drill PDF, and in
operator muscle memory:

1. **Literal catalog inquiry body strings.** WLE has a "Winlink Catalog
   Requests…" menu that shows a TREE of named inquiries (per the existing
   inventory doc). Each leaf is a (recipient, body-line) pair. Examples I
   *suspect* but cannot cite:
   - `WL2K_HELP` — request the help bulletin
   - `WL2K_USERS` — request the active-users list
   - `WX_FORECAST_AREA_<X>` — request a NWS forecast for region X
   - Various NOAA / NWS / propagation IDs
   The tree structure + actual category names: **need operator input or a
   live WLE session to enumerate.**

2. **Catalog request destination.** Inventory doc cites
   `query@winlink.org` but says "templated forms"; I have not verified
   end-to-end against a live CMS (would require a test connect that
   doesn't pollute the operator's real catalog state).

3. **Response shape per inquiry type.** Some catalog items return body
   text; some return attachments. Mapping is per-inquiry and not
   documented anywhere I can fetch.

## Implementation approach (proposed)

Given the gaps, I propose the sprint ships as a **data-driven catalog
framework** rather than hard-coded inquiry knowledge:

### 1. CatalogEntry data model (Rust + TS)

```ts
interface CatalogEntry {
  id: string;            // stable id, e.g. "wl2k_help"
  category: string;      // tree grouping, e.g. "WL2K System"
  label: string;         // operator-facing name
  description: string;   // hover/help text
  recipient: string;     // "query@winlink.org" or "query@saildocs.com" etc.
  bodyTemplate: string;  // "send wl2k_help" or "send gfs:{{region}}|{{grid}}|..."
  responseHint: string;  // "text body" or "GRIB attachment" or "multiple messages"
  parameters?: ParameterSpec[];  // for GRIB-style parameterized requests
}
```

Stored as JSON in `src-tauri/resources/catalog/` (or similar), bundled with
the app. **Operator-editable** — they can add inquiries we don't ship with.

### 2. Sub-issue breakdown (revised)

Instead of one issue per "WLE Request menu item," I propose:

- **A. CatalogEntry framework + Catalog Requests UI (tree picker + send)** —
  the data model, the UI for browsing + sending, the "open in compose"
  flow. Ships with a starter catalog of inquiries we're confident about
  (likely just `WL2K_HELP` until operator vets more).
- **B. GRIB file request (Saildocs parameter form)** — independent of A
  because GRIB has a real parameter UI (region picker, time, params).
  Routes through compose with the templated body. Ships as a dedicated form.
- **C. RMS station list (HTTPS — Pat-style)** — separate because it's a
  different mechanism (HTTPS REST + JSON parse + table renderer). No mail
  round-trip.
- **D. RMS station list (in-band catalog inquiry — RF fallback)** — uses
  the framework from A, with a renderer for the response. Defer until A
  + C are stable.
- **E. Bulletins (subscribe/fetch via Inquiry messages)** — uses the
  framework from A; deferred until catalog protocol is verified end-to-end
  against a live CMS.

### 3. Sprint sequencing (revised)

1. **A** (framework + Catalog Requests UI) — ships first because everything
   else either uses it or stands alone (C).
2. **C** (RMS list via HTTPS) — independently shippable; biggest immediate
   user value (browse local gateways for connect dialog); doesn't depend
   on A.
3. **B** (GRIB via Saildocs) — parameterized form, depends on A's compose
   flow but uses Saildocs not Winlink CMS.
4. **D + E** — defer until A + C + B are operator-smoked and the
   in-band protocol is verified.

## Three operator decisions needed

These block the rest of the sprint. Surfacing for direction:

### Decision 1: Catalog inquiry list — your input or empirical?

Two paths:

- **(a) Operator-provided starter list.** You paste or describe the actual
  inquiries you've used in WLE (literal `send <category>` strings + the
  recipient). We ship those bundled.
- **(b) Empirical discovery.** First catalog framework iteration includes
  a special "list available inquiries" request that we send to
  `query@winlink.org` (likely something like `send catalog` or `send help` —
  TBD). The response IS the catalog. We then build the bundled list from
  what comes back.
- **(c) Ship the framework empty, you populate via the UI as you go.**

(a) is fastest if you have the knowledge; (b) is self-documenting but
requires one round-trip against the live CMS; (c) is the
"infrastructure-first" play that defers the content question.

### Decision 2: RMS station list — HTTPS or in-band?

- **(a) HTTPS only** (Pat's approach). Fast, modern, but online-only.
  Defeats the RF-only EmComm scenario.
- **(b) In-band only** (legacy WLE-RF approach). Works without internet,
  but slow + requires the catalog framework first.
- **(c) Both, with auto-fallback.** Default to HTTPS when online; degrade
  to in-band when not. Most operator-friendly, most code.

(c) is the "right" answer; (a) is the fastest ship; (b) is the EmComm
purist answer.

### Decision 3: GRIB scope

- **(a) Saildocs only.** WLE's actual implementation. Ships a parameter
  form + send + "save attachment, open externally."
- **(b) Saildocs + Winlink-CMS GRIB.** Some CMS instances may serve GRIB
  via the catalog system (would surface as a `WX_GRIB_*` entry under A).
  Adds complexity; unclear value over Saildocs.
- **(c) Saildocs + an in-app GRIB viewer.** Renderer in the reading
  pane. Significant additional surface (GRIB-1 binary parsing, weather
  visualization). Probably a future sprint.

(a) is the minimum-viable ship; (b) is opportunistic; (c) is a separate
feature.

## What this iteration accomplished

- Cloned wl2k-go + Pat (depth-1, to `/tmp/{pat,wl2k-go}-reference`,
  throwaway, not vendored per `project_pat_complete_strip_directive`).
- Verified Winlink B2F Type taxonomy against canonical source.
- Identified the GRIB / Saildocs decoupling — major scope simplification.
- Identified the RMS-list dual-mechanism — major scope decision.
- Identified the catalog-inquiry literal-content gap — operator-input
  decision.
- Surfaced three blocking decisions BEFORE writing any protocol code, per
  the `feedback_ai_amateur_radio_reliability` discipline.

**No Rust/TS code shipped this iteration**, per the v8ee scope.

## Next step

Loop pauses here. Operator answers the three decisions; next session's
agent files the revised sub-issues (A/B/C/D/E above with operator's
choices baked in) and starts implementation. The sprint tracker
(`tuxlink-2u4n`) and this doc stay in place across sessions.

---

# Update 2026-06-02 (afternoon) — empirical findings from WLE reference install (tuxlink-tkdc)

Operator pointed at a real RMS Express install at
`dev/scratch/winlink-re/install/RMS Express/RMS Express/N7CPZ/` with the
operator's actual callsign data + sent message archive. This resolves
Q1 and Q2 fully and narrows Q3.

## Source 1: `N7CPZ/Data/Winlink Queries.txt` — the catalog database itself

WLE ships (and auto-updates) a flat catalog file with **1477 entries
across 127 categories**, pipe-delimited:

```
CATEGORY|FILENAME|DESCRIPTION|SIZE
```

Sample lines:

```
WX_BUOY|NDBC44009|Station 44009 Buoy Report 3427'35" N 7441'31"|15696
WL2K_HELP|INQUIRIES|Description of the Winlink inquiry system - how to use|1886
WL2K_RMS|PUB_PACKET|Packet Public Gateways Frequency List|219867
WL2K_RMS|PUB_VARA|VARA Public Gateways Frequency List|75234
WL2K_RMS|PUB_ARDOP|ARDOP Public Gateways Frequency List|26706
WL2K_RMS|PUB_PACTOR|Pactor Public Gateways Frequency List|32933
WL2K_RMS|PUB_ROBUST|Robust Packet Public Gateways Frequency List|2831
WL2K_USERS|CMS_STATUS|Real time Operational Status of Winlink CMS's|2018
WL2K_USERS|CMS_TRAFFIC|Winlink Message Traffic History|2106
WL2K_HELP|UPDA_CAT_WE|How to update the Winlink Express catalog list.|3649
WL2K_HELP|CUSTOM.GRIB|How to request and use Custom GRIB files from SailDocs|4943
```

Categories observed include: `ARCTIC_ICE`, `ARES_RACES`, `AURORA`,
`HF_NETS`, `HONDURAS`, `INDIAN_OCEAN`, `METAR`, `METAREA_I..XVI`,
`NEWS`, `NICARAGUA`, `PROPAGATION`, `SAT_KEPS`, `SAT_PIX`,
`S/PACIFIC_WX`, `UK_CADET`, `WL2K_HELP`, `WL2K_RMS`, `WL2K_TERMS`,
`WL2K_USERS`, `WX_*` (many), and more.

**Action:** bundle this file as-is with the app (`src-tauri/resources/
catalog/winlink-queries.txt`), parse on load, render as a category-tree
picker. The catalog updates itself via the `WL2K_HELP/UPDA_CAT_WE`
help doc + an "update catalog" UI affordance that fetches a fresh
catalog file (the update process itself is one of the things the help
doc describes — TBD next iteration).

## Source 2: `N7CPZ/Messages/*.mime` — literal wire format

Multiple inquiry messages in N7CPZ's outbox. All identical structure:

```
Date: <RFC 2822>
From: <CALLSIGN>@winlink.org
Reply-To: <CALLSIGN>@winlink.org
Subject: REQUEST
To: INQUIRY@winlink.org
Message-ID: <generated MID>
X-Cancel: <yyyy/mm/dd hh:mm>
X-Source: <CALLSIGN>
MIME-Version: 1.0

multipart/mixed; boundary="<random>"
  text/plain; charset="iso-8859-1"; Content-Transfer-Encoding: quoted-printable
    <FILENAME>
    [<FILENAME>]
    [<FILENAME>]
    ...
```

**Confirmed examples from N7CPZ's outbox:**

| Sent | Body |
|---|---|
| `3TK09WKG9QBC.mime` | `AZ_ZON_NOFLA` + `CMS_TRAFFIC` |
| `0NXS7HZNEKA7.mime` | `WCVS.JPG` |
| `3LL2MO1TF24M.mime` | `PROP_SGAS_27` |
| `347UN33R5VOJ.mime` | `CMS_STATUS` + `PROP_WWV` |
| `5YTNBV3JOZA8.mime` | `PUB_PACKET` + `PUB_VARA` ← **literal RMS list request** |

Confirms:
- Constant `To:` = `INQUIRY@winlink.org`, `Subject:` = `REQUEST`
- Body is one filename per line — the FILENAME column from the catalog
  database, **without** category prefix
- Multiple inquiries per message supported (just add more lines)
- Each inquiry triggers a separate reply message from the CMS

## Q1 RESOLVED: Catalog inquiry list source

**Answer: Operator-provided starter (option a), via the WLE catalog file.**

Bundle `winlink-queries.txt` (1477 entries) with the app. Render as a
tree picker grouped by `CATEGORY`. Composition: build a message to
`INQUIRY@winlink.org` with `Subject: REQUEST` and the selected
filenames in the body, one per line. Send through the existing
outgoing rails (`tuxlink-l55l`). Replies arrive in inbox as regular
Private messages.

The empirical-discovery path (option b) is no longer needed — we have
the literal catalog. The ship-empty path (option c) is unnecessary too.

## Q2 RESOLVED: RMS station list

**Answer: WLE legacy IS the catalog mechanism (in-band via
`INQUIRY@winlink.org`).** The `WL2K_RMS` category has filenames
`PUB_PACKET`, `PUB_VARA`, `PUB_ARDOP`, `PUB_PACTOR`, `PUB_ROBUST`. A
"Request RMS List" UI action composes a message with the chosen mode's
filename in the body. The response is a text body listing all
gateways for that mode.

The HTTPS `api.winlink.org/gateway/status.json` Pat uses is a
**separate, modern, online-only path** — it is NOT what WLE does.
Operator's choice on whether to offer HTTPS as a fast-path:

| Option | Behavior | Effort | Trade |
|---|---|---|---|
| **A1. Catalog only** (WLE parity) | Works over RF; matches legacy WLE muscle memory; requires a CMS round-trip | Folds entirely into Q1's catalog framework | Slow first load (mail round-trip); always works |
| **A2. HTTPS only** | Pat-style; fast; JSON parse + table render | New code (HTTPS fetch + JSON DTO) | Useless on RF-only deploys |
| **A3. Both with auto-fallback** | HTTPS when online, in-band when not | A1 + A2 + a routing decision | Best operator experience; most code |

My recommendation: **A1 ships with Q1 catalog as a single feature**
(zero-extra-cost; uses the same framework). A2/A3 are a follow-up
issue if you want HTTPS later — defer.

## Q3 NARROWED: GRIB scope

WLE's catalog database contains **zero** GRIB filenames — only
`WL2K_HELP/CUSTOM.GRIB` and `WL2K_HELP/MAXSAEA_GRIB` which are HELP
DOCS about Saildocs, not actual GRIB inquiry items. So:

**Option (b) is ruled out** — there is no Winlink-CMS GRIB to add;
WLE itself doesn't request GRIB from the CMS. Saildocs is the only
path.

Real choices:

| Option | Behavior | Effort |
|---|---|---|
| **B1. Saildocs only — WLE parity** | Parameter form (region picker + grid spacing + valid times + params) → compose to `query@saildocs.com` with `send gfs:...` body → response arrives as a Private message with GRIB-1 attachment → "save attachment, open externally" (zyGrib / OpenCPN / Expedition) | One PR — UI form + composer; reuses existing outgoing rails |
| **B2. B1 + in-app GRIB viewer** | Above + GRIB-1 binary parser + map renderer with wind barbs / isobars / wave heights | Significant additional surface; probably a separate v0.x+1 sprint |

My recommendation: **B1 only for v0.x parity**. WLE itself punts
visualization to external viewers; matching that is honest scope.
B2 belongs in a future sprint.

## Revised sprint architecture

The original A/B/C/D/E breakdown collapses to **just two issues**:

1. **CATALOG framework + RMS in-band fold-in** (Q1 + Q2 A1):
   - Bundle `winlink-queries.txt`
   - Parser + in-memory `CatalogEntry[]` model
   - Tree picker UI under a new "Message → Catalog Request" menu (matches WLE)
   - "Send request" composes `To: INQUIRY@winlink.org`, `Subject: REQUEST`, body = newline-joined filenames
   - Responses land in inbox as regular Private messages (no new render path needed for v0.x — the existing reader pane handles text)
   - Includes RMS list, bulletins, station status, propagation, ICAO METAR, etc. — all 1477 entries
2. **GRIB request via Saildocs** (Q3 B1):
   - Parameter form (region + grid + times + params)
   - Composer to `query@saildocs.com`
   - GRIB-1 attachment is saved-as / open-externally on receipt

**Bulletins fold into issue 1** (they're catalog entries in
`ARES_RACES`, `NEWS`, etc.). **Station list folds into issue 1** for the
in-band path; A2/A3 HTTPS RMS list is a follow-up issue if wanted.

## Sub-issues to file (next iteration)

- **tuxlink-XXX (P2)**: Implement issue 1 above — Catalog Request framework
  + bundled `winlink-queries.txt` + tree picker UI + composer + UTM
  for the inquiry response routing.
- **tuxlink-XXX (P2)**: Implement issue 2 above — GRIB via Saildocs
  parameter form + composer.
- **tuxlink-XXX (P3, follow-up)**: HTTPS RMS list fast-path (Pat-style
  `api.winlink.org/gateway/status.json`) — only if operator decides
  online-fast-path is worth the additional code.

## Operator decision still needed

Just one: **B1 vs B2 for GRIB**. (My recommendation: B1; matches WLE.)
Q1 and Q2 are fully answered by the empirical evidence — no operator
input needed; defaults are obvious.

Once B1/B2 is picked, the sprint can resume with a much smaller scope
than originally framed.
