# Bug Hunt Report — Radio link/transport config persistence (tuxlink-hoi1)

Agent: sumac-vetch-dahlia (read-only exploratory hunt)
Worktree: `worktrees/bd-tuxlink-ube7-aprs-statusbar-and-caps`
Date: 2026-06-15

## Scope

Operator-confirmed symptom: link/transport configuration (especially `linkKind`,
notably UV-Pro native) does NOT persist across an app restart; the APRS connect
strip shows "no link / not set up" after a working UV-Pro setup. Physical
device-level fields seem retained but the transport selection is dropped.

Files explored deeply:
- Backend persistence: `src-tauri/src/ui_commands.rs` (`PacketConfigDto`,
  `into_packet_config`, `packet_config_get/set`, `aprs_transport_from_link`),
  `src-tauri/src/config.rs` (`Config`, `PacketConfig`, `deserialize_lenient_link`,
  `read_config`, `write_config_atomic`, `validate`),
  `src-tauri/src/winlink/ax25/link.rs` (`KissLinkConfig`),
  `src-tauri/src/modem_commands.rs` + `winlink/modem/vara/commands.rs`
  (ARDOP/VARA writers).
- Frontend writers: `src/packet/usePacketConfig.ts`,
  `src/radio/modes/PacketRadioPanel.tsx`, `src/radio/sections/ModemLinkSection.tsx`,
  `src/aprs/AprsConnectStrip.tsx`, `src/shell/AppShell.tsx`,
  `src/radio/useVaraConfig.ts`, `src/radio/modes/ArdopRadioPanel.tsx`.

## Verification of the prior finding (confirmed, with the exact wiping caller pinned)

The prior finding is **correct**: `packet_config_set` is a full replace
(`ui_commands.rs:3702` `cfg.packet = dto.into_packet_config()?`), and
`into_packet_config` (`ui_commands.rs:3626`, `None => None` at line 3661) maps an
absent/null `link_kind` → `link: None`. So any persist of a DTO whose `linkKind`
is null wipes the saved link. The backend never emits `packet_config:change`
(confirmed: no emit anywhere; `usePacketConfig.ts:88` listens for an event that is
never fired). The ARDOP/VARA writers do NOT share this exact hazard (see Bug 3).

The previously-UNPINNED question — *which caller persists a `linkKind`-less / stale
DTO across a restart* — is answered below as **Bug 1** (the central defect) and
**Bug 2** (a corruption that feeds Bug 1).

## Bugs

### Bug 1 — PacketRadioPanel persists a STALE config snapshot, wiping a link configured elsewhere (THE root cause)

**Location:** `src/radio/modes/PacketRadioPanel.tsx:132-150` (load-once effect),
`:167-177` (`persistDto`), `:184` / `:189` / `:218` (the merge-from-stale callers).
**Severity:** critical

**Evidence:**
`PacketRadioPanel` keeps its OWN `config` state, loaded exactly once on mount via
`packet_config_get` (effect deps `[]`, lines 132-150). It does **not** subscribe to
the same-window `tuxlink:packet-config:change` CustomEvent and never re-loads.
Contrast `usePacketConfig.ts:77-84`, which DOES listen and re-seed.

Every PacketRadioPanel write is a full read-modify-write off that frozen snapshot:
```
persistDto({ ...config, ssid: n });                 // onSsidChange  (:184)
persistDto({ ...config, ...fields });               // onLinkChange  (:189)
persistDto({ ...config, linkKind:'Managed', ... }); // onManagedChange (:218)
```
and `persistDto` (`:174`) calls `packet_config_set` with that full DTO — a backend
full-replace.

The AppShell-side APRS connect strip writes the link through a *different* state
container (`usePacketConfig`, AppShell.tsx:549 → `setLink`). The two containers are
NOT kept in sync on the PacketRadioPanel side, because PacketRadioPanel ignores the
broadcast event.

**Concrete repro (matches the operator symptom exactly):**
1. Open the Packet radio panel. It loads config; say `link` is null or a TCP link.
   PacketRadioPanel's `config` snapshot is now frozen with `linkKind = null|Tcp`.
2. Configure UV-Pro through the APRS connect strip (AppShell hook → `setLink` →
   persists `linkKind:'UvproNative', btMac:<mac>` to disk). Disk + AppShell hook now
   correct. PacketRadioPanel's snapshot is now STALE.
