# Bug Hunt Report — link/transport config persistence across restart (tuxlink-hoi1)

Agent: sumac-vetch-dahlia (read-only hunt)
Worktree: worktrees/bd-tuxlink-ube7-aprs-statusbar-and-caps

## Scope

Radio link/transport config persistence across app restart. Operator-confirmed
symptom: transport selection (`linkKind`, esp. UV-Pro native) is lost across
restart; APRS connect strip shows "no link" after a working UV-Pro setup;
device-level fields seem retained.

Files analyzed (source only, no tests):
- `src-tauri/src/config.rs` — `Config`, `PacketConfig`, `deserialize_lenient_link`,
  `read_config`, `write_config_atomic`, `validate`, schema-version handling.
- `src-tauri/src/ui_commands.rs` — `PacketConfigDto`, `From<&PacketConfig>`,
  `into_packet_config`, `packet_config_get`/`packet_config_set`,
  `AprsConfigDto`/`aprs_config_set`.
- `src-tauri/src/modem_commands.rs` — `config_get_ardop`/`config_set_ardop`.
- `src-tauri/src/winlink/ax25/link.rs` — `KissLinkConfig` enum + serde.
- `src/packet/usePacketConfig.ts`, `src/packet/packetTypes.ts`,
  `src/radio/sections/ModemLinkSection.tsx`, `src/aprs/AprsConnectStrip.tsx`,
  `src/shell/AppShell.tsx`.

All five passes performed: contract violations, cross-sibling patterns, failure
modes, concurrency/ordering, error propagation.

---

## Bugs

### 1. AprsConnectStrip never feeds persisted link fields into the picker → re-opening setup wipes the link on next emit

**Location:** `src/aprs/AprsConnectStrip.tsx:189-193` (the `<ModemLinkSection>` render);
interacts with `src/radio/sections/ModemLinkSection.tsx:186,202` (empty-input fallback).
**Severity:** critical
**Evidence:**
`AprsConnectStrip` renders the picker with only `kind` and `allowUvproNative`:
```tsx
<ModemLinkSection
  kind={pickerKind(linkKind)}
  allowUvproNative={allowUvproNative}
  onChange={onLinkChange}
/>
```
It passes NONE of `host`, `port`, `serialDevice`, `serialBaud`, `btMac` — even
though those props exist on `ModemLinkSectionProps` (ModemLinkSection.tsx:62-71)
and the persisted DTO carries them (`packetConfig.config.btMac` is available in
AppShell — it is read at AppShell.tsx:575-577 to build `radioLabel`). The strip
deliberately stays "presentational" but drops the address fields on the floor.

Consequence inside `ModemLinkSection`: `btMacInput` initializes to `btMac ?? ''`
= `''` (line 119), and the empty-input fallback in `emit` is
`btMac: mac || (btMac ?? null)` (line 202) where the `btMac` PROP is `undefined`
→ resolves to `null`. So any `emit` triggered for the bt/uvpro segment before the
operator re-types the MAC produces `{ linkKind: 'UvproNative', btMac: null }`.
`selectSegment` (line 207-210) calls `emit(seg)` on EVERY segment click, and the
re-seed `useEffect` (line 131-138) also fires when props change. The same hole
exists for TCP (host/port fall back to hardcoded defaults `127.0.0.1:8001`) and
USB (`serialDevice ?? null` → `null`).
**Impact:** After restart the config loads correctly (linkKind=UvproNative,
btMac=AA:..), and the strip's `radioLabel` shows the right device. But the moment
the operator touches the ⚙ setup picker — clicks the UV-Pro segment, or any
segment toggle — `emit` fires with `btMac: null` (UvproNative/BT) or with the
default `127.0.0.1:8001` (TCP, silently overwriting a real TCP host). That null
MAC is persisted, replacing the working link. This is the most direct match to
"loses the UV-Pro link after a working setup": the picker can only DESTROY the
loaded link, never preserve+edit it, because it was never told what the loaded
link was.
**Found in:** Pass 1 — Contract Violations (prop contract: picker advertises
host/port/btMac props the caller refuses to supply).

### 2. `setLink` persist of `UvproNative`/`Bluetooth` with empty MAC fails silently in the backend; the error is swallowed in the frontend

