# Telnet Post Office & Network Post Office — design spec (v2, post-adrev)

> **Status:** design v2 — incorporates the 5-round cross-provider adversarial review (Codex + 4 Claude lenses,
> 2026-06-08) and two operator coordination decisions. Pending a convergence review → `writing-plans`.
> **Date:** 2026-06-08 · **Agent:** sequoia-pika-maple · **bd:** `tuxlink-6c9y` (depends on `tuxlink-bsiy`).
> **Grounding:** [`2026-06-08-telnet-post-office-grounding.md`](2026-06-08-telnet-post-office-grounding.md).
> **Mock:** [`mockups/2026-06-08-post-office-mocks.html`](mockups/2026-06-08-post-office-mocks.html).
> **Adversarial transcripts:** `dev/adversarial/2026-06-08-post-office-design-codex.md` (gitignored) + the Claude
> 4-lens synthesis; dispositions folded in below (§11).

## 1. Goal & coordination

Wire the two Post Office session types — **Telnet RMS Post Office** and **Network Post Office** — onto the
Telnet/IP path, replacing their `soon` stubs. Deliver WLE-equivalent capability while diverging from WLE's
compose-time routing model in favor of a connection-determined, select-at-send-time model. Ship an
operating-mode documentation page. Both modes are pure TCP/IP; neither keys a transmitter; the feature is
entirely outside RADIO-1.

### 1.1 The three session types at a glance

These two modes join the existing Winlink (CMS) mode as three **distinct** session types — separate panes,
logins, and backends — differing on two axes: *how the client connects* and *what the relay does with the
mail*. Network Post Office sits between the other two.

| Session type | How it connects | What happens to the mail | When an operator reaches for it |
|---|---|---|---|
| **Winlink (CMS)** | direct to the CMS (internet / RF gateway) | enters the global Winlink system | the CMS is directly reachable |
| **Network Post Office** | to an RMS Relay on a LAN / AREDN mesh (full callsign, `MESH`/`C`) | normal Winlink routing — the relay forwards onward, and can deliver to local mesh recipients | a relay is reachable but the CMS is not |
| **Telnet RMS Post Office** | to an RMS Relay, typically local (`CALL-L` login, `L`) | held in the relay's local pool for local pickup — never forwarded globally | local delivery that stays off the global system |

Telnet RMS Post Office is the only mode whose mail is *local-only*. Network Post Office carries *normal* mail —
distinct from CMS on the **transport** axis, not the routing axis.

### 1.2 Relationship to in-flight work (`kld3`, `u5hl`, `bsiy`) — operator-decided 2026-06-08

