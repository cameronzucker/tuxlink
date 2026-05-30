# Winlink Express → Tuxlink — capability comparison + parity decisions

> Date: 2026-05-29 · Agent: yew-cypress-oak · bd: tuxlink-95z
> Rev: 2026-05-30 — restructured around capabilities (verbs) rather than menu items (nouns) per operator feedback on the first cut.

## 0. Purpose

This document is the **capability-gap audit** between Winlink Express (RMS Express) and Tuxlink. The goal is to make every omission a rationalized design choice, not an accidental gap — so the v0.1+ scope is defensible to the user base rather than "we just forgot."

**What changed in rev-2.** The first cut (rev-1) enumerated atomic UI elements — menus and dialogs by name. That made it hard to read across categories and obscured the actual capability gaps. This rev pivots to **what the operator can DO** (verbs), maps WLE → Tuxlink per capability, and assigns a status + rationale. The menu/UI-surface detail is demoted to an appendix for when "where does WLE put this?" matters.

**Source of truth.** Decompiled `RMS Express.exe` v(2025-03-13 build) at `dev/scratch/winlink-re/decompiled/rms-express/` — 259 `.cs` files, 145 Form classes. Per [`feedback_winlink_re_authoritative_sources`](../../README.md), the decompiled binary is the ground truth — prose docs are unreliable.

**Out of scope.** VARA modem internals (DSP/protocol) stay clean-sheet per `project_v05_modem_design_posture`. This audit is the **email-client capability surface**, not modem internals.

**Status codes.**
- 🟢 **shipped** — works today in Tuxlink (verifiable via `cargo run` + UI)
- 🟡 **planned** — in scope for v0.1 / v0.5 / v1.0 per current roadmap or bd ready queue
- 🟠 **deferred** — intentional later (post-v1.0 candidate), with rationale
- 🔴 **dropped** — intentional non-goal, with rationale
- ⚪ **undecided** — needs operator decision

---

## 1. Core messaging

The basics every email/messaging client provides. Where WLE and Tuxlink diverge in implementation, the table notes it.

| # | Capability | What it does | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 1.1 | **Send a plain-text message** | Compose To/Cc/Subject/Body and queue for next session | ✓ MessageEditor | 🟢 | Compose window shipped; B2F outbound framing verified (RFC5322 + Winlink encoding). |
| 1.2 | **Receive plain-text messages** | Decompress + parse + render inbound mail | ✓ MessageReader | 🟢 | Reading pane shipped; date parsing fixed in PR #145 (Winlink B2F format). |
| 1.3 | **Reply / Reply All** | Quote-reply with original headers preserved | ✓ | 🟢 | UI buttons shipped (MessageView.tsx) wired via `replyActions.buildReplyDraft` + `openReplyWindow`. |
| 1.4 | **Forward (with edit)** | Quote-forward to new recipients | ✓ | 🟢 | UI button shipped (`forward-btn`) → `fireReply('forward')`. |
| 1.5 | **Forward without change** | Re-route unchanged | ✓ | ⚪ | NOT shipped — `ReplyMode` only has `'reply' \| 'replyAll' \| 'forward'`. Specialized routing case; operator confirm if relevant for emcomm dispatch flows. |
| 1.6 | **Send and receive attachments** | Bundle binary files with messages | ✓ | 🟡 partial | Receive-side detected on parse; compose-side attach is ⚪. |
| 1.7 | **Image auto-resize on attach** | Prompt to shrink large images before send | ✓ ImageEdit + ImageResize | ⚪ | Bandwidth UX; relevant once attach-on-compose ships. |
| 1.8 | **Acknowledge receipt** | Send a Winlink read-receipt back | ✓ | ⚪ | NOT shipped (no UI surface; no "acknowledge" Tauri command in the message verbs). EmComm-relevant — "did the dispatch order arrive?" |
| 1.9 | **Spell check** | In-editor spell check | ✓ (NetSpell.dll) | ⚪ | Linux native: hunspell. Low priority. |
| 1.10 | **Save message as file** | Export single message to .txt/.rtf/.eml | ✓ | ⚪ | |
| 1.11 | **Print message** | Send to printer | ✓ | ⚪ | OS-level; Tauri webview → window.print() is one line. |
| 1.12 | **Drafts folder** | Save in-progress compose | ✓ | 🟢 | Shipped (compose has draft persistence). |
| 1.13 | **Custom folders (Personal / Global)** | User-created folder organization beyond Inbox/Outbox/etc. | ✓ PersonalFolders + GlobalFolders | ⚪ | Useful for filing by incident / net / topic. |
| 1.14 | **System folders** | Inbox / Outbox / Sent / Saved / Deleted / Read / Drafts | ✓ (7 fixed) | 🟢 | Shipped, same 7. |
| 1.15 | **Find / search messages** | Full-text + header search across folders | ✓ FindMessages | ⚪ | Essential for any non-trivial mailbox. |
| 1.16 | **Archive messages** | Move old messages out of active DB | ✓ DialogArchiveMessages | ⚪ | |
| 1.17 | **Export / import messages (.mbo / bulk)** | Bulk-move messages between installs | ✓ | ⚪ | Useful for cross-machine sync, donate-to-historian. |

