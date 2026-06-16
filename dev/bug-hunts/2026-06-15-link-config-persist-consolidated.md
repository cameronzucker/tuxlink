# Link/Transport Config Persistence — Consolidated Bug Hunt

**Date:** 2026-06-15
**Issue:** tuxlink-hoi1
**Scope:** radio link/transport config persistence across app restart (frontend `usePacketConfig` / `ModemLinkSection` / `PacketRadioPanel` / ARDOP+VARA panels; backend `packet_config_set/get`, `into_packet_config`, `config.rs` round-trip).
**Hunters:** Exploratory, Holistic, Multipass (all read-only). **Agent:** sumac-vetch-dahlia.

Strong 3-way consensus on the destructive primitive; each hunter pinned a distinct trigger. All cited code re-read and verified during consolidation.

---

## Confirmed Bugs

### B1. `packet_config_set` is a destructive full-replace; absent `link_kind` → `link: None` (the primitive)
**Consensus:** all 3 + verified.
**Location:** `src-tauri/src/ui_commands.rs:3702` (`cfg.packet = dto.into_packet_config()?`) + `:3661` (`None => None`).
**Evidence:** `packet_config_set` reads the whole config, REPLACES the entire `[packet]` section from the DTO, writes atomically. `into_packet_config` maps a DTO with `link_kind: None` to `link: None`. Device + transport both live in the single `PacketConfig.link` (`KissLinkConfig`), while `ssid`/`params`/`listen_default` are separate fields — so a `link_kind`-less persist wipes ONLY the link, exactly matching "the rest is retained, the link is gone."
**Impact:** any caller that persists a DTO lacking `link_kind` silently erases the saved radio link. Root primitive for B2/B3.
**Blast radius:** `packet_config_set` is the sole packet-config writer; the fix is local to it + `into_packet_config`. Must NOT break a deliberate "clear link" path — there is none in the UI today (verified), so "absent = preserve" is safe.
**Fix approach:** in `packet_config_set`, when `dto.link_kind` is `None`, preserve the existing `cfg.packet.link` instead of clearing (or field-level patch). Trigger-agnostic — neutralizes B2/B3's clobber.

### B2. `AprsConnectStrip` mounts `ModemLinkSection` with no address props → picker can only emit a null MAC (the direct APRS trigger)
**Consensus:** Holistic + Multipass + Exploratory.
**Location:** `src/aprs/AprsConnectStrip.tsx:190-198` (passes only `kind`/`allowUvproNative`/`onChange`) + `src/radio/sections/ModemLinkSection.tsx:202` (`btMac: mac || (btMac ?? null)`) + `:208` (`selectSegment`→`emit`).
**Evidence:** `PacketRadioPanel` passes `btMac`/`host`/`port`/`serialDevice` into `ModemLinkSection`; `AprsConnectStrip` is the lone sibling that omits them. So `btMacInput` seeds to `''`, the `btMac` PROP is `undefined`, and a UV-Pro/Bluetooth segment tap emits `{ linkKind: 'UvproNative', btMac: null }`. The backend rejects null-MAC UvproNative (`ui_commands.rs:3648` `ok_or_else`), so disk is *preserved on that write* — BUT `setLink` already applied the optimistic local update and swallowed the rejection (B4), leaving in-memory state at "no link," which a later SSID/link write then persists for real via B1.
**Impact:** opening the ⚙ setup on a configured UV-Pro from the APRS strip corrupts the live link state; a subsequent persist makes it permanent. Most direct match for the operator's APRS-surface symptom.
**Blast radius:** `AprsConnectStrip` only. Fix: thread the persisted address fields into `ModemLinkSection` (AppShell already has them — it reads `config.btMac` to build the radio label).
**Fix approach:** pass `btMac`/`host`/`port`/`serialDevice`/`serialBaud` props from the loaded packet config into `AprsConnectStrip`→`ModemLinkSection`.

