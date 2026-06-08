# Telnet Post Office & Network Post Office — WLE-parity grounding

> **Status:** grounding research → operator brainstorm → `writing-plans`.
> **Date:** 2026-06-08 · **Agent:** sequoia-pika-maple · **bd:** `tuxlink-6c9y`.
> **Ground truth:** decompiled RMS Express (`library-of-hamexandria/winlink-re/decompiled/RMS Express/RMS_Express/`,
> local-only — never pushed) + the Winlink Programs Group corpus (`winlink-annex/corpus.jsonl`) +
> the Hamexandria YT vector DB. Precedence: decompiled-WLE primary source **>** repo-doc summaries **>**
> community/AI recollection. Where a community claim and the decompile disagree, the decompile wins.

## 1. Executive summary

"Telnet Post Office" and "Network Post Office" are **two distinct WLE session types**, not one feature.
Both exchange mail with an **RMS Relay** server (the store-and-forward "post office") over TCP/telnet
instead of the central CMS, and both share a server type, a default port (8772), and a non-secret login
password (`CMSTelnet`) — which is why the community routinely conflates them. They differ in implementing
class, B2 session type, and message-routing semantics:

| | CMS Telnet (built) | Telnet RMS Post Office | Network Post Office |
|---|---|---|---|
| C# class | `TelnetSession` | `TelnetSession` + `blnPostOfficeSession=true` | `TelnetMESHSession` |
| `B2SessionType` | `CMS` | `PostOffice` | `MESH` |
| Target | CMS (SSL 8773) | one configured RMS Relay (`127.0.0.1:8772`) | a favorite/discovered relay (any IP:8772) |
| Login callsign | full callsign | **base + `-L`** | full callsign |
| Password | `CMSTelnet` (+ MD5 secure-login on CMS) | `CMSTelnet` (no secret) | `CMSTelnet` (no secret) |
| Routing flag | `C` | **filters to / stamps `L`** | **normal `C`** (not filtered) |
| Discovery | none | none | AREDN `sysinfo.json` |

The load-bearing distinction: **Telnet RMS Post Office = local-only `L` pool against one relay; Network Post
Office = any-IP/AREDN-reachable relay(s) carrying normal CMS-class `C` mail.** The B2F wire protocol is
identical to a normal CMS Telnet session in both cases — the only deltas are the TCP endpoint, the login
suffix, and a routing-flag filter applied to the existing B2F exchange. Both are pure TCP/IP (internet, LAN,
or AREDN-over-IP); neither keys a transmitter, so the whole feature sits **outside RADIO-1**.

## 2. Telnet RMS Post Office (`B2SessionType.PostOffice`)

**Exact menu string:** `"Telnet RMS Post Office"` (`Main.cs:4177`, `Main.cs:10448`). Bare "Telnet Post
Office" is the *legacy* name (renamed upstream); the bd task title and the current stub label "Post Office"
both denote this current mode.

**Dispatch (`Main.cs:5884-5888`):** instantiates the **same `TelnetSession`** as plain "Telnet Winlink" and
sets the global `blnPostOfficeSession = true`. That one flag rewires four behaviors:

1. **TCP target → RMS Relay, not CMS.** With `blnPostOfficeSession` set, the CMS-path guard
   (`!blnUseRMSRelay & !blnPostOfficeSession & !blnRadioOnlySession`, `TelnetSession.cs:773`) is false, so the
   session skips `ConnectToCMS("", 8773, blnSSL:true)` and connects to `strRMSRelayHost:intRMSRelayPort`
   (`:835-846`). Defaults: host `127.0.0.1` (INI `Telnet/RMS Relay Host`, `:593`), port `8772` (INI `RMS Relay
   Port`, `:594`; constructor fallback `:447`). Contrast CMS Telnet → TLS **8773** (`:801`).
2. **Login callsign gets the `-L` suffix** (`TelnetSession.cs:2004-2018`): the post-office branch sends
   `GetBaseCallsign(strMyCallsign) + "-L"` (SSID stripped; base callsigns >6 chars are rejected because `-L`
   is appended — `PactorWL2KSession.cs:2259-2267`). Radio-only uses `-T`; CMS uses the full callsign.