**Headline gaps in this section**: Reply and Forward (basic edit) ARE shipped (corrected from rev-1 — buttons exist + are wired). The actual missing message-action verbs are **Acknowledge receipt** (1.8) and **Forward without change** (1.5) — both still ⚪. Find-messages (1.15) and custom folders (1.13) are non-blocking but matter for power-users in a long-running net.

---

## 2. Transports (how the client gets messages on/off the air)

WLE supports 8 transports in 4–5 role variations. The capability is "connect to a Winlink CMS gateway over `<radio system>`."

| # | Transport | Description | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 2.1 | **Telnet → CMS** | TCP/IP to internet-reachable CMS (cms.winlink.org) | ✓ | 🟢 | Telnet CMS shipped; production rejects unregistered SIDs (tuxlink-9h8). |
| 2.2 | **Telnet → P2P** | TCP/IP to another station directly | ✓ TelnetP2PSession | ⚪ | Direct station-to-station — bypasses CMS. |
| 2.3 | **Packet (AX.25) → CMS** | 1200/9600 baud AX.25 to RMS over VHF/UHF | ✓ PacketWL2KSession | 🟡 | tuxlink-7fr in flight; tuxlink-5vx for the UI. |
| 2.4 | **Packet → P2P** | AX.25 to another packet station | ✓ PacketP2PSession | 🟡 (same arc) | |
| 2.5 | **Pactor → CMS** | HF data via SCS Pactor hardware modem | ✓ PactorWL2KSession | ⚪ | Commercial hardware ($1000+); niche. |
| 2.6 | **ARDOP → CMS** | HF data via OSS soundcard modem | ✓ ArdopSession | 🟢 (MVP) | PR #138; on-air pending tuxlink-9ky BT page-timeout. |
| 2.7 | **ARDOP → P2P** | Direct ARDOP between stations | ✓ | ⚪ | |
| 2.8 | **VARA HF → CMS** | HF data via VARA commercial soundcard modem | ✓ VaraSession | ⚪ | Operator-validated G90 + VARA Standard works (project_g90_vara_standard_works_firsthand). v0.5+ scope (clean-sheet rebuild). |
| 2.9 | **VARA FM → CMS** | VHF/UHF FM data via VARA FM | ✓ VaraFMSession | ⚪ | |
| 2.10 | **Robust Packet (RPR) → CMS** | SCS HF packet variant | ✓ RPRSession | 🔴 candidate | Requires SCS hardware; minority transport. |
| 2.11 | **Iridium GO → CMS** | Satellite | ✓ TelnetIridiumGoSession | 🔴 candidate | Niche; commercial sat hardware. |
| 2.12 | **AREDN MESH → CMS** | Telnet over an AREDN mesh network | ✓ TelnetMESHSession | ⚪ | Specialized; operator confirm if mesh is target audience. |
| 2.13 | **Radio-only / RMS Relay** | Connect to a local RMS Relay hub (no internet to CMS) | ✓ (multiple variants) | ⚪ | Disaster-mode operation: cluster of stations relay to a hub that batches uploads when internet returns. |
| 2.14 | **Post Office mode** | Be the hub yourself (store-and-forward for others) | ✓ | ⚪ | Hub-operator feature, advanced. |
| 2.15 | **Best-channel selection (HF)** | Pick the right HF gateway frequency for current conditions | ✓ BestChannelSetup + HFChannelSelector | ⚪ | Requires propagation forecast + channel database. Critical for HF usability. |
| 2.16 | **Auto-poll / scheduled connect** | Connect on a schedule (every N min) or when signal is good | ✓ AutoConnectSetup + DialogAutoPoll | ⚪ | Power-user / unattended-station feature. |
| 2.17 | **Multi-leg connect script** | Try transports in order until one works | ✓ EditConnectScript | ⚪ | Power-user; helpful when the operator doesn't know what's working today. |