### B3. `PacketRadioPanel` holds a private stale config snapshot and full-replaces from it
**Consensus:** Exploratory (+ verified).
**Location:** `src/radio/modes/PacketRadioPanel.tsx:61` (own `useState`), `:132` (load once, deps `[]`), `:167` `persistDto`, `:184/:189/:218` callers. `useEffect@88` is `listenGatewayPrefill`, NOT the packet-config broadcast.
**Evidence:** unlike `usePacketConfig` (`usePacketConfig.ts:77-84` subscribes to `tuxlink:packet-config:change`), `PacketRadioPanel` keeps its own snapshot, never subscribes, never reloads. Every panel write full-replaces `[packet]` from that frozen snapshot. If the link was configured elsewhere (APRS strip) after the panel mounted, a panel SSID/link/managed change wipes the link via B1.
**Impact:** cross-surface clobber; a second route into B1.
**Blast radius:** `PacketRadioPanel`. Fix: consume the shared `usePacketConfig` hook, OR subscribe to the broadcast + reload, OR rely on B1's preserve-link guard (which alone neutralizes the clobber even with stale snapshots).
**Fix approach:** simplest durable fix is B1 (preserve link) + B5 (emit event) so stale snapshots can't clobber and do re-sync.

### B4. Optimistic local update + swallowed persist error → UI/disk silently diverge
**Consensus:** all 3.
**Location:** `src/packet/usePacketConfig.ts:137-150` (`setConfig(next)` then `invoke(...).catch(()=>undefined)`); `PacketRadioPanel.tsx:167-177` (same pattern).
**Evidence:** the optimistic `setConfig(next)` runs before persist; a rejected/failed `packet_config_set` is swallowed, so the UI shows a state that never reached disk. Compounds B2 (the null-MAC reject is invisible) and seeds a later real clobber.
**Impact:** silent divergence; the operator sees a state that isn't persisted, with no error.
**Fix approach:** roll back (or defer) the optimistic update on persist rejection, and surface the error (session log / inline). Lower urgency than B1/B2 but part of "never silently wrong."

### B5. Backend never emits `packet_config:change` (sync gap enabling stale snapshots)
**Consensus:** all 3.
**Location:** `src/packet/usePacketConfig.ts:88` (frontend listens) — no backend emitter exists (grep-confirmed).
**Evidence:** the cross-component sync the frontend relies on is dead. `usePacketConfig` instances + `PacketRadioPanel`'s private state can't re-sync across surfaces, enabling B3's stale snapshot.
**Fix approach:** emit `packet_config:change` from `packet_config_set` (Tauri `app.emit`) and have all packet-config readers subscribe. (Design choice — see D2.)

---

## Design Decisions Requiring Operator Input

### D1. Canonical fix shape for B1
**The concern:** how to stop the destructive full-replace.
**Options:**
- **(a) Preserve-on-absent (recommended):** `packet_config_set` keeps the existing `cfg.packet.link` when `dto.link_kind` is `None`. Minimal, local, trigger-agnostic; "absent = unchanged." Risk: no UI "clear link" path today, so safe; if one is ever added it needs an explicit sentinel.
- **(b) Field-level patch:** only overwrite fields the DTO marks present. Cleaner conceptually but the flat DTO uses explicit `null`s, so "present vs clear" is ambiguous — more invasive.
- **(c) Eliminate the flattening:** persist the tagged `KissLinkConfig` directly through the DTO instead of flat scalars. Largest change; best long-term but out of proportion here.
**Recommendation:** (a), plus B2 (seed the picker) — together they fix the operator's symptom with the least risk.

### D2. Sync mechanism (B5) + `PacketRadioPanel` (B3)
**Options:**
- **(a) Emit `packet_config:change` + subscribe everywhere (recommended):** activates the already-written frontend listener; PacketRadioPanel subscribes too.
- **(b) Single source of truth:** make `PacketRadioPanel` consume the shared `usePacketConfig` hook (delete its private snapshot).
**Recommendation:** (a) for this cycle (small, matches existing design intent); (b) is a nice follow-up refactor but broader.

### D3. Scope of this fix cycle
**Options:** (a) data-loss core only (B1+B2); (b) core + silent-divergence/sync (B1+B2+B4+B5+B3); (c) include the out-of-scope items below.
**Recommendation:** (b) — B4/B5/B3 are the same defect family (silent, stale, clobber) and cheap alongside the core; fixing only B1+B2 leaves the silent-divergence foot-gun.

---

## False Positives