3. **Password is the fixed literal `CMSTelnet`** (`TelnetSession.cs:2025`) — identical to the CMS plaintext
   login. The relay/post-office handshake carries **no per-user secret**; the `-L` suffix is the routing
   discriminator. tuxlink already hardcodes this same constant at `telnet.rs:56`.
4. **`B2SessionType.PostOffice` → filter to `L`-flagged mail only.** Enum at `B2Protocol.cs:17-25`
   (`CMS, RadioOnly, PostOffice, P2P, MESH, Automatic`).
   - **Outbound** (`B2CheckSendMessage`, `B2Protocol.cs:874-880`): a `PostOffice` session rejects any message
     whose routing flag `!= "L"` (`RadioOnly` rejects `!= "R"`; the generic `else` rejects anything `R`/`L`).
   - **Inbound** (`B2Protocol.cs:1142-1153`): a `PostOffice` session stamps received messages `L`
     (`RadioOnly`→`R`, `CMS`/`Automatic`→`C`).
   - **Display** (`Message.cs:1247-1254`): `L` → "RMS Routing: Post Office".

The B2F exchange is otherwise unchanged — `B2OnConnected("WL2K", …)` runs the same proposal/exchange
(`TelnetSession.cs:2027`). The post-office layer is a **queue-filter + endpoint-swap atop existing B2F**, not
a new wire protocol. The message server is tagged `"RMS-Relay:" + strRMSRelayHost` (`:842`). WLE's
post-connect RMS-Relay-warning modal (`ConfirmConnection`) is **suppressed** for PostOffice sessions
(`B2Protocol.cs:1904` gate requires `enmB2SessionType != PostOffice`).

## 3. Network Post Office (`B2SessionType.MESH`)

**Exact menu string:** `"Network Post Office"` (`Main.cs:4179`, `Main.cs:10448`).

**Dispatch — a DIFFERENT class** (`Main.cs:6020-6024`): `new TelnetMESHSession()` with
`strCurrentSession = "Network Post Office (RMS Relay)"`. B2 session type is `MESH`
(`TelnetMESHSession.cs:811`).

**The critical routing distinction from Telnet Post Office:** `MESH` is **not** handled in
`B2CheckSendMessage` — only `RadioOnly` and `PostOffice` get routing-flag filters; `MESH` falls into the
generic `else` (`B2Protocol.cs:881`) that excludes `R`/`L` mail, and inbound `MESH` has no stamping branch
(`:1142-1153`), so received mail defaults to `C`. **A Network Post Office session sends/receives ordinary
CMS-routed (`C`) mail** — it behaves like a telnet-to-CMS session that traverses a mesh/LAN, not a local-only
`L` pool. (Corroborated by the corpus: developer W4PHS and AL0R both state any IP transport works;
`corpus.jsonl:129,166`.)

**TCP target — favorite station, not a single localhost** (`TelnetMESHSession.cs:582,883-913`): connects to
the selected favorite `objCurrentStation`'s IP/port (default 8772). Favorites persist to
`"Telnet PostOffice Favorites.dat"`.

**Login — full callsign + hardcoded `CMSTelnet`** (`TelnetMESHSession.cs:1616`). Because the password is
hardcoded, the Add-Station dialog **disables the password field**
(`DialogAddTelnetStation.cs:470-471`, in the `Add Post Office Server` branch constructing
`TelnetStationType.PostOffice, B2SessionType.MESH`).

**AREDN mesh discovery (the actual "Network" integration):** `DialogUpdateMeshNodes` fetches
`http://localnode.local.mesh:8080/cgi-bin/sysinfo.json?services=1` (`:382,540`), parses
`mesh_gateway`/`mesh_supernode`/`NodeDetails`, filters advertised services by name (default
`"WINLINK;POST OFFICE"`), and caches to `MeshNodes.txt`. Window title `"Telnet Session to Network Post Office
Server"` (`:750`); toolbar `"LAN and MESH stations:"` (`:713`).

**Latent upstream bug (verified):** the `Mesh Master Node` INI setting is read into `txtMasterNode.Text`
(`:362`), written back (`:519`), and *shown* in error dialogs (`:389,393`) — but the actual fetch URL is
**hardcoded** to `localnode.local.mesh` at `:382` and `:540` and never substitutes the operator's value. A
non-default master node is persisted and displayed but never queried. This is a WLE bug, not parity to mirror.

