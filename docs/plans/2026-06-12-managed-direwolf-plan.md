# Plan: Managed Dire Wolf (tuxlink-yq3l, Slice B)

**Date:** 2026-06-12 · **Agent:** opossum-taiga-hawk · **bd:** tuxlink-yq3l
**Branch:** bd-tuxlink-yq3l/managed-direwolf (off origin/main) · **Worktree:** worktrees/bd-tuxlink-yq3l-managed-direwolf
**Spec:** [docs/design/2026-06-12-managed-modem-onair-accessibility-design.md](../design/2026-06-12-managed-modem-onair-accessibility-design.md) (operator-approved, adversarially reviewed)
**Finishes:** ADR-0015 decision #1 (manage Dire Wolf as tuxlink already manages ardopcf).

## Goal (definition of done)

A non-technical operator gets on VHF FM packet by picking **sound card + PTT line + callsign** in tuxlink's packet panel and clicking Connect. Tuxlink enumerates devices, generates `direwolf.conf`, validates it, spawns + supervises Dire Wolf, connects over loopback KISS-TCP, runs B2F, and on disconnect SIGINT-stops Dire Wolf and releases the audio device. The operator never authors a `.conf`. The existing bring-your-own KISS (Tcp/Serial/Bluetooth) paths remain as the escape hatch. End-to-end reachable from the production packet UI — not a component boundary (alpha = vettedness).

## Cross-cutting constraints (EVERY task obeys)