- **`kld3` (MERGED, PR #322)** — the foundation this feature builds on: `SessionIntent { Cms, RadioOnly,
  PostOffice, Mesh, P2p }`, the `RoutingFlag` enum + `routing_flag()` mapping, and `winlink/relay_banner.rs`
  (a parser for the relay's real banner state). 6c9y consumes all three.
- **`u5hl` (IN_PROGRESS) — 6c9y SUPERSEDES its compose-time model.** `u5hl` proposed tagging a per-message
  `MessageMeta.routing_flag` at compose time (the model this design rejects) but shipped only "Pattern B": a
  **safety gate** in `build_outbound_proposals` that returns `MessageRejected` for any non-`Cms` intent
  (so Post Office/Mesh outbound currently sends nothing). **Operator decision:** 6c9y's connection-determined
  send-time selection is the replacement leakage guard for Post Office/Mesh. The gate is **narrowed** — removed
  for `PostOffice`/`Mesh`, **retained for `P2p`/`RadioOnly`** (which 6c9y does not address). `u5hl`'s deferred
  compose-time tagging (Pattern A) is unnecessary for these modes; `u5hl`'s residual concern is P2P/RadioOnly
  leakage, to be re-scoped by the operator.
- **`bsiy` (IN_PROGRESS) — 6c9y DEPENDS on it for inbound selection.** **Operator decision:** full inbound
  message selection ships in 6c9y v1. To get it, Post Office connect is built on the **native telnet-exchange
  path** (the decide-closure seam `bsiy` installs), *not* the `telnet_p2p_connect` template — so the operator
  is prompted to select inbound messages, exactly as for CMS. A `bd` dependency edge (`6c9y` → `bsiy`) records
  that 6c9y's inbound-selection slice reuses `bsiy`'s mechanism; sequence that slice after `bsiy`'s seam lands.

## 2. Scope

**In scope:** Telnet RMS Post Office (local `L` pool, `-L` login) and Network Post Office (`MESH`/`C`, full
callsign) over Telnet/IP; manual `host:port` (+ Network PO favorites); **send-time Outbox selection** and
**inbound message selection** (via `bsiy`) for both; inbound routing-source display; the operating-mode docs.

**Out of scope:** AREDN auto-discovery (§7.2, Babel-obsolescence — documented); Packet/VARA/ARDOP variants
(the `post-office` row's `packet` protocol stays `soon`); hosting/being a relay (client-of-relay only);
compose-time routing pools (deliberately not built — §3); RF/RADIO-1.

## 3. Routing model — headline divergence from WLE

**Protocol fact (verified, decompiled WLE).** An RMS Relay decides whether a session is *local* (`L`) or
*global* (`C`) from the **login** alone: a Telnet RMS Post Office session logs in as `CALL-L`; that `-L` suffix
is the entire routing discriminator (`TelnetSession.cs:2011-2013`). The bytes transmitted over B2F are built by
`B2AssembleMessage()` (`Message.cs:269-303`), whose fixed header set **never includes a routing header** — the
`X-RMS-Routing` line at `Message.cs:1404-1411` belongs to `EncodeHeader()`, which builds WLE's *local `.mime`
storage/display* copy, and is never placed on the wire (the send path reads `bytCompressed`,
`B2Protocol.cs:564-581`; `.Mime` is never referenced there). So WLE's compose-time `C`/`R`/`L` flag is **purely
client-side bookkeeping** — it does nothing on the wire.

**tuxlink divergence.** tuxlink does not implement compose-time routing pools. Routing is determined by the
connection; the operator selects which Outbox messages to send in that session. Composing a message carries no
routing decision (the compose "Send as" control stays a disabled stub). This supersedes `u5hl`'s Pattern A
(§1.2).

**Rationale (operator, 2026-06-08).** Compose-time pools do not serve a real-world deployment to a degree that
justifies the near-certain operator footgun and added complexity; the WLE failure mode (mail silently stranded
in the Outbox) is real and worsened by tuxlink's status-bar quick-Connect, which reuses the last modem.
Eliminating the compose-time flag eliminates the failure mode.

### 3.1 Wire compatibility with WLE clients on a shared relay (corrected)

tuxlink is **fully interoperable** with WLE clients on the same RMS Relay, and the interop surface is just the
**login** plus the **B2F exchange** — both of which tuxlink already produces. **There is no routing header to
emit:** the prior "emit `X-RMS-Routing: L` at send time" requirement is **struck** (it rested on mis-reading
`EncodeHeader` as the wire format; emitting it would make tuxlink *less* like WLE).

The correct contract is **field-compatibility, not byte-identity** — which the existing CMS path already
proves: tuxlink's `Message::to_bytes()` sorts headers alphabetically, omits WLE's `Type: Private` line, and
canonicalizes keys (`Mid`, `Mbo`), and uses lowercase `wl2k` vs WLE's `WL2K`, yet interoperates with the real
CMS, because Winlink parsers match header prefixes order-independently and the `FC EM` proposal carries its own
length fields. The whole spec is reworded from "byte-identical" to "field-compatible / semantically
equivalent." The inbound "Post Office" display chip (§4.5) is derived from the **receiving session type**
(mirroring WLE's inbound stamping at `B2Protocol.cs:1146-1148`), not from any transmitted header.

## 4. UX

### 4.1 Compose — unchanged
No new control. The disabled "Send as" stub stays disabled. A composed message carries no routing attribute.

### 4.2 Connections sidebar
The `post-office` and `network-po` rows flip from `soon` to live for the `telnet` protocol. Labels remain
**"Post Office"** / **"Network Post Office"** (operator-confirmed).

### 4.3 Telnet RMS Post Office pane
- A banner stating the session exchanges *local* mail (held at the relay, not global). Where the relay sends a
  real banner, surface its parsed `B2RelayState` (`relay_banner.rs`, §5) rather than static text only.
- **Relay host** + **Port** fields (default `127.0.0.1` / `8772`), persisted in config.
- A read-only "Logs in as `CALL-L`" indicator (`-L` appended automatically after base-callsign extraction §5;
  no password field — the handshake password is the non-secret constant `CMSTelnet`).
- An **Outbox send-selection** checklist (To/Subject/size + checkbox), with **select-all / select-none** and
  large-Outbox handling (§4.6).
- An **inbound** prompt: when the relay proposes messages, the operator selects which to download (via the
  `bsiy` seam, §1.2).
- **Connect** is enabled even at **zero outbound selection** (receive-only is a primary use — pull local mail
  without sending). The action label reflects state ("Connect" at N=0; "Connect & send N" otherwise).
- A session log. On partial/failed connect, the checklist re-renders with sent items removed and unsent items
  still checked (selection survives; §4.7).

### 4.4 Network Post Office pane
Same as §4.3 except: login uses the **full callsign** (no `-L`); a **saved relay endpoints** (favorites) list
of `{callsign, label, host, port}` with **add / edit-in-place / remove**, a `host:port` uniqueness key, and no
credential field (§6). A note that AREDN auto-discovery is intentionally omitted.

### 4.5 Reading pane — routing source
Mail received from a Telnet RMS Post Office session is tagged "Post Office", derived from the **receiving
session type** and persisted at receive time as a **separate** marker (NOT overloading the existing
transport-provenance `routing` field, which is header-extracted and means "via CMS-SSL" etc.). Network PO and
CMS mail is unmarked. See §5.7 for the receive-path plumbing this requires.

### 4.6 Empty selection & large Outbox
N=0 → connect for receive-only. Provide select-all/select-none. For large Outboxes, the checklist paginates or
virtualizes (no 200-row wall).

### 4.7 Partial / failed connect
Surface per-message outcome (sent / not-attempted / failed). Selection survives a partial failure: sent items
drop out, unsent stay checked, retry is one click. The Outbox→Sent move is server-confirmed and idempotent
(MID-keyed), so partial failure never loses data.

## 5. Backend design

**Plan-time re-grounding required.** The exact touch-point line numbers below are approximate and partly
**stale against the current tree** (the adrev flagged this); the planner MUST re-derive them against
`origin/main` before writing tasks. Known corrections from the review:
- There are **three** outbound-build loops, not two: `build_outbound_proposals` (the helper), an AX.25/packet
  inline mirror, and a CMS native-telnet inline mirror that does **not** call the helper. Decide explicitly
  whether the inline mirrors migrate to the helper or are documented tech-debt.
- `radioPanelVisibility.ts` already handles `cms | p2p | radio-only` and **falls through to `cms`** for
  `post-office`/`network-po` — so flipping `built:true` **without** extending the intent map would render the
  **CMS pane** (wrong login, wrong TLS-8773 port). The frontend change spans ~4 files (the `radio/types.ts`
  intent union, the exhaustive `panelTitle` switch, the `computePanelMode` map, and the `AppShell` dispatch).
  Decide the modeling: a new `kind:'post-office'` vs a new intent on `kind:'telnet'`.

Design-level touch-points:

1. **Session-type registry** (`sessionTypes.ts`): set `built:true` for `post-office.telnet` + `network-po.telnet`.
2. **Connection intent routing**: add `post-office` / `network-po` intents + dispatch (the ~4-file change above).
3. **New pane component** (templated on `TelnetRadioPanel`): parameterized by mode (`local`/`network`) — host:port
   vs favorites, the send-selection checklist, the inbound-selection prompt, the correct login indicator.
4. **Connect on the native telnet-exchange path** (NOT the `telnet_p2p_connect` template — §1.2 bsiy decision):
   a command that connects plain-TCP to `host:port`, sends the login (extracted-base-callsign `+ "-L"` for
   `local`, full callsign for `network`) then the `CMSTelnet` constant, runs the existing B2F exchange against
   target call `WL2K` (match WLE's casing), and routes inbound through the `bsiy` decide-seam. It transmits the
   same routing-header-free B2F payload WLE does (§3.1). It passes the hardcoded `CMSTelnet` constant and
   **never invokes the keyring**.
5. **Outbound proposal selection** — **narrow the safety gate** (§1.2): `build_outbound_proposals` no longer
   rejects `PostOffice`/`Mesh`; instead it proposes only the operator-selected MIDs (selection is the leakage
   guard). The `Mesh` intent selects the **`C`/normal** pool, **not** `None` (do not treat `Mesh` like `P2p`).
   Selection is **advisory**: intersected with the live Outbox at connect time on `meta.id`; a
   selected-but-vanished or unreadable MID is **skipped, not fatal**. Keep the gate for `P2p`/`RadioOnly`.
   **Multi-batch:** WLE sends in batches (≤5 proposals + ~10 KB compressed per turn) across multiple turns;
   tuxlink currently sends one turn then clears the remainder. 6c9y must either send all selected messages in
   successive turns or cap selection to the per-turn limit and say so in the UI — "Connect & send N" must be
   truthful. (Plan decides; multi-batch is the WLE-faithful choice.)
6. **Base-callsign extraction** (`GetBaseCallsign`-equivalent, `Globals.cs:3136-3154`): uppercase; tactical
   passthrough; else split on `.` take `[0]`, then split on `-` take `[0]`; append `-L` (local only).
   **No >6-char rejection** — that check is Pactor-TNC-only (`PactorWL2KSession.cs:2259`) and importing it into
   the telnet path would be a tuxlink-added safeguard. Network PO uses the full callsign unchanged. Unit-test
   the vector table (`n7cpz-10 → N7CPZ-L`, `N7CPZ.P → N7CPZ-L`, …).
7. **Inbound routing-source plumbing** (§4.5): thread the session intent into message filing; persist a
   session-derived routing marker (separate field/header) at receive time; expose it through `message_read`;
   update the `MessageView` test that currently asserts routing is NOT rendered. Add Rust receive-side coverage
   (today the inbound path is untested).
8. **Network PO favorites persistence**: a `Vec<RelayFavorite>` (`{callsign, label, host, port}`) on `Config`
   with `#[serde(default)]` (or a sidecar JSON mirroring the telnet allowlist), respecting `deny_unknown_fields`
   + `schema_version`; atomic read-modify-write; `host:port` dedup key.
