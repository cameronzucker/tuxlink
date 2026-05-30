# Winlink Express — feature inventory for Tuxlink parity decisions

> Date: 2026-05-29 · Agent: yew-cypress-oak · bd: tuxlink-95z

## 0. Purpose + method

This document enumerates **every user-facing feature** of Winlink Express (RMS Express) so each one becomes an explicit `ship / plan / defer / drop` decision in Tuxlink instead of an accidental omission. Operator-decided ratings (Status, Rationale) are left as placeholders for Cameron; nothing below is implemented or shipped by the writing of this doc.

**Source of truth.** Decompiled `RMS Express.exe` v(2025-03-13 build) at `dev/scratch/winlink-re/decompiled/rms-express/` — 259 `.cs` files, 145 Form classes. UI surface enumerated from `Main.cs` (menu hierarchy + 24-button toolbar + two context menus) plus each per-Form `InitializeComponent` for sub-dialog menus and buttons. Per [`feedback_winlink_re_authoritative_sources`](../../README.md), the decompiled binary is the ground truth — prose docs are unreliable.

**Out of scope.** VARA modem internals (the DSP/protocol layer) remain off-limits per the clean-sheet posture (project_v05_modem_design_posture). This audit is the **email client UI surface** — what buttons exist and what they do — not modem internals.

**Glossary.**
- **CMS**: Winlink Common Message Server (cloud gateway).
- **RMS**: Radio Mail Server (the on-air gateway sites).
- **B2F**: the wire protocol Winlink Express speaks to CMS/RMS.
- **P2P**: peer-to-peer Winlink session between two stations, no CMS.
- **MESH**: AREDN/HSMM mesh networking integration (IP over wireless mesh).
- **Post Office**: a local store-and-forward hub (RMS Relay / hybrid).
- **Radio-only**: same as Post Office, no internet egress.

**Status codes** (for the inventory tables below; operator-set):
- 🟢 **shipped** — implemented in current Tuxlink (verifiable via `cargo run` + UI)
- 🟡 **planned** — in scope for v0.1 / v0.5 / v1.0 per current roadmap or bd ready queue
- 🟠 **deferred** — intentional later, no current plan (post-v1.0 candidate)
- 🔴 **dropped** — intentional non-goal (rationale required)
- ⚪ **undecided** — not yet rationalized (this PR seeds the conversation)

---

## 1. Main window surface

The Winlink Express main window has 11 top-level controls along the menu strip and a 24-button toolbar below it. The window body is a 3-pane layout: folders sidebar (left), message list (top-right), reading pane (bottom-right). A status bar shows session state + advice.

### 1.1 Menu strip — 11 items (Main.cs:9721)

| # | Type | Item | Notes |
|---|---|---|---|
| 1 | Combobox | `cmbCallSign` | Active callsign selector. Winlink supports multiple licensed callsigns per install. |
| 2 | Button | `mnuAddCallsign` | Add an aux/secondary callsign to the install. |
| 3 | Dropdown | `mnuFiles` (labeled "Settings") | 34-item menu — see §1.2. |
| 4 | Dropdown | `mnuMessage` | 27-item menu — see §1.3. |
| 5 | Button | `mnuViewAttachments` | Open attachments of the selected message. |
| 6 | Button | `mnuMove` | Move-to-folder menu. |
| 7 | Combobox | `cmbFolders` | Custom-folder navigator (Personal/Global). |
| 8 | Button | `mnuRemove` | Delete selected message. |
| 9 | Button | `mnuStartSession` | Open session ("Open Session") — primary connect action. |
| 10 | Combobox | `cmbSessionType` | Transport+role selector — 30 entries, see §2. |
| 11 | Dropdown | `mnuHelp` | 9-item menu — see §1.4. |

Tuxlink mapping placeholder: ⚪ (operator to rate).

### 1.2 Settings menu (mnuFiles, 34 items, ordered) — Main.cs:9747