### FP1. "`read_config` fails closed → reverts ALL settings to defaults on disk" (Multipass #5, as stated)
**Why invalid:** `read_config` (config.rs:645-658) returns `Err` (`Serde`/`Validation`/`NotFound`) on failure; it does NOT construct-and-write a default. Writers use `read_config()?` (propagate), so a corrupt/skewed config makes a write FAIL, not clobber-to-default. The real, narrower residual is O1 (link-only lenient degradation). The "device read from a different store" explanation is unsubstantiated — device fields live in the same `config.json`.

---

## Bugs Outside Primary Scope

### O1. Cross-build schema-skew link degradation via `deserialize_lenient_link`
**Location:** `config.rs:469`. **Evidence:** a link variant written by one dev build that another build's `KissLinkConfig` can't parse degrades to `None` on read (by design — tuxlink-efo). Dev-only (shared `~/.config/tuxlink/config.json` across worktree builds), link-scoped (not whole-config). **Recommendation:** document; low priority. Mitigated in practice once B1/B2 stop the in-app clobber.

### O2. `ManagedDireWolf` link unrepresentable in the shared picker
**Location:** `ModemLinkSection` / `pickerKind` (AprsConnectStrip.tsx:63). **Evidence:** a Managed link has no segment and is mis-shown as Serial; re-emit can't reconstruct it. **Recommendation:** document; separate from the persistence bug.

### O3. Stale-closure `aprsLinkKind` in `onAprsConnect`
**Location:** `AppShell.tsx` `onAprsConnect` (~597). **Evidence:** fast pick-then-connect could read a stale `aprsLinkKind`; a related race was addressed by the 2026-06-14 Codex adrev (`aprsLinkPersist` await). **Recommendation:** re-verify; fold a guard in if cheap, else document.

---

## Test Gap Analysis

### B1. full-replace clobbers link
**Why missed:** existing tests (`usePacketConfig.test.tsx`, `packetConfig.test.ts`) exercise each writer in ISOLATION with a DTO that *includes* `link_kind`; there is no test that persists a `link_kind`-less DTO against a config that *has* a link and asserts the link survives. No Rust test for `into_packet_config`'s `None`-link_kind branch's destructive effect through `packet_config_set`.
**Pitfall coverage:** adjacent to the existing "test the production mount path, not just the unit" lesson, but the specific *multi-writer-clobbers-one-record* / *absent-field-erases-stored-value* class is not documented → new pitfall warranted.
**Catch test:** Rust — seed config with `packet.link = Some(UvproNative{mac})`, call `packet_config_set` with a DTO where `link_kind = None` (SSID-only change), assert `read_config().packet.link` is STILL `Some(UvproNative{mac})`. Fails against current code.

### B2. prop-starved picker emits null MAC
**Why missed:** `ModemLinkSection` tests pass full props; no test mounts it the way `AprsConnectStrip` does (props omitted) and asserts the emitted DTO preserves the existing MAC. AprsConnectStrip tests don't assert the props threaded to `ModemLinkSection`.
**Pitfall coverage:** same new "absent-field-erases" class.
**Catch test:** render `AprsConnectStrip` with a configured UvproNative link, open setup, tap the UV-Pro segment, assert `onLinkChange` fires with `btMac` = the existing MAC (not `null`).

### B3. stale snapshot
**Why missed:** no integration test mounts two packet-config writers and verifies a write from one doesn't clobber the other's field. Unit tests pass individually.
**Pitfall coverage:** new class.
**Catch test:** mount AppShell-path config + a `PacketRadioPanel`; set link via the APRS strip; change SSID in the panel (stale snapshot); assert disk link survives.

### B4. silent divergence
**Why missed:** tests assert the happy-path persist; none assert that a REJECTED persist rolls back the optimistic UI state.
**Catch test:** make `packet_config_set` reject; call `setLink`; assert `config` reverts (or shows error) rather than holding the un-persisted value.

### Testing Pitfalls Updates
Candidate (to be added to `docs/pitfalls/testing-pitfalls.md` during the fix): **"Absent-field-erases / multi-writer clobber."** When a persisted record is full-replaced from a DTO and a writer omits a field (or holds a stale snapshot), unit tests of each writer pass while the integration silently erases stored data. Test: writer A sets field X, then writer B (omitting X / holding a pre-A snapshot) persists; assert X survives on disk. Deferred to the fix PR (read-only hunt does not edit tracked docs).
