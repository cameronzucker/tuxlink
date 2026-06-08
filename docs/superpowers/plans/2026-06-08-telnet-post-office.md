# Telnet Post Office & Network Post Office — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Light up the two `built:false` Post Office session types — **Telnet RMS Post Office** (local `L` pool, `-L` login) and **Network Post Office** (`MESH`/normal `C`, full callsign) — over plain TCP, with send-time Outbox selection, full inbound message selection (via `bsiy`), a relay-state banner, favorites, and an operating-mode doc.

**Architecture:** Connection-determined routing (no compose-time flag — supersedes `tuxlink-u5hl`). The Post Office connect command builds on the **native telnet-exchange path** (`telnet::connect_and_exchange`), not the P2P template, so it inherits the `bsiy` inbound decide-seam. Outbound is gated by explicit operator selection (the leakage guard that replaces the `tuxlink-u5hl` safety gate for PostOffice/Mesh). Interop is field-compatible: WLE transmits no wire routing header, the `-L` login is the sole discriminator, so `message.rs` is unchanged.

**Tech Stack:** Rust (Tauri backend, `src-tauri/src`), React+TS (`src`), `vitest` + `cargo test`. Spec: [`docs/design/2026-06-08-telnet-post-office-design.md`](../../design/2026-06-08-telnet-post-office-design.md). bd: `tuxlink-6c9y` (depends on `tuxlink-bsiy`).

---

## Dependency & sequencing (READ FIRST)

> **⚠ Line numbers below are pre-`kld3`-merge estimates (off by ~1–90 lines).** Re-grep every `:NNN` anchor by function/field/marker name before editing — do NOT trust the absolute coordinates. `cargo build` / `pnpm typecheck` are the exhaustiveness backstops for any "update every call site / literal" step.
>
> **Load-bearing shared identifiers** (use verbatim across tasks, or out-of-order subagents break each other): the new pane component is `TelnetPostOfficeRadioPanel` (file `src/radio/modes/TelnetPostOfficeRadioPanel.tsx`); the connect command string is `telnet_post_office_connect` (B3 ↔ C1); the inbound marker header is `X-Tuxlink-Received-Session`.