**Location:** backend `src-tauri/src/ui_commands.rs:3648-3652`
(`into_packet_config` UvproNative arm) + `:3643-3647` (Bluetooth arm);
frontend `src/packet/usePacketConfig.ts:148-151` (`setLink` `.catch(() => undefined)`).
**Severity:** significant
**Evidence:**
When bug #1 emits `{ linkKind: 'UvproNative', btMac: null }`, `setLink` builds
`next = { ...config, linkKind: 'UvproNative', btMac: null }` and calls
`invoke('packet_config_set', { dto: next })`. The backend's `into_packet_config`
does:
```rust
Some("UvproNative") => Some(KissLinkConfig::UvproNative {
    mac: self.bt_mac.ok_or_else(|| UiError::Internal {
        detail: "UvproNative link needs bt_mac".into() })?,
}),
```
`bt_mac` is `None` → returns `Err(UiError::Internal{..})`, so `packet_config_set`
returns Err and writes NOTHING (good — no half-write). But the frontend swallows
it: `setLink` ends `.then(() => undefined).catch(() => undefined)` (lines 149-150),
and `setSsid` likewise `.catch(() => { /* surface via session log */ })`
(usePacketConfig.ts:124-126). No session-log write actually happens in this hook;
the comment is aspirational. Meanwhile the OPTIMISTIC `setConfig(next)` at line 139
already updated the in-memory UI to the bad/dropped state, and broadcast it via the
CustomEvent (line 143).
**Impact:** Two compounding harms. (a) The persist failure is invisible — the
operator gets no error, the UI optimistically shows the (now broken) link, and disk
still has the OLD value, so UI and disk diverge until next reload. (b) When the emit
carries a VALID change (e.g. a real new MAC) but the backend rejects for an unrelated
reason, the operator believes it saved. The swallow also masks the schema-version /
write failures from Bug #5. The optimistic-update-then-silently-fail pattern is the
mechanism by which a user perceives "I set it, but it didn't stick."
**Found in:** Pass 5 — Error Propagation.

### 3. `KissLinkConfig::ManagedDireWolf` (Managed) link is silently DROPPED by `setLink`'s partial merge