**Headline gaps**: HF transport story is currently ARDOP-only (and MVP-only). VARA HF and best-channel selection are the next two for HF parity with WLE. P2P (direct station-to-station) is unshipped across all transports — relevant for tactical comms without CMS / internet dependence.

---

## 3. Position & GPS

Winlink supports a "position report" — a structured message sent to `QTH@winlink.org` that updates the operator's location in the Winlink global directory. SAR teams use this to find people in the field.

| # | Capability | What it does | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 3.1 | **Manual grid entry** | Operator types a Maidenhead grid | ✓ GridSquare | 🟡 | tuxlink-2y5 open (P2). |
| 3.2 | **Auto-fetch from GPS device** | Read NMEA from a connected GPS | ✓ | 🟢 partial | tuxlink-686 added subsystem; tuxlink-39b privacy controls. |
| 3.3 | **Send a position report message** | Compose + send the structured Winlink position-report | ✓ PositionReport | ⚪ | The actual MESSAGE-SEND flow on top of position capture. |
| 3.4 | **Privacy / precision controls** | Limit broadcast location precision | (limited; full-precision default) | 🟢 | Tuxlink: 4-char Maidenhead default per project memory `feedback_gps_precision_reduction`. We're ahead of WLE here. |
| 3.5 | **Periodic auto-position** | Send a position report on a schedule | ✓ (configurable) | ⚪ | Operator confirm if this is in scope or operator-by-operator. |

**Headline gap**: the position-report SEND flow (capability 3.3) is the missing piece. Position capture + privacy is solid; we just don't have the "broadcast my location to Winlink" button. ~1 day of work.

---

## 4. Structured forms (HTML Forms + ICS-213 + ICS-309)

**This is the highest-leverage EmComm gap.** Forms are the operational reason for Winlink in emcomm — ICS-213 (general message), Red Cross 1077 (damage assessment), SHARES forms, etc. Without forms, Tuxlink is "Linux email over radio"; with forms, it's "the EmComm client."

| # | Capability | What it does | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 4.1 | **Render an inbound form** | Decode the XML in the message body, render structured fields in the reading pane | ✓ FormManager (embedded browser renders the form's HTML view template) | ⚪ (detected, not rendered) | `parse_raw_rfc5322` already sets `is_form = true` when body starts with `<?xml` — the rendering is missing. |
| 4.2 | **Author a new form (ICS-213 etc.)** | Fill out a structured form, serialize values to XML, send | ✓ FormManager (browse → pick → fill → send) | ⚪ | The compose-side. |
| 4.3 | **Catalog of community forms** | Browse + install community-contributed form definitions | ✓ FormManager + auto-update | ⚪ | WLE bundles dozens; refreshed from winlink.org periodically. |
| 4.4 | **Auto-update form catalog** | Pull latest form definitions periodically | ✓ DialogFormsAutoupdate | ⚪ | Tied to ship-form-support. |
| 4.5 | **Import custom XML form** | Side-load a non-catalog form | ✓ mnuImportFormXmlFile | ⚪ | |
| 4.6 | **Generate an ICS-309 log** | Build a Comms Log summary from a date range of sent/received messages | ✓ DialogGenerate309 (+ CSV variant) | ⚪ | Standard ICS form; usually filed at end-of-shift. High emcomm value. |
| 4.7 | **Form data report (across many incoming forms)** | Summary view across a stack of received forms | ✓ DialogFormDataReport + DialogFormDataSummary | ⚪ | Aggregator (e.g., "show all damage reports I received this week"). Advanced. |
| 4.8 | **Map view of form-data origins** | Plot incoming form locations on a map | ✓ FormDataGMap + FormDataMap + DialogFormDataMapFilters | ⚪ | Situational awareness for incident command. |
| 4.9 | **Export form data to CSV/KML** | Extract structured data for external tools | ✓ ExportFormData + GenerateFormCSV + GenerateFormKML | ⚪ | |
| 4.10 | **Templates (user-private boilerplate)** | Save reusable text patterns | ✓ TemplateEditor + TemplateList + TemplateManager | ⚪ | Distinct from forms — these are plain-text boilerplate, not structured. Low-priority compared to forms. |
| 4.11 | **Favorite-templates quick-pick** | Shortcut bar for top-N templates | ✓ SetFavoriteTemplates | ⚪ | Power-user. |