- **`bsiy` (inbound message selection) must be MERGED TO `main` before Phase C.** `bsiy` generalizes the B2F decide-seam from `Fn(&[Proposal]) -> Vec<Answer>` to `Fn(&[Proposal]) -> Result<Vec<Answer>, ExchangeError>` and adds `winlink::inbound_selection` (`build_selecting_decider`, `SelectionRegistry`, `resolve_selection`). 6c9y's Phase C reuses these. Until `bsiy` lands on `main` and this branch merges `main`, Phase C cannot compile.
- **Phases A, B, D are independent of `bsiy`** and can be executed now in any order. Phase C is gated.
- **Before starting:** `git -C <worktree> fetch origin && git -C <worktree> merge origin/main` (pick up `kld3`'s merged foundation; `kld3` PR #322 is already on `main`). Re-check that `bsiy` is on `main` (`git -C <worktree> log origin/main --oneline | grep -i bsiy`) before Phase C.
- **Worktree:** `worktrees/bd-tuxlink-6c9y-telnet-post-office`, branch `bd-tuxlink-6c9y/telnet-post-office`. All paths below are worktree-relative. Commands assume `cd` into the worktree (or `-C`/`--manifest-path` as shown).

### Pitfalls in force (from `docs/pitfalls/`)
- **SCOPE-1:** Post Office *dials* a relay; it never listens/hosts/MPS-bridges. No task adds a listener or gateway role. (Invariant the narrowed gate relies on: no listener path ever constructs a `Mesh`/`PostOffice` session, so the now-ungated `selected=None` listener drain can't leak. If a future task lets the listener answer `Mesh`, that re-opens the leak.)
- **RADIO-1:** zero transmit — pure TCP. Phase B includes a no-consent-modal assertion; never add a consent gate.
- **Testing §5 (TOCTOU):** the selection-staleness and single-flight tasks include concurrency tests.
- **Testing §6/§7:** favorites default/validation tests; Post Office never touches the keyring (assert it).
- **`feedback_scoped_vitest_misses_contract_tests`:** before every push run `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` (re-run to exit 0; it hides later-target lints) **and** full `pnpm vitest run` — the far-from-change contract tests (`radioPanelVisibility`, `sessionTypes`, the Rust DTO test) only fail under the full gate.

---

## File structure

| File | Phase | Responsibility / change |
|---|---|---|
| `src-tauri/src/winlink/telnet.rs` | A | NEW `base_callsign_for_post_office()` + tests; NEW `post_office_connect_and_exchange` thin wrapper (optional). `telnet_login`/`CMS_TARGET_CALL` UNCHANGED. |
| `src-tauri/src/winlink/session/mod.rs` | A | `Mesh.routing_flag()` → `Cms`; multi-batch `remaining.clear()`→`drain(..offered)`; `ExchangeResult.relay_state`. |
| `src-tauri/src/winlink/handshake.rs` | A | Classify relay banner lines → `RemoteHandshake.relay_state`. |
| `src-tauri/src/winlink/relay_banner.rs` | A | `#[derive(Default)]` (`NotRelay`) on `RelayState`. (Parser already exists.) |
| `src-tauri/src/winlink_backend.rs` | A/C | Narrow safety gate; add `selected` param + filter to `build_outbound_proposals`; `file_exchange_result` gains `intent` param + marker; migrate the CMS inline drain to the helper. |
| `src-tauri/src/config.rs` | A | NEW `RelayFavorite` type + `network_po_favorites: Vec<RelayFavorite>` `#[serde(default)]`. |
| `src-tauri/src/ui_commands.rs` | A/C | NEW favorites commands; NEW `extract_received_session` + `ParsedMessageDto.received_session`; NEW `telnet_post_office_connect`/`_abort` (Phase C). |
| `src-tauri/src/lib.rs` | A/C | Register new commands in `generate_handler!`. |
| `src/connections/sessionTypes.ts` (+ `.test.ts`) | B | Flip `built:true` for `post-office`/`network-po` telnet. |
| `src/radio/types.ts` (+ `.test.ts`) | B | Add `'post-office'\|'network-po'` telnet intents; extend `panelTitle` `intentSuffix`. |
| `src/radio/radioPanelVisibility.ts` (+ `.test.ts`) | B | Map the two session types → telnet intents (fix the CMS fall-through). |
| `src/shell/AppShell.tsx` | B | Lazy import + preload + reading-pane branch + radio-panel mount for the new pane. |
| `src/radio/modes/TelnetPostOfficeRadioPanel.tsx` (+ `.test.tsx`) | B | NEW pane (host:port/favorites + Outbox checklist + login indicator + connect + log). |
| `src/mailbox/MessageView.tsx` (+ `.test.tsx`), `src/mailbox/types.ts` | B | NEW "Post Office" inbound chip keyed on `receivedSession`. |
| `docs/help/...` (operating-modes page) | D | NEW operating-mode doc. |

> **Three outbound-build loops** exist (spec §5): the helper `build_outbound_proposals`, the AX.25 inline mirror (`native_packet_exchange`), and the CMS inline mirror (`native_connect`). **Decision:** Task A3 migrates the CMS inline mirror to the helper (it must change anyway and gains skip-not-abort for free); the AX.25 mirror stays as documented tech-debt (RF-path, out of 6c9y scope) with a `// TODO` marker.

---

## Phase A — Backend foundations (independent of `bsiy`)

### Task A1: `base_callsign_for_post_office()` helper

**Files:**
- Modify: `src-tauri/src/winlink/telnet.rs` (add fn near `telnet_login`, ~`:72`/`:425`)
- Test: same file, `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests** (append to the `tests` mod in `telnet.rs`):

```rust
#[test]
fn base_callsign_for_post_office_local_appends_dash_l_after_stripping() {
    // Vector table pins the WLE GetBaseCallsign algorithm: uppercase, split '.'
    // first, then '-', take the first token; append -L for local. NO >6 rejection.
    assert_eq!(base_callsign_for_post_office("n7cpz-10", true), "N7CPZ-L");
    assert_eq!(base_callsign_for_post_office("N7CPZ.P", true), "N7CPZ-L");
    assert_eq!(base_callsign_for_post_office("W7XYZ-10", true), "W7XYZ-L");
    assert_eq!(base_callsign_for_post_office("N7CPZ", true), "N7CPZ-L");
    assert_eq!(base_callsign_for_post_office("RELAY1", true), "RELAY1-L"); // tactical passthrough
    // '.' splits BEFORE '-' (load-bearing order): "w7xyz-5.bbs" -> ".".0="w7xyz-5" -> "-".0="w7xyz"
    assert_eq!(base_callsign_for_post_office("w7xyz-5.bbs", true), "W7XYZ-L");
}

#[test]
fn base_callsign_for_post_office_network_keeps_full_base_no_dash_l() {
    assert_eq!(base_callsign_for_post_office("n7cpz-10", false), "N7CPZ");
    assert_eq!(base_callsign_for_post_office("N7CPZ.P", false), "N7CPZ");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink base_callsign_for_post_office`
Expected: FAIL — `cannot find function base_callsign_for_post_office`.

- [ ] **Step 3: Implement** (add to `telnet.rs`):

```rust
/// Login callsign for an RMS Relay "post office" telnet session, mirroring WLE
/// `GetBaseCallsign` (`Globals.cs:3136-3154`): uppercase, drop any `.`-qualifier
/// then SSID, then (for the local `L` pool) append `-L` — the `-L` suffix is the
/// entire local-vs-global routing discriminator (`TelnetSession.cs:2011-2013`).
/// Network PO passes `local = false` for the full base callsign, no `-L`.
///
/// No >6-char rejection: that check is Pactor-TNC-only (`PactorWL2KSession.cs:2259`);
/// importing it here would be a tuxlink-added safeguard (see memory
/// `feedback_no_tuxlink_added_safeguards`).
pub fn base_callsign_for_post_office(raw: &str, local: bool) -> String {
    let base = raw
        .trim()
        .to_uppercase()
        .split('.')
        .next()
        .unwrap_or("")
        .split('-')
        .next()
        .unwrap_or("")
        .to_string();
    if local { format!("{base}-L") } else { base }
}
```

- [ ] **Step 4: Run to verify it passes** — same command. Expected: PASS (both tests).
- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/telnet.rs
git commit -m "feat(post-office): base-callsign extraction for -L login (tuxlink-6c9y)" \
  -m "$(printf 'Agent: <SESSION-MONIKER>\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

> **Target-call note (resolve the §5.4-vs-§3.1 inconsistency):** REUSE the existing `telnet::CMS_TARGET_CALL` (`"wl2k"`, lowercase) for the Post Office path. §3.1 establishes interop is field-compatible (casing doesn't matter; the live CMS path proves lowercase works on-air). Do NOT add a `WL2K` constant. The Post Office `ExchangeConfig.targetcall = telnet::CMS_TARGET_CALL.to_string()`.

---

### Task A2: `Mesh` routing flag → `Cms` (normal pool, not `None`)

**Files:**
- Modify: `src-tauri/src/winlink/session/mod.rs:205-212` (`routing_flag()`), doc comments `:150-167`
- Test: same file, `mesh_intent_carries_no_routing_flag` (`:919-925`) → rename/rewrite

- [ ] **Step 1: Rewrite the failing test** (`session/mod.rs`, replace `mesh_intent_carries_no_routing_flag`):

```rust
#[test]
fn mesh_intent_carries_cms_routing_flag() {
    // Network Post Office (Mesh) carries NORMAL mail (the C pool) — distinct from
    // CMS only on transport, not routing (spec §1.1/§3/§5.5). P2p stays None.
    assert_eq!(SessionIntent::Mesh.routing_flag(), Some(RoutingFlag::Cms));
    assert_eq!(SessionIntent::P2p.routing_flag(), None);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink mesh_intent_carries_cms_routing_flag`
Expected: FAIL — `assertion ... Some(Cms)` vs current `None`.

- [ ] **Step 3: Implement** — move `Mesh` to the `Cms` arm (`session/mod.rs:205-212`):

```rust
pub fn routing_flag(self) -> Option<RoutingFlag> {
    match self {
        Self::Cms | Self::Mesh => Some(RoutingFlag::Cms), // Mesh = normal/C mail (tuxlink-6c9y §5.5)
        Self::RadioOnly => Some(RoutingFlag::RadioOnly),
        Self::PostOffice => Some(RoutingFlag::PostOffice),
        Self::P2p => None,
    }
}
```
Update the `Mesh` variant doc (`:150-155`) and the `RoutingFlag` enum doc (`:161-167`) to say Mesh carries `Cms`/normal mail.

- [ ] **Step 4: Run to verify it passes** — same command + `cargo test ... -p tuxlink session::` to confirm no sibling test regressed. Expected: PASS.
- [ ] **Step 5: Commit** (`feat(post-office): Mesh intent carries normal C routing flag`).

> **Coupling guard (do NOT skip):** Task A3's narrowed gate keys off `matches!(intent, P2p | RadioOnly)`, **not** off `routing_flag()`. Keep it that way — re-keying the gate off `routing_flag() != Some(Cms)` would wrongly re-gate `PostOffice` (flag `L`).

---

### Task A3: Narrow the safety gate + add selected-MIDs filter; migrate the CMS inline drain

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` — `build_outbound_proposals` (`:275-321`), its callers, `native_connect` inline drain (`:2084-2098`)
- Test: `winlink_backend.rs` `mod build_outbound_proposals_tests` (`:444-500`)

- [ ] **Step 1: Rewrite/extend the failing tests.** Keep `safety_gate_fires_for_p2p_intent` (`:444-462`) and `_radio_only_intent` (`:464-473`) UNCHANGED. Replace `safety_gate_fires_for_post_office_intent` (`:475-481`) and `_mesh_intent` (`:483-489`) with selection-scoped tests, and add the new-signature back-compat + skip tests:

```rust
use std::collections::HashSet;

#[test]
fn post_office_intent_proposes_only_selected_mids() {
    let mailbox = /* fixture with 3 Outbox drafts mids "A","B","C" — mirror queued_drafts_produce_one_proposal_each */;
    let selected: HashSet<String> = ["A".into(), "C".into()].into_iter().collect();
    let out = build_outbound_proposals(&mailbox, SessionIntent::PostOffice, Some(&selected)).unwrap();
    let mids: HashSet<String> = out.iter().map(|m| m.proposal.mid.clone()).collect();
    assert_eq!(mids, ["A".to_string(), "C".to_string()].into_iter().collect());
}

#[test]
fn mesh_intent_drains_selected_not_gated() {
    let mailbox = /* fixture 1 draft mid "A" */;
    let selected: HashSet<String> = ["A".into()].into_iter().collect();
    let out = build_outbound_proposals(&mailbox, SessionIntent::Mesh, Some(&selected)).unwrap();
    assert_eq!(out.len(), 1);
}

#[test]
fn selected_but_vanished_mid_is_skipped_not_fatal() {
    let mailbox = /* fixture 1 draft mid "A" */;
    let selected: HashSet<String> = ["A".into(), "GHOST".into()].into_iter().collect();
    let out = build_outbound_proposals(&mailbox, SessionIntent::PostOffice, Some(&selected)).unwrap();
    assert_eq!(out.len(), 1); // GHOST absent from live Outbox -> simply never matched
}

#[test]
fn none_selection_drains_all_back_compat() {
    let mailbox = /* fixture 2 drafts */;
    let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None).unwrap();
    assert_eq!(out.len(), 2);
}
```
Also update the signature in `cms_intent_drains_unchanged_through_safety_gate` (`:491-500`), `empty_outbox_returns_empty_vec` (`:335`), `queued_drafts_produce_one_proposal_each` (`:372`), `no_per_peer_filtering_ships_all_drafts` (`:408`): append `, None`.

- [ ] **Step 2: Run to verify failure** — `cargo test ... -p tuxlink build_outbound_proposals_tests`. Expected: compile error (new arg) → the new tests fail.

- [ ] **Step 3: Implement** — (a) new signature + narrowed gate + filter:

```rust
pub fn build_outbound_proposals(
    mailbox: &Mailbox,
    intent: SessionIntent,
    selected: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<session::OutboundMessage>, BackendError> {
    // Safety gate (narrowed — tuxlink-6c9y §5.5): P2p/RadioOnly still fail-closed
    // (6c9y does not address their leakage; tuxlink-u5hl re-scopes them). Cms/
    // PostOffice/Mesh drain; for the Post Office modes, `selected` IS the leakage guard.
    if matches!(intent, SessionIntent::P2p | SessionIntent::RadioOnly) {
        return Err(BackendError::MessageRejected(format!(
            "safety gate: outbound mail filtering not yet implemented for \
             {intent:?} sessions (tracked as bd issue tuxlink-u5hl)."
        )));
    }
    let mut outbound = Vec::new();
    for meta in mailbox.list(MailboxFolder::Outbox)? {
        // §5.5(b): advisory selection intersected with the live Outbox on the MID
        // (meta.id.0 == proposal.mid). Vanished MID never appears here (skip-not-abort).
        if let Some(sel) = selected {
            if !sel.contains(&meta.id.0) { continue; }
        }
        let body = match mailbox.read(MailboxFolder::Outbox, &meta.id) {
            Ok(b) => b,
            Err(e) => { eprintln!("build_outbound_proposals: skipping outbox message {:?}: {e}", meta.id); continue; }
        };
        if let Ok(message) = Message::from_bytes(&body.raw_rfc5322) {
            if let Some((proposal, compressed)) = message.to_proposal() {
                let title = message.header("Subject").unwrap_or_default().to_string();
                outbound.push(session::OutboundMessage { proposal, title, compressed });
            }
        }
    }
    Ok(outbound)
}
```
Keep the `"safety gate"` + `"tuxlink-u5hl"` substrings (the P2p test asserts them).

- [ ] **Step 4:** Update every existing caller to pass `None` (the new `selected` arg is a hard compile error at each): `winlink_backend.rs` ~`:2395`, `:2477`, `:2564`, `:2717` (four distinct ARDOP/VARA sites), `ui_commands.rs:5376` (`telnet_p2p_connect`), `winlink/telnet_listen.rs:542`, plus the 8 in-module test sites. **Trust `cargo build --manifest-path src-tauri/Cargo.toml` (run until clean), not this list, for exhaustiveness.**

- [ ] **Step 5: Migrate the CMS inline drain.** Replace the `native_connect` inline loop (`winlink_backend.rs:2084-2098`) with `let outbound = build_outbound_proposals(mailbox, SessionIntent::Cms, None)?;` (gains skip-not-abort; deletes the third loop). Leave the AX.25 mirror (`:1667-1677`) but add `// TODO(tuxlink-u5hl follow-up): migrate to build_outbound_proposals for skip-not-abort parity` at `:1667`.

- [ ] **Step 6: Run + commit** — `cargo test ... -p tuxlink build_outbound_proposals_tests` PASS; then full `cargo test`. Commit (`feat(post-office): narrow safety gate to P2p/RadioOnly + send-time MID selection`).

---

### Task A4: Multi-batch send (fix `remaining.clear()` one-turn-only drop)

**Files:** Modify `src-tauri/src/winlink/session/mod.rs:399` (the turn loop). Test: same file.

- [ ] **Step 1: Failing test** — a scripted relay/peer fixture that accepts ≥6 proposals across ≥2 turns; assert all 6 selected MIDs land in `result.sent` (mirror the existing `run_exchange` fixture tests). (Use the in-module test harness pattern already present for `run_exchange_with_role`.)
- [ ] **Step 2: Run → fail** (only first 5 land today).
- [ ] **Step 3: Implement** — replace `remaining.clear();` (`:399`) with a per-turn drain:

```rust
            // Offer at most MAX_BATCH this turn; keep the tail for the next my_turn cycle
            let offered = remaining.len().min(MAX_BATCH);
            // (send_turn already slices &outbound[..min(len, MAX_BATCH)] at :620)
            remaining.drain(..offered); // was remaining.clear() — multi-batch (tuxlink-6c9y §5.5)
```
(The loop re-enters `my_turn` after the remote turn via `my_turn = !my_turn` at `:419`; `MAX_TURNS = 1000` at `:31` is ample. Rewrite the misleading `// each message is offered once` comment.)
- [ ] **Step 4: Run → pass.** Confirm no other `run_exchange` test regressed.
- [ ] **Step 5: Commit** (`fix(post-office): multi-batch send — offer Outbox tail across turns`).

> **Scope:** drains *offered* per turn (matching existing offer-once semantics); deferred/rejected-message re-queue is explicitly OUT of scope (a `Defer`'d message is not re-offered later). Don't expect deferred-requeue.

---

### Task A5: Relay-state banner wiring (`relay_banner.rs` → handshake → `ExchangeResult`)

**Files:** `src-tauri/src/winlink/relay_banner.rs` (add `Default`), `src-tauri/src/winlink/handshake.rs` (classify + `RemoteHandshake.relay_state`), `src-tauri/src/winlink/session/mod.rs` (`ExchangeResult.relay_state`).

- [ ] **Step 1: Failing test** (in `handshake.rs` tests): script a handshake stream with a `"THIS IS A RADIO NETWORK HUB\r"` banner line before the SID; assert `read_remote_handshake(...).relay_state == RelayState::RadioNetwork`; and an ordinary-CMS stream → `RelayState::NotRelay`.
- [ ] **Step 2: Run → fail** (`relay_state` field missing).
- [ ] **Step 3: Implement:**
  - `relay_banner.rs:41`: add `Default` with `#[default] NotRelay`:
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum RelayState { #[default] NotRelay, LocalDatabase, RadioNetwork, RadioNetworkAndInternet, NoCmsConnectionAvailable }
    ```
  - `handshake.rs:32-39`: add `pub relay_state: crate::winlink::relay_banner::RelayState,` to `RemoteHandshake`.
  - `handshake.rs:160-178`: seed `let mut relay_state = RelayState::NotRelay;` before the loop; add a final `else if let Some(state) = crate::winlink::relay_banner::classify_banner_line(&line) { relay_state = state; }` branch (after the `>`-break branch); construct `RemoteHandshake { sid, forwarders, challenge, relay_state }`.
  - `session/mod.rs`: add `pub relay_state: RelayState` to `ExchangeResult` (which `#[derive(... Default, PartialEq, Eq)]`s — so `RelayState` must also derive `Default`+`PartialEq`+`Eq`, which Step 3 above supplies); set it from the `read_remote_handshake` result in `run_exchange_with_role` (~`:302-310`). **Do NOT touch the `ExchangeResult::default()` call site (`:592`) and do NOT convert the `#[derive(Default)]` to a manual `impl` — the derived default is zero-edit once `RelayState: Default`.**
- [ ] **Step 4: Run → pass.** Update any `RemoteHandshake { .. }` / `ExchangeResult { .. }` test literals to include the new field.
- [ ] **Step 5: Commit** (`feat(post-office): surface relay banner state through handshake to ExchangeResult`).

> The Phase C connect command returns `relay_state` in its result DTO so the pane shows a live banner strip (spec §5.9). An ordinary CMS connection yields `NotRelay` (no behavior change for existing paths).

---

### Task A6: Inbound routing marker (`X-Tuxlink-Received-Session`)

**Files:** `src-tauri/src/winlink_backend.rs` (`file_exchange_result` `:2305-2323`), `src-tauri/src/ui_commands.rs` (`extract_received_session`, `ParsedMessageDto`), `src/mailbox/types.ts`.

- [ ] **Step 1: Failing Rust test** — run `file_exchange_result(&mailbox, &result_with_one_received, SessionIntent::PostOffice, &noop)`, read the stored Inbox bytes back via `mailbox.read` + `parse_raw_rfc5322`, assert `received_session == Some("post-office")`; a `SessionIntent::Cms` twin asserts `None`. Plus the DTO serialization test (`ui_commands.rs:6204-6219`) asserting `v["receivedSession"] == "post-office"`.
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement:**
  - `file_exchange_result`: add `intent: SessionIntent` param; for `intent == SessionIntent::PostOffice`, `let mut m = message.clone(); m.set_header("X-Tuxlink-Received-Session", "post-office"); mailbox.store(Inbox, &m.to_bytes())?;` (else store unchanged — keeps non-PO bodies byte-identical, avoids churning golden vectors). `file_exchange_result` has exactly **one** production caller — `native_connect` (~`:2191`, pass `SessionIntent::Cms`) — plus two unit-test call sites (~`:2924`, `:2941`, pass `Cms`). The Phase C command (C1) calls it with `PostOffice`/`Mesh`. **ARDOP/VARA/packet inbound paths inline `mailbox.store(Inbox, …)` directly and do NOT call `file_exchange_result` — leave their loops untouched (out of scope; marker-free by construction, RADIO-1/SCOPE-1 surface).**
  - `ui_commands.rs`: add `extract_received_session(msg)` reading only `X-Tuxlink-Received-Session` (do NOT add it to `extract_routing`'s `TRANSPORT_HEADERS`); add `pub received_session: Option<String>` to `ParsedMessageDto` (after `routing` `:544`); populate in `parse_raw_rfc5322` (`:665`/struct literal `:667-680`).
  - `src/mailbox/types.ts:52`: add `receivedSession?: string | null;`.
- [ ] **Step 4: Run → pass** (Rust + the camelCase DTO assertion).
- [ ] **Step 5: Commit** (`feat(post-office): persist session-derived inbound routing marker`).

---

### Task A7: Network PO favorites persistence

**Files:** `src-tauri/src/config.rs` (`RelayFavorite` + `network_po_favorites`), `src-tauri/src/ui_commands.rs` (4 commands), `src-tauri/src/lib.rs` (register). Tests: `config.rs`, `ui_commands.rs`.

- [ ] **Step 1: Failing tests** — (a) `config.rs` migration test (mirror `config_modem_vara_absent_migrates_to_none` `:1224-1239`): a config JSON without `network_po_favorites` deserializes with an empty Vec; (b) round-trip test (mirror `vara_ui_config_round_trips_through_serde`); (c) `ui_commands` test: `network_po_favorites_add` then `_get` returns the favorite; adding a `(host,port)` duplicate returns `UiError::Rejected`; `_remove` is idempotent.
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement:**
  - `config.rs`: add the `RelayFavorite` struct (`#[serde(deny_unknown_fields)]`, fields `callsign,label,host,port`) and `#[serde(default)] pub network_po_favorites: Vec<RelayFavorite>` on `Config` (after `telnet_listen` `:52`). `CONFIG_SCHEMA_VERSION` stays `1` (additive-default field; `Config` carries `#[serde(deny_unknown_fields)]` so the new key being *known* is what makes old configs deserialize). `Config` has **no** `Default` impl, so **every top-level `Config {` struct literal is a hard compile error until it names `network_po_favorites: Vec::new()`** — there are ~10 across 6 files: `bootstrap.rs:299`, `wizard.rs:155`, `wizard.rs:318`, `test_helpers.rs:19`, `modem_commands.rs:1788`, `winlink_backend.rs:3024`, `winlink_backend.rs:3836`, `ui_commands.rs:6635`, `ui_commands.rs:6848`, `ui_commands.rs:7581`. **Do NOT trust this list for exhaustiveness — `grep -rn 'Config {' src-tauri/src`, then `cargo build --manifest-path src-tauri/Cargo.toml` until every literal compiles.**
  - `ui_commands.rs`: `network_po_favorites_get/add/remove/set` per the grounding table (read_config → mutate → validate non-empty host/callsign → write_config_atomic; dedup on `(host.eq_ignore_ascii_case, port)`; no backend `set_config` refresh; no new file-lock — match `config_set_connect`'s unguarded read-modify-write convention).
  - `lib.rs`: register the four commands in `generate_handler!` (`:328`).
- [ ] **Step 4: Run → pass.**
- [ ] **Step 5: Commit** (`feat(post-office): Network PO relay favorites in config`).

---

## Phase B — Frontend wiring + pane (independent; unit-tested with mocked invoke)

### Task B1: Flip `built` + extend the intent union + `panelTitle`

**Files:** `src/connections/sessionTypes.ts` (+`.test.ts`), `src/radio/types.ts` (+`.test.ts`).

- [ ] **Step 1: Failing tests** — `types.test.ts`: `panelTitle({kind:'telnet',intent:'post-office'}) === 'Telnet Post Office'` and `network-po → 'Telnet Network Post Office'`. `sessionTypes.test.ts`: `isBuilt({sessionType:'post-office',protocol:'telnet'}) === true` and `network-po` twin (update any existing assertion that expects `false`).
- [ ] **Step 2: Run → fail** (`pnpm vitest run src/radio/types.test.ts src/connections/sessionTypes.test.ts`).
- [ ] **Step 3: Implement** — `sessionTypes.ts:53-62,80-88` flip the session `built` + the `TEL` protocol `built` to `true` (leave `PKT` `false`). `types.ts:20` widen the telnet arm to `intent: 'cms' | 'p2p' | 'post-office' | 'network-po'`; `types.ts:45-48` extend `intentSuffix` with the two new arms (→ `'Post Office'` / `'Network Post Office'`).
- [ ] **Step 4: Run → pass** + `pnpm typecheck` (widening the union without the `intentSuffix`/switch arms is a silent-wrong-label trap, not a compile error — the explicit test catches it).
- [ ] **Step 5: Commit** (`feat(post-office): enable Post Office session types + panel titles`).

### Task B2: `computePanelMode` intent mapping (fix the CMS fall-through)

**Files:** `src/radio/radioPanelVisibility.ts` (+`.test.ts`).

- [ ] **Step 1: Failing test** — `radioPanelVisibility.test.ts`: `computePanelMode({sidebarSelected:{sessionType:'post-office',protocol:'telnet'},...})` → `{kind:'telnet',intent:'post-office'}` (NOT `intent:'cms'`); `network-po` twin. Mirror the p2p case at `:80-86`.
- [ ] **Step 2: Run → fail** (currently returns `intent:'cms'`).
- [ ] **Step 3: Implement** — `radioPanelVisibility.ts:33-40`: widen the local `intent` union and add the two `sessionType ===` mappings; the `telnet` case passes them through (`intent === 'radio-only' ? 'cms' : intent`). Narrow the `packet`/`ardop`/`vara` case returns to coerce any non-`cms|p2p|radio-only` intent to `cms` so their narrower unions typecheck (verify with `pnpm typecheck`).
- [ ] **Step 4: Run → pass** + `pnpm typecheck`.
- [ ] **Step 5: Commit** (`fix(post-office): route Post Office session types to telnet intents`).

### Task B3: The `TelnetPostOfficeRadioPanel` component

**Files:** NEW `src/radio/modes/TelnetPostOfficeRadioPanel.tsx` (+`.test.tsx`). Template: `src/radio/modes/TelnetP2pRadioPanel.tsx`. Reuse `RadioPanel`, `SessionLogSection`, `useSessionLog`, `useQueryClient`, `useMailbox('outbox')`, and the `AllowedStationsEditor` add/remove pattern for favorites.

- [ ] **Step 1: Failing tests** (`.test.tsx`, clone `TelnetRadioPanel.test.tsx` mock scaffold): select-all/none toggles selection; **N=0 keeps Connect enabled, label = "Connect"**; N>0 label = "Connect & send N"; Connect invokes `invoke('telnet_post_office_connect', { req: { mode, host, port, my_callsign, locator, selected_mids } })` — the **`{ req: {...} }` wrapper is required** (mirrors `telnet_p2p_connect`; `TelnetP2pRadioPanel.tsx:190-191` documents that Tauri rejected flat args). The Rust command is `async fn telnet_post_office_connect(req: PostOfficeDialRequest)` (single `req` param); login indicator shows `<base>-L` (local) vs full callsign (network); **no-consent-modal** assertion (clone `ArdopRadioPanel.test.tsx:128-147`: after Connect, `screen.queryByRole('dialog')` is null and `invoke` not called with `modem_mint_consent`); favorites add/remove (network mode).
- [ ] **Step 2: Run → fail** (component missing).
- [ ] **Step 3: Implement** the component per the grounding skeleton (props `{ mode:'local'|'network'; onClose }`; state per grounding; `start()` clones the P2P connect flow but invokes `telnet_post_office_connect`; favorites loaded via `network_po_favorites_get`; Outbox checklist from `useMailbox('outbox')`; partial-send survival is automatic — selection `Set` is keyed on `m.id`, sent rows drop from `outbox` after `invalidateQueries`).
- [ ] **Step 4: Run → pass** + `pnpm typecheck`.
- [ ] **Step 5: Commit** (`feat(post-office): TelnetPostOfficeRadioPanel (host/favorites + Outbox selection)`).

### Task B4: AppShell dispatch + reading-pane branch

**Files:** `src/shell/AppShell.tsx`.

- [ ] **Step 1: Failing test** — extend an AppShell-level test (or the panel-mount test) asserting that selecting `{sessionType:'post-office',protocol:'telnet'}` mounts `TelnetPostOfficeRadioPanel` (mode `local`) and `network-po` mounts it (mode `network`), and the reading pane renders mail (not `StubPanel`).
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement** — add the lazy import + preload (`:117-131`,`:330-337`), the reading-pane branch (`:925-963`: `if ((sessionType==='post-office'||sessionType==='network-po')&&protocol==='telnet') return readingPane;`), and the radio-panel mount arm (`:986-998`) passing `mode={intent==='post-office'?'local':'network'}`.
- [ ] **Step 4: Run → pass** + `pnpm typecheck`.
- [ ] **Step 5: Commit** (`feat(post-office): mount Post Office pane + reading-pane dispatch`).

### Task B5: Inbound "Post Office" chip in MessageView

**Files:** `src/mailbox/MessageView.tsx` (+`.test.tsx`).

- [ ] **Step 1: Failing tests** — KEEP the existing `does NOT render a routing/Via row` test green (it concerns `routing`, not the marker). ADD: chip renders when `receivedSession === 'post-office'` (`data-testid="message-received-session"`, text "Post Office"); no chip when `receivedSession` is null. Extend the `parsed(...)` factory to default `receivedSession: null`.
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement** — render the chip in the meta block (after the date `<dd>` ~`:386`) keyed on `message.receivedSession === 'post-office'`.
- [ ] **Step 4: Run → pass.**
- [ ] **Step 5: Commit** (`feat(post-office): inbound Post Office routing chip`).

---

## Phase C — `bsiy`-GATED connect command + integration

> **GATE:** Do NOT start until `bsiy` is merged to `main` and this branch has merged `main`. Verify: `git -C <worktree> log origin/main --oneline | grep -i 'bsiy\|inbound.selection'` AND `grep -rn "build_selecting_decider" src-tauri/src/winlink/inbound_selection.rs` resolves. The decide-seam must be `Fn(&[Proposal]) -> Result<Vec<Answer>, ExchangeError>`.

### Task C1: `telnet_post_office_connect` / `_abort` commands

**Files:** `src-tauri/src/ui_commands.rs` (commands + `PostOfficeDialRequest`/`PostOfficeDialResult` + `PostOfficeConnectState`), `src-tauri/src/lib.rs` (register).

- [ ] **Step 1: Failing test** — an integration test cloning `bsiy`'s `selecting_connect_emits_offer_and_files_selected_message_into_inbox` (`bsiy winlink_backend.rs:3615`): a fixture relay; assert the login line is `<base>-L` (local) / full callsign (network), the `CMSTelnet` password, that only selected outbound MIDs are proposed, that inbound selection is exercised via the `bsiy` decider, that received PO mail is filed with the `X-Tuxlink-Received-Session` marker, and that `relay_state` is returned. Plus a **keyring-never** assertion (the command path never calls `credentials::read_password`).
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement** the command:
  - DTO `PostOfficeDialRequest { mode, host, port, my_callsign, locator, selected_mids: Vec<String> }`; result `{ sent_count, received_count, relay_state }`.
  - `let local = mode == "local";` then build `ExchangeConfig { mycall: telnet::base_callsign_for_post_office(&my_callsign, local), targetcall: telnet::CMS_TARGET_CALL.into(), locator, password: None, intent: if local { SessionIntent::PostOffice } else { SessionIntent::Mesh } }`. (`mode: String` is the DTO field; bind `local` once, use it consistently.)
  - `outbound = build_outbound_proposals(&mailbox, intent, Some(&selected_mids.into_iter().collect()))?`.
  - `decide` = `bsiy::build_selecting_decider(registry, attempt_id, emit, aborting)` (reuse the `CmsSelectionContext` analog; or reuse `cms_resolve_inbound_selection` for the resolve command).
  - Call `telnet::connect_and_exchange(host, port, Transport::Plaintext, &config, outbound, &progress, &wire_log, &register_socket, decide)`; on result, `file_exchange_result(&mailbox, &result, intent, &mailbox_change)`; return `{ sent_count, received_count, relay_state }`.
  - Single-flight `AtomicBool` (clone `P2pConnectState` `:5229`); N=0 selection still connects (receive-only).
  - Register `telnet_post_office_connect`, `telnet_post_office_abort`, and the resolve command in `lib.rs`.
- [ ] **Step 4: Run → pass** (+ TOCTOU test: selected-but-vanished MID across the compose→connect window is skipped, not fatal — A3's filter already gives this; add the integration-level assertion).
- [ ] **Step 5: Commit** (`feat(post-office): telnet_post_office_connect with inbound selection (bsiy)`).

### Task C2: End-to-end frontend↔backend integration + operator smoke prep

- [ ] Wire the pane's `invoke('telnet_post_office_connect', { req })` to the real command (the string contract from B3 ↔ C1). Run `pnpm typecheck` + full `pnpm vitest run`.
- [ ] Document the operator smoke (Tier B, spec §9): operator stands up an RMS Relay on `127.0.0.1:8772`, dials local + network modes, verifies send-selection + inbound selection + the relay banner + the inbound chip. No RF. Capture as a smoke checklist in the PR body.
- [ ] Commit (`feat(post-office): wire pane to connect command`).

### Task C3: Full verify gate before PR

- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` (re-run to exit 0).
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` (all green).
- [ ] `pnpm vitest run` (full — confirm `radioPanelVisibility`, `sessionTypes`, `menuModel` (unchanged), the Rust DTO test all pass).
- [ ] `pnpm typecheck`. Then open the PR (READY, not draft; operator smokes).

---

## Phase D — Operating-mode documentation (independent)

### Task D1: Operating-mode page (spec §8, smoke-walk item 8)

**Files:** the help topics live in `src/help/topics.ts` (rendered by `src/help/HelpView.tsx`); the contract test is `src/help/topics.test.ts`. Add the new operating-mode topic there (clone an existing operating/digital-mode topic entry's shape) and update `topics.test.ts` to include the new topic ID.

- [ ] Author the page: what an RMS Relay "post office" is; the three session types (§1.1 table) and the local-vs-global distinction; tuxlink's connection-determined + send-time-selection routing model and the **explicit divergence** from WLE's compose-time pools (with the footgun rationale, operator-decided); why AREDN auto-discovery is omitted (OLSR→Babel); when to use each mode. Writing voice: declarative, formal, present-indicative (no "I", no "today/currently").
- [ ] Update `src/help/topics.test.ts` to include the new topic ID (it asserts the topic vocabulary — a new topic without the test update fails the full `pnpm vitest run`, per `feedback_scoped_vitest_misses_contract_tests`).
- [ ] Run `pnpm lint:docs` (the pre-push gate) + commit (`docs(post-office): operating-mode guide for Post Office modes`).

---

## Self-review (run before handoff)

- **Spec coverage:** §1.1 table → D1; §3/§3.1 (no wire header) → A-level (message.rs unchanged, confirmed); §4.3/4.4 panes → B3; §4.5 chip → A6+B5; §4.6 N=0/large Outbox → B3; §4.7 partial-send → B3; §5.4 connect → C1; §5.5 gate+selection+Mesh → A2+A3; §5.5 multi-batch → A4; §5.6 base-callsign → A1; §5.7 inbound marker → A6; §5.8 favorites → A7; §5.9 relay_banner → A5; §7 divergences → D1; §9 tests → woven per task. **No spec section unmapped.**
- **Placeholder scan:** test-fixture bodies (`/* fixture ... */`) in A3 are the one intentional gap — the executor builds them from the cited sibling test `queued_drafts_produce_one_proposal_each` (at `winlink_backend.rs:340`, in `mod build_outbound_proposals_tests`); every code-bearing step has real code. Flag for the executor.
- **Type consistency:** `selected: Option<&HashSet<String>>` (A3) ↔ `selected_mids: Vec<String>` (C1, collected to a set); `received_session`/`receivedSession` (A6/B5); `RelayFavorite` fields (A7/B3); `RelayState` (A5/C1). Consistent.
- **Sequencing:** Phases A/B/D independent; C gated on `bsiy`-on-main. The connect-command string contract (`telnet_post_office_connect`) is shared B3↔C1 — keep the names identical.
