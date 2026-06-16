# Remediation Plan — Link/Transport Config Persistence (tuxlink-hoi1)

**Date:** 2026-06-15 · **Bug hunt:** dev/bug-hunts/2026-06-15-link-config-persist-consolidated.md
**Operator decisions:** B1 fix = "preserve link when absent"; scope = the erase + silent-failure + drifting-copies (B1–B5); out-of-scope O1/O2/O3 filed separately (tuxlink-tlo5 / -zbtx / -d1wb).
**Base:** main. New worktree/branch off main (this branch bd-tuxlink-ube7 is for an unrelated PR #741).

## Definition of done (wire-walk flows — operator supplies/confirms at done-time)
1. Configure a UV-Pro (UvproNative) link in the APRS panel → restart app → link is STILL configured (not "no link").
2. With a link configured, change SSID (ribbon or packet panel) → link survives.
3. Open the APRS ⚙ setup on an already-configured UV-Pro → it shows the saved MAC; tapping the segment does NOT blank it.
4. A failed save does not leave the UI showing an un-saved value.

---

## Task preamble (EVERY task)
```
BEFORE starting:
1. Invoke /test-driven-development (read the skill).
2. Read docs/pitfalls/testing-pitfalls.md and docs/pitfalls/implementation-pitfalls.md.
Follow TDD: failing test → implement → green. RADIO-1/ADR-0018: this is config-path
code — write/test freely; only on-air execution is operator-gated.
```
## Task completion check (EVERY task)
```
BEFORE marking complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md (error paths + edge cases covered?).
2. Run the relevant gate green: `pnpm exec tsc --noEmit` + `pnpm exec vitest run <files>`;
   for Rust, the change MUST compile + tests pass on CI (do NOT cold-build locally per the
   contended-Pi rule — push and let CI compile).
3. Confirm no data-testid removed that a test depends on.
```

---

## Task 1 — B1 + B5 (backend): stop the erase + emit a change event
**Files:** `src-tauri/src/ui_commands.rs` (only).
**Evidence:** `packet_config_set` (:3696) full-replaces `cfg.packet = dto.into_packet_config()`; `into_packet_config` (:3661) maps `link_kind: None → link: None`. So any persist of a DTO without `link_kind` erases the saved link. Backend never emits `packet_config:change` (the frontend at usePacketConfig.ts:88 listens but it's dead).

**Change (B1 — preserve-on-absent):** make the persist treat an absent `link_kind` as "leave the saved link unchanged." Capture intent BEFORE `dto` is consumed:
```rust
let mut cfg = config::read_config().map_err(...)?;
let had_link_kind = dto.link_kind.is_some();          // capture before move
let existing_link = cfg.packet.link.clone();
cfg.packet = dto.into_packet_config()?;
if !had_link_kind {
    // A DTO with no link_kind means "unchanged", not "clear" (tuxlink-hoi1):
    // an SSID/timing-only write must never erase the configured radio link.
    cfg.packet.link = existing_link;
}
cfg.validate().map_err(...)?;
config::write_config_atomic(&cfg).map_err(...)?;
```
**Change (B5 — emit):** add `app: tauri::AppHandle` to the command signature; after a successful write, `let _ = app.emit("packet_config:change", PacketConfigDto::from(&cfg.packet));`. (Match the existing `tauri::command` + `AppHandle` pattern used elsewhere in this file; register order unaffected — `invoke_handler` already lists the command.)

**Testability — extract a pure helper** (so the merge logic is unit-testable without fs/State):
```rust
/// Apply a packet DTO over the existing config's link: absent link_kind preserves
/// the existing link (tuxlink-hoi1). Pure; unit-tested.
fn apply_packet_dto(existing_link: Option<KissLinkConfig>, dto: PacketConfigDto)
    -> Result<config::PacketConfig, UiError> {
    let had_link_kind = dto.link_kind.is_some();
    let mut packet = dto.into_packet_config()?;
    if !had_link_kind { packet.link = existing_link; }
    Ok(packet)
}
```
`packet_config_set` calls `apply_packet_dto(cfg.packet.link.clone(), dto)?`.

**Tests (Rust, in ui_commands.rs test module):**
- `apply_packet_dto` with `link_kind: None` over `existing = Some(UvproNative{mac})` → result `.link == Some(UvproNative{mac})` (preserved). **This fails against current code.**
- `apply_packet_dto` with `link_kind: Some("UvproNative") + bt_mac` over `existing = None` → result `.link == Some(UvproNative{mac})` (set).
- `apply_packet_dto` with `link_kind: Some("Tcp") + host/port` over an existing UvproNative → result is the Tcp link (replace works).
- ssid/params still applied from the DTO in all three.

**Do NOT:** add a "clear link" path (there is none in the UI); do not change `into_packet_config`'s variant mapping.

---

## Task 2 — B2 (frontend): seed the APRS setup picker so it can't blank the MAC
**Files:** `src/aprs/AprsConnectStrip.tsx`, `src/shell/AppShell.tsx` (wiring), `src/aprs/AprsConnectStrip.test.tsx`.
**Evidence:** `AprsConnectStrip.tsx:190-198` renders `<ModemLinkSection kind=... allowUvproNative=... onChange=... />` with NO address props. `ModemLinkSection.tsx:202` computes `btMac: mac || (btMac ?? null)`; with the prop `undefined` and `btMacInput=''`, a UV-Pro segment tap (`selectSegment`→`emit`) fires `{ linkKind:'UvproNative', btMac: null }`. `PacketRadioPanel` passes these props; AprsConnectStrip is the lone violator.
**Change:** add optional address props to `AprsConnectStripProps` (`btMac`, `tcpHost`, `tcpPort`, `serialDevice`, `serialBaud`) and pass them through to `ModemLinkSection` (same prop names ModemLinkSection already accepts — confirm against `ModemLinkSection`'s props / how PacketRadioPanel passes them at PacketRadioPanel.tsx ~:336). In `AppShell.tsx`, source these from `packetConfig.config` (it already reads `config.btMac` to build `aprsRadioLabel`) and pass them into `<AprsConnectStrip .../>`.
**Test:** render `AprsConnectStrip` with `linkKind="UvproNative"` + `btMac="AA:BB:CC:DD:EE:FF"`, open setup (it auto-expands only when noLink; for a configured link, click the ⚙ `aprs-connect-setup-toggle`), tap the UV-Pro segment, assert the `onLinkChange` mock received `btMac: "AA:BB:CC:DD:EE:FF"` (NOT null). **Fails against current code.**
**Do NOT:** change `ModemLinkSection`'s emit logic; only thread the existing props in.

---

## Task 3 — B4 + B3 (frontend): no silent divergence, no stale snapshot
**Files:** `src/packet/usePacketConfig.ts`, `src/radio/modes/PacketRadioPanel.tsx`, their tests.
**Evidence:** B4 — `usePacketConfig.ts:137-150` and `PacketRadioPanel.tsx:167-177` apply the optimistic `setConfig(next)` then swallow persist errors (`.catch(()=>undefined)`), so a rejected save leaves the UI showing an un-persisted value. B3 — `PacketRadioPanel` keeps a private `useState` snapshot (`:61`), loads once (`:132`, deps `[]`), and does NOT subscribe to `tuxlink:packet-config:change` (its `useEffect@88` is gateway-prefill), so it full-replaces from a frozen snapshot.
**Change (B4):** in both `setLink`/`setSsid` (usePacketConfig) and `persistDto` (PacketRadioPanel), capture the prior config; on persist rejection, restore it (`setConfig(prior)`) and surface the failure (session-log/inline — match how other persist errors surface). Keep the optimistic update for the success path.
**Change (B3):** make `PacketRadioPanel` re-seed on config change — subscribe to the backend `packet_config:change` (emitted by Task 1) AND the same-window `tuxlink:packet-config:change` CustomEvent, re-seeding its local state (mirror `usePacketConfig.ts:77-100`). (Do not rip out its local state in this cycle — minimal subscribe-and-reseed.)
**Tests:**
- usePacketConfig: mock `packet_config_set` to reject → call `setLink` → assert `config` reverts to the prior value (not the optimistic one).
- PacketRadioPanel: dispatch a `tuxlink:packet-config:change` (or emit the backend event) with a new link → assert the panel re-seeds.
**Do NOT:** convert PacketRadioPanel to `usePacketConfig` in this cycle (broader refactor — note as a follow-up).

---

## Review loop (after the 3 tasks)
You MUST review the batch from multiple perspectives and revise. Minimum three rounds; if the third still finds substantive issues, keep going. Then update your journal. Especially check: Task 1's `had_link_kind` is captured BEFORE `dto` is moved; Task 2 doesn't regress the noLink auto-expand; Task 3's rollback doesn't fight the broadcast (avoid a setConfig loop).

## Wire-walk + ship
Run the `wire-walk` skill against the 4 done-flows above (operator supplies them greenfield). Open a PR off main; CI compiles Rust. Operator validates flows 1–4 in a real build + on-air where relevant.

## Appendix: deferred (filed separately, NOT in this plan)
- **tuxlink-tlo5** (O1) — cross-build schema-skew link degradation (`deserialize_lenient_link`); dev-only; mitigated once this lands.
- **tuxlink-zbtx** (O2) — ManagedDireWolf link unrepresentable in the picker (separate UX change).
- **tuxlink-d1wb** (O3) — stale-closure `aprsLinkKind` in `onAprsConnect`.