**Location:** `src/packet/usePacketConfig.ts:137` (`{ ...config, ...fields }`) +
`src/radio/sections/ModemLinkSection.tsx:46-56` (`ModemLinkFields` has no
`linkKind: 'Managed'` and no `managedAudioDevice`/`managedPtt`).
**Severity:** significant
**Evidence:**
`ModemLinkFields.linkKind` is typed `'Tcp' | 'Serial' | 'Bluetooth' | 'UvproNative'`
— it CANNOT carry `'Managed'`, and the field set has no `managedAudioDevice`/
`managedPtt`. `setLink` merges `next = { ...config, ...fields }`, so when the picker
emits any non-Managed kind, `linkKind` flips to e.g. `'Serial'` but the stale
`managedAudioDevice`/`managedPtt` from `config` REMAIN on the DTO (they are not in
`fields`, so the spread leaves them). On the backend `into_packet_config` for
`Some("Serial")` ignores them, so they are dropped on write — fine. But the reverse
is impossible: there is no UI path through `ModemLinkSection`/`setLink` that can ever
RE-select `Managed`, because the picker has no Managed segment and `ModemLinkFields`
can't express it. If a config was written with `linkKind: 'Managed'` (e.g. by the
managed-Dire-Wolf P5/P7 flow elsewhere), the APRS strip's `pickerKind` maps it to
`'Serial'` (AprsConnectStrip.tsx:69, the fall-through `return 'Serial'`), so opening
the picker mis-renders a Managed link as USB and the first emit converts it to a
broken Serial link.
**Impact:** A Managed-Dire-Wolf link is unrepresentable in the shared picker and is
silently converted to (or mis-shown as) Serial the moment the APRS setup picker is
opened. For the APRS/packet shared-config surface this is a latent transport-loss
path parallel to #1.
**Found in:** Pass 2 — Cross-Sibling Pattern Violations (the DTO supports 5 link
kinds; the picker's field-set type supports 4; Managed is the dropped sibling).

### 4. Stale closure: `onAprsConnect` captures `aprsLinkKind` from a prior render → connect can use the wrong transport after an in-session link change

**Location:** `src/shell/AppShell.tsx:597-616` (`onAprsConnect` deps `[aprsLinkKind]`)
vs `:591-596` (`onAprsLinkChange`) and `:565` (`aprsLinkKind` derived from
`packetConfig.config?.linkKind`).
**Severity:** minor (mitigated, but a real ordering hole)
**Evidence:**
`onAprsConnect` reads `aprsLinkKind` (line 600, 615), which is closured from
`packetConfig.config?.linkKind` at render time. The persist path is async:
`onAprsLinkChange` → `setLink` → optimistic `setConfig(next)` + CustomEvent →
`packet_config_set`. The optimistic `setConfig` does re-render AppShell (so
`aprsLinkKind` updates and `onAprsConnect` is rebuilt). The `await
aprsLinkPersist.current` at line 599 waits for the WRITE, not for React's state
commit. If the operator picks UV-Pro and immediately clicks Connect within the same
tick before the optimistic re-render commits, `onAprsConnect`'s closured
`aprsLinkKind` can still be the OLD value → it takes the KISS branch
(`aprs_listen_start` only) for what is actually a UvproNative link, skipping
`uvpro_connect`. The `aprsActiveTransport.current = aprsLinkKind` at line 615 then
records the wrong transport, so disconnect (line 624) also mis-routes.
**Impact:** A fast pick-then-connect can arm the listener on the wrong transport
(no UV-Pro session opened), and teardown won't clean up the UV-Pro session it never
opened — or, in the inverse race, tries to disconnect a session that was opened.
Low frequency (requires sub-render-tick timing), but it is a genuine
stale-closure/ordering defect, not hypothetical: the same render that updates
`packetConfig.config` is the only thing that updates `aprsLinkKind`.
**Found in:** Pass 4 — Concurrency / Ordering.

### 5. Whole-config read fails closed on ANY schema mismatch or single-field validation error → all sticky settings (incl. link) revert to defaults

**Location:** `src-tauri/src/config.rs:654-658` (`read_config`) +
`:502-514` (`deserialize_schema_version` hard-errors on mismatch) +
`src/packet/usePacketConfig.ts:67-73` (`packet_config_get` reject → leave `null`,
"UI uses default 0") and `src-tauri/src/modem_commands.rs:30-34`
(`config_get_ardop` `.unwrap_or_default()` on ANY read error).
**Severity:** significant
**Evidence:**
`read_config` runs `serde_json::from_slice` then `config.validate()`, returning Err
on the FIRST failure of either. `deserialize_schema_version` hard-fails if
`schema_version != CONFIG_SCHEMA_VERSION` (lines 507-512), and `Config` is
`#[serde(deny_unknown_fields)]` (line 181) so ANY stray/forward field fails the whole
read. The frontend treats a failed `packet_config_get` as "pre-wizard, use defaults"
(usePacketConfig.ts:71-73) and `config_get_ardop` collapses ANY read error to
`ArdopUiConfig::default()` (modem_commands.rs:33). So a config that fails to
deserialize for ANY reason — a forward field written by a newer dev build, a
schema-version bump, a single bad sub-field — makes every consumer behave as if NO
config exists: `linkKind` reads as `null` ("no link"), SSID reads as 0, ARDOP/VARA
revert to defaults. Note this is BROADER than the lenient `packet.link` path: the
`deserialize_lenient_link` helper (config.rs:469-475) only protects the `link` field
from an unknown-variant error; it does NOT protect against a sibling field or the
top-level `deny_unknown_fields` failing, which takes the whole config (link included)
down with it.
**Impact:** Matches the operator report that the symptom spans "APRS tac chat AND
other modes" and presents as "no link": one cross-build schema skew or one
unmigrated field reverts EVERYTHING to defaults at read time, not just the link.
The device-level fields "seeming retained" is consistent with those being read from a
DIFFERENT store (IdentityStore / keyring) that doesn't go through this fail-closed
`read_config`.
**Found in:** Pass 3 — Failure Mode Reasoning.

### 6. `into_packet_config`: absent `link_kind` silently yields `link: None`, and `From<&PacketConfig>` cannot distinguish "no link" from "link the DTO couldn't express"

**Location:** `src-tauri/src/ui_commands.rs:3661` (`None => None`) +
`:3589` (`From` `None` arm).
**Severity:** significant (the verified prior finding — confirmed + bounded)
**Evidence:**
`into_packet_config` maps `link_kind: None => None` (line 3661), and
`packet_config_set` does a FULL replace: `cfg.packet = dto.into_packet_config()?`
(ui_commands.rs:3702). So any caller that sends a DTO with `linkKind` absent/null
wipes `cfg.packet.link` to `None` on write — there is no field-level preservation.
The DTO round-trips every KissLinkConfig variant faithfully in BOTH directions
(`From<&PacketConfig>` lines 3566-3618 and `into_packet_config` lines 3624-3682
cover Tcp/Serial/Bluetooth/UvproNative/Managed symmetrically — UvproNative {mac} →
bt_mac → UvproNative {mac} confirmed at 3581-3582 / 3648-3651), so the DTO mapping
itself is NOT the leak. The leak is the FULL-REPLACE contract combined with callers
(#1, #2, #3) that hand it a DTO whose link can't be reconstructed. This is the
mechanism the prior finding flagged; the unpinned caller is now pinned: it is the
APRS `onLinkChange` → `setLink` path feeding a `null`-MAC/absent-link DTO produced
by Bug #1's prop-starved picker.
**Impact:** `packet_config_set` is a loaded gun: any DTO with a missing/invalid link
silently nulls the persisted link. Confirmed end-to-end with #1 as the trigger.
**Found in:** Pass 1 — Contract Violations (verifying the prior finding).

---

## Cross-sibling comparison (Pass 2 summary)

All three persistence setters use the correct read → mutate-one-section →
`write_config_atomic` pattern and full-replace only their OWN section:
- `packet_config_set` (ui_commands.rs:3696) — replaces `cfg.packet`.
- `aprs_config_set` (ui_commands.rs:3764) — validates path first, replaces `cfg.aprs`.
- `config_set_ardop` (modem_commands.rs:40) — replaces `cfg.modem_ardop`.
- (VARA mirrors ARDOP — `config_set_vara`/`config_get_vara` in modem_commands.rs.)

No sibling NUKES another section. The ARDOP/VARA setters take a fully-typed
`ArdopUiConfig`/`VaraUiConfig` (no flat DTO, no optional-field reconstruction), so
they have NO analog of the link-reconstruction hole — their persistence is robust.
The packet path is the lone deviant BECAUSE it flattens a tagged enum
(`KissLinkConfig`) into a flat DTO and reconstructs it from `link_kind` + scalar
fields, which is exactly where #1/#2/#6 live. The ARDOP/VARA "device-level fields"
robustness is consistent with the operator's "device-level fields seem retained"
observation.

One read-side asymmetry: `config_get_ardop` collapses ANY read error to
`::default()` (modem_commands.rs:33) — so an ARDOP config DOES exist after a
fail-closed read but reads as all-defaults (Bug #5), whereas `packet_config_get`
rejects and the frontend shows `null`/"no link". Same root cause (fail-closed
`read_config`), different surface symptom.

---

## Design Concerns

- **Flat-DTO reconstruction of a tagged enum is fragile by construction.** The
  link is the only config field marshalled by exploding a Rust enum into flat
  optional scalars and rebuilding it. Every bug above (#1/#2/#3/#6) traces to that
  flattening. A future refactor that sends `KissLinkConfig` as a tagged object on the
  wire (like `managedAudioDevice`/`managedPtt` already are) would eliminate the
  "which scalar belongs to which kind / what if it's missing" class entirely.
- **Optimistic-update-then-swallow is a divergence engine.** `setSsid`/`setLink`
  update UI state and broadcast BEFORE the persist resolves, then `.catch(() =>
  undefined)`. UI and disk silently diverge on any backend rejection. At minimum the
  persist failure should roll back the optimistic state and surface an inline error
  (the strip already has an `error` slot — AprsConnectStrip.tsx:197-201 — but
  `onLinkChange` is fire-and-forget and never feeds it).
- **No backend `packet_config:change` event** (usePacketConfig.ts:55-57, 86-90 wire
  a listener for an event the backend never emits — confirmed: no `emit("packet_config:change")`
  anywhere in src-tauri). Cross-window/cross-surface staleness is unguarded; the
  same-window CustomEvent is the only sync. Not the restart-loss root cause, but it
  means a wizard-window write won't refresh the main shell without a reload.
- **Fail-closed whole-config read with no field-level degradation** (#5). Only
  `packet.link` has a lenient path. A single forward/skewed field reverts ALL sticky
  settings to defaults. Consider per-section lenient deserialize (the `link`
  precedent) or a recovery path that preserves parseable sections.
