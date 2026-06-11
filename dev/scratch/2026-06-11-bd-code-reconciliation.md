# bd ↔ code reconciliation — fine-toothed sweep

**Date:** 2026-06-11  ·  **Agent:** arroyo-tamarack-slate  ·  **bd issue:** tuxlink-ahnz
**Method:** 32 subsystem batches → per-issue verify (sonnet, read origin/main @ 5e87c827) → adversarial confirm (refute-by-default).
**Scope:** 362 issues (196 closed-today + 165 open/in-progress).

## Verdict totals (verify phase)

| matches | cant_tell | false_open | partial_wired | false_closed |
|--:|--:|--:|--:|--:|
| 319 | 8 | 31 | 1 | 3 |

**Flagged:** 35  ·  **Confirmed after refutation:** 33  ·  **Refuted away:** 2

> The 319 `matches` and 8 `cant_tell` (operator-smoke / on-air-only / subjective) need no action. Below are the 33 confirmed tracker↔code discrepancies, each with file:line evidence that survived an independent refutation attempt.

## 🔴 partial_wired — closed but NOT reachable in production (highest value) (1)

### tuxlink-0ye6 — radio-transport
**Proposed:** Reopen tuxlink-0ye6 (or file a successor issue) scoped to: (a) replace intent: 'cms' hardcodes in VaraRadioPanel.tsx lines 262 and 334 with mode.intent read from props, (b) add ARD to the p2p protocols list in sessionTypes.ts, and (c) implement or verify the Phase 5 shared RadioSessionPanel — or document a deliberate decision to ship the panels separately with per-panel intent derivation instead.

