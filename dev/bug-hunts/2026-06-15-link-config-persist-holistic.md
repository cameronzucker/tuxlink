# Bug Hunt Report — radio link/transport config persistence across restart (tuxlink-hoi1)

## Scope

Holistic read of the complete write→disk→read round-trip for the packet/APRS
link config, and the analogous ARDOP/VARA writers.

Files read in full or in the relevant region:

- Backend: `src-tauri/src/ui_commands.rs` (`PacketConfigDto`, `From<&PacketConfig>`,
  `into_packet_config`, `packet_config_get/set`, `aprs_config_get/set`,
  `aprs_transport_from_link`, `aprs_listen_start`, the round-trip tests),
  `src-tauri/src/config.rs` (`Config`, `PacketConfig`, `deserialize_lenient_link`,
  `read_config`, `write_config_atomic`, `validate`, `ArdopUiConfig`,
  `VaraUiConfig`, the round-trip tests), `src-tauri/src/winlink/ax25/link.rs`
  (`KissLinkConfig`), `src-tauri/src/modem_commands.rs` (`config_get_ardop`,
  `config_set_ardop`), `src-tauri/src/winlink_backend.rs` (`set_config`).
- Frontend: `src/packet/usePacketConfig.ts`, `src/packet/packetTypes.ts`,
  `src/radio/sections/ModemLinkSection.tsx`, `src/radio/sections/ManagedModemSection.tsx`,
  `src/radio/modes/PacketRadioPanel.tsx`, `src/radio/modes/ArdopRadioPanel.tsx`,
  `src/aprs/AprsConnectStrip.tsx`, `src/shell/AppShell.tsx`.

I did NOT run a cold cargo build (contended Pi). The backend serde round-trip is
asserted by existing unit tests (`packet_config_dto_round_trips_uvpro_native`,
`packet_config_round_trips_with_sticky_ssid_and_link`, the `KissLinkConfig`
serde tests) and I reasoned about it from the serde representation rather than
re-running it.

## Summary of the round-trip

The backend serde round-trip is symmetric and correct. `KissLinkConfig` is an
externally-tagged enum (`UvproNative { mac }` → `{"UvproNative":{"mac":"…"}}`),
`From<&PacketConfig>`/`into_packet_config` map it losslessly to/from the flat
`PacketConfigDto`, and `deserialize_lenient_link` accepts every variant the same
binary writes. There is no asymmetry that yields `None` on reload for a config
written and read by the SAME build. The persistence defect is in the WRITE
contract (full-replace) plus the FRONTEND read-modify-write that can feed a
link-less / device-less DTO into that full-replace. Both wipe the saved link
on disk, which is exactly the operator's "shows no link after a working setup".

## Bugs

### 1. `packet_config_set` is a destructive full-replace — an absent `linkKind` wipes the saved link (device + kind together)

**Location:** `src-tauri/src/ui_commands.rs:3696-3712` (`packet_config_set`) +
`src-tauri/src/ui_commands.rs:3624-3667` (`into_packet_config`, `link_kind: None
=> link: None`).

**Evidence:** `packet_config_set` does `cfg.packet = dto.into_packet_config()?`
— a whole-section replace, not a merge. `into_packet_config` maps
`self.link_kind == None` to `link: None`. So ANY DTO persisted with
`linkKind == null` overwrites the on-disk `packet.link` with `None`, discarding
both the kind AND the device (host/MAC/serial path all live inside the single
`PacketConfig.link: Option<KissLinkConfig>`). `ssid`, `params`, and
`listen_default` are independent fields, so they survive — matching "the rest is
retained, the link is gone."

**Impact:** A single persist with a missing `linkKind` permanently clears a
configured link on disk. After the next `packet_config_get`/restart, the APRS
connect strip and packet panel read `linkKind: null` → "no link / not set up,"
even after a working UV-Pro setup. This is the destructive primitive every other
bug below feeds into.

### 2. `AprsConnectStrip` mounts `ModemLinkSection` with no field props, so editing through the APRS strip persists `btMac: null` (link cleared / rejected)

