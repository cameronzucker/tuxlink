# Radio-mode right-panel UX design

> **Status:** Authoritative for the redesign of every radio-mode connection UI in tuxlink — Telnet, AX.25 Packet, ARDOP HF, and the eventual VARA HF / VARA FM. Supersedes the "v0.5+ modem-console placement" deferral in [`docs/design/v0.0.1-ux-mockups.md`](../../design/v0.0.1-ux-mockups.md) §3.5 by pulling the canonical 3-state modem console forward to v0.0.1 scope with simplifications. Resolves the operator-flagged "the entire ARDOP UI is horribly malformed" diagnosis from the 2026-05-31 brainstorm.
>
> **Lineage:** [`tuxlink-74mx`](https://github.com/cameronzucker/tuxlink) (this spec). Brainstorm conducted 2026-05-31 by agent `crag-hemlock-kestrel` using the `superpowers:brainstorming` visual-companion flow; intermediate screens persisted at `.superpowers/brainstorm/610438-1780202736/content/` (gitignored). Reference materials consulted: decompiled RMS Express (action vocabulary, session-type list), `docs/design/v0.0.1-ux-principles.md`, `docs/design/v0.0.1-ux-mockups.md`, `docs/design/mockups/images/modem-{compact,full}.png`, operator's `ardop-ux-options.png` (May 29).

## 1. Scope

Covers the user-facing UX of every connection mode that opens a radio (or radio-like) session: Telnet CMS, AX.25 Packet (CMS-gateway and P2P intents), ARDOP HF (CMS), VARA HF / VARA FM (design-only here; backends not yet built). Does NOT cover backend protocol implementation, the wizard flow, the mailbox / reading UX, the compose window, or settings dialogs except where they intersect this redesign (the Settings dialog continues to own per-modem device config — audio devices, PTT, cmd port, etc. — per operator's Option-B preference and existing `tuxlink-j0ij` ARQBW work).

The brainstorm decision was that the **same UX paradigm must serve all radio modes**, not just resolve the ARDOP-specific symptoms — so this spec is mode-agnostic where possible and per-mode where each mode's data demands it.

Per the [documentation propagation contract](../../../CLAUDE.md#documentation-propagation-contract), this doc is the canonical source for the decisions it captures. CLAUDE.md, AGENTS.md, the implementation-plan files, and bd-issue descriptions are pointers, not parallel statements.

## 2. Problem statement

The current state at the time of writing (2026-05-31, origin/main at v0.7.0):

| Mode | Current pattern | Status |
|---|---|---|
| Telnet CMS | Inline reading-pane panel (`TelnetCmsPanel`) | Works |
| AX.25 Packet | Inline reading-pane panel (`PacketConnectionPanel`, 571 LOC) | Works structurally; sprawled — buttons stretched, ~600 px of negative space per field on a 1fr column |
| ARDOP HF | Right-side dock (`ArdopDock`, ~290 px) + reading-pane stub (`ArdopHfStub`) | Operator-flagged broken: stub points at an empty dock on cold start (until [PR #166](https://github.com/cameronzucker/tuxlink/pull/166) wired it); dock is a sparse half-modem-console; duplicate Connect buttons (ribbon CMS Connect + dock ARDOP Connect, same word, different effects); no inline bandwidth/audio/PTT surface |
| VARA HF / VARA FM | Not built; sidebar entries are `built: false` placeholders | Will inherit whatever ARDOP normalizes |

Three problem axes:

1. **Pattern divergence.** Telnet and Packet use the inline-panel pattern; ARDOP uses a fragmented two-slot (stub + dock) pattern. Operators see this as "wildly divergent."

2. **Stretched negative space.** AX.25 packet's reading-pane panel (~700 px on a 1280 wide window) holds ~300 px of actual form content. Buttons stretch to fill column width; inputs (a 2-char SSID dropdown, a 6-char callsign) render at ~600 px of column width. This was the operator's specific framing: *"Most of the space in the AX.25 current window is wasted as-built. We largely stretch the buttons and fields to fill large amounts of negative space."*

3. **Duplicate Connect class.** The dashboard ribbon's `cms_connect` button (legacy CMS quick-connect) and ARDOP HF's per-dock Connect button both render at the same time when the operator is on the ARDOP HF view, with identical labels and different effects. This is the most-visible symptom of the broken paradigm.

The canonical design in `v0.0.1-ux-mockups.md` §3.5 acknowledged that **software-mediated modems** (ARDOP, VARA) would eventually need a different surface than the inline-panel pattern that suits hardware-TNC modes (AX.25 Packet, Telnet CMS), and locked a 3-state (Off / Compact / Full) togglable Radio Dock as the design — but deferred the implementation to v0.5+ with a placeholder block in v0.0.1. ARDOP shipped without the canonical surface, taking a shortcut and cramming half a modem console into the dock-slot that was reserved for both the v0.0.1 session-timer/outbox/last-sessions content and the v0.5+ modem console. The half-cocked result is what this spec corrects.

## 3. Locked decisions

The brainstorm output. These are not options anymore.

### 3.1 Reading pane is for messages only

The reading-pane slot houses `MessageView` (when a message is selected) and nothing else after this redesign. No connection-config panels, no stub messages pointing at other surfaces, no protocol panels. `ArdopHfStub` is removed. `TelnetCmsPanel` is migrated to the right panel (see §5.1). `PacketConnectionPanel` is migrated to the right panel (see §5.2).

Rationale: the reading pane is the mail-client reading surface — its actual job in the layout. Connection forms don't have 1fr-worth of content; they have ~300 px of content stretched to fill 800 px+. Moving connection forms out resolves the stretched-button / negative-space problem at its source.

### 3.2 Compact right-hand radio panel at 360 px

A single right-hand column carries all radio UX: connection setup, modem-link config (for hardware-TNC modes), live state, action buttons. One panel per mode, mounted based on which connection-sidebar entry is selected or which modem is currently in a non-stopped state.

- **Width:** 360 px default. Validated against 320 / 360 / 400 at 1080p scale during the brainstorm; 360 gives gauges and labels room to breathe without compromising reading-pane readability (reading pane is ~980 px wide on 1920 — comfortably above Apple Mail's ~700 px and Gmail web's ~700-800 px).
- **Calibration:** VARA HF v4.1.2's standalone window is ~520-600 px wide and presents the same information classes (gauges, state lights, constellation, action row) in roughly the same footprint as 360 + the dock chrome.

### 3.3 Visibility rule

The panel mounts when ANY of:
- A connection entry is selected in the sidebar (Telnet / Packet / ARDOP HF / VARA HF / etc.)
- Any modem is in a non-stopped state (Connecting / Connected / etc.)
- View → Toggle Radio Panel is on (existing menu item, repurposed; existing `Ctrl+Shift+M` accelerator continues to bind)

When none of those apply, the panel is hidden and the reading pane gets the 360 px back. Layout reflows on mount/unmount. The repurposed View menu item — previously "Show Radio Dock" / "Toggle Radio Dock" — is renamed to **"Toggle Radio Panel"** so the menu label matches what it actually controls.

### 3.4 Full overlay for Mode-2 troubleshooting

The compact panel header includes a `⤢` button (next to `☓` close). Clicking expands the panel into the reading-pane area: the right panel widens out and overlays the reading pane. Operator's framing: *"shit's broke, needs attention can eat the reading pane since an operator is definitionally not doing both."*

- Triggered ON-DEMAND only — never auto-expands on conditions degraded
- `⤡` collapses back to compact panel
- Reading-pane state (selected message, scroll position) is preserved across expand/collapse
- Message list compresses (220 → 200 px, identity-only) while overlay is active
- Full overlay contains: throughput chart (60s history), embedded ardopcf WebGUI iframe (closes [`tuxlink-ed51`](https://github.com/cameronzucker/tuxlink)), full-size live gauges (S/N, VU, CPU, TX delay), full ARQ state lights with 10-cell ARDOP vocabulary, session log live tail, troubleshoot action row (Change band / Lower bandwidth / Try different gateway / Modem settings / Send-Receive / Disconnect)

### 3.5 Express vocabulary

Throughout the radio surface, adopt RMS Express's session vocabulary verbatim. Operators coming from Express get muscle-memory parity; tuxlink's invented terms ("Connect" on the ribbon, "Dock" everywhere) get replaced.

| tuxlink today | Express canonical | New in tuxlink |
|---|---|---|
| "Connect" (per-mode dock) | `mnuStart` "Start" | **Start** (per-panel primary action) |
| "Disconnect" (per-mode dock) | `mnuStop` "Stop" | **Stop** (graceful end) |
| (no equivalent) | `mnuAbort` "Abort" | **Abort** (emergency stop) |
| "Connect" (ribbon CMS button) | (no equivalent in Express main form) | **Removed** — ribbon becomes informational only |
| "Show / Toggle Radio Dock" | n/a | **Toggle Radio Panel** |
| `Session → Connect` menu | `mnuStart` in session window | `Session → Start` |
| `Session → Disconnect` menu | `mnuStop` in session window | `Session → Stop` |
| F5 / Ctrl+Shift+O (fire `cms_connect`) | (no equivalent) | **Start the currently-selected mode** (contextual; details in §6.3) |

Panel titles use Express's `"{Mode} {Intent}"` format from the decompiled main-form combobox:

- `Telnet Winlink`, `Telnet P2P`
- `Packet Winlink`, `Packet P2P`
- `Ardop Winlink`, `Ardop P2P` (P2P deferred per §7)
- `Vara HF Winlink`, `Vara HF P2P`, `Vara FM Winlink`, `Vara FM P2P`

Making the intent visible at the panel level addresses an emcomm operator's question "is this Start going to hit CMS or a peer?" without forcing them to read context — important for ARDOP and VARA where the same modem serves both intents.

### 3.6 Dashboard ribbon Connect button removed

The current ribbon `Connect` button (firing `cms_connect`) is removed. The ribbon becomes purely informational — callsign, grid, GPS, UTC + local time, current-session state. No action buttons.

Rationale: the button was tuxlink-invented as a one-click "check mail" shortcut, but it created the duplicate-Connect-button class the operator flagged. Express's main form has no equivalent because sessions are inherently mode-aware. The 2-click flow (sidebar → Start in panel) matches Express, eliminates the duplicate-button class definitively, and lets the ribbon be just informational.

The F5 / Ctrl+Shift+O accelerators continue to exist but rebind per §6.3.

## 4. Layout architecture

### 4.1 Column grid

```
┌──────────────────────────────────────────────────────────────────────┐
│  Title bar                                                           │  32 px
├──────────────────────────────────────────────────────────────────────┤
│  Menu bar (File / Message / Session / Mailbox / View / Tools / Help) │  44 px
├──────────────────────────────────────────────────────────────────────┤
│  Dashboard ribbon (callsign / grid / GPS / time / session state)     │  ~52 px
├──────────┬──────────────┬───────────────────────────┬────────────────┤
│          │              │                           │                │
│ Sidebar  │  Message     │   Reading pane            │  Radio panel   │  flex
│  220 px  │  list        │   1fr (flex)              │  360 px        │
│          │  360 px      │                           │  (when mounted)│
│          │              │                           │                │
├──────────┴──────────────┴───────────────────────────┴────────────────┤
│  Session log strip (human-shaped projection, raw-toggle)             │  ~80 px
├──────────────────────────────────────────────────────────────────────┤
│  Status bar                                                          │  ~24 px
└──────────────────────────────────────────────────────────────────────┘
```

When the radio panel is hidden: reading pane takes the 360 px back, becomes `1fr + 360 px`. When the radio panel is in Full overlay mode: see §4.2.

### 4.2 Full overlay reflow

```
┌──────────────────────────────────────────────────────────────────────┐
│  Title bar                                                           │
├──────────────────────────────────────────────────────────────────────┤
│  Menu bar                                                            │
├──────────────────────────────────────────────────────────────────────┤
│  Dashboard ribbon                                                    │
├──────────┬─────────┬─────────────────────────────────────────────────┤
│          │ Message │                                                 │
│ Sidebar  │ list    │   Full radio panel (overlay)                    │  flex
│  220 px  │ 200 px  │   = reading pane width + 360 px                 │
│          │ (comp-  │                                                 │
│          │  ressed)│                                                 │
├──────────┴─────────┴─────────────────────────────────────────────────┤
│  Session log strip                                                   │
├──────────────────────────────────────────────────────────────────────┤
│  Status bar                                                          │
└──────────────────────────────────────────────────────────────────────┘
```

The reading pane DOM is preserved (not unmounted) — the selected-message state, scroll position, and any inline-edit state survive `⤡` collapse. The overlay sits visually on top via CSS layering, eating the reading-pane width.

The message list compresses to 200 px to preserve identity context (who's calling, what folder) without dominating the screen. Operator can still click another message — clicking collapses the overlay (auto-collapse: see §8).

### 4.3 Panel chrome

```
┌────────────────────────────────────────────────────┐
│ ● MODEM · Ardop Winlink            ⤢ ☓             │  header
├────────────────────────────────────────────────────┤
│ ─── CONNECT ─────────────────────────────────────  │  collapsible section
│   Target          [ W7RMS-10 ____________ ]        │
│   ARQ bandwidth   [ 500 Hz ⌄ ]                     │
├────────────────────────────────────────────────────┤
│ ─── LIVE ────────────────────────────────  ●       │  collapsible section
│   S/N         +8.4 dB    ▮▮▮▮▮▯▯                  │
│   VU input    −18 dBFS   ▮▮▮▮▯▯▯                  │
│   Throughput  540 bps    ▮▮▮▮▮▮▯                  │
│   Mode 4FSK 500 · Width 500 Hz · Up 3m 42s         │
│   RX 4128 B  TX 982 B                              │
├────────────────────────────────────────────────────┤
│ ─── ARQ STATE ───────────────────────────────────  │
│   ▢ DISC  ▢ CON   ▢ IDLE                          │
│   ▢ ISS   ▰ IRS   ▢ BUSY                          │
│   ▢ RX    ▢ TX    ▰ DREQ                          │
├────────────────────────────────────────────────────┤
│ ─── ACTIONS ─────────────────────────────────────  │
│   [ Start ]  [ Send/Receive ]  [ Stop ]            │
│                                                    │
│   Audio devices, PTT, cmd port: Settings           │
└────────────────────────────────────────────────────┘
```

**Header:** state-dot (color = link state) + name (`MODEM · {Mode} {Intent}`) + sub (peer / uptime / bandwidth, brief) + `⤢` expand + `☓` close-and-disconnect. The dot has the live pulse animation when the modem is in a transmitting state.

**Sections:** every mode uses a subset of `Connect / Modem / Station / Listen / Live / ARQ State / Actions`. Each is collapsible (chevron toggle); collapsed state persists per mode. Stopped-state modes show fewer sections (no Live, no ARQ State) — the panel grows naturally as the modem progresses through its states.

**Actions row:** always the bottom block. Start is the primary; Stop/Abort are secondary; mode-specific buttons (Send/Receive, Open WebGUI, etc.) sit between.

## 5. Per-mode panel content

### 5.1 Telnet Winlink / Telnet P2P

Smallest content surface. Telnet doesn't have a modem to configure — the relevant block is the CMS endpoint.

**Sections:**

- **Connection** — `Endpoint: cms.winlink.org:8773`, `Transport: CMS-SSL (TLS)` — read-only display from config; the dropdown to switch CMS-SSL ↔ Telnet lives in Settings → Connection (per the transport-visibility anti-pattern fix from `docs/design/v0.0.1-ux-mockups.md` §4.1, already shipped)
- **Session** — last result + timestamp (`2 sent · 3 received · 14:22 UTC`), live state when active (`Connecting…`, `In session — 1.2 KB so far`, `Disconnected at 14:25 UTC`)
- **Actions** — `Start` (the new ribbon Connect replacement) + `Stop`

**Notes:** the existing `TelnetCmsPanelContainer` reading-pane component migrates here, simplified — most of its content already fits the chrome.

### 5.2 Packet Winlink / Packet P2P

The most content-dense panel; current AX.25 reading-pane panel migrated and densified.

**Sections:**

- **Modem link** — segmented transport picker (`TCP · USB · BT`), device field (TCP host:port for TCP; serial path + baud for USB/BT), persist on blur. The current `PacketModemBlock` collapses cleanly to this.
- **My station** — base callsign (read-only, sourced from identity), SSID dropdown (0-15), "operating as `N7EXP-7`" hint. The current "NEW: SSID" badge stays.
- **Listen** (P2P only; hidden for `cms-gateway` intent) — armed-or-idle toggle, listenDefault preference checkbox.
- **Connect** — target callsign input, digipeater path (collapsible row editor — up to 2 relays); a small accordion fits at 360 px without a modal.
- **Live** (only when state != Idle) — counts of TX/RX frames, current link state, last error if any.
- **Actions** — `Start` + `Stop`.

**Notes:** Packet doesn't have a software-modem live-state surface (no S/N, no constellation; it's a hardware TNC), so the Live block is small. `⤢` Full overlay for Packet is reserved but minimally useful — could show a frame-by-frame log; design deferred to implementation.

### 5.3 ARDOP Winlink

The mode that triggered this redesign. Content is rich because ARDOP is a software modem with sample-stream live state.

**Sections:**

- **Connect** — `Target` callsign, `ARQ bandwidth` dropdown (200 / 500 / 1000 / 2000 Hz; backed by `tuxlink-j0ij` ARQBW work already shipped)
- **Live** (when state != Stopped) — three compact meters: S/N, VU input, Throughput. Each with `key + bar + value` row. Below: a 4-line monospace stat block — `Mode 4FSK 500 · Width 500 Hz`, `PTT serial:/dev/ttyUSB1`, `RX 4128 B · TX 982 B`, `Up 3m 42s`.
- **ARQ state** — 3×3 grid with the 9 ARDOP state cells: `DISC / CON / IDLE / ISS / IRS / BUSY / RX / TX / DREQ`. Active cells get green tint + bold. Inactive cells dimmed monospace.
- **Actions** — `Start` + `Send/Receive` (B2F exchange trigger; only enabled while `connected-irs` / `connected-iss`) + `Stop`. Tertiary button: `Open WebGUI` (existing, persistent) — opens ardopcf's WebGUI in external browser (sidesteps the iframe approach for operators who want a separate window). The iframe surface lives in the Full overlay (see below).
- **Footer note** (small, faint): `Audio devices, PTT, cmd port: Settings →`. Operators dive into Settings → ARDOP HF for those (per Cameron's `ardop-ux-options.png` Option B and the existing `tuxlink-j0ij` Settings work).

**Full overlay (`⤢`):**

- **Header** — `MODEM · ARDOP Winlink` + `W4PHS-7 → W7RMS-10 · 10 W QRP · 500 Hz` + `⤡ collapse` + `☓ close`
- **Throughput chart (60 s history)** — bar sparkline rendered from the existing broadcaster's S/N + bytes samples; degrade segments colored amber. Footer: `−60s ... 540 bps · peak 720 · avg 510 ... now`.
- **ardopcf WebGUI iframe** — `<iframe src="http://localhost:8514/" />` adjacent to the throughput chart. Closes [`tuxlink-ed51`](https://github.com/cameronzucker/tuxlink) in the same surface. Note: ardopcf must be running with `-G 8514` (already wired by `tuxlink-60wh`).
- **Live gauges** — 4 big gauges (S/N, VU input, CPU, TX delay) with arc bars + sublabel categorization (`marginal-to-good`, `in range`, `working hard`, `acceptable`).
- **ARQ state full lights** — 5×2 grid, larger cells than compact panel.
- **Session log live tail** — last ~6-8 lines of the session log, color-coded (info / ok / warn / alert).
- **Troubleshoot action row** — conditions-degraded hint message (auto-detected from broadcaster signal: e.g. `⚠ Throughput dropped 30% in last 10 s`), buttons: `Change band` / `Lower bandwidth (500 → 200 Hz)` / `Try different gateway` / `Open modem settings` / `Send/Receive now` / `Disconnect`.

### 5.4 VARA HF / VARA FM (forward-looking; design-only)

When VARA backends land, the chrome inherits unchanged. Mode-specific content fits the same section grammar:

**Sections:** Connect (target + mode/bandwidth dropdown specific to VARA's modes), Live (gauges + constellation for HF, simpler gauges for FM), Protocol state (NACK / REQ / IDLE / BREAK / BUSY / etc. — VARA's vocabulary, ~5-6 cells), Actions (Start / Send-Receive / Stop).

**Full overlay (`⤢`):**

- Throughput chart (same)
- **Constellation diagram** — VARA's OFDM constellation. Tuxlink renders this natively (not iframed — VARA's UI is a Windows binary; the constellation data comes from the VARA TNC's status feed). Sized ~140 × 140 px in the overlay.
- Live gauges (4-up; same as ARDOP but with VARA-specific metrics)
- Protocol state full lights
- Session log
- Troubleshoot row — VARA-specific suggestions (`Drop to NARROW 500`, `Try VARA Tactical 2750 Hz`, etc.)

**Why iframe doesn't apply:** VARA is a Windows binary with no web UI; spectrum/constellation visualization must be rendered native from the VARA TNC status stream. ARDOP gets the iframe freebie because ardopcf has a built-in WebGUI; VARA pays for what it uses.

## 6. Vocabulary and binding details

### 6.1 Per-panel action labels

| Action | Label | Tooltip | Disabled when |
|---|---|---|---|
| Primary | **Start** | "Open the session" | mode is already started, or required fields empty (target callsign, etc.) |
| Secondary | **Stop** | "End the session gracefully" | state is `stopped` |
| Tertiary | **Abort** | "Emergency stop — interrupt any in-flight TX" | state is `stopped` |

For ARDOP / VARA modes, additionally:

| Action | Label | Tooltip | Disabled when |
|---|---|---|---|
| Per-session | **Send/Receive** | "Run a B2F exchange — flush outbox, retrieve inbox" | not in connected state (`connected-irs` or `connected-iss`) |
| Persistent | **Open WebGUI** (ARDOP only) | "Open ardopcf's spectrum/waterfall in external browser" | always enabled when modem path configured |

### 6.2 Menu items

The Session menu becomes:

- `Session → Start` (F5) — start the currently-selected mode (contextual; see §6.3)
- `Session → Stop` — stop the currently-selected mode (gracefully)
- `Session → Abort` — abort the currently-selected mode (emergency)

The View menu's existing `Show / Toggle Radio Dock` becomes:

- `View → Toggle Radio Panel` (Ctrl+Shift+M) — toggles the compact panel's visibility

### 6.3 F5 / Ctrl+Shift+O accelerator behavior

Currently both fire `cms_connect`. After this redesign:

**Contextual binding.** F5 and Ctrl+Shift+O start the currently-selected mode:
- If a connection entry is selected in the sidebar AND that mode's panel is mounted: fire that mode's Start.
- If no connection is selected AND a modem is in a non-stopped state: no-op (already running; F5 → "restart" would be surprising).
- If no connection is selected AND no modem is active: no-op. (Future: an operator-configurable "preferred default mode" could fire here.)

Rationale: the user's "F5 to start something" mental model maps to "start what I'm looking at." Without context, no-op is safer than firing a default that the user might not have intended.

### 6.4 Ribbon informational layout

After the Connect button is removed, the ribbon carries (left to right): `Callsign · Grid · GPS state · UTC + local time · Current session state`. The session-state field reflects whichever modem is most-relevant: an active modem (highest priority), else the most-recently-active modem, else "Idle". Format: `Idle` / `Connecting via ARDOP HF…` / `In session via ARDOP HF (W4PHS-7 ↔ W7RMS-10)` / `Disconnected at 14:25 UTC`.

## 7. Migration plan

Implementation lands across multiple PRs, in this sequence. Each phase = one bd-issue + one PR.

| Phase | What | Touches | Risk |
|---|---|---|---|
| **P1** | `RadioPanel` shell component scaffold — chrome (header, collapsible sections, action row), CSS, panes-grid reflow logic, visibility rule, View menu rename. No mode content yet — placeholder panels. | New: `src/radio/RadioPanel.tsx`, `src/radio/RadioPanel.css`, `src/radio/useRadioPanelVisibility.ts`. Mod: `src/shell/AppShell.tsx`, `src/shell/AppShell.css`, `src/shell/chrome/menuModel.ts`. | Low — pure additive scaffolding |
| **P2** | Telnet mode — implement `TelnetRadioPanel`; migrate `TelnetCmsPanel`'s connect logic. Test in isolation against Telnet CMS. | New: `src/radio/modes/TelnetRadioPanel.tsx`. Mod: `src/shell/AppShell.tsx` (route Telnet selection to new panel). Delete: `src/connections/TelnetCmsPanel.tsx` once migration verified. | Low — Telnet is the simplest mode, smallest content surface |
| **P3** | Packet mode — implement `PacketRadioPanel`; densify the current panel's content for 360 px. Migrate digipeater-relay editor into a compact accordion. Drop `PacketConnectionPanel`. | New: `src/radio/modes/PacketRadioPanel.tsx`. Delete: `src/packet/PacketConnectionPanel.tsx`. Mod: routing. | Medium — rich content surface; AX.25 listen state and digipeater editor are non-trivial |
| **P4** | ARDOP mode — implement `ArdopRadioPanel`; consolidate `ArdopDock` + `ArdopHfStub` into the new panel. Remove both. | New: `src/radio/modes/ArdopRadioPanel.tsx`. Delete: `src/modem/ArdopDock.tsx`, `src/modem/ArdopDock.css`, `src/connections/ArdopHfStub.tsx`. Mod: routing, integration tests. | Medium — ARDOP integration tests will need refactoring; existing `useModemStatus` and `ConsentModal` keep their roles |
| **P5** | Full overlay (`⤢`) — expand-to-overlay layout, reading-pane state preservation, list compression. ARDOP-specific full content: throughput chart, gauges, big ARQ state, session log tail. | New: `src/radio/RadioPanelOverlay.tsx`, `src/radio/charts/ThroughputChart.tsx`. Mod: `RadioPanel.tsx` (`⤢` button + overlay state). | Medium-high — layout reflow when overlay opens/closes is the tricky bit; reading-pane DOM must persist |
| **P6** | ardopcf WebGUI iframe in the Full overlay (closes `tuxlink-ed51`). Verify Tauri's WebKitGTK allows iframes to `http://localhost`. | Mod: `RadioPanelOverlay.tsx` for ARDOP. | Low if WebKitGTK cooperates; medium otherwise (CSP / same-origin investigation) |
| **P7** | Vocabulary cleanup — rename `Session → Connect/Disconnect` to `Start/Stop`; rebind F5 / Ctrl+Shift+O to contextual Start; remove the ribbon Connect button; rename `Show Radio Dock` → `Toggle Radio Panel`. | Mod: `src/shell/chrome/menuModel.ts`, `dispatchMenuAction.ts`, `useAccelerators.ts`, `DashboardRibbon.tsx`, `MenuHandlers`. Delete: ribbon's `connect-button` element. | Low |

**PR #166 (`tuxlink-mnk4` dock-visibility fix) is superseded by P4** — the whole `ArdopDock` goes away, so a dock-visibility patch on top of it is moot. Close PR #166 without merging once P4 lands. The fix's intent (operator can reach the dial form on cold start) is satisfied structurally by P4 putting the dial form inline in a panel that mounts on sidebar selection.

**Phase ordering rationale:** P1 first to land the new chrome before any mode migration; P2 (Telnet) second because it's smallest and proves the shell; P3 (Packet) third because it's the most content-dense migration and validates that the chrome handles a hardware-TNC mode; P4 (ARDOP) fourth because it triggered the whole redesign and consolidates the most code; P5 (Full overlay) fifth because it requires the compact panel to exist; P6 (iframe) sixth because it requires the overlay to exist; P7 (vocabulary) last because it's the renaming sweep across already-migrated components.

## 8. Out of scope

These came up during the brainstorm; explicitly deferred.

- **VARA HF / VARA FM panel IMPLEMENTATION** — design-only in §5.4 above; build when the VARA backends ship.
- **ARDOP P2P** — `ardop-hf` is currently `cms` intent only; P2P over ARDOP is a v0.1+ ProtocolEntry. Panel design will inherit `Ardop Winlink`'s chrome with intent swap.
- **Mode-2 auto-degradation detection** — the troubleshoot row's conditions-degraded hint requires a heuristic that watches the broadcaster signal for "throughput dropped >X% in Y seconds." For v1, the operator manually triggers `⤢` and the troubleshoot row always shows full action set; the hint banner is implemented later.
- **Multi-modem concurrent operation** — one active modem at a time. The ribbon's session-state field reflects the single active modem.
- **Rig control** — `Tools → Rig Control` menu item is already `disabled: true` (v0.1+). The panel's frequency display is read-only (sourced from any connected rig); no frequency-set surface.
- **Mobile / narrow-window layout** — if window <1024 px wide, the panel may want a different mount strategy (overlay-only? slide-in?). Design deferred.
- **Persistent dock content (session-timer / outbox / last-sessions)** that `docs/design/v0.0.1-ux-mockups.md` §3.5 specified for v0.0.1 — that content was for an "Off" state of the dock, which doesn't exist in this redesign. The session-timer (for adaptive-polling background sessions) belongs in the ribbon or status bar; outbox + last-sessions are already in the sidebar / message-list. Separate small redesign pass when those surfaces are wanted.

## 9. Open implementation questions

To resolve during implementation (not blocking the spec):

- **Empty-state of the radio panel when modem != stopped but no peer info yet** — show "Starting…" placeholder vs hide the Live section vs show the Live section with `—` values. Recommendation: show Live with `—` values so the section structure is stable; the dot pulses to indicate liveness.
- **Auto-collapse Full overlay on message-list click?** — when overlay is active and operator clicks a different message in the compressed list, does the overlay collapse (revealing the new message in the reading pane)? Recommendation: yes — clicking a message signals "I want to read this," which is reading-pane intent.
- **Sidebar selection persistence across app restarts** — should the panel auto-mount on launch with the last-selected connection? Recommendation: no — operator opens fresh on Inbox; no surprise dock appearance.
- **Settings panel reachability from the radio panel** — the footer note `Audio devices, PTT, cmd port: Settings →` is text-only. Should it be a clickable link that opens Settings → ARDOP HF directly? Recommendation: yes — one-click affordance for operators who realize they need to change the audio device mid-session.
- **Session menu — should `Stop` / `Abort` be present when no mode is active?** Recommendation: present but disabled. Discoverability matters; disabled state with a tooltip ("No active session") is clearer than "menu items appear and disappear."

## 10. References

- [`docs/design/v0.0.1-ux-principles.md`](../../design/v0.0.1-ux-principles.md) — anchor principles (Principle 2: single-pane preference; Principle 4: persistent radio-connection-state pane; Principle 5: don't waste screen real estate). This spec advances Principle 5 explicitly.
- [`docs/design/v0.0.1-ux-mockups.md`](../../design/v0.0.1-ux-mockups.md) — locked decisions doc; this spec supersedes §3.5 (modem-console placement).
- [`docs/design/mockups/2026-05-17-modem-placements-v05.html`](../../design/mockups/2026-05-17-modem-placements-v05.html) — original Compact / Full canonical mocks (VARA-flavored).
- [`docs/design/mockups/images/modem-compact.png`](../../design/mockups/images/modem-compact.png) and [`modem-full.png`](../../design/mockups/images/modem-full.png) — visual references.
- `ardop-ux-options.png` (workspace root) — operator's May 29 Option B recommendation (Settings + slim connect pane).
- Decompiled RMS Express at `dev/scratch/winlink-re/decompiled/rms-express/` (gitignored) — action vocabulary (Start / Stop / Abort), session-type list (`Ardop Winlink` / `Packet P2P` / etc.).
- 2026-05-31 brainstorm screens at `.superpowers/brainstorm/610438-1780202736/content/` (gitignored).

## 11. Closing notes for the implementation plan

When `superpowers:writing-plans` runs against this spec, the seven phases in §7 are the natural plan structure. Each phase should:

- Land on its own per-task branch (`bd-<id>/<slug>`)
- Pass `cargo test --lib`, `cargo clippy -- -D warnings`, `pnpm vitest run`, `pnpm exec tsc --noEmit`
- Include vitest coverage for the new chrome / panel components
- Get one Codex adversarial round on the diff before merge (per the `feedback_no_carveout_on_cross_provider_adrev` discipline; small per-phase scope keeps the rounds cheap)
- Operator-smoke the visible UI changes via `pnpm tauri dev` before close (per `feedback_browser_smoke_before_ship`)

The cascade closes the following bd issues by absorption (no separate PRs needed):

- `tuxlink-mnk4` (dock-visibility fix) — superseded by P4
- `tuxlink-ed51` (iframe-embed WebGUI) — absorbed into P6
- `tuxlink-mzr7` (TWOTONETEST) — absorbed into P6 (ardopcf WebGUI iframe provides the test-tone trigger)

After P7 lands, this spec is considered satisfied and `tuxlink-74mx` closes.