**AREDN auto-discovery is parity with a *deprecated* upstream mechanism (scope decision: OUT, 2026-06-08).**
WLE's `sysinfo.json?services=1` service list was historically populated by AREDN's **OLSR** nameservice/txtinfo
plugin. AREDN has replaced OLSR with the **Babel** routing protocol: production release 4.26.1.0 is the first
to omit OLSR entirely, and the cutover is network-wide (measured "in years" but underway). Babel reimplemented
service advertisement via its own service-discovery daemon, so discovery *exists* under Babel — but third-party
tools that scrape the OLSR-era data path **break on babel-only nodes**: the `mesh-info` mapper fails with
`Failed to connect to OLSR daemon on localnode.local.mesh` (github.com/smsearcy/mesh-info#140), a direct
structural analog to WLE's discovery. WLE is frozen and does not track AREDN's changes. **The core Network
Post Office function — dial an RMS Relay by IP — is routing-protocol-agnostic** (works over Babel-AREDN,
OLSR-AREDN, LAN, or internet); only the auto-discovery convenience is coupled to the protocol AREDN is
deleting. tuxlink therefore omits AREDN auto-discovery deliberately: manual `host:port` entry is the durable
approach and is *more* robust across the OLSR→Babel transition than WLE's discovery. Sources: AREDN docs
(`docs.arednmesh.org/.../babel.html`), AREDN release 4.26.1.0 / 3.25.5.0 announcements, `mesh-info` issue #140.

## 4. Current tuxlink state & exact parity gap

**Nothing is wired — both modes are pure UI stubs with zero backend** (`src/connections/sessionTypes.ts`):

- `post-office` (`:48-56`): `built:false`, protocols `[telnet, packet]` both `built:false`,
  blurb "Local RMS Relay store-and-forward (pool L)."
- `network-po` (`:74-82`): `built:false`, protocols `[telnet]` only, blurb "Local RMS Relay network."
- `isBuilt()` returns false whenever the parent intent is unbuilt, so the rows render `disabled` with a `soon`
  badge — the operator cannot click them.

No RMS Relay / store-and-forward / pool-`L` / pool-`R` / mesh / discovery code exists anywhere. Two naming
collisions to keep straight: the `CMSTelnet`/`wl2k` constants in `telnet.rs:56,60` and the "post office" prose
at `telnet.rs:53,205` describe the **CMS login handshake**, not the post-office *mode*; and the
"WLE-as-Post-Office model" comment at `ui_commands.rs:2528` attaches to the **P2P** dial path, also unrelated.

**The working reference flow these modes branch from:** `{cms, telnet}` → `radioPanelVisibility.ts` maps to
`{kind:'telnet', intent:'cms'}` → `telnet::connect_and_exchange` (TLS 8773 → `telnet_login` sends `mycall\r`
then `CMSTelnet\r` → B2F to `wl2k`). The `telnet_p2p_connect` command is the structural template for
"offer the Outbox to a relay" semantics.

**What "wire these modes" must build:**

1. Flip `built:true` in `sessionTypes.ts` for the intent + the `telnet` protocol.
2. Add a **third connection intent** to panel routing. `radioPanelVisibility.ts` currently collapses
   `sessionType` to `cms|p2p` only; `AppShell.tsx:723-761` branches only on `cms`/`p2p`.
3. An **RMS-Relay TCP target + login**: operator-configured `host:port` (default `127.0.0.1:8772`),
   callsign-with-`-L`-suffix login (Post Office) or full callsign (Network PO), fixed `CMSTelnet` password.
   Confirm the relay uses `WL2K`/`wl2k` as the B2F target call (WLE passes `"WL2K"` for both — parity-safe).
4. A **routing-flag model on messages** (`C`/`R`/`L`) — the one genuinely new primitive tuxlink lacks today —
   with the **PostOffice→filter-to-`L`, MESH→normal-`C`** distinction.
5. A new Tauri command analogous to `telnet_p2p_connect`.
6. For Network PO only: AREDN `sysinfo.json` discovery + favorites (and, if shipped, the master-node bug-fix).

## 5. Design decision space (for the operator brainstorm)

Each carries a recommended default; **these are proposals, not decisions**.

- **Q1 — Scope: one mode or both, staged or single plan?** The two are separable; Telnet RMS Post Office is
  far smaller (endpoint-swap + `-L` + `L`-filter on the existing telnet path). *Default:* ship Telnet RMS Post
  Office first as the load-bearing mode; Network Post Office as a second phase. Whatever ships must be complete
  (alpha = vettedness), not a partial slice.
- **Q2 — Dial-a-hub vs be-a-hub?** *Default:* client-of-relay only; hosting an RMS Relay is a separate, much
  larger feature with no bd issue and is out of scope.
- **Q3 — Is AREDN mesh discovery in scope?** The `sysinfo.json` fetch + service-filter + favorites cache is the
  bulk of Network PO's complexity. *Default:* make discovery optional — allow a manual `host:port` entry; ship
  AREDN auto-discovery only if the operator confirms a mesh audience (also sidesteps the master-node bug for v1).
- **Q4 — Credentials.** The post-office handshake has **no per-user secret** (fixed `CMSTelnet` constant).
  *Default:* hardcode `CMSTelnet` as a protocol constant in code; route any operator-supplied relay auth through
  the OS keyring, never plaintext `.dat`/INI as WLE does.
- **Q5 — RF vs non-RF?** Both modes are TCP/IP. *Default:* scope to the Telnet/IP path only — the feature is
  entirely outside RADIO-1 (no transmit, no on-air consent gate).
- **Q6 — Dev-smoke without a relay or RF hardware.** *Default:* two-tier. (a) CI/protocol-shape smoke against a
  **local TCP fixture** that emulates the RMS Relay login (expect `<base>-L` / `CMSTelnet`) plus a canned B2F
  exchange — proves the suffix, password, `L`-filter, and endpoint-swap. (b) Optional operator-run real RMS
  Relay smoke (`127.0.0.1:8772`). No RF hardware for either.

## 6. Divergences tuxlink should deliberately make

1. **Keyring, never plaintext.** WLE stores favorite/relay/AREDN passwords as plaintext in
   `Telnet PostOffice Favorites.dat` and INI. The handshake itself has no secret (`CMSTelnet` is a constant →
   code), but any operator-supplied auth goes through the OS keyring.
2. **Fix the `Mesh Master Node` bug** if AREDN discovery ships — honor the configured node in the discovery URL
   rather than hardcoding `localnode.local.mesh`.
3. **One unambiguous label + semantic per session type.** The community's central pain is WLE's overloaded
   "Telnet / Network / RMS Post Office" naming and stale Help text. Pick clear, non-overloaded labels and make
   the routing-flag requirement automatic (avoid the WLE regression where messages silently stuck in the Outbox
   unless manually typed "Post Office Message").
4. **Do not mirror the misleading "Only post office messages will be sent" banner for Network PO** — it
   misrepresents `MESH`'s normal-`C` routing (W4PHS agreed upstream to remove it).
5. **No tuxlink-added safeguards.** No airtime caps, no extra modals; mirror WLE's *suppression* of the
   RMS-Relay warning modal for these sessions rather than adding a confirmation.

## 7. Open / out-of-scope facts (server-side, not in the client source)

- Whether a real RMS Relay or mesh post-office server requires auth beyond `CMSTelnet` is **not determinable
  from the decompiled client** (the client sends only callsign + `CMSTelnet`). Flag for the real-relay smoke.
- Server-side `L`-pool routing mechanics (how RMS Relay holds/forwards `L` mail, retention windows, hybrid-
  network sync) are server-side and out of scope; tuxlink is a client that sends and receives `L` mail.
- The `post-office` stub lists a `packet` protocol while `network-po` lists only `telnet`. This asymmetry is
  **correct parity**: WLE has a `"Packet RMS Post Office"` (`Main.cs:5998`) but Network Post Office is
  telnet-only (`TelnetMESHSession`, no packet variant). `tuxlink-6c9y` scopes to the **telnet** cells.
- Port hygiene: MESH/post-office dial default = **8772**; the tuxlink P2P-telnet *listener* default is **8774**
  (P2P dial is 8772). Do not cross-wire these.