3. Back in the Packet panel, change anything — SSID dropdown (`:181-185`), switch to
   Managed, or edit a modem field. `persistDto({ ...config, ... })` re-serializes the
   STALE snapshot whose `linkKind` is still null/Tcp, and full-replaces the on-disk
   `[packet]` section — **wiping UvproNative**.
4. Restart → APRS strip reads `link: None` → "no link / not set up".

This also fires WITHOUT touching the Packet panel post-setup if step 1 happened in
the same app run before step 2 (panel mounted earlier, stale snapshot retained for
the session). The SSID control is a particularly easy unwitting trigger because it
feels unrelated to the link.

**Impact:** Silent loss of the operator's transport selection. "Physical
device-level fields seem retained" because the OTHER writer (the strip / ribbon hook)
still holds them in its own state for the live session — only the persisted file is
clobbered, so the loss surfaces on restart. Exactly the reported bug.

### Bug 2 — AprsConnectStrip's ModemLinkSection drops all field props; a segment re-tap emits a null-MAC DTO and corrupts in-memory state

**Location:** `src/aprs/AprsConnectStrip.tsx:189-193` (no field props passed),
`src/radio/sections/ModemLinkSection.tsx:195-203` + `:207-210` (`selectSegment` →
`emit` with empty input), `src/packet/usePacketConfig.ts:137-139` (optimistic update
applied even when the persist will fail).
**Severity:** significant

**Evidence:**
`AprsConnectStrip` renders `ModemLinkSection` with only `kind` + `allowUvproNative`
— it passes NO `host`/`port`/`serialDevice`/`serialBaud`/`btMac` (lines 189-193).
So after a UV-Pro link is configured and the operator re-opens the ⚙ setup, the
section initializes `btMacInput = ''` (ModemLinkSection.tsx:119, `btMac ?? ''`).

If the operator taps the UV-Pro (or BT) segment button, `selectSegment('uvpro')`
calls `emit('uvpro')` with no overrides; `mac = btMacInput.trim()` is `''`, and
`mac || (btMac ?? null)` resolves to `null` because the `btMac` prop is also
undefined (ModemLinkSection.tsx:195-203). `onChange({ linkKind:'UvproNative',
btMac:null })` fires.

On the backend, `into_packet_config` for `UvproNative` requires `bt_mac`
(`ui_commands.rs:3648-3652`) and returns `Err` — so `write_config_atomic` is NOT
reached and the DISK link is preserved. BUT `usePacketConfig.setLink` already
applied the optimistic local update (`usePacketConfig.ts:137-139`,
`setConfig(next)`) and broadcast it before awaiting the (failing) persist
(`:148-150` swallows the error). The live `config` now holds `btMac:null` /
no-link, disagreeing with disk.

**Impact:** Two consequences. (a) The APRS strip / ribbon now show "no link"
mid-session even though disk is intact — a confusing UX that mimics the persistence
bug. (b) Worse, this corrupted in-memory `config` is the snapshot the SHARED hook's
next `setSsid`/`setLink` reads from; a subsequent SSID change would then persist the
null link to disk for real (full-replace), turning the transient corruption into a
durable wipe — a second independent route into the Bug-1 symptom.

### Bug 3 — ARDOP/VARA: same stale-snapshot read-modify-write pattern (lower severity, no nullable discriminator)

**Location:** `src/radio/modes/ArdopRadioPanel.tsx:381-407` (load-once, no event
listener) + `:412-419` (`persistArdop` merges into the frozen `ardopConfig`);
backend `src-tauri/src/modem_commands.rs:40-44` (`config_set_ardop` full-replaces
`modem_ardop`). VARA: `src/radio/useVaraConfig.ts:65-103` + backend
`winlink/modem/vara/commands.rs:993`.
**Severity:** minor

**Evidence:**
ARDOP's `persistArdop({...ardopConfig, ...patch})` uses the once-loaded snapshot
just like Bug 1, and the backend full-replaces the whole `modem_ardop` sub-struct.
However the hazard is materially lower: `ArdopUiConfig`/`VaraUiConfig` have no
optional discriminator field that maps absent→cleared (every field is concrete and
always serialized), so a stale merge only risks reverting a *concurrently-edited
sibling field*, not nuking the whole transport. It bites only if two ARDOP (or two
VARA) editors are mounted at once with divergent snapshots — which the current UI
does not do. VARA's hook (`useVaraConfig`) DOES subscribe to its broadcast event
(`useVaraConfig.ts:71-78`), so it is the correctly-wired reference; ARDOP does not
(no `tuxlink:ardop-config:change` listener) and is the weaker of the two.