**Headline gap**: 4.1 (render inbound forms) is essential, 4.2 (author forms) is essential, 4.6 (generate ICS-309) is essential. 4.7–4.9 are advanced. 4.10–4.11 are non-essential.

**Build order recommendation for v0.1**:
1. **4.1 inbound render**: parse the XML body, render fields in the reading pane. We already detect `is_form`; render is the missing half.
2. **4.2 compose-side authoring**: a forms picker → fill → serialize → send. Start with ICS-213 (the most common form).
3. **4.6 ICS-309**: ship after 4.1+4.2 are stable. Pure local data extraction; doesn't need form-catalog infrastructure.
4. **4.3 / 4.4** (catalog + auto-update): later — start with a bundled set of canonical forms; defer dynamic catalog until needed.

---

## 5. Data products (Catalog / GRIB / Weatherfax / Propagation)

The "Winlink Catalog" is a request-response system run by CMS: send a structured request, get back a data product as an attached message. This is why offshore cruisers and EmComm operators use Winlink — get a hurricane forecast without internet.

| # | Capability | What it does | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 5.1 | **Request a Winlink Catalog data product** | Send a `query@winlink.org` request (e.g., NOAA marine forecast, NWS bulletin, propagation table, SHARES catalog) | ✓ WL2KCatalog | ⚪ | High operational value — this is THE reason many users adopted Winlink. |
| 5.2 | **Request a GRIB weather file** | Send a structured wind/wave forecast request; get binary GRIB back | ✓ MnuGribFile + tsbGRIB | ⚪ | Sailing-niche but in Tuxlink's stated audience (CLAUDE.md: "offshore cruisers"). |
| 5.3 | **Display Catalog / GRIB responses** | Render the returned data product | ✓ (catalog text in mail body; GRIB via external viewer) | ⚪ | For GRIB, "open in external viewer" (zyGrib / OpenCPN) is the WLE pattern. |
| 5.4 | **Weatherfax viewer prompt** | When a fax-type attachment arrives, suggest a viewer | ✓ DialogNeedViewfax | ⚪ | Could be xdg-open passthrough. |
| 5.5 | **HF propagation forecast** | Predict band conditions for HF connect | ✓ PropagationForecast (uses dvoa.dll + propagation parameters) | ⚪ | Needed for HF best-channel (capability 2.15). Defer until HF transports ship. |
| 5.6 | **Update propagation prediction data** | Refresh SSN / solar flux for propagation calc | ✓ DialogPropParameters + DialogNeedPropUpdate | ⚪ | Tied to 5.5. |

**Headline gap**: Catalog requests (5.1) are the highest-value missing data-product capability. Tuxlink would ship this as: "Send Catalog Request" → templated forms (NWS, NOAA, GRIB) → receive into Inbox as a regular message. ~2-3 days of work.

---

## 6. Maps & visualization

| # | Capability | What it does | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 6.1 | **Show RMS gateway map** | Visualize all gateway locations on a world map | ✓ RMSMap + RMSGMap (uses GMap.NET / Google Maps) | ⚪ | Operator-useful for "where can I connect from here?" Could be an external link to winlink.org/RMSChannels rather than embedded. |
| 6.2 | **Show per-message origin on map** | Pin the sender's reported QTH for one message | ✓ tsbShowMessageMap | ⚪ | Cute, low priority. |
| 6.3 | **Map of form-data origins** | Aggregate locations from received forms | ✓ FormDataGMap | ⚪ | See 4.8. |
| 6.4 | **Map provider config** | Choose Google / OSM / Bing / etc. | ✓ DialogMapSettings | ⚪ | If maps are in scope, OSM is the obvious Linux-native choice (no API key, no commercial tos). |
| 6.5 | **Region picker** | Pick a region for context-limited operations (e.g. propagation forecast) | ✓ MapRegion | ⚪ | |