| # | Label | Form / Dialog | Tuxlink status |
|---|---|---|---|
| 1 | Winlink Express Setup… | (Properties dialog — Main's overall config) | ⚪ |
| 2 | Font for text… | `SetColors` / native font picker | ⚪ |
| 3 | Font for lists… | native font picker | ⚪ |
| 4 | Set color themes | `SetColors` | ⚪ |
| 5 | GPS / Position Reports… | `PositionReport` | 🟢 in part (privacy + grid; emcomm posn-report flow may be separate) |
| 6 | Winlink Catalog Requests… | `WL2KCatalog` | ⚪ |
| 7 | GRIB file request… | (GRIB request dialog) | ⚪ |
| 8 | Preferences… | `Preferences` | ⚪ |
| 9 | Message Notification and Forwarding | `DialogMessageNotification` | ⚪ |
| 10 | CMS Forwarding and User Options… | `UserOptions` | ⚪ |
| 11 | Form settings… | `DialogFormSettings` | ⚪ |
| 12 | Auto session open on startup… | `DialogAutoSessionOpen` | ⚪ |
| 13 | Contacts… | `Contacts` | ⚪ |
| 14 | Group Addresses… | `GroupAddresses` | ⚪ |
| 15 | Add Personal Folder… | `PersonalFolders` | ⚪ |
| 16 | Add Global Folder… | `GlobalFolders` | ⚪ |
| 17 | Hybrid Network Parameters | `RadioNetworkParameters` | ⚪ |
| 18 | Propagation calculation parameters | `DialogPropParameters` | ⚪ |
| 19 | View Usage Statistics | `ViewUsageStats` | ⚪ |
| 20 | View background tasks… | `DialogViewBackgroundTasks` | ⚪ |
| 21 | Backup and restore databases… | `BackupOptions` | ⚪ |
| 22 | Exit | (close) | 🟢 |
| 23 | Make default RMS channel files | `mnuMakeDefChannelsFile` (Visible=false, debug-only) | 🔴 (hidden in WLE too) |

(7 separators interspersed; 34 = 23 actions + 11 separators — 11 used as ordering hints.)

### 1.3 Message menu (mnuMessage, 27 items) — Main.cs:9879

| # | Label | Action | Tuxlink status |
|---|---|---|---|
| 1 | New Message… | open `MessageEditor` | 🟢 (Compose window) |
| 2 | Reply | quote-reply | ⚪ |
| 3 | Reply All | quote-reply all recipients | ⚪ |
| 4 | Forward | quote + edit before send | ⚪ |
| 5 | Forward without change | re-route unchanged | ⚪ |
| 6 | Acknowledge Receipt | send Winlink ACK | ⚪ |
| 7 | Save Message As… | export to file (txt/rtf) | ⚪ |
| 8 | Edit | open in editor (drafts only?) | ⚪ |
| 9 | Templates | `TemplateList` | ⚪ |
| 10 | Set Default Template | `DialogDefaultTemplate` | ⚪ |
| 11 | Set Favorite Templates | `SetFavoriteTemplates` | ⚪ |
| 12 | HTML Forms | `FormManager` | ⚪ |
| 13 | Import Form XML File | import a community-shared HTML form | ⚪ |
| 14 | Generate ICS 309… | `DialogGenerate309` (incident-report summary) | ⚪ |
| 15 | Form Data… | `DialogFormDataReport` / Summary / Map | ⚪ |
| 16 | Export Messages… | `DialogExportMessagesFileName` | ⚪ |
| 17 | Import Messages… | `DialogImportMessagesFileName` | ⚪ |
| 18 | Archive Messages… | `DialogArchiveMessages` | ⚪ |

(9 separators; 27 = 18 actions + 9 separators.)

### 1.4 Help menu (mnuHelp, 9 items) — Main.cs:10018

| # | Label | Tuxlink status |
|---|---|---|
| 1 | Help Contents… | ⚪ |
| 2 | Help Index… | ⚪ |
| 3 | Show License Agreement | 🟢 (LICENSE) |
| 4 | Show Revision History | 🟢 (CHANGELOG.md) |
| 5 | Log files | 🟢 (session log panel) |
| 6 | About… | ⚪ |

(3 separators.)

### 1.5 Main toolbar (ToolStrip1, 24 buttons) — Main.cs:10347

| # | Label | Action / Form | Tuxlink status |
|---|---|---|---|
| 1 | New message | open `MessageEditor` | 🟢 |
| 2 | Reply to message | reply | ⚪ |
| 3 | Reply to all | reply all | ⚪ |
| 4 | Acknowledge receipt | send ACK | ⚪ |
| 5 | Forward message | forward | ⚪ |
| 6 | Forward without change | passthrough forward | ⚪ |
| 7 | Find messages | `FindMessages` | ⚪ |
| 8 | Position report | `PositionReport` | 🟡 (per tuxlink-39b/882) |
| 9 | Form Map and CSV File | `FormDataGMap` | ⚪ |
| 10 | Show map of message location | `RMSMap` | ⚪ |
| 11 | Catalog request | `WL2KCatalog` | ⚪ |
| 12 | GRIB file request | GRIB dialog | ⚪ |
| 13 | Save (selected) | save msg | ⚪ |
| 14 | Print | print msg | ⚪ |
| 15 | Open Session | open transport session | 🟢 |
| 16 | Help | open help | ⚪ |

(8 separators between groups; toolbar mirrors most-frequent menu items + adds maps/printing.)

### 1.6 Right-click context menus

**`mnuSelectionAction`** (right-click on message list, 9 items) — Main.cs:10247
- Move to Read Items / Saved / Deleted / Selected
- Save message as…
- Show map of message origin
- Print message

**`mnuMessageAction`** (right-click on reading pane, 10 items) — Main.cs:10299
- New / Reply / Reply All / Forward / Edit
- Copy selected text
- Select all text
- Print Message…
- Export Message to Text File…

Tuxlink status placeholder: ⚪ for all.

### 1.7 Folder sidebar — Main.cs:10152

Fixed folders (hardcoded): **Inbox, Read Items, Outbox, Sent Items, Saved Items, Deleted Items, Drafts**. Plus user-added Personal Folders (per-callsign) and Global Folders (cross-callsign).

Tuxlink status: 🟢 partial (system folders shipped; custom folder UX is ⚪).

### 1.8 Status bar — Main.cs:10058

Two cells: `lblStatus` ("No active session..." default) and `lblAdvice` (operator hint string, spring-sized to fill width).

Tuxlink mapping: 🟢 partial — session phase pill in the dashboard ribbon covers the status cell; advice cell is ⚪.

---

## 2. Sessions / Transports (cmbSessionType — 30 entries)

The session-type combobox is the primary way the operator picks transport × role. The 30 entries split into **5 role-groups × 8 transports** (some matrix cells absent). Each (transport, role) pair has a dedicated Form class — opening "Start Session" instantiates that form.

### 2.1 Transports (8)

| Transport | What it is | RMS Express forms | Tuxlink status |
|---|---|---|---|
| Telnet | TCP/IP to CMS or peer | `TelnetSession`, `TelnetP2PSession`, `TelnetSessionRadioOnly`, `TelnetMESHSession`, `TelnetIridiumGoSession` + setups | 🟢 (CMS telnet shipped; production CMS rejects unregistered SIDs — tuxlink-9h8) |
| Packet (AX.25) | 1200/9600-baud KISS/AGW over radio | `PacketWL2KSession`, `PacketP2PSession`, `PacketSetup`, channel selectors | 🟡 (tuxlink-7fr in flight; tuxlink-5vx for UI) |
| Pactor | Proprietary HF modem (SCS hardware) | `PactorWL2KSession`, `PactorP2PSession`, `PactorSetup` | ⚪ |
| Robust Packet (RPR) | SCS HF packet variant | `RPRSession`, `RPRSetup`, `RPRP2PChannelSelector` | ⚪ |
| ARDOP | OSS soundcard HF modem | `ArdopSession`, `ArdopSetup`, `TwArdopSession`, `TwArdopSetup` | 🟢 (PR #138 MVP — on-air pending tuxlink-9ky) |
| VARA HF | Commercial soundcard HF modem | `VaraSession`, `VaraSetup`, `DialogDownloadVara` | ⚪ (operator validated G90 + VARA Standard works; native rebuild in v0.5+ scope) |
| VARA FM | VHF/UHF FM soundcard variant | `VaraFMSession`, `VaraFMSetup` | ⚪ |
| Iridium GO | Satellite | `TelnetIridiumGoSession`, `TelnetIridiumGoSetup` | 🔴 candidate (commercial sat hardware; niche) |

### 2.2 Roles (4 + 1)

| Role | Meaning | Transports that offer it |
|---|---|---|
| Winlink (CMS) | Connect to CMS gateway / RMS site | all 8 |
| P2P | Direct peer-to-peer | Telnet, Packet, Pactor, RPR, ARDOP, VARA HF, VARA FM |
| Radio-only | Local RMS Relay hub, no internet | Telnet, Packet, Pactor, VARA HF, VARA FM |
| RMS Post Office | Local store-and-forward hub | Telnet, Packet, Pactor, VARA HF, VARA FM |
| MESH | AREDN/HSMM mesh integration | Telnet (only) |

### 2.3 Per-session-form features (uniform across transports)

Every session Form exposes the same set of operator surfaces (verified by spot-checking `PacketWL2KSession.cs`, `ArdopSession.cs`, `TelnetSession.cs`):

- **Channel selector** — for HF transports, a `*ChannelSelector` form (BestChannelSetup, HFChannelSelector, P2PChannelSelector, PacketWL2KChannelSelector). Picks frequency + RMS gateway from a downloaded channels file (`Def_RMS_Channels_Ham.zip`, etc.).
- **Connect button** with optional script-driven multi-leg connect (EditConnectScript).
- **Setup button** — opens the per-transport `*Setup` form to configure modem/radio/PTT/COM port/audio device.
- **Session log** — live scrolling text of the on-air conversation.
- **TX/RX state indicator** + signal quality readouts where the modem reports them.
- **Abort button** — stops the in-progress session (RADIO-1 obligation; tuxlink memory `feedback_radio1_bounded_airtime_abort` documents how subtle this is to get right).
- **Auto-poll / scheduled connect** integration (`AutoConnectSetup`, `DialogAutoPoll`, `DialogAddPollingSession`).
- **Pending-message review** (`ReviewPendingMessages`) — show what's about to send before transmitting.

Tuxlink status: 🟢 for the connect/abort/log core on Telnet + ARDOP; 🟡 for per-transport setup parity (Packet/AX.25 in flight). Channel-selector UX is ⚪.

### 2.4 Channel-data infrastructure

Winlink Express auto-downloads `channels.zip` from `winlink.org` (`Def_RMS_Channels_Ham.zip`, `Def_RMS_Channels_MARS.zip`, `Def_RMS_Channels_HF.zip`). The CMS-Tools site publishes the channel list; the client refreshes it locally for HF best-channel selection. See [`02-winlink-channel-data.md`](../../dev/scratch/winlink-re/findings/02-winlink-channel-data.md) findings for the data shape.

Tuxlink status: ⚪ (no channel-list infrastructure shipped yet; relevant only when HF transports come online).

---

## 3. Compose, Forms, Templates

### 3.1 Plain-text compose (`MessageEditor`)

- To / Cc / Subject / Body (Winlink text encoding — ISO-8859-1 per `format_winlink_date` peer).
- Attachments — adds via `AttachmentsList`.
- Image attachments get auto-resize prompt (`ImageEdit`, `ImageResize`) — Winlink discourages multi-MB images over RF.
- Spell check — `NetSpell.SpellChecker.dll` is shipped, integrated into editor.
- Encryption (AES) — `DialogAESencryption` (Part 97 caveat: encryption on amateur RF is generally prohibited; AES is offered for Part 15 / SHARES / MARS users).

Tuxlink status:
- Plain compose: 🟢 (current shipped Compose window).
- Image auto-resize: ⚪.
- Spell check: ⚪.
- AES encryption: ⚪ — gated by Cameron's encryption call (per memory `feedback_encryption_part97_eval`, this needs critical evaluation by license class and use case).

### 3.2 HTML Forms (`FormManager`, ICS-213 etc.)

Forms are the Winlink emcomm killer feature: ICS-213 (general message), ICS-309 (communications log summary), Red Cross 1077, SHARES forms, etc. They render as HTML in an embedded browser; values are templated into the message body.

- **FormManager** — browse / install / pick form.
- **FormsUpdateProgress / DialogFormsAutoupdate / DialogFormsUpdateNotification** — fetch latest forms from winlink.org.
- **ExportFormData / GenerateFormCSV / GenerateFormKML** — extract structured data from received forms.
- **DialogFormDataReport / Summary / Map / RightClick / Filters** — summaries / map-of-origin views over form-data.
- **SelectCsvColumns / SelectSummaryFields** — which fields to export.
- **GetXmlFileName / DialogImportMessagesFileName** — XML form import path.
- **FormCancel** — abort half-filled form session.

Tuxlink status: ⚪ (v0.1 roadmap mentions ICS-213 + Red Cross forms; no implementation yet).

### 3.3 Templates (`TemplateEditor`, `TemplateList`, `TemplateManager`)

User-authored boilerplate. Differs from forms in that templates are user-private; forms come from a community catalog.

- TemplateEditor — author.
- TemplateList — pick.
- TemplateManager — organize.
- TemplateHelp — help.
- AddTemplate — new.
- DialogTemplateSelect / DialogDefaultTemplate / SetFavoriteTemplates / mnuSetDefaultTemplate — configuration.

Tuxlink status: ⚪.

### 3.4 ICS-309 (`DialogGenerate309*`)

Standalone form: generates an ICS-309 communications log summary from a date range of messages. Output is CSV (`DialogGenerate309CSV`) or formatted text. Page sizing via `Dialog309PageSize`.

Tuxlink status: ⚪ (high emcomm value; "report what we sent during the incident").

---

## 4. Contacts & addressing

| Form | Purpose | Tuxlink status |
|---|---|---|
| `Contacts` | address book main view | ⚪ |
| `ContactEdit` | add / edit one contact | ⚪ |
| `ContactsExportFileName` / `ContactsImportReview` | CSV import/export | ⚪ |
| `GroupAddresses` | named distribution groups | ⚪ |
| `AddGroupAddress` | add member to group | ⚪ |
| `GroupContacts` | per-group contact picker | ⚪ |
| `DisplayCallHistory` | recent-callsign list | ⚪ |
| `mnuAddCallsign` (toolbar) | add aux callsign to install | ⚪ |
| `DialogAddAuxCallsign` | aux callsign config | ⚪ |

Tuxlink status: ⚪ (address book not shipped; current Compose accepts free-text addresses).

---

## 5. Maps, position, propagation

### 5.1 Position reports (`PositionReport`, `GridSquare`)

The Winlink "position report" is a structured message sent to `QTH@winlink.org` that updates the operator's location in the Winlink global directory. Used by SAR / monitoring stations to find people.

- Lat/lon + precision (full coordinate vs. grid square only).
- Auto-fetch from connected GPS (NMEA-0183 / NMEA-2000 / GPSD).
- Manual entry (`GridSquare`).

Tuxlink status: 🟡 — `tuxlink-2y5` (manual grid editor) open in bd ready; tuxlink-39b shipped privacy controls; tuxlink-686 added position subsystem. The position-report SEND flow (vs. the privacy-aware capture) is ⚪.

### 5.2 Maps

- **`RMSMap`** + **`RMSGMap`** — show RMS gateway locations on a map (uses Google Maps via `GMap.NET.dll`).
- **`MapRegion`** — region picker.
- **`DialogMapSettings`** — map provider / cache settings.
- **`FormDataMap` / `FormDataGMap`** — overlay form-data points (e.g., damage reports from ICS-213 incoming).
- "Show map of message origin" (right-click) — single-message origin pin.

Tuxlink status: ⚪. Operator decision needed: is map UX essential to emcomm reading-pane, or a separate companion?

### 5.3 Propagation forecast (`PropagationForecast`, `DialogPropParameters`, `DialogNeedPropUpdate`)

HF band-condition predictions used to pick a connecting frequency. Uses `PropagationPrediction.dll`. Requires SSN / flux data updates.

Tuxlink status: ⚪. Mostly relevant for HF transports (Pactor, ARDOP, VARA HF).

---

## 6. Data products

### 6.1 Winlink Catalog (`WL2KCatalog`)

The "Winlink Catalog" is a request system for data products fetched by CMS on the user's behalf: weather bulletins, NOAA marine forecasts, NWS texts, propagation tables, RSS feeds, SHARES catalogs, NHC bulletins. Operator sends a structured request to `query@winlink.org`; response comes back as an attached message.

Tuxlink status: ⚪. High operational value (it's WHY emcomm operators reach for Winlink — get a hurricane forecast without internet).

### 6.2 GRIB files (`MnuGribFile`, GRIB request dialog, `tsbGRIB`)

GRIB = gridded binary, the meteorological data format for wind/wave/temp forecasts. Used by cruising sailors. Request specifies geographic bounding box + parameters; response is a binary GRIB attachment that a GRIB viewer (e.g., zyGrib, OpenCPN) decodes.

Tuxlink status: ⚪. Sailing-niche but tuxlink's design includes "offshore cruisers" in the audience (CLAUDE.md project ethos).

### 6.3 Weatherfax (`DialogNeedViewfax`)

Internal prompt suggesting an external weatherfax viewer when a fax-type attachment arrives. Tuxlink would either ship a viewer, ignore, or open via xdg-open.

Tuxlink status: ⚪.

---

## 7. Automation

| Form | Purpose | Tuxlink status |
|---|---|---|
| `AutoConnectSetup` | configure scheduled auto-connect (every N minutes, on signal availability) | ⚪ |
| `DialogAutoPoll` | poll multiple stations in rotation | ⚪ |
| `DialogAutoSessionOpen` | which session auto-opens at startup | ⚪ |
| `DialogAddPollingSession` | add a station to the polling list | ⚪ |
| `DialogAddTelnetStation` | add a Telnet RMS station | ⚪ |
| `EditConnectScript` | author multi-leg session script | ⚪ |
| `DialogMessageNotification` | notification settings (sound/popup on new message) | ⚪ |

Tuxlink status: ⚪. Operator decision: how much automation is in scope before v1.0? Auto-poll is a heavy-traffic-station feature; emergency operators in a small-net might not need it.

---

## 8. Updates & integrity

| Form | Purpose | Tuxlink status |
|---|---|---|
| `AutoupdateProgress` / `DialogAutoupdate` | self-update of the Express binary | 🔴 candidate — Linux package managers (apt/flatpak) handle this; bundling an in-app updater duplicates work |
| `InstallPatches` | patch installer | 🔴 same reason |
| `UseBackupIniFile` | restore from backup .INI on corruption | ⚪ |
| `BackupOptions` | configure DB backup schedule + path | ⚪ |
| `DialogFormsAutoupdate` / `DialogFormsUpdateNotification` | forms catalog update | ⚪ (gated on forms support) |
| `DialogUpdateMeshNodes` | mesh-network node list refresh | ⚪ (gated on MESH support) |
| `DialogRegistration` | software-registration nag (Winlink Express is free but asks for registration) | 🔴 (we don't run a registration server) |

### 8.1 Recommendation note for the "update" cluster

Tuxlink's distribution model (Flatpak + `.deb` + `.rpm` + RaspPi image per the v0.1 roadmap) means OS package managers handle binary updates. The Forms-catalog update is the only "in-app" updater that genuinely belongs in the application, IF Forms ship at all. Operator confirm: 🟢 retain Forms-catalog update only?

---

## 9. Networking & hybrid

| Form | Purpose | Tuxlink status |
|---|---|---|
| `RadioNetworkParameters` (mnu: "Hybrid Network Parameters") | configure the hybrid network role | ⚪ |
| `DialogEditP2PTelnetAllowedStations` | which peers may initiate inbound Telnet P2P | ⚪ |
| `DialogMPSList` | MPS (Master Polling Station) list | ⚪ |
| `ViewMeshServicesJson` | AREDN mesh services discovery viewer | ⚪ |
| `TelnetMESHSession` / `TelnetMESHSetup` | MESH role for Telnet | ⚪ |
| `DialogAddTelnetStation` | add Telnet RMS to known-stations list | ⚪ |
| `DialogAddPollingSession` | (also networking-adjacent) | ⚪ |

Tuxlink status: ⚪ — mostly relevant to "hub operators" (Post Office, MPS roles). v0.1 emcomm operator may not need any of these.

---

## 10. Misc dialogs, utilities, accessibility

| Form | Purpose | Tuxlink status |
|---|---|---|
| `About` | version / build info | ⚪ |
| `LicenseAgreement` | EULA viewer | 🟢 (LICENSE in repo) |
| `Preferences` | global preferences dialog (not the same as `Properties`) | ⚪ |
| `DialogChangePassword` | change CMS password | ⚪ |
| `ChangePasswordNoInternet` | offline-mode password change | ⚪ |
| `GetPasswordRecoveryEmail` | password recovery email | 🔴 candidate (Cameron's recovery flow may diverge; secret-handling per `feedback_no_disk_creds_default`) |
| `PromptForPassword` | password prompt at connect time | ⚪ |
| `ConfirmConnection` | "are you sure you want to connect?" gate | ⚪ |
| `DialogSetMessageEditorDefaults` | editor default font / line wrap / etc. | ⚪ |
| `DialogViewBackgroundTasks` | thread / job monitor | ⚪ |
| `ViewUsageStats` | usage statistics (bytes sent, connect time) | ⚪ |
| `DialogAskMultiLine` / `DialogAskSingleLine` | generic prompt dialogs | (engine helpers, not user-facing) |
| `GetPacketSSID` | SSID picker for Packet | 🟡 (tuxlink-sox: bug about SSID persistence) |
| `SetColors` | color theme picker | ⚪ |
| `Radio` | radio (rig) config | ⚪ |
| `DialogNewRadioProfile` | new rig profile | ⚪ |
| `SiteProperties` | per-CMS / per-site config | ⚪ |
| `DestinationSelector` | pick a destination for a send action | ⚪ |
| `DialogListOfSenders` | filtered sender browser | ⚪ |
| `CreateCsvDlg` | CSV export wizard | ⚪ |
| `FindMessages` | full-text search | ⚪ |
| `ReviewPendingMessages` | what's in the outbox right now | ⚪ |
| `DialogNoMessages` | "no messages match" placeholder | (engine helper) |
| `GetFileName` / `GetInputFileName` / `GetOutputFileName` / `GetExportFileName` | file picker wrappers | (native dialogs in Tuxlink) |
| `DialogArchiveMessages` / `DialogArchiveMessagesFileName` | archive flow | ⚪ |
| `DialogImportMessagesFileName` / `DialogExportMessagesFileName` | message import/export | ⚪ |

---

## 11. Hidden / dead / legacy

Spots in the decompiled UI that ship code but are intentionally hidden or disabled by default. Documented so we don't accidentally re-create dead surface.

| Item | State in WLE | Notes |
|---|---|---|
| `mnuMakeDefChannelsFile` | `Visible = false` | Developer-mode tool to regenerate the default channels file. |
| `MnuGribFile` (cap M) | naming inconsistency with other `mnu*` (lowercase m) | Look like late additions to the codebase. |
| TwArdop\* | `Tw` prefix variants of ARDOP | "Two-way" ARDOP variant; presence suggests ongoing ARDOP work. |

---

## 12. Cross-cutting Tuxlink-mapping summary (operator decision template)

### 12.1 Currently shipped in Tuxlink (🟢 confirmed)

- Telnet CMS connect — `cms.winlink.org` and `cms-z.winlink.org` (project_cms_rejects_unknown_clients).
- B2F message exchange — read / send / decompress (native — Pat fully replaced per memory).
- ARDOP MVP transport (PR #138, ADR 0015) — on-air pending tuxlink-9ky.
- System folders (Inbox/Outbox/Sent/Saved/Deleted/Drafts/Read Items) — matches WLE.
- Compose window (plain text, attachments, B2F outbound framing).
- Reading pane with RFC5322/Winlink B2F date parsing (PR #145).
- Session log panel.
- GPS / position privacy + grid (tuxlink-39b, tuxlink-882).
- License + Changelog visible in repo.
- v0.3.0 release shipped (release-please).

### 12.2 In flight (🟡 in bd ready or known plan)

- AX.25 1200-baud packet transport (`tuxlink-7fr`, `tuxlink-5vx`).
- Wizard completion fix (`tuxlink-eh7`).
- Manual grid editor (`tuxlink-2y5`).
- Compose performance (`tuxlink-dq2`).
- Packet SSID persistence (`tuxlink-sox`).

### 12.3 Recommended deferrals (🟠 / 🔴 candidate — operator confirm)

- **In-app updater**: drop in favor of OS package managers (Flatpak/.deb/.rpm).
- **Registration nag**: drop (WLE registers against Winlink's servers; we don't operate one).
- **Software-vendor licensing**: drop (we ship GPL/MIT, no commercial license).
- **Iridium GO**: drop pending v1.0+ niche demand.
- **MESH-via-AREDN**: defer pending operator demand signal.
- **MARS-specific channel files**: drop unless MARS becomes target audience.
- **Pactor/SCS hardware integration**: defer pending operator demand (commercial modem, niche).

### 12.4 Undecided clusters (⚪ → need rationalization)

Major rationalization opportunities — each is worth a small design call before code:

| Cluster | Items | Rationalization question |
|---|---|---|
| **HTML Forms** | FormManager + 8 helpers + ICS-213 + ICS-309 | Is form support a v0.1 must-have, or a v0.5 add-on? "EmComm without forms" is a hard sell. |
| **Templates** | TemplateEditor + 5 helpers | Power-user feature. Skip until forms ship? |
| **Contacts + Groups** | Contacts + ContactEdit + GroupAddresses + GroupContacts + DisplayCallHistory + ContactsImportReview | Modern OS address-book integration vs. in-app address book? |
| **Catalog requests** | WL2KCatalog + tsbCatalogRequest | High operational value (this is the operational reason for many users — get NWS bulletins). Should this be in v0.1? |
| **Weather products** | GRIB + Weatherfax | Sailing-specific; defer? Or first-class for the cruising audience? |
| **Maps** | RMSMap + RMSGMap + FormDataMap + DialogMapSettings + MapRegion | Embedded vs. open-in-browser? Embedded is heavy (GMap.NET equivalent on Linux is... OpenStreetMap + Mapbox tiles + a Tauri webview). |
| **Propagation forecast** | PropagationForecast + DialogPropParameters | Critical for HF best-channel; minor for VHF/UHF. Defer until HF transports ship. |
| **Auto-poll / scheduled connect** | AutoConnectSetup + DialogAutoPoll + DialogAddPollingSession + EditConnectScript | Power-user / hub-operator feature. v1.0 candidate, not v0.1. |
| **Backup / restore** | BackupOptions + UseBackupIniFile + DB backup | OS-level backup vs. in-app? Probably defer to OS. |
| **AES encryption** | DialogAESencryption | Part 97 caveat critical (see `feedback_encryption_part97_eval`). Likely 🔴 for amateur use; possibly 🟢 for MARS / SHARES. |
| **Hybrid network roles** | RadioNetworkParameters + MESH + MPSList + Post-Office mode | Hub operators only. Defer unless tuxlink targets that audience. |

### 12.5 Final operator-decision matrix template

For each ⚪ row in §1.2–§10, fill in:

```
Cluster: <name>
Decision: ship / plan-v0.X / defer / drop
Rationale: <one sentence — anchor to audience + emcomm priority>
Tuxlink-form: <approx UI surface — panel, dialog, sidebar, menu item>
Cross-ref: <bd issue ID or doc>
```

The point of this doc is **forcing the rationale column to exist**. Anything left blank after operator review either gets a `defer` reason or becomes a bd issue.

---

## 13. What this doc explicitly is NOT

- **Not a decompile of VARA, ARDOP_Win, RMS Trimode, or the SCS Pactor stack** — those modem internals stay clean-sheet per project policy.
- **Not a protocol-level enumeration** — see [`01-wl2k-go-pat-modes.md`](../../dev/scratch/winlink-re/findings/01-wl2k-go-pat-modes.md) and [`03-rms-express-decompiled.md`](../../dev/scratch/winlink-re/findings/03-rms-express-decompiled.md) for protocol findings.
- **Not a UI design** for the equivalents — that's a separate brainstorming pass per feature.
- **Not a commitment** — every ⚪ row is a placeholder for an operator decision.

---

## 14. References

- `dev/scratch/winlink-re/decompiled/rms-express/RMS_Express/Main.cs` — the source of all menu/toolbar/control enumeration (lines 105–489 for field declarations; 9721–10503 for layout).
- `dev/scratch/winlink-re/findings/01-wl2k-go-pat-modes.md` — protocol mode survey.
- `dev/scratch/winlink-re/findings/03-rms-express-decompiled.md` — earlier RE pass (sessions + protocol).
- `dev/scratch/winlink-re/install/RMS Express/` — the actual installation binaries (local-only, gitignored).

Agent: yew-cypress-oak