**Impact:** Latent. Not the reported bug, but the same class; worth hardening when
Bug 1 is fixed so the fix pattern is consistent across all three modes.

## Threads checked and CLEARED (so the fix doesn't chase the wrong layer)

- **Read side is sound.** `deserialize_lenient_link` (`config.rs:469-475`) reads to
  `serde_json::Value` then `from_value::<KissLinkConfig>().ok()`. `KissLinkConfig` is
  an externally-tagged enum (`link.rs:35-70`), so `UvproNative { mac }` serializes as
  `{"UvproNative":{"mac":...}}` and round-trips cleanly within one build. The
  `.ok()`-swallow only drops a link under genuine cross-build schema skew, which is
  NOT this same-build restart symptom.
- **`deny_unknown_fields` / schema_version** do not silently drop the link:
  `PacketConfig` carries `#[serde(deny_unknown_fields, default)]` (`config.rs:447`);
  an unknown field hard-errors the whole read (it does not partial-drop), and a
  schema mismatch errors too (`config.rs:502-514`). Neither produces a quietly
  link-less but otherwise-valid config.
- **No load-time default/migration resets the link.** `read_config`
  (`config.rs:645-659`) is read-parse-validate only; `validate` (`:604-626`) only
  range-checks SSID and identity. The `#[serde(default)]` on `Config.packet`
  (`:198`) only fills a wholly-absent `[packet]` section, not a present one.
- **The same-hook (`usePacketConfig`) `setSsid`/`setLink` pair is internally
  coherent.** Both are `useCallback([config])` and the local CustomEvent listener
  re-seeds `config` (`usePacketConfig.ts:77-84`), so writes within the AppShell hook
  instance don't clobber each other. The staleness is strictly CROSS-component
  (PacketRadioPanel's independent state), per Bug 1.
- **`aprs_transport_from_link` / `aprs_config_set`** are not implicated — `aprs_config_set`
  only touches `cfg.aprs`, never `cfg.packet.link`.

## Design Concerns

- **Three independent full-replace writers of one `[packet]` section, synced only by
  a fire-and-forget CustomEvent that one of them ignores.** `usePacketConfig` (hook),
  `PacketRadioPanel` (own `useState`), and `ManagedModemSection`-via-PacketRadioPanel
  all `packet_config_set` the entire section. The intended sync channel
  (`tuxlink:packet-config:change`) is observed by `usePacketConfig` but NOT by
  `PacketRadioPanel`, so the panel's snapshot goes stale and overwrites. The robust
  fixes (any of): (a) PacketRadioPanel subscribes to the broadcast + re-seeds (mirror
  `usePacketConfig.ts:77-84`); (b) PacketRadioPanel consumes the shared
  `usePacketConfig` hook instead of its own state; (c) make `packet_config_set` a
  field-level PATCH (only mutate fields present in the DTO) instead of a full replace,
  removing the absent-link→None foot-gun entirely; (d) backend emits
  `packet_config:change` so all listeners re-sync after every write.
- **A nullable discriminator (`link_kind: Option<String>`) on a full-replace DTO is a
  structural foot-gun.** Absent/null silently means "delete the link." Any
  partial/stale/error-path DTO clears it. The Managed path (`PacketRadioPanel.tsx:218`)
  deliberately nulls the BYO scalars, which is correct THERE, but proves how easy it is
  for a writer to null fields it didn't mean to drop.
- **Optimistic local update is applied even when the persist will reject** (Bug 2):
  `usePacketConfig.setLink` (`:137-150`) sets + broadcasts the optimistic value, then
  swallows the persist rejection. State and disk diverge on any backend validation
  failure (e.g. UvproNative with null MAC). Consider rolling back the optimistic
  update on reject, or validating the field set before the optimistic apply.
- **AprsConnectStrip renders the link editor with no current-value props**
  (`AprsConnectStrip.tsx:189-193`), so the editor always starts "empty" for an
  already-configured link. Pass the persisted `host/port/serialDevice/serialBaud/btMac`
  through so a re-open / segment-tap doesn't synthesize a null-field DTO.