**Headline gap**: Embedded maps are heavy infra — a webview tile renderer + caching + offline-tile management. Recommendation: **defer all embedded maps; provide deep-links to existing tools** (browser-based RMSChannels map, etc.) until concrete user demand emerges.

---

## 7. Address book & organization

| # | Capability | What it does | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 7.1 | **Address book** | Saved contact list with email/callsign + name | ✓ Contacts + ContactEdit | ⚪ | Alternative: integrate OS address book (KAddressBook / GNOME Contacts). |
| 7.2 | **Group / distribution list** | Named groups for multi-recipient sends | ✓ GroupAddresses + AddGroupAddress + GroupContacts | ⚪ | EmComm: dispatching to a whole ARES net at once. |
| 7.3 | **Import/export contacts (CSV)** | Move address book between installs | ✓ ContactsExportFileName + ContactsImportReview | ⚪ | Tied to 7.1. |
| 7.4 | **Recent-callsign history** | Quick-pick recently used callsigns | ✓ DisplayCallHistory | ⚪ | Compose UX. |
| 7.5 | **Multi-callsign install** | One install handles multiple licensed callsigns (cmbCallSign) | ✓ | ⚪ | Useful for clubs / served-agency operators. Tuxlink current: single-callsign. |
| 7.6 | **Add auxiliary callsign** | Add an emergency / served-agency callsign | ✓ DialogAddAuxCallsign | ⚪ | |

**Headline gap**: Address book + groups are the operator-essential org features. Multi-callsign is power-user.

---

## 8. Encryption (AES)

| # | Capability | What it does | WLE | Tuxlink | Notes |
|---|---|---|---|---|---|
| 8.1 | **AES message encryption** | Symmetric-key encrypted message body | ✓ DialogAESencryption | ⚪ | **Part 97 caveat critical** per `feedback_encryption_part97_eval`: encryption is generally prohibited on amateur RF. WLE offers it for SHARES/MARS users (Part 5 / non-amateur). Tuxlink decision depends on target audience and operator-of-record evaluation. |

---

## 9. Hub / network roles (MESH, Post Office, hybrid)

These are advanced "be a hub for others" features. WLE's design lets a single binary run as either a client or a hub.

| # | Capability | WLE | Tuxlink | Notes |
|---|---|---|---|---|
| 9.1 | Run as Post Office (local store-and-forward hub) | ✓ | ⚪ | Defer; hub operators are a tiny minority. |
| 9.2 | Run as Radio-only / RMS Relay hub | ✓ | ⚪ | Same. |
| 9.3 | Configure hybrid network parameters | ✓ RadioNetworkParameters | ⚪ | |
| 9.4 | MPS (Master Polling Station) list | ✓ DialogMPSList | ⚪ | |
| 9.5 | AREDN mesh services discovery | ✓ ViewMeshServicesJson | ⚪ | Mesh-specific. |
| 9.6 | P2P-Telnet allowed-stations list | ✓ DialogEditP2PTelnetAllowedStations | ⚪ | Security gate for inbound P2P. |
| 9.7 | Add Telnet RMS to known-stations | ✓ DialogAddTelnetStation | ⚪ | |

**Headline gap**: None for client-mode operation. The hub-role features are a separate product surface that probably warrants its own ADR before any of it ships. Recommendation: **drop hub-mode from v0.1+; revisit if user demand emerges.**

---

## 10. Diagnostics & operations