**Location:** `src/aprs/AprsConnectStrip.tsx:189-194` (renders
`<ModemLinkSection kind={pickerKind(linkKind)} allowUvproNative=… onChange=… />`
— NO `host`/`port`/`serialDevice`/`serialBaud`/`btMac`), against
`src/radio/sections/ModemLinkSection.tsx:131-205`.

**Evidence:** `ModemLinkSection` seeds its editable input state from props
(`btMacInput = btMac ?? ''`, re-seeded by the effect at lines 131-138 on every
prop change). Because `AprsConnectStrip` passes none of those props, `btMacInput`
is always `''` even when a UV-Pro/Bluetooth link with a real MAC is persisted.
`emit()` (lines 164-205) then computes `btMac: mac || (btMac ?? null)` =
`'' || (undefined ?? null)` = `null`. Contrast `PacketRadioPanel.tsx:336-353`,
which DOES pass `btMac={config?.btMac ?? undefined}` (and host/port/serial), so
the same component re-seeds correctly there. The APRS strip is the one sibling
that violates the prop contract.

Two concrete failure paths from the APRS strip:
- **Segment click:** `selectSegment('uvpro')` → `emit('uvpro')` with no override
  → `onChange({ linkKind: 'UvproNative', btMac: null, … })`. Backend
  `into_packet_config` then returns `Err("UvproNative link needs bt_mac")`
  (ui_commands.rs:3648-3652); the persist is swallowed (`.catch(() => undefined)`
  in `setLink`, usePacketConfig.ts:148-151). The optimistic `setConfig(next)` has
  already flipped JS state to `btMac: null`, so the UI now disagrees with disk.
- **Blur of the BT/manual field while it shows empty (the re-seeded ''):**
  `emit(segment)` likewise emits `btMac: null`.

**Impact:** Re-opening the APRS transport editor and touching the picker (a
segment switch, a blur) either persists a link with `btMac: null` (clearing the
device) or sends a DTO the backend rejects — leaving JS state and disk
inconsistent. After reload the strip reports "no link." This is the most direct
match for the operator's "worked, then showed no link" via the APRS surface.

### 3. Cross-mode full-replace: `config_set_ardop` / `config_set_vara` (and every other section writer) re-persist `packet.link` through `read_config`, so any lenient-deser degradation becomes permanent

**Location:** `src-tauri/src/modem_commands.rs:40-44` (`config_set_ardop`:
`read_config()` → mutate `modem_ardop` → `write_config_atomic`), the analogous
VARA writer, and the 13 other `write_config_atomic` callers in `ui_commands.rs`
(3239, 3776, 4391, 6037, 6113, 6271, 6317, 6357, 6434, 6452, 6472, 7721) +
`wizard.rs:223,431`. Round-trips through `config::read_config`
(`config.rs:645-659`) which uses `deserialize_lenient_link`
(`config.rs:469-475`).

**Evidence:** Every section writer reads the whole `Config` via `read_config()`,
mutates ONE section, and writes the whole `Config` back. `read_config` parses
`packet.link` through `deserialize_lenient_link`, which intentionally degrades
any `KissLinkConfig` it cannot parse to `None` (`serde_json::from_value(...).ok()`).
If the on-disk `packet.link` is ever a shape this binary cannot parse — the
documented forward/sideways dev-build schema skew the lenient deser was built
for (a config touched by a build that knew a variant or field this build does
not, e.g. mixed worktree builds writing the shared `~/.config/tuxlink/config.json`
per `project_worktree_dev_port_collision`) — then read-side silently yields
`link: None`, and the very next save of ANY unrelated section (changing an ARDOP
device, a VARA port, a CMS host, completing the wizard) re-serializes
`packet.link: null` and PERMANENTLY destroys the link. No `packet_config_set`
call is needed; touching ARDOP settings wipes the packet/APRS link. This is the
"same for other modes" the operator reported: the destructive primitive is the
shared full-config rewrite, not anything APRS-specific.

**Impact:** A link configured under one build (or one field-set) is silently
erased by an unrelated settings change after a skew event, with no error
surfaced. Because the lenient deser swallows the cause, the operator sees only
the downstream "no link" symptom and cannot tell why.

### 4. The promised `packet_config:change` event is never emitted, so concurrent consumers run read-modify-write on a stale snapshot and clobber each other