- **Why:** All three sub-claims from the original auditor independently confirmed. (1) VaraRadioPanel.tsx hardcodes intent: 'cms' at both vara_open_session (line 262) and modem_vara_b2f_exchange (line 334); the panel never reads mode.intent from props, only mode.kind. Both sites carry comments naming 'Phase 5 RadioSessionPanel' as the future fix. (2) RadioSessionPanel.tsx does not exist anywhere under src/ — the two separate panel files remain the only implementation. (3) sessionTypes.ts lines 65-81 show the p2p protocols list contains only PKT, TEL, VHF, VFM — ARD (ardop-hf) is absent, so ardop-hf+p2p is not selectable from the sidebar. The ArdopRadioPanel.tsx correctly uses mode.intent at line 603, but that combination is unreachable because the sidebar never offers it. The grounding-sweep close (PRs #360/#399) appears to have reconciled earlier phases as shipped without completing Phase 5.
- **Evidence:** VaraRadioPanel.tsx:262 — `intent: 'cms'` in vara_open_session invoke; VaraRadioPanel.tsx:334 — `intent: 'cms'` in modem_vara_b2f_exchange invoke; VaraRadioPanel.tsx:254,329 — comments confirm transitional hardcode pending Phase 5 RadioSessionPanel; find /src -name 'RadioSessionPanel*' returns nothing; sessionTypes.ts lines 65-81 — p2p protocols block has PKT/TEL/VHF/VFM only, no ARD; ArdopRadioPanel.tsx:603 — uses mode.intent correctly but ardop-hf+p2p is unreachable from sidebar

## 🟠 false_closed — closed but work not actually landed (3)

_Note: 7b0z and unb0 are stranded handoff-doc commits on worktree branches, tied to the existing recover-handoffs effort — doc recovery, not feature gaps._

### tuxlink-n2uz — radio-transport
**Proposed:** reopen — vu_dbfs is permanently None in production; STATUS-string VU parsing deferred per transport.rs:841

- **Why:** The original auditor is correct. vu_dbfs is permanently None in production. The Command::Status arm at transport.rs:841-843 is an explicit no-op with a comment deferring parsing to v2. The only non-None assignment of vu_dbfs in the entire codebase is a test fixture at modem_status.rs:1193 (vu_dbfs: Some(-18.0)), not reachable from any real ardopcf event path. The default initializer at modem_status.rs:254 sets vu_dbfs: None, and no apply_ardop_event_to_status branch ever writes to it. The issue required all five live meters including vu_dbfs; that field has never been wired from real events.
- **Evidence:** src-tauri/src/winlink/modem/ardop/transport.rs:841-843 — Command::Status arm is a no-op. src-tauri/src/modem_status.rs:254 — default initializer sets vu_dbfs: None. src-tauri/src/modem_status.rs:1193 — only non-None assignment is in a test fixture (connected_irs_roundtrips). No grep match for any `status.vu_dbfs =` assignment anywhere in src-tauri/src/.

### tuxlink-7b0z — docs
**Proposed:** Reopen tuxlink-7b0z. Cherry-pick e946d41b onto recover-handoffs (or directly onto main) to land the handoff doc, then close per the issue's stated criterion.

- **Why:** The issue's own notes state 'Close this issue once handoff lands on main + worktree disposed per ADR 0009.' Commit e946d41b (dev/handoffs/2026-06-04-birch-isthmus-lichen-pr399-clippy-fix.md) is confirmed accessible but is NOT an ancestor of origin/main and NOT an ancestor of origin/bd-tuxlink-xygm/recover-handoffs — verified via git merge-base --is-ancestor. The handoff file is absent from both branches per git ls-tree. The grounding-sweep rationale ('authored/committed; not ongoing work') is logically insufficient — the issue's explicit stated close condition is landing on main, not merely authoring. The original auditor's finding stands.
- **Evidence:** git merge-base --is-ancestor e946d41b origin/main → NOT ANCESTOR; git merge-base --is-ancestor e946d41b origin/bd-tuxlink-xygm/recover-handoffs → NOT ANCESTOR; git ls-tree origin/main -- dev/handoffs/ shows no birch-isthmus file; git ls-tree origin/bd-tuxlink-xygm/recover-handoffs -- dev/handoffs/ shows no birch-isthmus file; bd show tuxlink-7b0z notes: 'Close this issue once handoff lands on main + worktree disposed per ADR 0009'

### tuxlink-unb0 — docs
**Proposed:** Reopen tuxlink-unb0. Cherry-pick 1a5a172b onto recover-handoffs to land the handoff doc. No ongoing code work required beyond the cherry-pick.

- **Why:** Commit 1a5a172b (dev/handoffs/2026-06-03-pika-cedar-tanager-user-folders-complete.md) exists on origin/bd-tuxlink-unb0/session-end-handoff but is NOT an ancestor of origin/main and NOT an ancestor of origin/bd-tuxlink-xygm/recover-handoffs — confirmed via git merge-base --is-ancestor. The issue description explicitly states the intent: 'operator merges/cherry-picks when convenient.' That merge/cherry-pick never happened. The grounding sweep's rationale ('authored/committed; not ongoing work') is accurate as a description of the state but treats authorship as equivalent to landing on main — which it is not for a handoff-parking issue. The auditor's confidence-3 qualification is appropriate: the issue notes lack an explicit 'close once on main' criterion (unlike tuxlink-7b0z), so the grounding sweep made a defensible judgment call that still appears incorrect given the issue's stated purpose.
- **Evidence:** git merge-base --is-ancestor 1a5a172b origin/main → NOT ANCESTOR; git merge-base --is-ancestor 1a5a172b origin/bd-tuxlink-xygm/recover-handoffs → NOT ANCESTOR; git ls-tree on both branches shows no pika-cedar-tanager file; bd show tuxlink-unb0 description: 'operator merges/cherry-picks when convenient'; the purpose of the issue was to facilitate that landing, not merely to record authorship

## 🟡 false_open — open/in-progress but actually done (close these) (29)

### tuxlink-96lu — connection-cms
**Proposed:** Close tuxlink-96lu — the feature is fully implemented and merged to main. No code changes required.

- **Why:** Independent inspection confirms the original auditor's verdict. PR #587 merged to main as commit a8572de9. The feature is fully present in the audit worktree (origin/main): gridToNwsZone at src/request/geo.ts:179, gridToRadarRegion at geo.ts:201, location section constructed in src/request/sections.ts:108, and the 'For your location' hero rendered in RequestCenter.tsx:323-364. The bd issue body itself says 'Close on merge' — the merge is done, the issue simply was not closed. No contrary code evidence found.
- **Evidence:** git log shows a8572de9 'Merge pull request #587 from cameronzucker/bd-tuxlink-loc/location-hero'. src/request/geo.ts:179 exports gridToNwsZone, geo.ts:201 exports gridToRadarRegion. sections.ts:108 pushes the location section with title 'For your location'. RequestCenter.tsx:323 renders the '<h4>For your location</h4>' hero block with primary zone card (lines 332-362) and supporting locgrid (lines 364+). bd show tuxlink-96lu shows status OPEN with note 'Close on merge' — the merge predated this audit.

### tuxlink-37vi — connection-cms
**Proposed:** Close tuxlink-37vi — the fix shipped in PR #602.

- **Why:** The first auditor is correct. The fix is fully present in origin/main. Merge commit e9454c80 (PR #602, merged 2026-06-11) landed commit 0e41f2f8 'fix(catalog): Find-a-Station band selector covers the full amateur HF allocation (tuxlink-37vi)'. The worktree at /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ahnz-bd-recon-audit reflects this state. All three code artifacts cited by the auditor were independently verified: bandPlan.ts:10-35 defines all 10 HF bands (160m through 10m) with the comment 'prior 80/40/30/20 m subset was ALE-channel-shaped'; StationFinderControls.tsx:95 renders HF_BANDS tabs via map; bandPlan.test.ts:30-41 asserts HF_BANDS equals the full 10-band array. The bd issue status of in_progress is a stale tracker state — the code change it tracks is fully shipped to main.
- **Evidence:** git log shows e9454c80 'Merge pull request #602 from cameronzucker/bd-tuxlink-37vi/full-hf-bands' at HEAD~2 in the audit worktree. bandPlan.ts lines 10-35 define Band type with all 10 bands and HF_BANDS array ['160m','80m','60m','40m','30m','20m','17m','15m','12m','10m']. bandPlan.test.ts lines 29-42 assert HF_BANDS equals that full array. StationFinderControls.tsx line 95 maps HF_BANDS to tab elements. All files present at /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ahnz-bd-recon-audit/src/catalog/.

### tuxlink-7gb — connection-cms
**Proposed:** Close tuxlink-7gb — the docs refresh it tracked is complete. docs/development.md exists with native-client framing and no Pat-wrapping language.

- **Why:** The original auditor is correct. docs/development.md exists in the worktree and has been fully refreshed to native-client framing. The file states 'Tuxlink requires no Go toolchain' at line 9, lists only Rust/libax25-dev/libsecret-1-dev/Tauri as build deps, and contains zero Pat-wrapping language. The bd issue's own notes (from peregrine-maple-thistle, 2026-05-31) flagged that docs/development.md did not exist and proposed creating it with native-client framing — that creation has since happened. The issue remains OPEN in the tracker despite the work being complete.
- **Evidence:** /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ahnz-bd-recon-audit/docs/development.md line 9: 'Tuxlink is a native Rust + Tauri application. Tuxlink requires no Go toolchain.' The full 68-line file covers only Rust/Tauri prerequisites, AppImage builds, and keyring runtime requirements. grep for 'Pat', 'tuxlink-pat', 'Go 1.24', 'Tuxlink wraps' all return zero matches.

### tuxlink-ws45 — connection-cms
**Proposed:** Close tuxlink-ws45 — all deliverables are implemented and the tracker should reflect done.

- **Why:** Independent inspection confirms all five deliverables the first auditor cited are present and wired. (1) ICS-309 registered at src/forms/ics309/index.ts:8-13 with Form: Ics309FormV2. (2) Position registered at src/forms/position/index.ts:8-13 with Form: PositionFormV2. (3) Bulletin registered at src/forms/bulletin/index.ts:5-10 with Form: BulletinForm. (4) Damage Assessment registered at src/forms/damage_assessment/index.ts:9-13 view-only as operator-decided. (5) All four plus checkin appear in src-tauri/src/forms/catalog.rs:7-14 BUNDLED_FORMS and are imported in src/forms/index.ts:6-10. The Reply-with-form button is wired at MessageView.tsx:494-503 behind the hasReplyWithFormSupport + lookupForm gate, with replyActions.ts exporting the full ReplyMode union including 'replyWithForm' and a working buildReplyDraft implementation at line 150.
- **Evidence:** src/forms/ics309/index.ts:8-13 (Ics309FormV2 registered); src/forms/position/index.ts:8-13 (PositionFormV2 registered); src/forms/bulletin/index.ts:5-10 (BulletinForm registered); src/forms/damage_assessment/index.ts:9-13 (view-only, no Form key, per operator decision); src-tauri/src/forms/catalog.rs:7-14 (all 4 in BUNDLED_FORMS); src/forms/index.ts:6-10 (all 4 imported); src/mailbox/MessageView.tsx:489-503 (Reply-with-form button live behind gate); src/mailbox/replyActions.ts:20,133-160 (hasReplyWithFormSupport + buildReplyDraft replyWithForm branch)

### tuxlink-28y — shell-ui-chrome
**Proposed:** Close tuxlink-28y as obsolete. Pat sidecar was removed in PR #175 and no child-process reaping logic is needed.

- **Why:** The original auditor is correct. Pat sidecar was fully stripped in PR #175. bootstrap.rs:280 documents 'no Pat process, no blocking spawn, no sidecar.' The `BootstrapAction::Spawn` label is misleading: it calls `install_native()` (bootstrap.rs:272), which constructs a NativeBackend via `tauri::async_runtime::spawn` (an async task, not a child process). No `PatBackend`, `PatProcess`, or any child-process spawn path exists anywhere under src-tauri/src/. The bug condition — an in-flight Pat child process orphaning on quit — cannot occur because there is no child process. The issue is genuinely obsolete.
- **Evidence:** grep -rn 'PatBackend|PatProcess|pat_backend|pat_process|pat_sidecar' src-tauri/src/ → zero results. bootstrap.rs:271-273: BootstrapAction::Spawn(cfg) => { install_native(&app_handle, &state, *cfg); } — install_native() constructs NativeBackend only, no std::process::Child. bootstrap.rs:280: comment explicitly states 'no Pat process, no blocking spawn, no sidecar.'

### tuxlink-9dg — shell-ui-chrome
**Proposed:** Close tuxlink-9dg as complete. All three icon uses are implemented. The pending grim smoke is an operator-run validation step, not a blocker.

- **Why:** The original auditor is correct. All three described icon uses are fully implemented and wired in the codebase. (1) TitleBar.tsx:2 imports `../../assets/tuxlink-icon.png` and renders it at line 19 as `<img className='tux-app-icon' src={iconUrl} alt='' />`; TitleBar is mounted in AppShell.tsx:1004. (2) `assets/tuxlink_icon.png` (1.5 MB source) exists and is referenced in README.md:2. (3) `src-tauri/icons/` contains 32x32.png, 128x128.png, icon.ico, icon.icns, icon.png, and the full Windows/macOS tile set. The only remaining item noted in the issue ('in-app visual confirm in grim smoke') is an operator smoke step, not an implementation gap.
- **Evidence:** src/shell/chrome/TitleBar.tsx:2 — `import iconUrl from '../../assets/tuxlink-icon.png';`. TitleBar.tsx:19 — `<img className='tux-app-icon' src={iconUrl} alt='' />`. AppShell.tsx:106 — import; AppShell.tsx:1004 — `<TitleBar folderLabel={...} />`. src/assets/tuxlink-icon.png exists (22701 bytes). assets/tuxlink_icon.png exists (1594605 bytes). README.md:2 — `<img src='assets/tuxlink_icon.png' ...>`. src-tauri/icons/ contains 32x32.png, 128x128.png, icon.png, icon.ico, icon.icns.

### tuxlink-b2s — settings-identity
**Proposed:** Close tuxlink-b2s. The CI gap it described is fully addressed by ci.yml (no path filter, full frontend gate on every PR). No code change needed.

- **Why:** The original auditor is correct. ci.yml exists at /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ahnz-bd-recon-audit/.github/workflows/ci.yml with a pull_request trigger on main and feat/v0.0.1 and NO paths filter (lines 3-11). Every PR to those branches triggers the workflow unconditionally. The workflow runs pnpm typecheck (line 85), pnpm vitest run (line 88), and pnpm build (line 92) — exactly the frontend gate the issue requested. release.yml lines 11-13 additionally include src/** in its own pull_request paths filter. The gap described in tuxlink-b2s — frontend-only PRs triggering no CI — is closed. The issue is correctly classified as false_open and should be closed.
- **Evidence:** .github/workflows/ci.yml lines 3-11: on.pull_request.branches=[main, feat/v0.0.1] with no paths: key — fires on all PRs. Lines 84-92: pnpm typecheck, pnpm vitest run, pnpm build all present. .github/workflows/release.yml lines 11-13: pull_request.paths includes src/**. The issue body itself describes the fix as adding a frontend CI job triggered on src/** — ci.yml delivers that and more (no path filter means it fires even if only non-src files change, which is strictly more coverage than the issue asked for).

### tuxlink-5vx — radio-transport
**Proposed:** Close tuxlink-5vx. The P4 inline Radio UI is fully implemented and reachable in production. Tracker status is stale.

- **Why:** Independent inspection confirms the original auditor's claim. PacketRadioPanel.tsx is exactly 464 lines and fully implements all claimed affordances: SSID picker at lines 278-288 (select with ssidOptions(), onChange=onSsidChange), Listen arming at lines 300-363 (button with data-testid='packet-listen-btn', only rendered when intent==='p2p'), device/transport selector via ModemLinkSection at lines 248-262, and the Connect dial at lines 366-459 (FavoritesTabs + Start button invoking packet_connect). The component is mounted at AppShell.tsx:1233 under the condition `radioPanelMode.kind === 'packet'`, covering both cms+packet and p2p+packet intents. sessionTypes.ts line 25 has `{ ...PKT, built: true }` under the cms entry and line 71 has `{ ...PKT, built: true }` under the p2p entry. The bd-tuxlink-5vx/ax25-p4-ui-gap-survey branch has no substantive work on it — the P4 UI was delivered as part of the main radio-panel-redesign epic. The issue is genuinely done and the OPEN status in the tracker is stale.
- **Evidence:** PacketRadioPanel.tsx:278-288 (SSID select), :300-363 (Listen section, intent==='p2p' guard), :248-262 (ModemLinkSection transport selector), :366-459 (Connect section + Start button). AppShell.tsx:1231-1239 mounts PacketRadioPanel for radioPanelMode.kind==='packet' (all intents). sessionTypes.ts:25 cms entry PKT built:true, :71 p2p entry PKT built:true.

### tuxlink-b6ad — radio-transport
**Proposed:** Close tuxlink-b6ad. The Network PO send-flow alignment is fully implemented and merged via PR #601. Tracker status is stale.

- **Why:** Independent inspection confirms the original auditor's claim. PR #601 (merge commit 218705cd) is present in the tree with 6 subsequent commits on top, confirming it is merged into the main line. The backend po_drain_selection() function at ui_commands.rs:6214 returns None for local=false (network/mesh — drains all) and Some(selected) for local=true (Telnet RMS Post Office — keeps explicit selection). It is called at ui_commands.rs:6255 inside post_office_exchange via build_outbound_proposals. The frontend TelnetPostOfficeRadioPanel.tsx:804 renders the po-outbox-section checklist only when mode==='local'; the else branch at line 856 renders data-testid='po-network-send-note' for network mode. Tests at TelnetPostOfficeRadioPanel.test.tsx:172-189 verify network mode has no po-outbox-section and local mode retains it. Backend unit tests at ui_commands.rs:9760-9770 verify po_drain_selection semantics for both paths. The issue is IN_PROGRESS in the tracker but the work is completely merged.
- **Evidence:** ui_commands.rs:6214-6223 (po_drain_selection — None for network, Some for local), :6255 (call site inside post_office_exchange). TelnetPostOfficeRadioPanel.tsx:804 (mode==='local' renders po-outbox-section), :856 (network renders po-network-send-note). TelnetPostOfficeRadioPanel.test.tsx:172-189 (network/local conditional tests). git log shows 218705cd merged PR #601 with 6 later commits on top.

### tuxlink-ns7k — radio-transport
**Proposed:** Close tuxlink-ns7k as a duplicate of tuxlink-u1r7. All 4 P2 fixes shipped under tuxlink-u1r7 (PR #399, merged to origin/main). No separate implementation work is needed.

- **Why:** Independent inspection confirms: all 4 P2 fixes described in tuxlink-ns7k are implemented in the codebase and attributed to tuxlink-u1r7 (CLOSED). tuxlink-ns7k remains OPEN in the tracker but describes identical work. The original auditor's duplicate-issue characterization is correct. No distinct implementation exists under the ns7k issue ID — zero code references to ns7k were found outside a scratch audit batch file.
- **Evidence:** P2#1 (stop-state clear): vara/commands.rs:1424-1437 — abort_writer, abort_stream, transport_owner cleared in stop_session_inner, comment reads 'Codex Phase 3-4 boundary P2 #1 (tuxlink-u1r7)'. P2#2 (b2f payload widening): vara/commands.rs:1504, 1533-1547 — modem_vara_b2f_exchange accepts full SessionIntent+TransportKind, with VaraHf/VaraFm validation, comment reads 'P2 #2 — tuxlink-u1r7'. P2#3 (HF/FM arming): ui_commands.rs:4557-4580, 4625-4628 — arm_vara_listener_inner takes transport_kind parameter, validates VaraHf|VaraFm, records operator-supplied transport_kind in arms log, comment reads 'Codex Phase 3-4 boundary P2 #3 (tuxlink-u1r7)'. P2#4 (DTO wire-in): vara/commands.rs:334, 326-329, 352-375 — listener_armed wired from TransportOwner state, exchange wired from current_exchange(), comment reads 'Codex Phase 3-4 boundary P2 #4 — tuxlink-u1r7'. tuxlink-u1r7 close reason explicitly states 'shipped PR #399 — full VARA P2 sweep'. grep for 'tuxlink-ns7k' in the worktree returns only dev/scratch/bd-audit-batches.json — no code references.

### tuxlink-syqb — radio-transport
**Proposed:** Close tuxlink-syqb — implemented end-to-end via tuxlink-61yg (PR #344).

- **Why:** Independent code inspection confirms the full ARDOP listener pipeline is present and wired. ardop_listen_inner at ui_commands.rs:3824 sends LISTEN TRUE via session.send_listen_command(true) (line 3915) when the modem is already running idle, or starts it in listen-only mode otherwise. It then spawns ardop_listener_consumer_task (lines 3945-3956) with a live mailbox Arc. The consumer task loops on transport.wait_for_listener_connect(), calls listener_decide_at (the doc comment at 3790 names gate_inbound_peer_now as a description of the gate function — the actual implementation uses listener_decide_at, which is functionally identical), and on ListenerDecision::Accept calls run_ardop_b2f_answer with the real mailbox (lines 4166-4174). Reject paths send DISCONNECT and append a forensics log entry (lines 4227-4251). ArdopRadioPanel.tsx:343 wires the 'ardop_listen' Tauri command through useListenerState. The feature is end-to-end implemented; the issue tracker should be closed.
- **Evidence:** src-tauri/src/ui_commands.rs:3824 (ardop_listen_inner), :3914-3917 (LISTEN TRUE side-channel), :3945-3956 (consumer task spawn), :4068-4283 (ardop_listener_consumer_task body including listener_decide_at gate and run_ardop_b2f_answer calls at lines 4144-4174); src/radio/modes/ArdopRadioPanel.tsx:341-344 (useListenerState with ardop_listen command); bd show tuxlink-9ls2 close_reason references PR #344 shipping this work.

### tuxlink-95g8 — radio-transport
**Proposed:** Close tuxlink-95g8 — superseded by tuxlink-syqb, which shipped in full via tuxlink-61yg (PR #344).

- **Why:** The bd issue description itself explicitly records the supersession: 'The LISTEN-flag-flip portion landed in tuxlink-7vea; this issue replaces tuxlink-95g8 as the focused successor for the CONNECTED + B2F routing piece.' The successor issue (tuxlink-syqb) was then shipped in full via tuxlink-61yg (PR #344), as confirmed by the code inspection above. The original issue's entire scope — LISTEN TRUE, CONNECTED routing, B2F answerer handoff, mailbox persistence — is implemented at ui_commands.rs:3780-4283. The issue was superseded and then its successor was implemented; the tracker shows it OPEN in error.
- **Evidence:** bd show tuxlink-95g8 description paragraph starting 'Closes: tuxlink-95g8 was filed...' confirming it was superseded by tuxlink-syqb; tuxlink-syqb itself confirmed implemented at ui_commands.rs:3824-4283 (see tuxlink-syqb findings above).

### tuxlink-k3ru — radio-transport
**Proposed:** Close tuxlink-k3ru — implemented at src-tauri/src/winlink/telnet_listen.rs:529-593; shipped via PR #344.

- **Why:** Independent code inspection of telnet_listen.rs confirms the full inbound-mail symmetry is implemented. The closing comment at line 531 reads 'Tuxlink-7vea Telnet inbound-mail symmetry (closes tuxlink-k3ru)'. Lines 542-551 load the operator Outbox via build_outbound_proposals before opening the B2F exchange. Lines 574-593 persist result.received messages to MailboxFolder::Inbox and move result.sent MIDs from Outbox to Sent. The Mailbox Arc is plumbed from ui_commands.rs:6882-6906 (the telnet_listen Tauri command builds the on-disk mailbox and passes it through run_accept_loop). All three deliverables from the issue description — Outbox drain, Inbox persist, Outbox→Sent move — are present.
- **Evidence:** src-tauri/src/winlink/telnet_listen.rs:531 (close comment), :542-551 (outbox drain via build_outbound_proposals), :574-593 (inbox persist + outbox→sent move); src-tauri/src/ui_commands.rs:6882-6906 (mailbox plumbing into telnet_listen command) and :6916 (run_accept_loop call with mailbox arg).

### tuxlink-xnoy — radio-transport
**Proposed:** Close tuxlink-xnoy — VARA listener fully implemented; scope absorbed into tuxlink-9ls2 (PR #348).

- **Why:** Independent code inspection confirms the VARA listener is fully implemented. vara/listener.rs exists with set_listen (sends LISTEN ON/OFF), serve_inbound_one (waits for CONNECTED, runs gate, accepts or disconnects), and decide_for_vara_event. arm_vara_listener_inner at ui_commands.rs:4557 sends LISTEN ON via vara_session.send_listen_on() (line 4635-4639) and spawns vara_listener_consumer_task (lines 4669-4684). The consumer task loops on serve_inbound_one and calls run_vara_b2f_answer (winlink_backend.rs:2821) on Accept (ui_commands.rs:4861-4868). VaraRadioPanel.tsx:133-134 wires 'vara_listen' and 'vara_set_listen'. lib.rs:637-638 registers both Tauri commands. tuxlink-9ls2 close_reason explicitly states 'dep xnoy scope absorbed' when PR #348 merged. The VARA listener deliverables from the issue description (command::Listen invoked, CONNECTED handler, Tauri commands, backend wiring) are all present.
- **Evidence:** src-tauri/src/winlink/modem/vara/listener.rs (set_listen, serve_inbound_one, decide_for_vara_event); src-tauri/src/ui_commands.rs:4557-4684 (arm_vara_listener_inner + vara_listener_consumer_task), :4861-4868 (run_vara_b2f_answer call); src-tauri/src/winlink_backend.rs:2821 (run_vara_b2f_answer definition); src/radio/modes/VaraRadioPanel.tsx:133-134 (vara_listen/vara_set_listen wiring); src-tauri/src/lib.rs:637-638 (command registration); bd show tuxlink-9ls2 close_reason ('dep xnoy scope absorbed').

### tuxlink-cyt — radio-transport
**Proposed:** close — Pat modules, binary, submodule, and Go build path all stripped; only deprecated schema-compat field remains per ADR 0016

- **Why:** The original auditor is correct. The Pat Rust modules (pat_client, pat_process, pat_config) do not exist. live_cms_smoke.rs does not exist in src-tauri/src/ or bin/. There is no external/ directory, no .gitmodules, and no Go build path in build.rs (only git SHA and rustc version env vars). The bootstrap.rs and wizard.rs files are native tuxlink modules (app-start bootstrap and onboarding wizard) — not Pat code. The only remaining Pat artifact is config.rs:196-200's #[deprecated] pat_mbo_address field, intentionally kept for schema migration compatibility per ADR 0016, which the issue itself notes as acceptable.
- **Evidence:** find returns no pat_client.rs, pat_process.rs, pat_config.rs, or live_cms_smoke.rs. No .gitmodules, no *.go files, no Go build path in src-tauri/build.rs. src-tauri/src/bootstrap.rs:1 — '//! App-start bootstrap: the decision logic + the .setup() worker.' src-tauri/src/wizard.rs:1 — '//! Wizard backend — Tauri commands + state machine error/outcome types.' src-tauri/src/config.rs:195-200 — #[deprecated] pat_mbo_address field with note confirming intentional retention for schema compat.

### tuxlink-xfo — radio-transport
**Proposed:** close — x86/ARM notice implemented in VaraRadioPanel.tsx:378-393 via platformBlocked check

- **Why:** The original auditor is correct. The x86/ARM notice is fully implemented. VaraRadioPanel.tsx loads platform_info on mount (lines 164-167), computes platformBlocked at line 197, and renders the informational banner at lines 378-393 with data-testid='vara-platform-banner'. The Tauri-side platform_info command in vara/commands.rs:969-976 sets vara_supported via cfg!(any(target_arch = 'x86', target_arch = 'x86_64')), which evaluates false on ARM/Pi at compile time. Tests at VaraRadioPanel.test.tsx:260-281 confirm the banner renders when platform_info returns varaSupported: false.
- **Evidence:** src/radio/modes/VaraRadioPanel.tsx:164-180 — platform_info invoked on mount. VaraRadioPanel.tsx:197 — platformBlocked = platform !== null && !platform.varaSupported. VaraRadioPanel.tsx:378-393 — banner rendered when platformBlocked. src-tauri/src/winlink/modem/vara/commands.rs:969-976 — platform_info Tauri command with vara_supported: cfg!(any(target_arch = 'x86', target_arch = 'x86_64')). src/radio/modes/VaraRadioPanel.test.tsx:64 (armPlatform fixture) + line 260-281 (ARM banner test).

### tuxlink-23v — radio-transport
**Proposed:** close — effective_ui_locator and ui_grid field implemented end-to-end; position/mod.rs:68, ui_commands.rs:5284, useStatus.ts:423

- **Why:** The original auditor is correct. The LocalUiOnly live-GPS display feature is fully implemented and wired end-to-end. effective_ui_locator at position/mod.rs:68-91 returns the live precision-reduced GPS fix under LocalUiOnly when a fresh fix is available, without affecting the broadcast path. PositionStatusDto at ui_commands.rs:5274-5296 carries both broadcast_grid and ui_grid, populated from effective_broadcast_locator and effective_ui_locator respectively. useStatus.ts:423-424 reads positionStatus?.ui_grid for the ribbon's liveGrid display. The two fields diverge correctly under LocalUiOnly: ui_grid shows live GPS, broadcast_grid returns the config-derived grid.
- **Evidence:** src-tauri/src/position/mod.rs:68-91 — effective_ui_locator returns live arbiter.broadcast_grid() for source=Gps + LocalUiOnly/BroadcastAtPrecision + fresh fix. src-tauri/src/ui_commands.rs:5274-5296 — PositionStatusDto with ui_grid field, position_status command sets ui_grid: crate::position::effective_ui_locator(&cfg, Some(&arbiter)). src/shell/useStatus.ts:423-424 — const liveGrid = positionStatus?.ui_grid ? positionStatus.ui_grid ...

### tuxlink-gheo — mailbox
**Proposed:** Close tuxlink-gheo — fix is shipped and tested.

- **Why:** The fix is independently confirmed in the worktree. `substitute_template` at `http_server.rs:372` now emits `.replace("{FormFolder}", "/folder")` — the prior encoded-folder-name suffix is gone. `folder_handler` at lines 860-897 treats the full `rest` capture as the file path relative to `template_folder_path` with no `splitn` stripping. The fix description at lines 847-853 explicitly cites `bd tuxlink-gheo`. The bd tracker still shows OPEN; the code is fixed.
- **Evidence:** src-tauri/src/forms/http_server.rs:372 — `.replace("{FormFolder}", "/folder")` (bare path, no encoded folder name). Lines 847-853 doc comment cites tuxlink-gheo and explains the splitn removal. Tests at lines 1295-1300 (folder_route_serves_file_in_nested_folder) and 1515-1520 (substitute_template_emits_bare_folder_root) both cite tuxlink-gheo and verify the fix.

### tuxlink-4g2n — mailbox
**Proposed:** Close tuxlink-4g2n — fix is shipped and tested.

- **Why:** The fix is independently confirmed. `const MAX_FOLDER_ASSET_BYTES: usize = 8 * 1_048_576` at line 88. `folder_handler` at lines 879-896 checks `!md.is_file()` (rejects non-regular files) then `md.len() > MAX_FOLDER_ASSET_BYTES` (returns 413) before any `std::fs::read`. Comment at line 879 cites `tuxlink-4g2n`. The bd tracker still shows OPEN; the code is fixed.
- **Evidence:** src-tauri/src/forms/http_server.rs:88 — `const MAX_FOLDER_ASSET_BYTES: usize = 8 * 1_048_576`. Lines 879-896 — `match std::fs::metadata(&canonical)` with `!md.is_file()` → 403, `md.len() > MAX_FOLDER_ASSET_BYTES` → 413. Comment at line 879 cites tuxlink-4g2n. Test at lines 1340-1351 (label `bd tuxlink-4g2n regression`) verifies the cap.

### tuxlink-m2o6 — mailbox
**Proposed:** Close tuxlink-m2o6 — Option A label disambiguation is shipped.

- **Why:** Label disambiguation is independently confirmed. `Ics213Form.tsx:122-124` uses 'Addressee (name and position)' (to_name), 'Originator (name and position)' (fm_name), and 'Form subject' (subjectline) — none collide with the compose window's envelope 'To'/'From'/'Subject' labels. `BulletinForm.tsx:109-110` uses 'For (Recipient)' (name) and 'Bulletin From' (from_name). Wire-format field IDs are unchanged. This is Option A from the issue description. The bd tracker still shows OPEN; the code is fixed.
- **Evidence:** src/forms/ics213/Ics213Form.tsx:122 — `Addressee (name and position)`, line 123 — `Originator (name and position)`, line 124 — `Form subject`. src/forms/bulletin/BulletinForm.tsx:109 — `For (Recipient)`, line 110 — `Bulletin From`.

### tuxlink-2y5 — mailbox
**Proposed:** Close tuxlink-2y5 — config_set_grid backend and GridEdit frontend are both shipped and wired.

- **Why:** The full manual grid-edit feature is independently confirmed end-to-end. `src/shell/GridEdit.tsx` exists. `DashboardRibbon.tsx:179-196` mounts `GridEdit` and calls `invoke('config_set_grid', { grid: g })` on commit. `ui_commands.rs:5117` declares `pub async fn config_set_grid(...)`. `lib.rs:643` registers it in the Tauri command handler list. The bd tracker still shows OPEN; the feature is shipped.
- **Evidence:** src/shell/GridEdit.tsx — file exists. src/shell/DashboardRibbon.tsx:179-196 — `<GridEdit ... onCommit={async (g) => { await invoke('config_set_grid', { grid: g }); ... }}>`. src-tauri/src/ui_commands.rs:5117 — `pub async fn config_set_grid(...)`. src-tauri/src/lib.rs:643 — `crate::ui_commands::config_set_grid,`.

### tuxlink-msr — mailbox
**Proposed:** Close tuxlink-msr — ComposeTitleBar replaces duplicate chrome; fix is shipped.

- **Why:** The duplicate chrome removal is independently confirmed. `ComposeTitleBar.tsx:9` comment reads 'Dark title bar for the compose window (tuxlink-ng3 / closes msr). No menu'. `Compose.tsx:791` comment reads 'Custom title bar (tuxlink-ng3: decorations:false, closes msr)'. `Compose.tsx:796` comment reads 'the duplicate in-form header was removed — ComposeTitleBar is the single title bar + close, tuxlink-ng3 smoke #4'. The compose window renders `<ComposeTitleBar onClose={handleRequestClose} />` at line 793 with no additional duplicated main-window chrome. The bd tracker still shows OPEN; the fix is shipped.
- **Evidence:** src/compose/ComposeTitleBar.tsx:9 — '(tuxlink-ng3 / closes msr)'. src/compose/Compose.tsx:791 — '(tuxlink-ng3: decorations:false, closes msr)'. Compose.tsx:796 — 'the duplicate in-form header was removed — ComposeTitleBar is the single title bar + close'. Compose.tsx:793 — `<ComposeTitleBar onClose={handleRequestClose} />`.

### tuxlink-hxia — mailbox
**Proposed:** Close tuxlink-hxia as moot. Option (a) was executed; the conditional that would have made this issue actionable was never triggered. No code change needed.

- **Why:** Independent inspection confirms the first auditor is correct. tuxlink-4ai0 was closed via PR #395 having chosen option (a): full WLE schema alignment. The issue's own condition ('If the operator picks option (b)... this issue is moot') was never triggered. The CheckInForm is actively registered (import is live, not commented out), buildPayload() emits all WLE keys, and the wire-format alignment test explicitly asserts every required key. There is no residual gap — the finding survives the refutation attempt.
- **Evidence:** src/forms/index.ts:17 — `import './checkin'` is uncommented and active. src/forms/checkin/index.ts:1,5 — imports CheckInForm and calls registerForm(). src/compose/CheckInForm.tsx:209-233 — buildPayload() returns {organization, newsubject, exercise_id, datetime, msgto, msgsender, contactname, assigned, status, service, band, session, location, maplat, maplon, mgrs, grid, locationsource, comments, templateversion, mapfilename}. src/compose/CheckInForm.test.tsx:142-186 — 'onSubmit payload emits every key in checkin.rs::FIELDS' test asserts all 21 keys present plus concrete value spot-checks. tuxlink-4ai0 CLOSED with reason: 'shipped — dedicated branch merged via PR #395'.

### tuxlink-urbv — catalog-station-map
**Proposed:** Close tuxlink-urbv — the feature is fully shipped. Run: bd close tuxlink-urbv

- **Why:** Independent code inspection confirms the original auditor's finding. The feature is fully implemented and wired in the production render tree. GridPickerOverlay.tsx (102 lines) is a complete in-app overlay wrapping GridMapPicker in pin mode. GridEdit.tsx imports it at line 36, renders the '▸ Pick on map…' button at lines 173-184 (only visible in edit mode), mounts the overlay at lines 190-195 with onConfirm wired to commitPickedGrid (lines 111-128), which calls onCommit → config_set_grid — the same path as typed grid entry. DashboardRibbon.tsx mounts GridEdit at line 179 with a live onCommit handler. Delivery commit cf9ab6f4 (2026-06-09, 385 lines across 5 files) is present. The tracker status is simply stale — the implementation shipped but bd close was never run.
- **Evidence:** src/shell/GridPickerOverlay.tsx:1-102 (fully implemented overlay); src/shell/GridEdit.tsx:36 (import), 173-184 (Pick on map button), 190-195 (overlay mount with onConfirm=commitPickedGrid), 111-128 (commitPickedGrid → onCommit); src/shell/DashboardRibbon.tsx:14 (import GridEdit), 179-203 (mounts GridEdit with live config_set_grid onCommit). Commit cf9ab6f4 authored 2026-06-09 with message 'feat(shell): set Maidenhead grid by dropping a pin on the map (triage #18)'.

### tuxlink-rk6s — compose-forms
**Proposed:** Close tuxlink-rk6s in the bd tracker. The fix is fully implemented, commented, tested, and merged via commit 767bf995 / PR #397. No further code work is required.

- **Why:** Independent inspection fully confirms the first auditor's finding. The bounded channel fix is unambiguously present in origin/main. mpsc::channel(1) is used at line 205 (Form session) and line 285 (Viewer session stub). The submit_handler at line 806 uses try_send, returning 503 SERVICE_UNAVAILABLE on TrySendError::Full (lines 814-822), exactly matching the fix sketch in the bd issue description. The comment at lines 803-805 explicitly cites bd tuxlink-rk6s. The tuxlink-gheo dependency (no splitn folder-segment strip in folder_handler, line 860) is also confirmed fixed — substitute_template now emits /folder with no trailing folder name (line 372), and folder_handler receives the raw path directly without a splitn prefix-strip. Commit 767bf995 ('feat(forms): P2 hardening bundle') explicitly closes both tuxlink-rk6s and tuxlink-gheo in its commit message. A regression test for the 503 behavior exists at line 1460 (post_root_second_submit_returns_503_when_channel_full). The tracker status OPEN is a false positive — the fix fully landed in PR #397.
- **Evidence:** src-tauri/src/forms/http_server.rs:205 — mpsc::channel(1); line 806 — try_send; lines 814-822 — TrySendError::Full returns 503; lines 150-155 — comment citing tuxlink-rk6s; line 372 — FormFolder expansion without folder name (tuxlink-gheo fix); line 860 — folder_handler with no splitn strip; line 1460 — regression test for 503; git log commit 767bf995 closes tuxlink-rk6s and tuxlink-gheo.

### tuxlink-thzd — docs
**Proposed:** Close tuxlink-thzd — screenshots refreshed and merged at 85472e4a; all README image paths resolve.

- **Why:** Independent inspection confirms the original auditor's verdict. Commit 85472e4a (PR #580 merge, branch bd-tuxlink-thzd/readme-live-screenshots) is a confirmed ancestor of HEAD (104 commits back). All three screenshot files exist at docs/readme/images/ (tuxlink-mailbox.png, tuxlink-first-run-wizard.png, tuxlink-request-center.png). All three are referenced in README.md at lines 36, 204, and 212 respectively. The bd issue notes confirm CI passed on both arches. The issue is IN_PROGRESS with no remaining work.
- **Evidence:** git merge-base --is-ancestor 85472e4a HEAD → exit 0 (IS ANCESTOR); ls docs/readme/images/ → tuxlink-first-run-wizard.png, tuxlink-mailbox.png, tuxlink-request-center.png; README.md:36 <img src="docs/readme/images/tuxlink-mailbox.png">, README.md:204 <img src="docs/readme/images/tuxlink-first-run-wizard.png">, README.md:212 <img src="docs/readme/images/tuxlink-request-center.png">; bd show tuxlink-thzd notes confirm CI passed both arches.

### tuxlink-n65 — docs
**Proposed:** Close tuxlink-n65 — CI workflow (ci.yml), release workflow (release.yml), and SHA256 generation all shipped; cs7 dependency is stale/superseded.

- **Why:** Independent inspection confirms the original auditor's verdict. ci.yml implements clippy (--all-targets --locked -D warnings) at line 95, cargo test at line 98, and pnpm vitest run at line 88 on every PR/push to main. release.yml implements deb/rpm/AppImage builds at line 134 (pnpm tauri build --bundles deb,rpm,appimage) and SHA256SUMS generation at lines 162-166. All three scope items from the issue description are implemented. The remaining dependency on cs7 is tracker-only; cs7's actual AppImage work was superseded by tuxlink-qybc (PR #325, closed). No substantive gap was found to refute the verdict.
- **Evidence:** ci.yml:95 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings; ci.yml:88 pnpm vitest run; ci.yml:98 cargo test --manifest-path src-tauri/Cargo.toml --locked --verbose; release.yml:134 pnpm tauri build --bundles deb,rpm,appimage; release.yml:162-166 sha256sum *.deb *.rpm *.AppImage > SHA256SUMS-${{ matrix.arch }}.

### tuxlink-cs7 — docs
**Proposed:** Close tuxlink-cs7 — original AppImage scope implemented natively in release.yml via tuxlink-qybc (PR #325); Pat-binary-bundling scope is dead post PR #175.

- **Why:** Independent inspection confirms the original auditor's verdict. The bd issue notes explicitly record supersession by tuxlink-qybc (PR #325) per an operator scoping decision on 2026-06-03. release.yml:134 confirms pnpm tauri build --bundles deb,rpm,appimage runs on tagged releases, which natively produces deb, rpm, and AppImage artifacts via Tauri 2's bundler. The original scope (bundle Pat v1.0.0 binary into AppImage) is dead because the Pat sidecar was stripped in PR #175. tuxlink-qybc is closed. No code or configuration gap was found that would refute the verdict.
- **Evidence:** bd show tuxlink-cs7 notes: 'Superseded by tuxlink-qybc (PR #325) per operator scoping decision 2026-06-03: deb/rpm/AppImage are all native to Tauri 2.11.2's bundler'; release.yml:133-134 — name: Build + bundle (deb, rpm, AppImage) / run: pnpm tauri build --bundles deb,rpm,appimage; release.yml:176-181 — Upload to GitHub Release on refs/tags/v* with dist-artifacts/* (includes SHA256SUMS).

### tuxlink-wqv — misc
**Proposed:** Close tuxlink-wqv. The fix was committed at 286e57de on 2026-05-30; the issue was created the same day but never closed. Run: bd close tuxlink-wqv

- **Why:** The original auditor is correct. Commit 286e57de (2026-05-30) landed the fix before or simultaneously with the issue's creation date. CLAUDE.md line 63 in the worktree explicitly documents the MUTUALLY EXCLUSIVE constraint and shows only the two working patterns (--base main without a prompt, and stdin-pipe via 'cat prompt.txt | codex review -'). The stale syntax the issue describes ('codex review --base main "<prompt>"') is gone. The issue remains OPEN in the tracker despite the fix being committed. No grounds to refute — this is a genuine false-open: the discrepancy is between tracker state (OPEN) and code state (already fixed).
- **Evidence:** /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ahnz-bd-recon-audit/CLAUDE.md line 63: 'the `review` subcommand requires picking EXACTLY ONE of `--uncommitted` / `--base` / `--commit` / `[PROMPT]` — they are MUTUALLY EXCLUSIVE'. Commit 286e57de log: 'CLAUDE.md "OpenAI Codex CLI" section: corrected the invocation pattern for v0.128.0. The prior `codex review --base main "<prompt>"` syntax is rejected (--base and [PROMPT] are mutually exclusive). The working pattern for directed adrev is `cat prompt.txt | codex review -`'. Issue tuxlink-wqv status: OPEN as of audit date.

## Proposed bd actions (pending operator approval)

```bash
# false_open → close (29)
bd close tuxlink-96lu tuxlink-37vi tuxlink-7gb tuxlink-ws45 tuxlink-28y tuxlink-9dg tuxlink-b2s tuxlink-5vx tuxlink-b6ad tuxlink-ns7k tuxlink-syqb tuxlink-95g8 tuxlink-k3ru tuxlink-xnoy tuxlink-cyt tuxlink-xfo tuxlink-23v tuxlink-gheo tuxlink-4g2n tuxlink-m2o6 tuxlink-2y5 tuxlink-msr tuxlink-hxia tuxlink-urbv tuxlink-rk6s tuxlink-thzd tuxlink-n65 tuxlink-cs7 tuxlink-wqv

# false_closed → reopen (3) — n2uz is a real code gap; 7b0z/unb0 are doc-recovery
bd update tuxlink-n2uz --status open   # vu_dbfs permanently None in production
bd update tuxlink-7b0z --status open   # handoff doc commit not on main
bd update tuxlink-unb0 --status open   # handoff doc commit not on main

# partial_wired → reopen + becomes next fix (1)
bd update tuxlink-0ye6 --status open   # VARA intent hardcoded cms; Phase 5 RadioSessionPanel unbuilt
```