| # | Capability | WLE | Tuxlink | Notes |
|---|---|---|---|---|
| 10.1 | Session log (live + history) | ✓ | 🟢 | Shipped (session log panel). |
| 10.2 | Background-task viewer | ✓ DialogViewBackgroundTasks | ⚪ | Useful for diagnosing stuck connect attempts. |
| 10.3 | Usage statistics (bytes sent/received, connect time) | ✓ ViewUsageStats | ⚪ | Nice-to-have; low priority. |
| 10.4 | Log files viewer | ✓ mnuLogs | 🟢 partial | Session log panel covers active session; persistent log archive viewer is ⚪. |
| 10.5 | Backup / restore database | ✓ BackupOptions | ⚪ | OS-level backup might suffice (rsync user config dir). |
| 10.6 | Restore from backup .INI on corruption | ✓ UseBackupIniFile | ⚪ | Self-healing config. |
| 10.7 | Confirm-before-connect gate | ✓ ConfirmConnection | ⚪ | RADIO-1 alignment — defensive prompt before TX. Worth considering for safety. |

---

## 11. Lifecycle & integrity

| # | Capability | WLE | Tuxlink | Notes |
|---|---|---|---|---|
| 11.1 | Self-update binary | ✓ AutoupdateProgress + DialogAutoupdate | 🔴 | **Drop** — Linux package managers (Flatpak/.deb/.rpm) handle this; in-app updater duplicates work. |
| 11.2 | Patch installer | ✓ InstallPatches | 🔴 | Same. |
| 11.3 | Forms catalog update | ✓ DialogFormsAutoupdate | ⚪ | Only updater that genuinely belongs in-app, IF forms ship. |
| 11.4 | Mesh node list refresh | ✓ DialogUpdateMeshNodes | ⚪ | Tied to MESH support. |
| 11.5 | Registration nag | ✓ DialogRegistration | 🔴 | **Drop** — WLE registers against Winlink's servers; we don't operate one. |
| 11.6 | Change CMS password (online) | ✓ DialogChangePassword | ⚪ | |
| 11.7 | Change CMS password (offline) | ✓ ChangePasswordNoInternet | ⚪ | Sets next-connection password; takes effect on next CMS auth. |
| 11.8 | Password recovery email | ✓ GetPasswordRecoveryEmail | ⚪ | Secret-handling needs care per `feedback_no_disk_creds_default`. |
| 11.9 | License agreement display | ✓ LicenseAgreement | 🟢 | LICENSE file in repo. |
| 11.10 | Revision history display | ✓ ShowRevisionHistory | 🟢 | CHANGELOG.md in repo. |
| 11.11 | About box | ✓ About | ⚪ | Easy: version + commit-hash + build-time. |

---

## 12. UI/UX (color, fonts, etc.)

Minor surface; collected here so it doesn't pollute the bigger tables.

| # | Capability | WLE | Tuxlink | Notes |
|---|---|---|---|---|
| 12.1 | Color theme | ✓ SetColors | 🟢 (light/dark) | Tuxlink already has theme switching. |
| 12.2 | Font for text | ✓ mnuFont | ⚪ | OS-default may suffice. |
| 12.3 | Font for lists | ✓ mnuFontLists | ⚪ | |
| 12.4 | Message-editor defaults | ✓ DialogSetMessageEditorDefaults | ⚪ | |
| 12.5 | Notification preferences | ✓ DialogMessageNotification | ⚪ | OS-native notification API on Linux is easy via Tauri. |

---

## 13. Recommended v0.1 → v0.5+ slicing (operator-decision template)

This is the **rationalization spine**. Each row should ultimately land in one of these buckets with a one-sentence rationale.

### 13.1 Recommended **v0.1 must-have** (before declaring 1.0-of-the-shippable-MVP)

The features below are what makes Tuxlink "a working Winlink client," not "a fancy email reader."

| Capability ID | Why it's must-have | Effort |
|---|---|---|
| 1.5 Forward without change | EmComm dispatch flows depend on re-routing unchanged (e.g., relay an ICS-213 to another net without re-quoting). | S |
| 1.8 Acknowledge receipt | Winlink-native ACK — operationally significant in emcomm ("did the order arrive?"). | S |
| 4.1 Render inbound forms | We already detect `is_form`; rendering the XML is the missing half. Without this, every Service Advice + ICS-213 received is raw XML to the user. | M |
| 4.2 Author ICS-213 (and a small set of common forms) | Compose-side complement to 4.1. ICS-213 is the canonical EmComm general message. | M-L |
| 3.3 Position report SEND | Position capture is shipped; "broadcast my QTH to Winlink" is the missing message-send. | S |
| 1.15 Find messages | Without search a multi-incident mailbox is unusable. | M |