- **NO cold cargo on this Pi.** Do not run full `cargo build`/`cargo test`. Write code + tests, run only narrow `cargo test --lib --manifest-path src-tauri/Cargo.toml <module>` if it compiles fast, else push to the **DRAFT PR** and let GitHub CI compile both arches. Cheap local gates: `pnpm -C <worktree> exec tsc --noEmit`, scoped `pnpm -C <worktree> vitest run <file>`. Memory: feedback_no_cold_cargo_on_contended_pi, feedback_prefer_cloud_ci_over_local_rust_builds.
- **Pin paths:** absolute `--manifest-path`, `pnpm -C <abs worktree>`. cwd reverts to the main checkout between calls. Memory: pin_paths_in_worktree_sessions.
- **Worktree commits:** standalone `cd <worktree>` call FIRST, then the git op in the NEXT call (the main-checkout-race hook reads payload cwd). Subagents canNOT commit from worktrees — they code+gate+STOP uncommitted; the PARENT (executing session) commits. Memory: worktree_git_hook_cwd_and_mergebase, subagents_cannot_commit_in_worktrees.
- **RADIO-1:** agent authorship of RF-path code is fine (ADR 0018); the agent never transmits. Hard correctness bar: SIGINT must stop Dire Wolf cleanly and **never leave PTT keyed** after a session ends. On-air verification is the operator's weekend smoke (DRA-100 → CDM-1550LS+).
- **TDD per task.** Before work: read `.claude/skills/test-driven-development/` + `docs/pitfalls/testing-pitfalls.md`. Failing test → implement → green. Tests must NOT require a real radio, real Dire Wolf binary, or real sound card — use fixtures + dependency injection (mirror the ardopcf mock-TNC test pattern in `src-tauri/src/winlink/modem/ardop/transport.rs` tests).
- **Pitfalls.** Review each task against `docs/pitfalls/testing-pitfalls.md` + `docs/pitfalls/implementation-pitfalls.md`. Note TEST-1 (jsdom can't detect missing CSS) and the production-mount-path test rule (memory test_production_mount_path_not_just_units) for UI tasks.

## Architecture decisions (pre-made — do NOT relitigate)

1. **Config shape:** add a new variant to `KissLinkConfig` (in `src-tauri/src/winlink/ax25/link.rs`, where the enum lives — NOT kiss.rs): `ManagedDireWolf { audio_device: StableAudioId, ptt: PttChoice }`. The existing `Tcp`/`Serial`/`Bluetooth` variants are untouched (bring-your-own). When the connect path sees `ManagedDireWolf`, tuxlink spawns Dire Wolf and internally dials `Tcp{127.0.0.1, <chosen loopback port>}`. Rationale: KISS link is already the abstraction; managed mode is just a fourth link kind that provisions the TCP endpoint itself. The lenient deserializer at `config.rs:461` keeps old configs forward-compatible.
2. **Managed lifecycle:** mirror ardopcf's `with_managed_modem` / shutdown pattern (`src-tauri/src/winlink/modem/ardop/transport.rs:143`, the SIGINT-clean-stop machinery). Same spawn/supervise/bind-wait/SIGINT shape; reuse helpers where they generalize.
3. **Sound-card arbitration:** tuxlink is the single arbiter (ADR 0015). Before spawning Dire Wolf, ensure no other managed modem (ardopcf) holds the card; on a swap, stop the other first and confirm device release.
4. **Generated conf is fixed-shape, timing-free.** TXDELAY/persistence/slot are pushed as KISS param frames (`push_kiss_params`, `src-tauri/src/winlink/ax25/datalink.rs`) — they MUST NOT appear in the conf. Plain AX.25 only (no FX.25/IL2P; tuxlink's stack has neither). `MODEM 1200` only.
5. **Packaging:** `Recommends: direwolf (>= <min>)`, NOT `Depends:`. Runtime presence-probe + bring-your-own-KISS fallback.

---

## Phase 0 — Executor orientation (no code)

Read, in the worktree, before writing anything:
- The spec doc (above) in full — Slice B, the RADIO-1 section, the open questions.
- `src-tauri/src/winlink/modem/ardop/transport.rs:143-260` + the `with_managed_modem` tests (~:2305-2360) — the spawn/supervise/SIGINT pattern to mirror.
- `src-tauri/src/winlink/modem/ardop/mod.rs:31` (`ArdopConfig`), `src-tauri/src/modem_commands.rs` `build_ardop_extra_args` (~:770-835) — the managed-spawn config + argv-builder pattern.
- `src-tauri/src/config.rs:437-472` (`PacketConfig`, lenient link deser), `src-tauri/src/winlink/ax25/link.rs:36` (`KissLinkConfig`).
- `src-tauri/src/ui_commands.rs:3158-3300` (`PacketConfigDto`, `packet_config_get/set`, the `From<&PacketConfig>` round-trip).
- `src/radio/modes/PacketRadioPanel.tsx` + `.test.tsx` (the production-mounted UI; how `packet_config_get/set` are invoked).
- Confirm `tuxmodem/crates/tux-rig-cm108/src/ptt.rs` exists (reference for CM108 HID resolution semantics — Dire Wolf does the keying via `PTT CM108`, but the device-resolution logic informs the picker).

## Phase 1 — Device discovery (pure Rust, fixture-tested) [no deps]

New module `src-tauri/src/winlink/ax25/devices.rs` (or `audio_devices.rs`).

- **Task 1.1 — Audio enumeration by stable id.** `enumerate_audio_devices(snapshot: &SysSnapshot) -> Vec<AudioDevice>` where `AudioDevice { human_name, alsa_plughw, stable_id }` and `stable_id` derives from `/dev/snd/by-id` symlink or sysfs USB `idVendor:idProduct` + serial. PURE over an injected snapshot struct (paths read into the snapshot by a thin impure shim). Tests: fixtures for (a) DigiRig only, (b) DRA-100 only, (c) both attached (two USB cards — assert each resolves to a DISTINCT stable id, not card index), (d) onboard HDMI present (assert it is excluded / not the default). Do NOT key off card index.
- **Task 1.2 — PTT discovery.** `discover_ptt(card: &AudioDevice, snapshot) -> Vec<PttChoice>` returning candidates: `Cm108Hid { hidraw_path }` and/or `SerialRts { tty }`, ranked HID-on-same-USB-parent first. PttChoice serializable for config. Tests: DRA-100 → CM108 HID candidate on same parent; DigiRig → SerialRts (CP2102); an adapter exposing both → HID ranked first.
- **STOP/complete check:** tests green (narrow `cargo test --lib ... devices`), reviewed vs testing-pitfalls. Parent commits.

## Phase 2 — `direwolf.conf` generation (pure Rust, TDD) [no deps]

In `devices.rs` or a new `direwolf_conf.rs`.

- **Task 2.1 — `generate_direwolf_conf(params: DwParams) -> String`.** `DwParams { adevice: String, mycall: String, ptt: PttDirective, kiss_port: u16 }`. Output EXACTLY:
  ```
  ADEVICE  <adevice>
  CHANNEL  0
  MYCALL   <mycall>
  MODEM    1200
  PTT      <rendered ptt>
  KISSPORT <kiss_port>
  ```
  PTT renders `CM108 <hidraw>` or `<tty> RTS`. Tests: exact-string for CM108 + RTS cases; assert output contains NO `TXDELAY`/`PERSIST`/`SLOTTIME`/`FX25`/`IL2P` (regex negative asserts — these are pushed over KISS, never in the conf); MYCALL/SSID handling (SSID lives in tuxlink's AX.25 layer, MYCALL here is the base call — confirm against how the packet path sets the SSID today).
- **STOP/complete check:** green + reviewed. Parent commits.

## Phase 3 — Presence probe + conf-parse gate + device-busy probe (Rust, injected exec) [dep: P2]

- **Task 3.1 — `direwolf_presence(exec: &impl CommandRunner) -> DwPresence`** = `Absent | Present { version }`. Parse `direwolf -v` / `which direwolf`. Inject a `CommandRunner` trait so tests don't need the binary. Tests: absent, present-too-old (< min), present-ok.
- **Task 3.2 — `validate_conf(path, exec) -> Result<(), ConfError>`** via `direwolf -t 0 -c <conf>` (config-parse only, no audio). Inject exec. Tests: parse-ok, parse-error surfaces a clear message.
- **Task 3.3 — device-availability probe.** Detect the chosen ALSA device already in use (parse `/proc/asound/card*/pcm*/sub*/status` or attempt an exclusive open via a thin shim) → named error "`<device>` is in use by another program". Test the parse/decision purely.
- **STOP/complete check:** green + reviewed. Parent commits.

## Phase 4 — Managed Dire Wolf lifecycle (Rust, mock-spawn tested) [dep: P2,P3]

New `src-tauri/src/winlink/ax25/managed_direwolf.rs`, mirroring ardopcf's managed transport.

- **Task 4.1 — `ManagedDireWolf::spawn(cfg) -> Result<Self>`:** write the generated conf to a temp path, validate (P3.2), device-busy probe (P3.3), then spawn `direwolf -t 0 -c <conf>` (KISSPORT in conf), bind-wait the KISS TCP port (mirror `with_managed_modem_timeout` bind-wait). Returns a handle exposing the loopback `(host, port)`.
- **Task 4.2 — `shutdown(self)`:** SIGINT the child, wait for clean exit, confirm the audio device is released (re-probe). **Hard RADIO-1 assertion: after shutdown the PTT line is de-asserted / the child is gone — never leave keyed.** Test with a fake child process (mirror the ardopcf SIGINT-clean-stop test at transport.rs ~:2171/:2258).
- **Task 4.3 — sound-card arbitration hook:** before spawn, if another managed modem holds the card, stop it + confirm release (single-arbiter, ADR 0015). Unit-test the arbitration decision with injected state.
- **STOP/complete check:** green + reviewed (3 rounds for this lifecycle phase — it's the RADIO-1-critical one). Parent commits.

## Phase 5 — Config schema + DTO [dep: P1]

- **Task 5.1 — `KissLinkConfig::ManagedDireWolf { audio_device: StableAudioId, ptt: PttChoice }`** in `link.rs`. Ensure the lenient deserializer (`config.rs:461`) still round-trips old + new. Tests: serialize/deserialize round-trip; old config without the variant still loads.
- **Task 5.2 — `PacketConfigDto`** (`ui_commands.rs:3174`): add `linkKind: "Managed"` + `audioDeviceId`, `pttKind`/`pttPath` fields; extend the `From<&PacketConfig>` (:3195) and the `into` round-trip (:3230). Tests: DTO round-trip for the Managed variant; existing Tcp/Serial/Bluetooth unaffected.
- **STOP/complete check:** green + reviewed. Parent commits.

## Phase 6 — Wire managed mode into the packet connect path [dep: P4,P5]

- **Task 6.1:** in the packet connect command (`ui_commands.rs` ~:3551 `packet_connect` path / `packet_transport_from_config`), when `link == ManagedDireWolf`: resolve device → generate conf → `ManagedDireWolf::spawn` → take its loopback `(host,port)` → build the existing `KissLinkConfig::Tcp` transport against it → run the existing B2F. On disconnect/abort: `ManagedDireWolf::shutdown`. Bring-your-own variants unchanged.
- **Task 6.2 — presence fallback:** if `direwolf_presence == Absent`, the connect surfaces the named fallback ("Dire Wolf not found — install it, or use a bring-your-own KISS endpoint"), NOT a black-box failure.
- **STOP/complete check:** green + reviewed (3 rounds — integration + abort path). Parent commits.

## Phase 7 — UI: packet panel managed mode [dep: P5,P6]

- **Task 7.1 — device-list command:** `packet_list_audio_devices() -> Vec<AudioDeviceDto>` (+ PTT candidates) Tauri command wrapping Phase 1.
- **Task 7.2 — PacketRadioPanel UI:** a "Connection" choice — **Managed (recommended)** vs **Bring your own KISS endpoint**. Managed shows: sound-card picker (human names), PTT picker (auto-detected default + override), callsign (from identity). Persist via `packet_config_set`. Keep the existing Tcp/Serial/Bluetooth UI under bring-your-own. Tests: vitest for the panel — mock `invoke` for `packet_list_audio_devices`/`packet_config_get/set`; assert managed selection persists the `ManagedDireWolf` DTO. Include an App-level production-mount-path test (memory test_production_mount_path_not_just_units). Browser-smoke deferred to operator (memory browser_smoke_before_ship — not a pre-merge gate).
- **STOP/complete check:** tsc + vitest green; reviewed vs TEST-1 (CSS not detectable in jsdom). Parent commits.

## Phase 8 — Health surfacing [dep: P6,P7]

- **Task 8.1:** surface managed-Dire-Wolf health in the panel: process up / KISS-port reachable / a decode-or-activity indicator. A `packet_managed_status()` command + a small status chip. Keep it honest (no fake "connected"). Tests: status mapping unit + panel render.

## Phase 9 — Packaging [dep: none; can parallel after P3]

- **Task 9.1:** debian control → `Recommends: direwolf (>= <min>)` (determine the min version with mature `PTT CM108` + KISS-over-TCP — research Dire Wolf changelog; document the reason inline). Confirm the package name across Debian/Ubuntu/Pi OS. The runtime probe (P3.1) + fallback (P6.2) cover absence. Note in the user guide that managed packet needs Dire Wolf (Recommends).

## Sequencing / parallelism

P1, P2, P9 independent. P3 needs P2. P4 needs P2+P3. P5 needs P1. P6 needs P4+P5. P7 needs P5+P6. P8 needs P6+P7. Run P1∥P2∥P9 first; then P3, P5; then P4; then P6; then P7; then P8.

## Deferred / not in this plan

- **Codex cross-provider adversarial round (MANDATORY before merge):** quota-blocked until ~2026-06-13 1:49 PM (memory codex_quota_gotcha). Run it on the PR diff after quota resets; do NOT substitute Claude (memory no_carveout_on_cross_provider_adrev). Until then the PR stays DRAFT.
- Slice A (CM108 PTT for ardopcf) and Slice C (tux-rig CAT plane / tuxlink-5jb) — separate bd issues, separate plans.
- Doc-truth fix for `12-cat-and-rigctld.md` — separate small docs PR (can ship independently/first).
- 9600 baud, FX.25/IL2P — out of scope.

## Execution recommendation

This session has heavy context already; the plan is self-contained. **Recommend: fresh session via `/executing-plans` (or subagent-driven-development) in this worktree**, draft PR up front so CI compiles each push. Phase 4 (lifecycle, RADIO-1) and Phase 6 (connect/abort) warrant the 3-round review. Operator's weekend on-air test is the smoke; the Codex round runs after quota reset before marking the PR ready.