9. **`relay_banner.rs` wiring**: consume the parser `kld3` built (surface the live `B2RelayState`) rather than
   leaving it dead and using only static banner copy.

The B2F wire exchange is otherwise unchanged.

## 6. Data model
- **No new outbound message field** (routing is not a message attribute).
- **Send selection** is a transient set of MIDs passed to the connect command, intersected with the live Outbox
  at connect time; never persisted on a message.
- **Inbound routing marker**: persisted at receive time, derived from the receiving session (separate from the
  transport-provenance `routing` field).
- **Network PO favorites**: `{callsign, label, host, port}`, `#[serde(default)]`, `host:port` uniqueness.

## 7. Divergences from WLE (all documented)
1. **No compose-time routing flag / pools** — connection-determined + send-time selection; supersedes `u5hl`
   Pattern A. (Headline.)
2. **No AREDN auto-discovery** — WLE's `sysinfo.json` discovery rides OLSR, which AREDN is removing for Babel
   (release 4.26.1.0 drops OLSR; third-party OLSR scrapers already break on babel-only nodes, e.g. `mesh-info`
   #140). Manual `host:port` is the durable approach; documented, not silent.
3. **Keyring, never plaintext, for any relay credential** — the handshake has no secret (`CMSTelnet` constant
   in code); WLE stores favorites in plaintext `.dat`, tuxlink does not.
4. **No misleading "Only post office messages will be sent" banner** (it misrepresents the `MESH`/`C` path).
5. **No tuxlink-added safeguards** — no airtime caps, no extra modals; mirror WLE's *suppression* of the
   RMS-Relay warning modal for these sessions. (A test asserts no consent modal fires — pure TCP, outside
   RADIO-1.)

## 8. Documentation deliverable
A new operating-mode page: what an RMS Relay "post office" is; the three modes (§1.1) and the local-vs-global
distinction; tuxlink's connection-determined + send-time-selection routing model and the explicit divergence
from WLE's compose-time pools (with the footgun rationale); why AREDN auto-discovery is omitted (OLSR→Babel);
when to use each mode. Authored accurately against decompiled behavior (this explanation is not otherwise
available). Targets smoke-walk item 8.

## 9. Testing strategy
Two-tier, no RF, no real relay required for the gate:
- **Tier A — protocol-shape (CI gate):** a local TCP fixture emulating the RMS Relay login asserts: the login
  is `<extracted-base>-L` (local) or the full callsign (network); the `CMSTelnet` constant follows; the
  transmitted B2F payload carries **no routing header** (matching WLE — §3.1); send-selection proposes only the
  selected MIDs; `Mesh` selects `C`/normal mail; the keyring is never touched. **Negative paths:** login
  rejected after password, relay closes mid-handshake, malformed banner, relay requiring auth beyond
  `CMSTelnet`, dial-to-refused-port. **Selection edges:** selected-but-vanished and selected-but-unreadable MID
  (skip-not-abort); N=0 receive-only; multi-batch (>5 selected actually sends all, or is capped truthfully).
- **Tier B — real-relay smoke (operator-run, optional but required pre-release evidence):** stand up an RMS
  Relay on `127.0.0.1:8772` and dial it.
- **Receive-side coverage** (new): inbound filing persists the session-derived routing marker; the inbound
  selection seam is exercised.
- **CI contract-test deltas** (must be enumerated by the plan, not just new tests): replace
  `safety_gate_fires_for_post_office_intent` / `_mesh_intent` with selection-scoped assertions (keep the
  `p2p`/`radio_only` gate tests); update `radioPanelVisibility.test`; confirm whether the new session types
  surface menu actions and pre-empt the `menuModel.test` `EXPECTED_IDS` break. Run `cargo clippy --all-targets
  -D warnings` + full vitest before pushing (per `feedback_scoped_vitest_misses_contract_tests`).
- **Frontend:** pane render, checklist + select-all/none state, connect wiring, partial-send re-render, the
  inbound routing chip, no-consent-modal assertion.

## 10. Plan-deferrable items (enumerate in `writing-plans`, not design-blocking)
The §5 touch-point re-derivation; the favorites `serde` migration + schema_version mechanics; multi-batch vs
cap implementation; large-Outbox rendering; partial-send per-MID legibility; selection-staleness intersection
in all three loops; CI contract-test deltas; post-send Outbox lifecycle (move on `FF`/`FQ`; per-relay
`MessageServer` binding is out of scope under a single-relay-per-deployment assumption — a §7-style documented
capability difference, not a silent drop); single-flight concurrency scope across the connect paths;
`relay_banner` wiring-or-disposal; negative-path + no-consent-modal tests.

## 11. Adversarial-review disposition (5 rounds: Codex + 4 Claude lenses)
- **P0 — "byte-identical via `X-RMS-Routing`":** struck; WLE transmits no wire routing header; reframed to
  field-compatible (§3.1). **Resolved (deletion).**
- **P0 — collision with `u5hl`:** operator decided 6c9y supersedes `u5hl` Pattern A; gate narrowed (§1.2, §5.5).
  **Resolved.**
- **P0 — safety gate sends 0 messages:** gate narrowed for `PostOffice`/`Mesh`; selection is the guard (§5.5).
  **Resolved.**
- **P0 — bsiy inbound bypass:** operator decided full inbound selection in v1 via the native path + `bsiy` seam
  (§1.2, §5.4). **Resolved.**
- **Coupled P1s** (`GetBaseCallsign` extraction, inbound receive-path plumbing, byte-identity reframe): folded
  into §5.6 / §5.7 / §3.1.
- **Remaining P1/P2s** (touch-point re-derivation, favorites migration, multi-batch, N=0, partial-send,
  selection-staleness, contract-test deltas, `MessageServer`/post-send lifecycle, single-flight,
  `relay_banner`, negative-path tests): captured in §4–§10 as design constraints or §10 plan items. The
  `MessageServer` endpoint-affinity finding is dispositioned as a documented capability difference under
  single-relay-per-deployment, not a P0 (tuxlink's explicit selection subsumes WLE's automatic filtering).