*(Reply / Reply All / Forward-with-edit are NO LONGER on this list — they were corrected from ⚪ to 🟢 in rev-2.0.1; buttons + handlers are shipped in MessageView.tsx.)*

**Estimated v0.1 completion path**: 1.3+1.4+1.5+1.8 (1-2 days) → 3.3 position-send (1 day) → 4.1 form-render (1-2 days) → 1.15 search (1-2 days) → 4.2 form-author (3-5 days). ~2 weeks of focused work.

### 13.2 Recommended **v0.5 must-have** (parity that distinguishes us)

| Capability ID | Why it's v0.5-priority |
|---|---|
| 2.8 / 2.9 VARA HF / FM transport (clean-sheet rebuild) | Already in the v0.5 plan. |
| 2.15 Best-channel selection (HF) | Critical for HF usability — operators don't want to manually scan. |
| 4.6 Generate ICS-309 log | End-of-incident reporting; standard expectation. |
| 5.1 Winlink Catalog requests | The "Winlink killer feature" for non-emcomm users (offshore + cruising). |
| 7.1 / 7.2 Address book + groups | Power-user comfort but not show-stopping pre-v0.5. |

### 13.3 Recommended **deferred** (no v1.0 commit)

| Capability ID | Why defer |
|---|---|
| 2.5 Pactor (SCS hardware) | Commercial $1000 modem; minority HF transport. |
| 2.10 RPR | Same — SCS hardware required. |
| 2.11 Iridium GO | Niche satellite hardware. |
| 2.12 AREDN MESH | Specialized network — confirm audience demand first. |
| 5.5/5.6 Propagation forecast | Tied to HF; revisit after HF transports work. |
| 6.x Maps (all embedded) | Heavy infra; deep-link to existing tools first. |
| 9.x All hub-role features | Separate product surface; confirm demand. |
| 4.7/4.8/4.9 Form aggregator + map + CSV/KML export | Power-user features atop the basic form support. |

### 13.4 Recommended **dropped** (intentional non-goals)

| Capability ID | Why drop |
|---|---|
| 11.1 / 11.2 In-app updater + patches | OS package managers handle this on Linux. |
| 11.5 Registration nag | WLE-specific; we don't operate a registration server. |
| 1.9 Spell check | Hunspell exists; OS-level integration is the right surface, not in-app. |
| 4.10 / 4.11 Templates | Plain-text boilerplate; forms cover the structured case; OS-level snippet managers handle the rest. |

### 13.5 **Operator decision required** (the meaty rationalization calls)

| Capability ID | What to decide | Why it's hard |
|---|---|---|
| 8.1 AES encryption | Ship for SHARES/MARS / drop entirely for Part 97 / ship with explicit "non-amateur use only" gate | Part 97 amateur-RF prohibition vs. SHARES/MARS where it's permitted; gate semantics matter. |
| 2.5 Pactor | Ship as TNC-config-only (defer modem integration) / fully drop | Audience question — do mariners with installed Pactor expect this? |
| 7.5 Multi-callsign | Ship / defer / drop | Clubs and served-agency operators want it; solo hams don't need it. |
| 5.2 GRIB requests | Ship (offshore audience) / defer (post-v0.1 niche) | "Offshore cruisers" are in the CLAUDE.md project ethos; how much weight? |
| 2.2 Telnet P2P + 2.4 Packet P2P + 2.7 ARDOP P2P | Ship as v0.1 / defer | P2P is "no CMS, no internet" — a real disaster-mode capability. |
| 2.13 Radio-only / RMS Relay role (client-side) | Ship as v0.1 / defer | Required for ARES/RACES nets that use a local hub. |

---

## 14. What this doc explicitly is NOT

