# Radio-mode right-panel UX design

> **Status:** Authoritative for the redesign of every radio-mode connection UI in tuxlink — Telnet, AX.25 Packet, ARDOP HF, and the eventual VARA HF / VARA FM. Supersedes the "v0.5+ modem-console placement" deferral in [`docs/design/v0.0.1-ux-mockups.md`](../../design/v0.0.1-ux-mockups.md) §3.5. Resolves the operator-flagged "the entire ARDOP UI is horribly malformed" diagnosis from the 2026-05-31 brainstorm.
>
> **Lineage:** [`tuxlink-74mx`](https://github.com/cameronzucker/tuxlink) (this spec). Brainstorm conducted 2026-05-31 by agent `crag-hemlock-kestrel` using the `superpowers:brainstorming` visual-companion flow; intermediate screens persisted at `.superpowers/brainstorm/610438-1780202736/content/` (gitignored). Reference materials consulted: decompiled RMS Express (action vocabulary, session-type list), `docs/design/v0.0.1-ux-principles.md`, `docs/design/v0.0.1-ux-mockups.md`, `docs/design/mockups/images/modem-{compact,full}.png`, operator's `ardop-ux-options.png` (May 29).
>
> **Revision history:**
> - 2026-05-31 v1 — initial commit (paradigm + Full overlay + compact framing).
> - 2026-05-31 v2 — brainstorm follow-up: Full overlay dropped (operator: "do we fit everything we need in the 360 px or not?"); "compact" framing dropped (operator: "we should just show the main panel size all the time"); bottom session-log strip dropped, log folded into the panel as a section (operator: "it takes up a full width of real estate it can't actually fill"); Troubleshoot section replaced with Signal section (operator: "tuxlink is not a useless Windows troubleshooting Wizard"); log typography rules added; migration plan trimmed 7 → 5 phases; cascade closures updated.

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

2. **Stretched negative space.** AX.25 packet's reading-pane panel (~700 px on a 1280 wide window) holds ~300 px of actual form content. Buttons stretch to fill column width; inputs (a 2-char SSID dropdown, a 6-char callsign) render at ~600 px of column width. Operator framing: *"Most of the space in the AX.25 current window is wasted as-built. We largely stretch the buttons and fields to fill large amounts of negative space."*

3. **Duplicate Connect class.** The dashboard ribbon's `cms_connect` button (legacy CMS quick-connect) and ARDOP HF's per-dock Connect button both render at the same time when the operator is on the ARDOP HF view, with identical labels and different effects. The most-visible symptom of the broken paradigm.

The canonical design in `v0.0.1-ux-mockups.md` §3.5 acknowledged that **software-mediated modems** (ARDOP, VARA) would eventually need a different surface than the inline-panel pattern that suits hardware-TNC modes (AX.25 Packet, Telnet CMS), and locked a 3-state (Off / Compact / Full) togglable Radio Dock as the design — but deferred the implementation to v0.5+ with a placeholder block in v0.0.1. ARDOP shipped without the canonical surface, taking a shortcut and cramming half a modem console into the dock-slot that was reserved for both the v0.0.1 session-timer/outbox/last-sessions content and the v0.5+ modem console. The half-cocked result is what this spec corrects.

## 3. Locked decisions

The brainstorm output. These are not options anymore.

### 3.1 Reading pane is for messages only

The reading-pane slot houses `MessageView` (when a message is selected) and nothing else after this redesign. No connection-config panels, no stub messages pointing at other surfaces, no protocol panels. `ArdopHfStub` is removed. `TelnetCmsPanel` is migrated to the right panel (see §5.1). `PacketConnectionPanel` is migrated to the right panel (see §5.2).

Rationale: the reading pane is the mail-client reading surface — its actual job in the layout. Connection forms don't have 1fr-worth of content; they have ~300 px of content stretched to fill 800 px+. Moving connection forms out resolves the stretched-button / negative-space problem at its source.

### 3.2 Right-hand radio panel at 360 px

A single right-hand column carries all radio UX: connection setup, modem-link config (for hardware-TNC modes), live state, signal quality, session log, action buttons. One panel per mode, mounted based on which connection-sidebar entry is selected or which modem is currently in a non-stopped state.

- **Width:** 360 px. Single size whenever mounted — no "compact vs full" distinction, no toggled summary mode, no collapse-to-icon state. The panel either renders at 360 with full content, or it's not mounted.
- **Validation:** 320 / 360 / 400 were rendered at 1080p scale during the brainstorm; 360 gives gauges and labels room to breathe without compromising reading-pane readability (reading pane is ~980 px wide on 1920 — comfortably above Apple Mail's ~700 px and Gmail web's ~700-800 px).
- **Calibration:** VARA HF v4.1.2's standalone window is ~520-600 px wide and presents the same information classes (gauges, state lights, constellation, action row) in roughly the same footprint as 360 + the right-panel chrome.

### 3.3 Visibility rule

The panel mounts when ANY of:
- A connection entry is selected in the sidebar (Telnet / Packet / ARDOP HF / VARA HF / etc.)
- Any modem is in a non-stopped state (Connecting / Connected / etc.)
- View → Toggle Radio Panel is on (existing menu item, repurposed; existing `Ctrl+Shift+M` accelerator continues to bind)

When none of those apply, the panel is hidden and the reading pane gets the 360 px back. Layout reflows on mount/unmount. The repurposed View menu item — previously "Show Radio Dock" / "Toggle Radio Dock" — is renamed to **"Toggle Radio Panel"** so the menu label matches what it actually controls.

### 3.4 All sections always rendered

The panel renders every section it knows about regardless of modem state. Empty data shows as `—` placeholders or as faint "no data yet" microcopy; the section structure stays stable across the modem lifecycle (Disconnected → Connecting → Connected → Disconnecting → Stopped). No collapsible-by-default sections, no progressive disclosure of structure as the modem progresses.

Rationale: a stable layout reads as a finished product; sections appearing and disappearing reads as broken. Operators learn the panel shape once and rely on it; data inside changes, structure does not.

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

### 3.7 Session log lives in the panel; bottom strip removed

The bottom session-log strip is removed from the layout. Session logs live as a section inside the radio panel itself — per-session by construction (panel mode = session mode), naturally fits 360 px width (text lines are short), and gets a fixed-height (~130 px) scrollable region that auto-scrolls to latest. `View → Show Session Log` menu item retires; logs are visible whenever the panel is.

Rationale (operator framing): *"It can't actually be expanded to be very large at the bottom of the screen, and it takes up a full width of real estate it can't actually fill."* The bottom strip was a wrong-slot decision — log content (short text lines per entry) doesn't justify 1920 px of horizontal real estate, and the strip couldn't grow vertically without eating the reading pane. The panel slot is the right home.

Closed-session log viewing: when the operator selects a connection-sidebar entry whose mode is currently Stopped, the panel mounts and shows the most-recent session's log (read-only). A "Last sessions" affordance for choosing older sessions is deferred to implementation if needed.

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
│  Status bar                                                          │  ~24 px
└──────────────────────────────────────────────────────────────────────┘
```

When the radio panel is hidden: reading pane takes the 360 px back, becomes `1fr + 360 px`. No reflow on overlay state — there is no overlay state.

### 4.2 Panel chrome

```
┌────────────────────────────────────────────────────┐
│ ● MODEM · Ardop Winlink            ☓               │  header
├────────────────────────────────────────────────────┤
│ CONNECT                                            │
│   Target          [ W7RMS-10 ____________ ]        │
│   ARQ bandwidth   [ 500 Hz ⌄ ]                     │
├────────────────────────────────────────────────────┤
│ LIVE                              ● connected · IRS │
│   S/N         +8.4 dB    ▮▮▮▮▮▯▯                  │
│   VU input    −18 dBFS   ▮▮▮▮▯▯▯                  │
│   Throughput  540 bps    ▮▮▮▮▮▮▯                  │
│   [60 s throughput sparkline]                      │
│   Mode 4FSK 500 · Width 500 Hz · Up 3m 42s         │
│   RX 4128 B  TX 982 B                              │
├────────────────────────────────────────────────────┤
│ SIGNAL                                             │
│   ┌────────┐   S/N trend  +8.4 dB                 │
│   │   78   │   [60 s S/N sparkline]                │
│   │Quality │                                       │
│   └────────┘                                       │
│   [recent frame ribbon: CON IDLE DATA ACK DATA …] │
│   ● CON  ● IDLE  ● DATA  ● ACK  ● NAK  ● REJ      │
├────────────────────────────────────────────────────┤
│ ARQ STATE                                          │
│   ▢ DISC  ▢ CON   ▢ IDLE                          │
│   ▢ ISS   ▰ IRS   ▢ BUSY                          │
│   ▢ RX    ▢ TX    ▰ DREQ                          │
├────────────────────────────────────────────────────┤
│ SESSION LOG                       live tail        │
│   [scrollable region, ~130 px, auto-scroll]        │
│   [ ] Show raw  [✓] Auto-scroll        Copy ↗      │
├────────────────────────────────────────────────────┤
│ ACTIONS                                            │
│   [ Send / Receive ]                               │
│   [ Open WebGUI ↗ ]  [ Stop ]                      │
│   Audio devices, PTT, cmd port: Settings →         │
└────────────────────────────────────────────────────┘
```

**Header:** state-dot (color = link state, pulses when transmitting) + name (`MODEM · {Mode} {Intent}`) + sub (peer / bandwidth, brief) + `☓` close-and-disconnect.

**Sections:** every mode renders a subset of `Connect / Modem / Station / Listen / Live / Signal / ARQ State / Session log / Actions`. Section bodies hold the mode's content; section structure is stable across the lifecycle. Empty data shows as `—` placeholders.

**Actions row:** always the bottom block. Start (primary, when state = stopped) OR Send/Receive (primary, when connected) + Stop (secondary) + Open WebGUI (ARDOP only) + Abort (tertiary, danger-color, when running).

### 4.3 Session log rendering rules

Mode-agnostic typography for the Session log section, applied identically across Telnet / Packet / ARDOP / VARA:

- **Column structure:** 56 px fixed timestamp column (HH:MM:SS in 9pt monospace) + flex message column. Hanging indent on wrap — wrapped continuation lines start at the message column's left edge, not under the timestamp.
- **Token wrapping:** `word-break: break-word` so long unbroken tokens (URLs, IP addresses, raw error strings) break at any character. No horizontal scroll.
- **Severity treatment:**
  - `info` — light-gray text, no glyph
  - `ok` — green text, no glyph (success outcomes)
  - `warn` — yellow text, `⚠` glyph prefix
  - `alert` — red text, `⊘` glyph prefix, left-border accent strip (4 px), faint background tint
- **Multi-paragraph errors:** the entry body can contain a bold summary line + a faint italic raw-error continuation block. Used when an error has both a human-shaped summary and an underlying raw transport-layer message.
- **Region size:** ~130 px scrollable height (about 5-7 entries at typical line length; fewer for entries with long wrapped messages). Auto-scroll keeps latest visible; pauses on operator scroll-back.
- **Show raw toggle:** controls whether the underlying B2F protocol lines + Express-style `***` annotations render (per `v0.0.1-ux-mockups.md` §4.4). When on, raw lines render with the same severity + wrapping rules; they appear with the `raw` style class (faint italic).
- **Copy:** small `Copy ↗` affordance copies the visible-or-full log (operator decides via context: `Copy visible` if log is filtered/scrolled; `Copy full` otherwise) to the clipboard.

## 5. Per-mode panel content

### 5.1 Telnet Winlink / Telnet P2P

Smallest content surface. No modem to configure.

**Sections rendered:**

- **Connection** — `Endpoint: cms.winlink.org:8773`, `Transport: CMS-SSL (TLS)` — read-only display from config; the dropdown to switch CMS-SSL ↔ Telnet lives in Settings → Connection (per the transport-visibility anti-pattern fix from `v0.0.1-ux-mockups.md` §4.1, already shipped)
- **Session** — last result + timestamp (`2 sent · 3 received · 14:22 UTC`), live state when active (`Connecting…`, `In session — 1.2 KB so far`, `Disconnected at 14:25 UTC`)
- **Session log** — per §4.3 rendering rules
- **Actions** — `Start` + `Stop`

**Sections NOT rendered for Telnet:** Live (no signal), Signal (no signal-quality data — TCP doesn't expose RTT in a meaningful "signal-quality" way), ARQ state (not applicable).

**Notes:** the existing `TelnetCmsPanelContainer` reading-pane component migrates here, simplified — most of its content already fits the chrome.

### 5.2 Packet Winlink / Packet P2P

Most content-dense panel for a hardware-TNC mode.

**Sections rendered:**

- **Modem link** — segmented transport picker (`TCP · USB · BT`), device field (TCP host:port for TCP; serial path + baud for USB/BT), persist on blur. The current `PacketModemBlock` collapses cleanly to this.
- **My station** — base callsign (read-only, sourced from identity), SSID dropdown (0-15), "operating as `N7EXP-7`" hint. The current "NEW: SSID" badge stays.
- **Listen** (P2P only; hidden for `cms-gateway` intent) — armed-or-idle toggle, listenDefault preference checkbox.
- **Connect** — target callsign input, digipeater path (compact relay-row editor — up to 2 relays, inline at 360 without modal).
- **Live** (state ≠ Idle) — counts of TX/RX frames, current link state, last error if any.
- **Session log** — per §4.3 rendering rules
- **Actions** — `Start` + `Stop`.

**Sections NOT rendered for Packet:** Signal (no software-modem signal-quality data — AX.25 over hardware TNC doesn't expose continuous SNR/quality the way ARDOP does; some TNCs expose AFSK level meters that could populate a future Signal section, but not in v0.0.1), ARQ state (not applicable).

### 5.3 Ardop Winlink

The mode that triggered this redesign. Content is rich because ARDOP is a software modem with sample-stream live state.

**Sections rendered:**

- **Connect** — `Target` callsign, `ARQ bandwidth` dropdown (200 / 500 / 1000 / 2000 Hz; backed by `tuxlink-j0ij` ARQBW work already shipped).
- **Live** — three compact meters: S/N, VU input, Throughput. Each with `key + bar + value` row. Below: 60s throughput sparkline (340 × 42 px, color-coded warning where throughput dropped). Below: 4-line monospace stat block — `Mode 4FSK 500 · Width 500 Hz`, `PTT serial:/dev/ttyUSB1`, `RX 4128 B · TX 982 B`, `Up 3m 42s`.
- **Signal** — operator-meaningful signal-quality view that replaces the dropped Troubleshoot wizard buttons:
  - **Quality score** — ardopcf reports a 0-100 Quality value via `PINGACK / PING` events. Surface as a big number (24 pt) in a radial-gradient box at the left of the section. Sourced from `tuxlink-1637` (PINGACK structured S/N parsing — absorbed into this work; closes the issue).
  - **S/N trend sparkline** — companion to the throughput sparkline, smaller height (~28 px). Shows 60s of S/N samples. Same warn/bad color thresholds as throughput. Operator at-a-glance "is conditions improving or degrading?"
  - **Recent frame ribbon** — horizontal flow of recent ARQ subprotocol frame types (CON / IDLE / DATA / ACK / NAK / REJ) over time. Color-coded — NAK clusters or REJ presence tell the operator exactly what's wrong without any interpretation imposed. Legend below.
- **ARQ state** — 3×3 grid with the 9 ARDOP state cells: `DISC / CON / IDLE / ISS / IRS / BUSY / RX / TX / DREQ`. Active cells get green tint + bold; inactive cells dimmed monospace.
- **Session log** — per §4.3 rendering rules.
- **Actions** — `Send / Receive` (primary when connected; B2F exchange trigger; only enabled while `connected-irs` / `connected-iss`) OR `Start` (when stopped) + `Open WebGUI ↗` (tertiary; always when modem path configured; opens ardopcf's WebGUI in external browser) + `Stop` (when running). Footer note: `Audio devices, PTT, cmd port: Settings →` (clickable; opens Settings → ARDOP HF).

### 5.4 Vara HF / Vara FM (forward-looking; design-only)

When VARA backends land, the panel chrome inherits unchanged. Mode-specific content fits the same section grammar:

**Sections rendered:** Connect (target + VARA mode/bandwidth dropdown), Live (gauges + throughput sparkline), Signal (constellation diagram in place of ARDOP's Quality-plus-frame-ribbon; OFDM scatter is VARA's natural signal-quality visualization), Protocol state (VARA's vocabulary — NACK / REQ / IDLE / BREAK / BUSY / etc., ~5-6 cells), Session log, Actions (Start / Send-Receive / Stop).

The Signal section is the mode-specific signal-quality slot: ARDOP gets Quality + S/N trend + frame ribbon; VARA gets the constellation. Same slot, different content.

**Why no spectrum/waterfall iframe:** VARA is a Windows binary with no web UI; spectrum/constellation visualization must be rendered native from the VARA TNC status stream. (ARDOP also doesn't iframe — operators who want ardopcf's spectrum/waterfall use `Open WebGUI ↗` to ardopcf's native UI in external browser, which is the established workflow for any ardopcf install.)

## 6. Vocabulary and binding details

### 6.1 Per-panel action labels

| Action | Label | Tooltip | Disabled when |
|---|---|---|---|
| Primary (stopped) | **Start** | "Open the session" | required fields empty (target callsign, etc.) |
| Primary (connected) | **Send / Receive** | "Run a B2F exchange — flush outbox, retrieve inbox" | not in connected state (`connected-irs` or `connected-iss`) |
| Secondary | **Stop** | "End the session gracefully" | state is `stopped` |
| Tertiary | **Abort** | "Emergency stop — interrupt any in-flight TX" | state is `stopped` |
| Persistent (ARDOP only) | **Open WebGUI ↗** | "Open ardopcf's spectrum/waterfall in external browser" | always enabled when modem path configured |

### 6.2 Menu items

The Session menu becomes:

- `Session → Start` (F5) — start the currently-selected mode (contextual; see §6.3)
- `Session → Stop` — stop the currently-selected mode (gracefully)
- `Session → Abort` — abort the currently-selected mode (emergency)

The View menu's existing `Show / Toggle Radio Dock` becomes:

- `View → Toggle Radio Panel` (Ctrl+Shift+M) — toggles the radio panel's visibility

The View menu's existing `Show Session Log` / `Show Raw Session Log` items retire — session logs live in the panel; raw toggle is an in-panel checkbox per §4.3.

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
| **P1** | `RadioPanel` shell component scaffold — chrome (header, fixed-position section structure, action row), CSS, panes-grid reflow logic, visibility rule, View menu rename, bottom session-log strip removal. No mode content yet — placeholder panels per mode. | New: `src/radio/RadioPanel.tsx`, `src/radio/RadioPanel.css`, `src/radio/useRadioPanelVisibility.ts`. Mod: `src/shell/AppShell.tsx`, `src/shell/AppShell.css`, `src/shell/chrome/menuModel.ts`. Delete: `src/session/SessionLog.tsx` (the bottom-strip surface — log moves into the panel as a per-mode section). | Low — pure additive scaffolding |
| **P2** | Telnet mode — implement `TelnetRadioPanel` per §5.1; migrate `TelnetCmsPanel`'s connect logic; include Session log section per §4.3. Test in isolation against Telnet CMS. | New: `src/radio/modes/TelnetRadioPanel.tsx`. Mod: `src/shell/AppShell.tsx` (route Telnet selection to new panel). Delete: `src/connections/TelnetCmsPanel.tsx` once migration verified. | Low — Telnet is the simplest mode |
| **P3** | Packet mode — implement `PacketRadioPanel` per §5.2; densify the current panel's content for 360 px. Migrate digipeater-relay editor into a compact inline editor. Drop `PacketConnectionPanel`. Include Session log section. | New: `src/radio/modes/PacketRadioPanel.tsx`. Delete: `src/packet/PacketConnectionPanel.tsx`. Mod: routing. | Medium — rich content surface; AX.25 listen state and digipeater editor are non-trivial |
| **P4** | ARDOP mode — implement `ArdopRadioPanel` per §5.3, including the **Signal section** (Quality score + S/N trend sparkline + recent frame ribbon — closes `tuxlink-1637` PINGACK structured S/N parsing). Consolidate `ArdopDock` + `ArdopHfStub` into the new panel. Remove both. Include Session log section. | New: `src/radio/modes/ArdopRadioPanel.tsx`, `src/radio/charts/Sparkline.tsx`, `src/radio/SignalSection.tsx`. Delete: `src/modem/ArdopDock.tsx`, `src/modem/ArdopDock.css`, `src/connections/ArdopHfStub.tsx`. Mod: routing, integration tests. | Medium — ARDOP integration tests will need refactoring; existing `useModemStatus` and `ConsentModal` keep their roles; PINGACK parsing in backend (closes `tuxlink-1637`) |
| **P5** | Vocabulary cleanup — rename `Session → Connect/Disconnect` to `Start/Stop` + add `Session → Abort`; rebind F5 / Ctrl+Shift+O to contextual Start; remove the ribbon Connect button; retire `View → Show Session Log`. | Mod: `src/shell/chrome/menuModel.ts`, `dispatchMenuAction.ts`, `useAccelerators.ts`, `DashboardRibbon.tsx`, `MenuHandlers`. Delete: ribbon's `connect-button` element. | Low |

**PR #166 (`tuxlink-mnk4` dock-visibility fix) is superseded by P4** — the whole `ArdopDock` goes away, so a dock-visibility patch on top of it is moot. Close PR #166 without merging once P4 lands. The fix's intent (operator can reach the dial form on cold start) is satisfied structurally by P4 putting the dial form inline in a panel that mounts on sidebar selection.

**Phase ordering rationale:** P1 first to land the new chrome + remove the bottom strip before any mode migration; P2 (Telnet) second because it's smallest and proves the shell; P3 (Packet) third because it's the most content-dense migration and validates that the chrome handles a hardware-TNC mode; P4 (ARDOP) fourth because it triggered the whole redesign, consolidates the most code, AND brings in the Signal section + closes `tuxlink-1637`; P5 (vocabulary) last because it's the renaming sweep across already-migrated components.

## 8. Out of scope

These came up during the brainstorm; explicitly deferred.

- **Full overlay / Mode-2 troubleshooting expanded view** — dropped during the brainstorm. The compact panel at 360 fits all the operator-meaningful data; the Full overlay's headline feature (ardopcf WebGUI iframe at full size) is already served by the `Open WebGUI ↗` button → external browser, which is the established workflow for any ardopcf install.
- **VARA HF / VARA FM panel IMPLEMENTATION** — design-only in §5.4 above; build when the VARA backends ship. Panel chrome inherits unchanged; the Signal section's constellation content slots into the same place ARDOP's Quality-plus-frame-ribbon lives.
- **Ardop P2P** — `ardop-hf` is currently `cms` intent only; P2P over ARDOP is a v0.1+ ProtocolEntry. Panel design will inherit `Ardop Winlink`'s chrome with intent swap.
- **Conditions-degraded auto-detection** — the Signal section visualizes trends; if a heuristic later flags "throughput dropped >X% in Y seconds," it could surface as a transient banner above the Signal section. Not in v1.
- **Multi-modem concurrent operation** — one active modem at a time. The ribbon's session-state field reflects the single active modem.
- **Rig control** — `Tools → Rig Control` menu item is already `disabled: true` (v0.1+). The panel's frequency display is read-only (sourced from any connected rig); no frequency-set surface.
- **Mobile / narrow-window layout** — if window <1024 px wide, the panel may want a different mount strategy (overlay-only? slide-in?). Design deferred.
- **Persistent dock content (session-timer / outbox / last-sessions)** that `v0.0.1-ux-mockups.md` §3.5 specified for v0.0.1 — that content was for an "Off" state of the dock, which doesn't exist in this redesign. The session-timer (for adaptive-polling background sessions) belongs in the ribbon or status bar; outbox + last-sessions are already in the sidebar / message-list. Separate small redesign pass when those surfaces are wanted.
- **Last-sessions / closed-session log browser** — for now, selecting a sidebar entry whose mode is stopped mounts the panel with that mode's most-recent session log. A "Last sessions" picker affordance for older logs is deferred to implementation if operators report needing it.

## 9. Open implementation questions

To resolve during implementation (not blocking the spec):

- **Empty-state of the radio panel when modem != stopped but no peer info yet** — show placeholder `—` values in Live / Signal / ARQ State sections vs hide the values entirely vs show `Connecting…` blanket. Recommendation: show sections with `—` placeholders so the structure is stable; the header dot pulses to indicate liveness.
- **Sidebar selection persistence across app restarts** — should the panel auto-mount on launch with the last-selected connection? Recommendation: no — operator opens fresh on Inbox; no surprise panel appearance.
- **Recent frame ribbon length** — how many frames to show? Recommendation: 14 cells (the brainstorm mock's count, fits naturally at 360 px column width). Each cell ~20 px wide; older frames scroll off the left.
- **Auto-scroll pause behavior** — when operator scrolls up in the log, auto-scroll should pause. Visual indicator: faint "Auto-scroll paused — click to resume" footer in the log box.
- **Settings panel reachability from the radio panel** — the footer note `Audio devices, PTT, cmd port: Settings →` is text. Should it be a clickable link that opens Settings → ARDOP HF directly? Recommendation: yes — one-click affordance for operators who realize they need to change the audio device mid-session.
- **Session menu — should `Stop` / `Abort` be present when no mode is active?** Recommendation: present but disabled. Discoverability matters; disabled state with a tooltip ("No active session") is clearer than "menu items appear and disappear."

## 10. References

- [`docs/design/v0.0.1-ux-principles.md`](../../design/v0.0.1-ux-principles.md) — anchor principles (Principle 2: single-pane preference; Principle 4: persistent radio-connection-state pane; Principle 5: don't waste screen real estate). This spec advances Principle 5 explicitly.
- [`docs/design/v0.0.1-ux-mockups.md`](../../design/v0.0.1-ux-mockups.md) — locked decisions doc; this spec supersedes §3.5 (modem-console placement) and partially supersedes §4.4 (session log) by moving the log into the panel rather than a separate strip.
- [`docs/design/mockups/2026-05-17-modem-placements-v05.html`](../../design/mockups/2026-05-17-modem-placements-v05.html) — original Compact / Full canonical mocks (VARA-flavored). This spec retains the Compact mock's chrome ethos and drops the Full overlay.
- `ardop-ux-options.png` (workspace root) — operator's May 29 Option B recommendation (Settings + slim connect pane).
- Decompiled RMS Express at `dev/scratch/winlink-re/decompiled/rms-express/` (gitignored) — action vocabulary (Start / Stop / Abort), session-type list (`Ardop Winlink` / `Packet P2P` / etc.).
- 2026-05-31 brainstorm screens at `.superpowers/brainstorm/610438-1780202736/content/` (gitignored) — landscape / right-panel-paradigm / width-at-1080p / compact-only-revised / log-in-panel / signal-not-troubleshoot / log-long-lines.

## 11. Closing notes for the implementation plan

When `superpowers:writing-plans` runs against this spec, the five phases in §7 are the natural plan structure. Each phase should:

- Land on its own per-task branch (`bd-<id>/<slug>`)
- Pass `cargo test --lib`, `cargo clippy -- -D warnings`, `pnpm vitest run`, `pnpm exec tsc --noEmit`
- Include vitest coverage for the new chrome / panel components
- Get one Codex adversarial round on the diff before merge (per the `feedback_no_carveout_on_cross_provider_adrev` discipline; small per-phase scope keeps the rounds cheap)
- Operator-smoke the visible UI changes via `pnpm tauri dev` before close (per `feedback_browser_smoke_before_ship`)

The cascade closes the following bd issues:

- `tuxlink-mnk4` (dock-visibility fix) — superseded by P4 (the whole `ArdopDock` is removed); close PR #166 without merging once P4 lands
- `tuxlink-ed51` (iframe-embed WebGUI) — close as resolved by existing `Open WebGUI ↗` button (shipped via `tuxlink-60wh`); the Full overlay that would have hosted the iframe was dropped from the design, and external-browser-via-button is the established ardopcf workflow
- `tuxlink-mzr7` (TWOTONETEST) — close as resolved by existing `Open WebGUI ↗` button; ardopcf's WebGUI has the test-tone trigger built in
- `tuxlink-1637` (PINGACK structured S/N) — absorbed into P4's Signal section work (Quality score is the user-visible result of parsing PINGACK events)

After P5 lands, this spec is considered satisfied and `tuxlink-74mx` closes.