**Location:** consumers listen at `src/packet/usePacketConfig.ts:88-100`
(comment: "present when/if the backend chooses to emit it"); no backend
`emit("packet_config:change", …)` exists. `packet_config_set`
(ui_commands.rs:3696-3712) emits nothing. Two independent config copies exist:
`usePacketConfig` (AppShell + ribbon + APRS strip, usePacketConfig.ts:60) and
`PacketRadioPanel`'s own `useState<PacketConfigDto>` loaded separately at
`PacketRadioPanel.tsx:132-150`.

**Evidence:** `setSsid`/`setLink` (usePacketConfig.ts:112-153) and
`PacketRadioPanel.persistDto` (PacketRadioPanel.tsx:167-177) all do
`{ ...config, ...patch }` against their own `config`, captured via `useCallback`
deps. The same-window `CustomEvent` (`tuxlink:packet-config:change`) bridges the
two `usePacketConfig` instances and `PacketRadioPanel.persistDto` dispatches it
too — but nothing makes `PacketRadioPanel`'s local `setConfig` LISTEN for that
event (only `usePacketConfig` subscribes, lines 77-83). So if the operator edits
the link in `PacketRadioPanel`, then later edits SSID via the ribbon's
`usePacketConfig` whose `config` snapshot predates the link edit, the ribbon
write replays its stale snapshot (with the old/absent link) and overwrites the
newer link on disk via the full-replace of bug #1.

**Impact:** Cross-surface edits race. A link set in one panel can be silently
reverted by a later unrelated edit in another panel whose cached DTO is stale —
again surfacing as a lost link after reload. The dead `packet_config:change`
contract is the missing coordination that would re-seed all consumers from
backend truth after each persist.

## Design Concerns

- **`Option<KissLinkConfig>` couples "which transport" and "which device" into
  one nullable field.** Every write path that produces `link: None` therefore
  loses BOTH at once. A merge-on-set contract (treat absent `linkKind` as
  "unchanged," not "clear") in `packet_config_set` would neutralize bugs #1, #2,
  and #3's blast radius, and is the right canonical fix per the bd-issue note.
- **`deserialize_lenient_link` is a silent data-degrader on a read path that
  feeds full-config rewrites.** Degrade-to-`None` is defensible for a one-shot
  read, but combined with "every section writer rewrites the whole file," it
  upgrades a transient parse skew into permanent loss. Consider preserving the
  raw `serde_json::Value` of an unparseable link and writing it back verbatim
  (round-trip-preserving unknown), or at minimum logging loudly when it fires.
- **Optimistic `setConfig(next)` before an awaited/checked persist** (both
  `setLink` and `persistDto` swallow the persist error) means JS state can
  diverge from disk whenever the backend rejects the DTO (e.g. the `btMac: null`
  UvproNative reject in bug #2). The UI then shows a link that disk does not have
  (or vice versa) until the next `packet_config_get`.
- **`ModemLinkSection`'s prop contract is enforced only by convention.**
  `PacketRadioPanel` honors it; `AprsConnectStrip` does not. The component cannot
  distinguish "no value" from "value is empty," so an unfed prop becomes an
  active "clear this field" on the next emit. Making the address fields
  required-when-`kind`-implies-them, or reading current values from a single
  shared config source rather than re-deriving from per-call props, would make
  the contract un-violable.

## Testing-pitfalls note

The existing tests assert the backend serde round-trip in isolation
(`packet_config_dto_round_trips_uvpro_native`, the disk-level
`packet_config_round_trips_with_sticky_ssid_and_link`) — all green, yet the link
still vanishes in practice. The gap is that no test exercises (a) a persist with
`linkKind` absent against `packet_config_set` to prove the full-replace wipes the
link, (b) a section-foreign writer (`config_set_ardop`) preserving `packet.link`
across the read-rewrite, or (c) the `AprsConnectStrip` editing path that emits
`btMac: null`. A relevant testing-pitfall: *unit round-trip tests of a DTO prove
the serialization is lossless but say nothing about the WRITE CONTRACT (merge vs
full-replace) or about callers that feed a partial/absent-field DTO into it —
add a "persist section A, then persist unrelated section B, assert A survived"
regression and a frontend test asserting the editor never emits a null device
for a non-null kind.*