- **Not a decompile of VARA, ARDOP_Win, RMS Trimode, or SCS Pactor** — modem internals stay clean-sheet.
- **Not a protocol-level enumeration** — see [`01-wl2k-go-pat-modes.md`](../../dev/scratch/winlink-re/findings/01-wl2k-go-pat-modes.md) and [`03-rms-express-decompiled.md`](../../dev/scratch/winlink-re/findings/03-rms-express-decompiled.md) (local-only).
- **Not a UI design** for the equivalents — each capability needs its own brainstorming pass before code.
- **Not a commitment** — every ⚪ row is a placeholder for an operator decision. The bd issues filed from this audit ARE commitments.

---

## Appendix A: WLE menu-surface reference (for "where does WLE put X?")

When a Tuxlink design decision needs to know "what does Winlink Express UI look like for this?", consult this section. It's condensed from rev-1.

### A.1 Settings menu (mnuFiles, labeled "Settings", 23 actions)

`Winlink Express Setup…` · `Font for text…` · `Font for lists…` · `Set color themes` · `GPS / Position Reports…` · `Winlink Catalog Requests…` · `GRIB file request…` · `Preferences…` · `Message Notification and Forwarding` · `CMS Forwarding and User Options…` · `Form settings…` · `Auto session open on startup…` · `Contacts…` · `Group Addresses…` · `Add Personal Folder…` · `Add Global Folder…` · `Hybrid Network Parameters` · `Propagation calculation parameters` · `View Usage Statistics` · `View background tasks…` · `Backup and restore databases…` · `Exit` · `Make default RMS channel files` (hidden)

### A.2 Message menu (mnuMessage, 18 actions)

`New Message…` · `Reply` · `Reply All` · `Forward` · `Forward without change` · `Acknowledge Receipt` · `Save Message As…` · `Edit` · `Templates` · `Set Default Template` · `Set Favorite Templates` · `HTML Forms` · `Import Form XML File` · `Generate ICS 309…` · `Form Data…` · `Export Messages…` · `Import Messages…` · `Archive Messages…`

### A.3 Help menu (mnuHelp, 6 actions)

`Help Contents…` · `Help Index…` · `Show License Agreement` · `Show Revision History` · `Log files` · `About…`

### A.4 Main toolbar (24 buttons, mostly mirrors of menu items)

`New message` · `Reply` · `Reply All` · `Acknowledge receipt` · `Forward` · `Forward without change` · `Find messages` · `Position report` · `Form Map and CSV File` · `Show map of message location` · `Catalog request` · `GRIB file request` · `Save` · `Print` · `Open Session` · `Help`

### A.5 Context menus

- **Right-click on message list** (`mnuSelectionAction`): Move to Read/Saved/Deleted/Selected · Save as · Show map of origin · Print
- **Right-click on reading pane** (`mnuMessageAction`): New · Reply · Reply All · Forward · Edit · Copy · Select all · Print · Export to text

### A.6 Per-session-form shape (uniform across all 8 transports)

- Channel selector (HF: best-channel browser; VHF: KISS port + freq).
- Connect / Abort buttons.
- Setup button (per-transport config Form).
- Live session log scrolling.
- TX/RX state indicator + signal-quality readouts (modem-dependent).
- Pending-message review before TX.

---

## Appendix B: Hidden / dead / legacy surface

| Item | State in WLE | Recommendation |
|---|---|---|
| `mnuMakeDefChannelsFile` | `Visible = false` | Developer-only tool; don't ship. |
| `MnuGribFile` (cap M) | Naming inconsistency vs. `mnu*` (lowercase) | Suggests late-addition code; cosmetic. |
| `TwArdop*` | `Tw` (two-way?) prefix variant of ARDOP | Watching for future significance; appears to be parallel ARDOP track. |

---

## Appendix C: References

- `dev/scratch/winlink-re/decompiled/rms-express/RMS_Express/Main.cs` — UI/toolbar/menu source (lines 105–489 field decls; 9721–10503 layout).
- `dev/scratch/winlink-re/findings/01-wl2k-go-pat-modes.md` — protocol mode survey.
- `dev/scratch/winlink-re/findings/02-winlink-channel-data.md` — channels database.
- `dev/scratch/winlink-re/findings/03-rms-express-decompiled.md` — earlier RE pass (sessions + protocol).
- `dev/scratch/winlink-re/install/RMS Express/` — actual install binaries (gitignored, local-only).

Agent: yew-cypress-oak
