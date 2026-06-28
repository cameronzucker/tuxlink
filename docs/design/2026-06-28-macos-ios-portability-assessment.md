# Tuxlink Platform Portability Assessment — macOS, iOS, Windows & Android

- **Status:** Draft assessment. Parts I–IV are static source analysis + external research; **Part V is an empirical macOS build** actually run on the target hardware (Rust compile verified). Non-macOS compile claims remain predictions to confirm by cross-build.
- **Date:** 2026-06-28
- **Scope:** Can Tuxlink run on (1) macOS, (2) iOS/iPadOS, (3) Windows, and (4) Android, and if not, what is the gap and viability — including which feature subsets are reachable and what must be cut.
- **Baseline:** branch `claude/nice-tu-ac3438`, Tauri **2.11.2**, MSRV 1.75, app crate + workspace subcrates (`tuxlink-security`, `tuxlink-mcp-core`, `tuxlink-mcp`, `tux-rig`, `tuxlink-agent-runner`, `d3zwe`).

---

## 0. Purpose & methodology

This document captures a portability assessment so its conclusions do not have to be re-derived. It exists because the question "could Tuxlink run on macOS / iPad?" is recurring and the answer is non-obvious: the app is described as "Linux-native," but much of that is deployment, not architecture.

**Method.** Four multi-agent workflows (one per target platform), each structured as **exhaustive survey → adversarial verification → synthesis**, followed by an empirical macOS build (Part V):

1. **Survey** — parallel readers swept the code surfaces (transports, system integration, GUI/webview, build/deps, subcrates), audited every dependency for target support, and ran web/research lanes for the external ecosystem.
2. **Adversarial verification** — every load-bearing portability claim was handed to an independent skeptic instructed to *refute* it against the real source (`file:line`) or cited platform rules. This caught and corrected several first-pass errors (see each part's "Corrections from verification").
3. **Synthesis** — reconciled findings into the gap matrices and phased plans below.

Roughly **112 agents** ran across the four workflows: macOS (39 agents; 108 findings; 28 verified, **22 corrected**), iOS (25; 49; 17 verified, 13 corrected), Windows (24; 39; 15 verified, 7 corrected), Android (24; 57; 16 verified, 11 corrected). A handful of research lanes hit the structured-output retry cap and were dropped; their synthesis drew on the surviving lanes (noted where it matters). **The macOS compile-level claims were then empirically tested with a real `cargo check` on this M5 Mac (Part V); the iOS / Windows / Android compile claims remain predictions** pending their respective cross-builds.

> **Assessment-environment note (2026-06-28):** the assessment host is an **Apple Silicon MacBook Air (M5), macOS Tahoe 26.5.1** — the actual macOS target hardware, not the Raspberry Pi dev box referenced in `CLAUDE.md`. The Rust toolchain (`rustup`/`cargo` 1.96.0) and `pnpm` 11.9.0 were installed during this session (`node` 26 / `xcrun` were already present), and a real macOS `cargo check` was run. See **Part V** for the exact toolchain, Homebrew dependencies, the one source fix required, and what compiled vs. what did not.

---

# Executive summary — four-platform verdict

Linux is and remains the reference platform. Of the four targets assessed, **macOS is the cheapest port and is now empirically confirmed to compile** (Rust core, one one-line fix — Part V). **Windows is the most Rust-porting work** (the Unix assumptions that compile on macOS become hard compile errors) **but unlocks the richest native radio ecosystem** — VARA, ardopcf, Dire Wolf, and Hamlib all ship native Windows binaries and are spawnable exactly as today. **iOS and Android are both ground-up re-architectures into network-client apps**: their sandboxes forbid spawning the external modem/control binaries Tuxlink's managed-modem design depends on, so local soundcard/packet modems are cut and RF must come from a *networked* or *Bluetooth* TNC. Of the two, **Android is the stronger mobile RF target** (Classic-Bluetooth SPP + USB host are open to apps; iOS exposes neither without MFi).

Effort, lowest → highest to a *useful* build: **macOS → Windows → Android → iOS.** Capability ceiling for an operator: **Windows ≈ Linux > macOS > Android > iOS.**

| Axis | macOS | Windows | iOS / iPadOS | Android |
|---|---|---|---|---|
| Tauri target | tier-1 desktop | tier-1 desktop | first-class, young | first-class, young |
| **Compiles as-is?** | **Yes — verified, 1-line fix** | No — 16+ Unix-ism sites (4 crates) | No — subsystem excision | No — subsystem excision |
| Compile-threshold effort | Low (days) | High (weeks) | High (re-architecture) | High (re-architecture) |
| Spawn external modems | ✅ yes (brew binaries) | ✅ yes (native binaries) | ❌ sandbox-banned | ❌ sandbox-banned |
| Serial / USB CAT | ✅ `/dev/cu.*` | ✅ `COM*` | ❌ none (MFi only) | 🔌 USB-host (native plugin) |
| Bluetooth TNC | 🟡 SPP-as-`/dev/cu.*` | 🟡 Win32/COM | 🔌 BLE only (plugin) | 🔌 Classic-SPP **+** BLE (plugin) |
| GPS | gpsd / remote | remote / Win Location | 🔌 Core Location (plugin) | 🔌 Location Services (plugin) |
| Tray / multi-window | ✅ retained | ✅ retained | ❌ single-window touch UI | ❌ single-window touch UI |
| Native ham ecosystem | medium (VARA = Wine) | **richest — all native** | none (remote-only) | BT/USB TNCs (cf. WoAD) |
| Spawned local modems | ✅ Dire Wolf, rigctld | ✅ + native VARA/ARDOP | ❌ remote-only | ❌ remote-only |
| Distribution | `.dmg` / notarize | MSI/NSIS / Authenticode | App Store / TestFlight | Play / APK sideload |
| **Overall effort** | **Lowest** | Moderate–High | Highest | High |
| Verdict | Viable port; compiles today | Viable; best radio, most Rust work | Viable *product*, ground-up | Viable *product*, best mobile RF |

The per-platform parts below give the evidence (`file:line`), gap matrices, phased plans, and the adversarial-verification corrections behind each cell.

---

# Part I — macOS

## I.1 Bottom line

**Can it run on macOS today? No.** Tuxlink is Linux-native by design and deployment: Linux-only bundle targets (`src-tauri/tauri.conf.json`), Linux-only CI (`.github/workflows/release.yml`), `description = "Linux-native full-capability Winlink client"` (`src-tauri/Cargo.toml:14`). It has never been built or run on Darwin and will not build 100% unmodified.

**Is a port viable? Yes — qualified, and more so than the "Linux-native" label implies.** The decisive architectural fact: the transport/modem layer is **pure logic over thin I/O shims**. The hard parts — KISS/AX.25 framing, CMS/TLS, device-ID derivation, conf generation, mailbox/FTS5 — are already platform-agnostic and fixture-tested. What blocks the port is concentrated and well-understood: a couple of dependency-config fixes, a Linux-filesystem device-discovery layer that needs a Core Audio/IOKit sibling, and — the genuinely load-bearing risk — **the external ham-radio binary ecosystem, not Tuxlink's own code.**

**Verdict.** A **CMS-over-internet + KISS-over-TCP (managed Dire Wolf) + VARA-over-TCP** macOS v1 is achievable with moderate effort. Full HF-soundcard (ARDOP) + in-app-Bluetooth is materially harder and should be cut from v1.

## I.2 Threshold: would it even compile?

This is the section adversarial verification changed most. The first pass called the Bluetooth `AF_BLUETOOTH` socket code *the* showstopping compile blocker. **That was wrong** — those are hardcoded integer constants, not libc symbols, so the code compiles and only fails at runtime.

### I.2a Genuine compile blockers — a short, *soft* list

| Concern | Evidence | Reality after verification | Fix |
|---|---|---|---|
| **`keyring` pins `sync-secret-service`** unconditionally | `src-tauri/Cargo.toml:89` | **Uncertain, lean "soft."** keyring 3.x internally `#[cfg]`-gates its backend, so it *may* compile on macOS even with this feature set; but it pulls D-Bus crates whose macOS build is unconfirmed. | Per-target cfg swap to keyring's `apple-native` feature (security-framework v3, ships in **3.6.3** — **not** a 4.x upgrade). Keep `sync-secret-service` Linux-gated. Defensive regardless. |
| **`libheif-rs`** pkg-configs system `libheif ≥ 1.17` | `src-tauri/Cargo.toml:66`, default-on `heif` feature (`:29-30`) | The most credible *actual* external-lib need. | `brew install libheif libde265`, **or** `--no-default-features` to drop HEIF (the intended escape hatch per ADR-0020). Lowest effort: cut HEIF for v1. |
| **`webp` / `zstd`** C-linked | `src-tauri/Cargo.toml:67`, `:119` | **Probably NOT blockers.** ADR-0020 states `libwebp-sys` / `zstd-sys` *vendor* their C code (no system floor) → self-build. Conflicts with the "needs system lib" read; **unconfirmed on macOS.** | Likely nothing; confirm at build time. |
| **Unguarded `statvfs`** | `src-tauri/src/basemap/commands.rs:391` | Latent risk *added* by verification — its sibling `src-tauri/src/logging/free_disk_guard.rs:49-66` is properly `#[cfg(target_os = "linux")]`-gated; this call is not. `statvfs` is POSIX on macOS, but whether `nix` 0.31's `fs` feature exports it on Darwin is unconfirmed. | One-line cfg guard, or confirm at build. |

**Net compile threshold:** one defensive keyring cfg-swap + a HEIF decision + confirming two probably-fine vendored libs and one `statvfs` call. **Nothing architectural.**

### I.2b Refuted as blockers (verified — they compile fine)

- **`AF_BLUETOOTH` / `rfcomm.rs`** — `src-tauri/src/winlink/ax25/rfcomm.rs:101-102` defines `const AF_BLUETOOTH: libc::c_int = 31;` / `const BTPROTO_RFCOMM: libc::c_int = 3;` as **hardcoded integers**, not libc symbols. `libc::socket` (`:126`) is universal POSIX FFI; `struct SockaddrRc` (`:108-112`) is `#[repr(C)]` over standard types; the module's own test (`:462-481`) **explicitly accepts `EAFNOSUPPORT`/`EPROTONOSUPPORT`**. The code **compiles on macOS** and fails only at runtime. Should be `#[cfg(target_os = "linux")]`-gated for cleanliness, but is **not** on the build critical path.
- **`webkit2gtk` / `gtk`** — gated at both `src-tauri/Cargo.toml:152-154` (`[target.'cfg(target_os = "linux")'.dependencies]`) **and** source (`src-tauri/src/forms/pdf_export.rs:68,156` Linux impls; `:195-211` `UnsupportedPlatform` stubs for non-Linux). Will **not** break a macOS build.
- **`nix` crate** — one verifier claimed it "doesn't support macOS" (whole-project resolution failure). **That is incorrect** — `nix` with `signal`/`process`/`fs` builds on Darwin. Only the specific unguarded `statvfs` (§I.2a) is the open question.
- **Unix-domain-socket MCP layer, `libc::getuid`/`umask`, `std::os::unix::fs`, signals** — all macOS-available.

Of **21** `#[cfg(target_os = "linux")]` sites, **20** have proper non-Linux fallbacks (verified). The lone gap is the `statvfs` call above.

### I.2c Compiles-but-fails-at-runtime (build succeeds; feature no-ops)

- **`rfcomm.rs` AF_BLUETOOTH socket** — `EAFNOSUPPORT` at runtime (§I.2b).
- **`tuxlink-gps-fix` privileged helper** (`src/bin/tuxlink-gps-fix.rs`) — hardcodes `systemctl`, `apt-get`, `/etc/default/gpsd`, `/etc/passwd`, `pkexec`. Compiles (plain `std`/process), entirely non-functional on macOS.
- **GPS fix path** `src/position/gps_fix.rs:43-66` — `which_pkexec()` returns `None` → `GpsFixOutcome::PkexecMissing`. Degrades gracefully.
- **Device/PTT auto-discovery** — all `/proc`, `/sys`, `/dev/snd` reads in `src/winlink/ax25/devices.rs:449-601`, plus `lsof`/`sdptool` shell-outs, are wrapped in `.ok()`/empty-collection fallbacks. Binary runs; the device picker returns empty on macOS.

## I.3 Gap matrix (macOS)

Legend: ✅ already cross-platform · 🟡 moderate work · 🔴 cut from v1 (see §I.5)

| Capability | Status | Evidence (`file:line`) | Notes |
|---|---|---|---|
| **CMS / Winlink-over-internet (telnet+TLS)** | ✅ | `src/winlink/telnet.rs:314` (`native_tls`) | → Secure Transport on macOS. Zero platform code outside tests. The v1 anchor. |
| **AX.25 framing / KISS encode** | ✅ | `src/winlink/ax25/frame.rs`, `kiss.rs` | Pure userspace. **No kernel `AF_AX25` anywhere** — the riskiest possible dependency is simply absent. |
| **KISS-over-TCP** | ✅ | `src/winlink/ax25/link.rs:173-177` | `std::net::TcpStream`. |
| **KISS-over-serial (transport)** | ✅ | `src/winlink/ax25/link.rs:412-435` | `serialport` open/read/write works on macOS. |
| **KISS-over-serial (device discovery)** | 🟡 | `src/winlink/ax25/devices.rs:571,574` | Discovery is Linux sysfs + `ttyUSB`/`ttyACM` filter; macOS needs `/dev/cu.*` (cu, not tty) + de-dup of `available_ports()`. Transport works; picker empty until ported. |
| **VARA (HF/FM) transport** | ✅ | `src/winlink/modem/vara/transport.rs:66-115`; `config.rs:1255` | Pure TCP; **remote host explicitly supported**. Gating is *architecture* (x86 vs aarch64, `commands.rs:976-979`), not OS. |
| **ARDOP transport (spawn + args)** | 🟡 | `src/winlink/modem/ardop/transport.rs:187-188` | Spawn is portable; `extra_args` carry ALSA `plughw:` / `/dev/snd` / `/dev/ttyUSB` strings needing Core Audio + `/dev/cu.*` substitution. External binary is the real wall (§I.4). |
| **Dire Wolf — managed mode (lifecycle)** | ✅ | `src/winlink/ax25/managed_direwolf.rs:260` | Spawn / KISS-bind-wait / SIGINT→SIGKILL stop are portable. Version parse pure. |
| **Dire Wolf — conf generation** | 🟡 low | `src/winlink/ax25/direwolf_conf.rs:110,95` | Pure string-gen; needs `CoreAudio:` device names + macOS PTT paths. ~2–3 line `#[cfg]` branch in the caller. |
| **Dire Wolf — device-busy probe** | 🟡 | `src/winlink/ax25/direwolf_probe.rs:348,300-323` | Parser pure; swap `/proc/asound` read for CoreAudio `kAudioHardwarePropertyDeviceIsRunningSomewhere`. |
| **Rig control / Hamlib (rigctld)** | ✅ (code) | `tux-rig/src/managed.rs:55-131`; `list.rs:72-76` | `std::process` spawn + kill/wait portable. External: `brew install hamlib` (4.7.2, arm64+Intel bottles, `rigctld` included). Serial is `/dev/cu.*`. |
| **GPS / gpsd (runtime read)** | 🟡 | — | Reading gpsd over TCP is portable; the **setup helper is not** (cut, §I.5). gpsd brew-installable. |
| **Credential storage / keyring** | 🟡 | `Cargo.toml:89`; usage `bootstrap.rs:167,465`, `wizard.rs:224` | Cfg-swap to `apple-native` mechanically simple; "moderate" only for signing/entitlement runtime behavior. The Linux-shaped wizard↔Pat cross-process contract is irrelevant on macOS (simplifies). |
| **Forms PDF export** | 🟡 | `src/forms/pdf_export.rs:68-145` (Linux), `:195-211` (stub) | Builds → `UnsupportedPlatform`. macOS path = `WKWebView.createPDFWithConfiguration` via `objc2-web-kit` + `with_webview().inner()` — reachable but hand-rolled FFI (wry print issue #707 open). |
| **Mailbox / search (rusqlite FTS5)** | ✅ | `Cargo.toml:97` (`bundled`) | Self-contained SQLite. |
| **Tray / window** | ✅ | `src/tray.rs:35-75` (`icon_as_template(true)`); `ui_commands.rs:6746-6748` (minimize vs hide) | Tauri cross-platform tray; macOS template-image handled. |
| **Audio / PTT device discovery** | 🟡 high | `devices.rs:449-601`; `ui_commands.rs:4408-4418` (`arecord`/`aplay`) | Heaviest single item: `/proc/asound`, `/dev/snd`, `/sys/class/hidraw`, `arecord`/`aplay`. **All logic is pure** (stable-ID derivation, ranking); only the I/O shim is Linux → cpal/CoreAudio + IOKit (mind MSRV 1.75 vs cpal's newer floor; needs `NSMicrophoneUsageDescription`). |
| **USB topology / stable IDs / HID PTT** | 🟡 | `devices.rs:500-526,531,716-727` | Pure ranking; replace sysfs walk with IOKit (`IOUSBDevice`/`IOHIDManager`). |
| **GPS setup / privileged helpers** | 🔴 | `bin/tuxlink-gps-fix.rs`; `position/gps_fix.rs:43-57` | systemd/apt/pkexec/`/etc/*`. Compiles, non-functional. Re-impl for launchd, or cut from v1. |
| **Bluetooth / UV-Pro (in-app RFCOMM)** | 🔴 | `rfcomm.rs:101-126`; `sdptool` `:266,332` | Not a compile blocker (§I.2b) but no functional macOS path without native rewrite. **Substitute:** route bonded SPP devices through `/dev/cu.Bluetooth-*` via `serialport`. |
| **Bundling / CI** | 🟡 | `tauri.conf.json:28-90` (`targets:"all"`, no `bundle.macOS`); `release.yml` ubuntu-only | `targets:"all"` is **safe** (Tauri ignores unrecognized platforms). Add `bundle.macOS` (icon, signing, entitlements, dmg) + macOS CI runner. Notarization needs a paid Apple Developer account; ad-hoc signing fine for dev. |
| **Frontend (React/TS)** | ✅ | grep: only `navigator.clipboard`, no UA/platform detection | Platform-neutral. |
| **MCP (UDS) / tuxlink-security / agent-runner / tux-rig** | ✅ | `transport_uds.rs`; `tuxlink-security/src/lib.rs`; `tuxlink-agent-runner/src/lib.rs` | Portable POSIX / pure Rust. MCP setup is `#[cfg(linux)]`-gated anyway. |

## I.4 External ecosystem reality (source-backed)

The decisive insight: **most remaining gaps are the availability of external ham binaries, not Tuxlink's code.**

- **Dire Wolf — ✅ available & mature.** `brew install direwolf` (1.8.1), prebuilt bottles for Apple Silicon **and** Intel; also MacPorts. On macOS it uses **PortAudio over Core Audio**, so `ADEVICE` takes **Core Audio device names, not ALSA `plughw:`** — exactly the conf-gen branch in §I.3. Tuxlink's managed-Dire-Wolf lifecycle is portable. **This makes managed packet over Dire Wolf the most viable soundcard path on macOS.**
- **Hamlib / rigctld — ✅ available & mature.** `brew install hamlib` (4.7.2, arm64+Intel bottles; `rigctld` ships in the package). CAT is serial/USB/TCP, so the audio split doesn't apply; the rig is a `/dev/cu.*` device. `tux-rig` spawn code already portable.
- **ardopcf — 🔴 NOT available (the load-bearing ecosystem gap).** No Homebrew formula, no MacPorts port, no prebuilt macOS binary. Maintainer's BUILDING.md states he knows of no macOS builds; the Makefile branches only `WIN32` vs else, and the else branch links `-lrt -lasound` and compiles `src/linux/ALSASound.o` — **ALSA-only, no PortAudio/macOS layer.** Porting ardopcf is a real project. And even if ported, Tuxlink's **own** audio-enumeration layer still needs the Core Audio sibling — a **double gap** (external binary + internal abstraction). This is why ARDOP is cut from v1.
- **VARA — closed Windows binary, "available-rough" via Wine/CrossOver (Tuxlink-side ✅).** No native macOS build exists or is planned (closed 32-bit Windows binaries from EA5HVK). Paths: CrossOver (~$74; works on Intel **and** Apple Silicon via its own x86 translation, not Rosetta) or free Wine/Whisky (rougher on ARM). **Irrelevant to Tuxlink's code:** Tuxlink talks to VARA over plain TCP and supports a **remote** host, so the realistic architecture is *native Tuxlink + VARA under CrossOver locally or on a remote Windows/x86 box, over TCP.* Operator-side rough edges: VARA's Wine port-rebind bug (restart between sessions; `varanny` nannies it), COM/CAT fragility (use rigctld on :4532), per-rig handshake variance. **Tuxlink's VARA transport works unchanged.** (Fact corrected in verification: VARA FM conventionally uses **8400/8401**, not 8302.)
- **Apple Silicon specifics.** Dire Wolf, Hamlib, Rust, Tauri/WKWebView, `serialport`, `cpal`/CoreAudio, Keychain all first-class arm64. Caveats: (1) VARA's Intel binary runs through CrossOver translation, not natively; (2) Tauri builds per-arch — a universal binary needs `--target universal-apple-darwin`.

## I.5 macOS v1 — include vs explicitly cut

**Include (viable subset):**
1. **CMS / Winlink-over-internet** — unchanged; the anchor.
2. **Mailbox, compose, search (FTS5), forms authoring** — portable once keyring is fixed.
3. **KISS-over-TCP**, incl. **managed Dire Wolf** (brew, Core Audio) — the macOS packet path.
4. **VARA-over-TCP** — Tuxlink-side ready; document the CrossOver/remote operator setup honestly.
5. **Rig control via Hamlib/rigctld** — brew-available, code portable.
6. **KISS-over-serial** *with macOS device discovery added* (`/dev/cu.*`).
7. **Tray/window, dialogs, frontend** — already cross-platform.
8. **PDF export as `UnsupportedPlatform` stub initially** (seam exists), WKWebView PDF as a fast follow.

**Cut from v1 (with rationale):**
- **ARDOP** — external `ardopcf` has no macOS binary + ALSA-locked audio + needs Tuxlink's Core Audio abstraction. Two compounding gaps.
- **In-app Classic-Bluetooth RFCOMM / UV-Pro over BT** — `AF_BLUETOOTH` has no macOS equivalent; native path is a few-hundred-line `objc2-io-bluetooth` rewrite. **Pragmatic substitute:** consume the OS-created `/dev/cu.Bluetooth-*` SPP serial port via `serialport` (operator pairs in System Settings first).
- **Privileged GPS setup helper (`tuxlink-gps-fix`)** — entirely systemd/apt/pkexec/`/etc/*`. gpsd-over-TCP read can stay; the *setup* helper is cut (or rewritten for launchd+brew later).
- **HEIF image support** — drop via `--no-default-features` to avoid a brew dep, unless HEIF attachments are required.

## I.6 Phased plan (macOS)

- **Phase 0 — build + boot (prove the threshold).** Add `[target.'cfg(target_os = "macos")']` keyring → `apple-native`; brew-install or confirm-vendored `webp`/`zstd`; decide HEIF in/out; gate `rfcomm.rs` under `#[cfg(linux)]`; resolve the unguarded `statvfs` (`basemap/commands.rs:391`); add `bundle.macOS` + a macOS CI runner; ad-hoc sign. **Exit:** app launches, mailbox/compose/search + tray work. *Cheapest, highest-information step — converts every §I.2 prediction into fact.*
- **Phase 1 — internet + packet-over-TCP.** Verify CMS end-to-end. Wire VARA-over-TCP (document CrossOver/remote). Wire Hamlib/rigctld. Add macOS serial discovery (`/dev/cu.*`, cu-dedup) so KISS-over-serial + rigctl pickers populate. Stand up managed Dire Wolf with the `CoreAudio:` `ADEVICE` conf branch. **Exit:** internet Winlink + VHF packet + VARA + CAT.
- **Phase 2 — HF soundcard (deferrable).** Build the Core Audio / IOKit shim behind the existing pure logic (cpal/CoreAudio; mind MSRV; `NSMicrophoneUsageDescription`); IOKit USB tree; CoreAudio busy-probe; cpal enumeration replacing `arecord`/`aplay`. Add WKWebView PDF export. **Exit:** full soundcard device management; PDF parity.
- **Deferred (post-v1):** ARDOP (gated on an ardopcf macOS port + Phase 2 audio); native in-app Bluetooth RFCOMM (`objc2-io-bluetooth`); launchd+brew GPS setup; Developer-ID notarization + signed/notarized dmg + `brew install --cask`.

## I.7 Effort & risk (macOS)

The cost is **not** in Tuxlink's transport/modem logic (pure, ready). It is in four places:
1. **Core Audio / IOKit abstraction (Phase 2)** — the largest code item, mitigated by the pure-over-impure architecture (you rewrite thin I/O shims, not decision logic). Still real (cpal MSRV friction, mic entitlement, IOKit USB-tree code).
2. **External binaries** — the *binding constraint on capability*, entirely outside Tuxlink. Dire Wolf + Hamlib = brew (easy). ardopcf = blocked (no macOS build). VARA = Wine-only, operator-side friction.
3. **Validation, not authorship** — every transport claim here is static. The genuine effort is *exercising* each path on real macOS hardware. Per **RADIO-1 / ADR 0018**, on-air transmit validation is **operator-only**; the agent validates transmit-path code via mocks/loopback/CI and never keys a real radio. CMS telnet is not a transmission and is fair game for dev testing.
4. **Distribution polish** — signing/notarization (paid Apple Developer account), entitlements (mic, hardened runtime), universal binary, dmg.

**Top unknowns (ranked):**
1. **Does it actually compile?** §I.2 is a prediction. The `webp`/`zstd` vendoring conflict and the unguarded `statvfs` are unresolved on static evidence. **Run the cross-build first** — decisive and cheap (now possible on this M5 Mac once a Rust toolchain is installed).
2. **ardopcf macOS port** — the wall for HF FSK/ARDOP. Not Tuxlink's to fix.
3. **VARA-under-Wine reliability on Apple Silicon** — works for determined hams; fragile (port-rebind bug, per-rig handshake variance); zero vendor support.
4. **Tauri multi-webview on macOS** — `Manager::get_webview` (read) stable; *spawning child webviews* is `unstable`-gated with active macOS bugs. If `pdf_export` only reads the existing webview, fine; if it spawns children, budget instability.
5. **Keychain ACL / re-prompt behavior** under varying signatures.

## I.8 Corrections from verification (macOS)

Listed so the process is auditable — each was overturned or materially altered by the adversarial pass:

1. **AF_BLUETOOTH / BTPROTO_RFCOMM are compile blockers — REFUTED (high confidence).** Hardcoded `c_int` constants (`rfcomm.rs:101-102`), not libc symbols; `libc::socket` universal; struct `#[repr(C)]`; test expects `EAFNOSUPPORT`. **Compiles on macOS; runtime-only failure.** Corrected: runtime blocker, `#[cfg]`-gate for cleanliness, not on the compile critical path.
2. **"The ONLY compile blocker is keyring" — REFUTED both ways.** Undercounted on C-libs (`webp`/`zstd`/`libheif`) while *over*counting `rfcomm`/`transport_uds` as blockers (both compile; UDS APIs are macOS-available).
3. **"KISS-over-serial is fully cross-platform" — CORRECTED.** Transport is; **device discovery is Linux-only** (`devices.rs:571,574`). macOS picker empty until `/dev/cu.*` added.
4. **"VARA cannot run on macOS in any practical way" — CORRECTED (+ fact fixed).** Tuxlink imposes **no OS restriction**; gating is **architecture-based** (`commands.rs:976-979`); **remote VARA hosts explicitly supported**. VARA FM port is **8400/8401**, not 8302.
5. **"rusqlite/mailbox fully portable" — true module-wise; was flagged only because the whole app couldn't build until keyring was fixed.** Once #1 is resolved, ✅.
6. **"nix/libc all compile on macOS" — CORRECTED (one exception):** the unguarded `nix::sys::statvfs` at `basemap/commands.rs:391`.
7. **webkit2gtk/gtk "break a macOS build" — REFUTED.** Properly gated at deps + source; **does not break the build.**
8. **Verification over-reach, not adopted:** a verdict claiming `nix` "does not support macOS" is itself wrong — `nix` builds on Darwin. Real concern isolated to the single `statvfs` call.
9. **ardopcf/Dire Wolf research — SHARPENED:** ARDOP is a **double gap** (no macOS ardopcf binary + missing internal Core Audio abstraction `ui_commands.rs:4408-4418`, `devices.rs:449-499`), firmly a cut.

---

# Part II — iOS / iPadOS

> Method note: this section is **static analysis + platform research only**. No `cargo build --target aarch64-apple-ios` was attempted, no `tauri ios init` was run, no Simulator boot was tried. Every compile-level claim is a **prediction** until a real iOS build runs on a macOS host. File:line references are to the worktree at ``. Where the adversarial pass overturned a survey claim, the corrected version is what appears below; §10 catalogs the overturns.

## 1. iOS bottom line

**Can it run on an iPad today? No.** There is zero iOS configuration in the repo: `tauri.conf.json` declares only Linux bundle targets (`deb`, `rpm`, `appimage`; line 30 `"targets": "all"` resolves to those three on the Linux host), there is no `bundle.iOS` section, no `Info.plist`, no entitlements, and no `#[cfg(target_os = "ios")]` code anywhere. The lone mobile artifact is `#[cfg_attr(mobile, tauri::mobile_entry_point)]` at `src-tauri/src/lib.rs:462` — a marker, not a working build. Nobody has ever compiled this for iOS.

**Is a port viable? Yes, but only as a narrow network-client app, and only as a re-architecture — not a recompile.** The pure-Rust protocol core (B2F, CMS-over-TLS, KISS framing, AX.25, mailbox/FTS5, forms serialization) is genuinely portable and would link into a Swift/iOS shell via FFI. A touch-first Winlink terminal that talks to the internet CMS and to *remote* modems over TCP is a real, shippable product (RadioMail proves the category). But the desktop wrapper does not transfer, the touch UI does not exist, and several native plugins must be written.

**The crucial difference from macOS: the sandbox changes everything, and the single biggest change is that iOS apps cannot spawn external processes.** This one constraint is what splits iOS away from macOS. The verified macOS analysis concluded macOS is *viable* precisely because macOS can `std::process::spawn` external modem/control binaries — managed Dire Wolf (via Homebrew), `rigctld` (via Homebrew). **On iOS that entire capability is gone.** The macOS "spawn a local modem" enabler is an iOS hard prohibition. Everything else (no serial, no Classic Bluetooth, single-window, no tray, Keychain instead of D-Bus) compounds it, but the no-fork/exec rule is the defining wall.

So: macOS is "viable port, mostly recompile + a few cfg-swaps, can still drive local radios." iOS is "viable *product*, but a ground-up app whose transport menu is dictated by Apple, with all local-RF spawning permanently cut."

## 2. The defining constraint: no external processes

iOS/iPadOS third-party apps **cannot fork/exec/posix_spawn/NSTask any external binary, at all.** This is stricter than the macOS App Sandbox (which is a macOS-only capability that still permits child processes); on iOS the OS sandbox simply does not grant child-process execution to third-party apps, and the Tauri `shell` plugin reflects this — on iOS it is documented as "only allows to open URLs via `open`," with no `Command`/sidecar/spawn.

**Important attribution correction (from verification):** Tuxlink's modem/control spawns do **not** go through `tauri-plugin-shell`. `tauri-plugin-shell` (`Cargo.toml:51`) is used only for `xdg-open` (file-manager open). The real spawns use `std::process::Command` / `ManagedModem::spawn` directly. So the iOS blocker is the universal sandbox ban on *any* child process, not a plugin limitation. The plugin's iOS no-op is a red herring; the `std::process` / `nix`-process-feature dependency is the substance.

A second, deeper consequence flagged by verification: the `nix` crate is pinned with the `process` feature (`Cargo.toml:72`, `features = ["signal", "process", "fs"]`). On iOS this is plausibly a **compile-time** problem, not merely a runtime one — the `process`/`signal` surface that the managed-modem lifecycle relies on (`std::process::Command` + `nix::sys::signal::kill` for SIGINT→grace→SIGKILL) is antagonistic to the iOS target. An iOS build would need the `nix` dependency (or its `process`/`signal` features) `cfg`-gated out, and the managed-modem lifecycle code excluded with it. Treat "drop managed modems on iOS" as "remove a whole subsystem," not "flip a flag."

### Features that die because they spawn a binary

| Feature | Spawn site (file:line) | What it spawns | iOS fate |
|---|---|---|---|
| **Managed Dire Wolf** (packet/KISS modem, default for accessibility) | `src-tauri/src/winlink/ax25/managed_direwolf.rs:324` | `direwolf -t 0 -c <conf>` | Local spawn impossible. **Survives only as remote KISS-over-TCP** (see §4). |
| **Dire Wolf presence probe** | `src-tauri/src/winlink/ax25/direwolf_probe.rs:92` | `direwolf -v` | Returns "Absent" gracefully; no crash. The managed *mode* it gates is dead. |
| **ARDOP** (`ardopcf`, HF/VHF digital) | `src-tauri/src/winlink/modem/ardop/transport.rs:228` | `ardopcf` (cmd+data TCP ports) | Local spawn impossible. Also no iOS `ardopcf` binary exists. **Remote ardopcf over TCP is the only path.** |
| **CAT-PTT python bridge** (close-serial keying, FT-710 class) | `src-tauri/src/winlink/modem/ardop/cat_ptt_bridge.rs:142` | `python3 catptt_bridge.py` | Dead. No wired network fallback — *the python process is the bridge*. Operator must use RTS keying or a remote CAT service. |
| **rigctld** (Hamlib CAT control, QSY + VFO polling) | `src-tauri/tux-rig/src/managed.rs:55` | `rigctld -m … -r … -t …` | Local spawn impossible. **Remote rigctld over TCP survives** (`RigConfigDto` has host/port). |
| **rigctl model list** | `src-tauri/tux-rig/src/list.rs:72` | `rigctl -l` | Returns empty list; operator enters Hamlib model manually. |
| **voacapl** (HF propagation forecast) | `src-tauri/src/propagation/engine.rs:80` | `voacapl <scratch>` | Dead, no network fallback. Non-essential; UI degrades to "unavailable." |
| **go-pmtiles** (regional basemap extraction) | `src-tauri/src/basemap/commands.rs:179` | `go-pmtiles extract …` | Dead, no network fallback. Falls back to bundled/streamed tiles. |
| **gpsd privileged setup** (`add-dialout`, mask ModemManager, install gpsd) | `src-tauri/src/position/gps_fix.rs:58,100`; helper binary `src-tauri/src/bin/tuxlink-gps-fix.rs:113-114,138,153,202,209,216` (`apt`/`systemctl`/`usermod`) | `pkexec /usr/libexec/tuxlink-gps-fix …` | Inapplicable on iOS (no systemd/apt/usermod). GPS becomes native Core Location instead (§4). |
| **lsof** (audio-device-release poll, ADR-0015) | `src-tauri/src/winlink/modem/process.rs:291` | `lsof <device>` | Soft-fails to a 200 ms sleep (line 298). Harmless on iOS. |
| **sdptool** (Bluetooth SPP/audio RFCOMM channel discovery) | `src-tauri/src/winlink/ax25/rfcomm.rs:266,332` | `sdptool records <mac>` | Dead; soft-fallback to channel 1 / empty list. Real BT needs a native plugin (§4). |
| **bluetoothctl** (paired-device picker) | `src-tauri/src/ui_commands.rs:4308` | `bluetoothctl devices Paired` | Dead; empty list. Native CoreBluetooth plugin needed for a real picker. |
| **arecord/aplay** (ALSA device picker) | `src-tauri/src/ui_commands.rs:4409,4416` | `arecord -L` / `aplay -L` | Dead; empty list. Native CoreAudio enumeration needed if audio path is built. |
| **xdg-open** (open custom-forms folder) | `src-tauri/src/ui_commands.rs:2424` (the one `tauri-plugin-shell` call) | `xdg-open <dir>` | Dead; replace with native file UI or show "unavailable." |
| **uname / env probes** (diagnostics) | `src-tauri/src/logging/manifest.rs:112`; `src-tauri/src/logging/env_probes/mod.rs:146` | `uname`, various probes | Soft-fail. Diagnostic only, no TX impact. |
| Test-only spawns (`mkfifo`, `gpsfake`, `secret-tool`) | `forms/http_server.rs:1587`, `tests/gpsd_fake_test.rs:112,120`, `tests/wizard_integration_test.rs:295` | — | Not in the app bundle; irrelevant on iOS. |

**Net:** the spawn-driven features split into (a) *recoverable* via an already-wired TCP alternative (managed Dire Wolf → KISS-TCP; rigctld → remote rigctld; ardopcf → remote ardopcf), (b) *non-recoverable but non-essential* (voacapl, go-pmtiles, lsof, the diagnostic probes), and (c) *non-recoverable and meaningful* (the python CAT-PTT bridge, which has no network shape). The Linux gpsd-setup spawns are simply replaced by native Core Location.

## 3. Threshold: would it compile / does Tauri even do iOS?

**Yes, Tauri 2 does iOS — first-class but young.** iOS/iPadOS is an officially supported Tauri 2 target since the 2.0 stable release (2024-10-02); the README claims "iOS/iPadOS (9+)." It is genuinely maturing, not experimental, but materially less battle-tested than the desktop targets. The model:

- **Rust-core-as-static-lib.** The Rust crate compiles to a static library linked into a thin Swift/Xcode shell; Swift↔Rust via FFI; iOS plugins are Swift `Plugin` subclasses. `tauri ios init` scaffolds `src-tauri/gen/apple/` (an Xcode project); `tauri ios dev` runs in Simulator (no Apple account) or on-device; `tauri ios build --export-method app-store-connect` produces an IPA for `xcrun altool` upload. **Build host must be macOS with full Xcode** — Tuxlink builds today on a Linux Pi, so the iOS build is a macOS-host activity, a logistics change in itself. Set `bundle.iOS.minimumSystemVersion` explicitly (docs default 13/14).
- **WKWebView** (same WebKit/`wry` as macOS). Frontend served via a custom URL-scheme handler, not the Linux loopback story; the known iOS custom-scheme caveats (historically no POST bodies on custom-scheme requests; secure-context origin handling) are Tauri's to manage. Safari remote DevTools works.

**Crate-by-crate prediction** (all on a macOS host; the "fails to cross-compile" reports online are Linux-host builds missing the iOS SDK and don't apply):

| Crate / dep | iOS verdict | Note |
|---|---|---|
| `rusqlite = { "0.40", ["bundled","modern_sqlite"] }` (`Cargo.toml:97`) | **Builds & runs unchanged** | `bundled` vendors SQLite, `cc` picks the iOS SDK on a macOS host; FTS5 guaranteed; runs in the app container. |
| `reqwest "0.13"` (`Cargo.toml:70`) / `native-tls "0.2"` (`:94`) | **Builds & runs** | `native-tls` uses Security.framework/Secure Transport on Apple; reqwest's default TLS works (rustls is the cleaner long-term iOS default). |
| `tokio "1" full` (`:71`) | **Compiles** | Runs on Darwin kqueue. Caveat: `tokio::process` compiles but is unusable at runtime (no spawn); `signal` constrained. |
| `axum 0.8` / `tower` / `hyper` | **Builds & runs** | Pure Rust loopback HTTP server is fine in-sandbox for the forms webview (foreground). |
| `printpdf 0.9` | **Builds & runs** | Pure Rust (rust-fontconfig, not C fontconfig); even targets wasm. |
| `webp 0.3`, `zstd 0.13` | **Builds** | Both **vendor their C** via `cc` (per ADR-0020 audit); no pkg-config / system floor. Cross-compile cleanly on macOS host. |
| `keyring = { "3.6.3", ["sync-secret-service","crypto-rust"] }` (`:89`) | **Needs per-target build-config swap** | Current feature is D-Bus Secret Service (absent on iOS). Must select `apple-native` (Security.framework Keychain) under `cfg(target_os="ios")`. 3.6.3 *has* `apple-native`. Note: keyring backend is a **compile-time** feature (no `use_native_store()` per `Cargo.toml:87`), so this is a `[target.'cfg(...)']` swap, not a runtime call. |
| `nix = { "0.31", ["signal","process","fs"] }` (`:72`) | **Compile risk + runtime-dead** | `fs`/statvfs is fine; `process`/`signal` are the managed-modem lifecycle and are antagonistic to iOS — gate the feature/dependency out and exclude the managed-modem code with it. |
| `serialport "4"` (`:95`) | **Hard non-starter** | No iOS backend; the iOS sandbox forbids `/dev/tty*`. KISS-over-serial and `/dev/rfcommN` simply don't exist on stock iOS. |
| `libc "0.2"` AF_BLUETOOTH socket (`rfcomm.rs:101-102`) | **Hard non-starter** | `AF_BLUETOOTH` raw sockets are unavailable to iOS apps; this path is dead (BLE via CoreBluetooth is the only in-app Bluetooth). |
| `libheif-rs = "=1.0.2"` (`:66`, optional `heif` feature `:30`) | **Workaround-only** | Pinned to `libheif-sys ^2.x`, which **pkg-configs system libheif** — none on iOS, and `--no-default-features` gates the Rust code but the `build.rs` pkg-config check is the real obstacle. Build iOS with `heif` off (already supported per ADR-0020); iOS decodes HEIC natively anyway. |
| `webkit2gtk` / GTK | **Already target-gated** | Linux-only `cfg`-guarded; iOS pulls WKWebView via `wry`. |

**Threshold verdict: SOFTER than the prose-level "it won't compile" fears, but HARDER than the macOS threshold.** The protocol/network/storage core is iOS-buildable on a macOS host with: a keyring `apple-native` cfg-swap, dropping `heif` for iOS, gating out `nix` process/signal + the managed-modem subsystem, and removing serialport/AF_BLUETOOTH paths. That is real build-config + subsystem-excision work, not a flag.

## 4. iOS gap matrix

Legend: ✅ works as-is · 🟡 port + touch-UI work · 🔌 needs native plugin (Swift/FFI) · 🔴 impossible on stock iOS.

| Capability | iOS class | Effort | Where (file:line) | Notes |
|---|---|---|---|---|
| **CMS over telnet + TLS** (internet) | ✅ | none (core) | `src-tauri/src/winlink/telnet.rs`; B2F `winlink/session/mod.rs` | Pure Rust TLS + turn-based protocol. *Strongest* iOS use case. Needs `NSLocalNetworkUsageDescription`? **No** — CMS is internet, not LAN. |
| **KISS-over-TCP** (networked TNC: Dire Wolf/SoundModem/HW TNC on a Pi) | ✅ core / 🟡 with LAN-perm UX | none (core) | `KissLinkConfig::Tcp` `winlink/ax25/link.rs:38`; `TcpStream` connect `link.rs:174`; framing `ax25/kiss.rs`, `ax25/frame.rs` | The remote-TNC model. Code unchanged. **But:** a TNC on the home LAN triggers iOS Local Network privacy → requires `NSLocalNetworkUsageDescription` + user grant (loopback is exempt; LAN is not). |
| **KISS-over-serial** (USB COM TNC) | 🔴 | n/a | `KissLinkConfig::Serial` `link.rs:40`; `serialport "4"` `Cargo.toml:95` | No `/dev/tty*` in sandbox; `serialport` has no iOS backend. Cut. |
| **VARA-over-TCP (remote host)** | ✅ | none (core) | `winlink/modem/vara/transport.rs` (cmd+data TCP pair) | Pure TCP client to a configurable host:port; no local spawn in this layer. Same LAN-permission caveat if the VARA host is on the LAN. |
| **Managed Dire Wolf** (local spawn) | 🔴 (→ replace with KISS-TCP) | n/a | `ax25/managed_direwolf.rs:324` | Spawn impossible. The *feature intent* (packet) survives only via remote KISS-TCP. |
| **ARDOP** (`ardopcf`) | 🔴 local / ✅ remote-over-TCP | none if remote | spawn `modem/ardop/transport.rs:228`; remote via `with_addrs` | No iOS `ardopcf`; local spawn banned. Point at a remote ardopcf. CAT-PTT python bridge (`cat_ptt_bridge.rs:142`) has no remote shape → close-serial keying is cut. |
| **Rig control (Hamlib)** | 🔴 local rigctld / ✅ remote rigctld | none if remote | spawn `tux-rig/src/managed.rs:55`; remote client `tux-rig/src/client.rs`; `RigConfigDto` host/port | Remote rigctld over LAN works (LAN-permission caveat). |
| **Bluetooth / BLE TNCs** | 🔌 (BLE only) / 🔴 (Classic) | high | AF_BLUETOOTH socket `ax25/rfcomm.rs:101-126`; sdptool `rfcomm.rs:266,332`; bluetoothctl `ui_commands.rs:4308` | Classic SPP/RFCOMM is **impossible** to a non-MFi radio (UV-Pro path dies). BLE KISS (Mobilinkd TNC3/4) is reachable but **entirely net-new native code** (CoreBluetooth, objc2/Swift FFI), not a pure-Rust port. AccessorySetupKit (iOS 18+) gives clean pairing UX. |
| **GPS** | 🔌 (Core Location) | high | gpsd client `position/gpsd.rs:55`; setup spawns `gps_fix.rs:58,100` | gpsd subprocess/TCP relay is Linux-only. Native CLLocationManager plugin replaces it — *superior* (native, no daemon) but a real native plugin + `NSLocationWhenInUseUsageDescription`. |
| **Mailbox / search** | ✅ | none | `native_mailbox.rs:49-53`; FTS5 `search/index.rs`; `forms/draft_library.rs` | `rusqlite[bundled]` → full FTS5 on iOS; runs in app container. |
| **Forms (author/render/submit)** | ✅ render / 🔌 host | core none; host moderate | axum loopback `forms/http_server.rs`; serialize `forms/serialize.rs`, `forms/multipart.rs`; host `src/compose/WebviewFormHost.tsx` | HTML+CSS render in WKWebView; loopback server allowed (foreground). The *child-Tauri-webview* embedding is desktop-only → embed WKWebView natively. Forms logic itself is portable. |
| **Keyring / credentials** | 🔌 (Keychain) | moderate | `Cargo.toml:89`; `identity/keyring_keys.rs`, `identity/service.rs` | Swap to `apple-native` per-target. Caveat: the cross-process Pat credential-reader contract (`tuxlink-pat` go-keyring) can't work via a spawned helper on iOS — must be in-process. |
| **Tray / minimize-to-tray** | 🔴 | n/a (remove) | `tray.rs:1-144`; `CloseAction` `lib.rs:386-416` | No tray on iOS; tray plugin is desktop-only. Close = suspend/quit. Remove + `cfg`-stub. |
| **Multi-window / compose-window** | 🔴 (→ single-window touch UI) | high | `compose_window.rs`; `help_window.rs`; `logging_window.rs`; `getCurrentWindow()` `src/App.tsx:50`; `AppShell.tsx:19-32` | Collapse separate windows into nav-stack/sheets/tabs. 0→1 redesign. Tauri multi-webview is `unstable` even on desktop. |
| **Touch UI / desktop UX** | 🟡 | moderate | keydown listeners `CloseBehaviorPrompt.tsx`, `ThemeDesigner.tsx`; accelerators `shell/chrome/menuModel.ts`; `:hover` in `App.css`/`forms.css` | No Esc key, no hover, no accelerators on iPad. Needs gesture layer, on-screen dismiss, larger hit targets. React/Leaflet/marked/mermaid deps themselves run fine in WKWebView. |
| **MCP / agent-runner** | 🔴 (exclude) | n/a | UDS `mcp_connection.rs:18-49`; dir-hardening `lib.rs:309-380`; `tuxlink-mcp-core` `transport_uds.rs` (`UnixListener`/`libc::umask`) | UDS in `/tmp`/XDG unavailable in sandbox. Already `#[cfg(target_os="linux")]`-gated (e.g. `mcp_connection.rs:76-81` stub). Exclude from iOS — correct call. The agent-runner crate is transport-agnostic; only the UDS *transport* is Linux-bound. |
| **Audio (soundcard modem path)** | 🔌 (AVAudioSession) | high | device pickers `ui_commands.rs:4409,4416`; `mini_sbc` decoder | Not built today as an in-app DSP modem on Linux; on iOS a soundcard ARDOP/AFSK path (Digirig over USB-C audio) is the *only* MFi-free local-RF option but is net-new DSP + `NSMicrophoneUsageDescription` + audio background mode. Defer. |
| **Pure-Rust deps** (serde/chrono/uuid/rand/quick-xml/mini_sbc) | ✅ | none | `Cargo.toml:68-132` | iOS-compatible, no C deps. |
| **Linux /proc + GL tuning** | ✅ (compiles out) | none | `/proc/device-tree/model` `lib.rs:89`; GL env `lib.rs:63-160` | Already `#[cfg(target_os="linux")]`. iOS never pulls it; GPU is UIKit-managed. |

## 5. iOS RF-interfacing reality

The genuinely viable iPad RF models, from most to least available, **distinguishing Tuxlink-code work from Apple-platform limits**:

**A. Internet-only (CMS over the public internet).** *Apple side:* trivial — outbound internet TCP needs no special permission. *Tuxlink side:* zero code work; `telnet.rs` + B2F are pure Rust. This is the strongest, lowest-friction iPad shape and works anywhere with a data connection. RadioMail's internet-CMS path is the existence proof.

**B. Networked modem on a Pi/PC over TCP (KISS-TCP / remote VARA / remote rigctld).** *Apple side:* the modem-on-the-LAN case trips iOS Local Network privacy — the app must ship `NSLocalNetworkUsageDescription` and the user must grant it (Settings → app → Local Network); Bonjour discovery and multicast each need additional permission/entitlement. Loopback is exempt but irrelevant here (the radio is on another box). RadioMail explicitly instructs users to grant Local Network for its WiFi/VARA path — confirming this is a solved, shippable pattern. *Tuxlink side:* `KissLinkConfig::Tcp` (`link.rs:38`), VARA transport, and the remote rigctld client are all already in the codebase and need no change. This is the "iPad as the head; the Pi is the radio" model and it is the realistic packet/HF story for iPad.

**C. BLE KISS TNC (Mobilinkd TNC3/TNC4 over Bluetooth LE).** *Apple side:* CoreBluetooth gives BLE to any app with no MFi; AccessorySetupKit (iOS 18+) gives a clean pairing flow; the BT-central background mode can keep the link alive — but App Review polices background modes. *Tuxlink side:* **entirely net-new native code** — there is no BLE in the codebase, and the existing AF_BLUETOOTH socket (`rfcomm.rs:101-126`) is Classic-only and dead on iOS. This is a CoreBluetooth↔KISS plugin (Swift/objc2 + FFI feeding the existing pure-Rust KISS framer). RadioMail pairs Mobilinkd over BLE — proof-of-possible, not proof-of-cheap.

**What Apple forecloses regardless of Tuxlink effort:**
- **USB-serial CAT/TNC from a stock iPad to a stock rig** — no `/dev/tty`; only ExternalAccessory+MFi (the rig must carry Apple's MFi coprocessor; generic FTDI/CP2102/CH340 are invisible) or a DriverKit DEXT (M-series iPad only, manual user install — essentially no ham app does this). Not viable.
- **Classic Bluetooth SPP/RFCOMM** (e.g. UV-Pro's SPP control) — iOS deliberately does not expose SPP to third-party apps; requires MFi. Not viable.
- **Backgrounded long sessions** — a multi-minute Winlink transfer over WiFi-to-Pi has *no* continuous-background entitlement and can be suspended mid-transfer; only audio / external-accessory / BT-central modes keep an app alive, and Review scrutinizes them. This is the single biggest iPad UX hazard and is a *platform* limit, not a Tuxlink bug.

## 6. What an iPad v1 should be vs cut

**Build: a touch-first NETWORK-CLIENT Winlink terminal.** The honest, shippable iPad v1 is:

- **Internet CMS** (the headline path; zero RF dependency, works on cellular/WiFi).
- **Remote/networked TNC over KISS-TCP** to a Pi/PC running Dire Wolf or a HW TCP-KISS TNC (with Local Network permission).
- **Remote VARA over TCP** and **remote rigctld** for HF, modem-on-another-box.
- **Native mailbox + FTS5 search**, **forms authoring/rendering** in WKWebView, **Keychain credentials**.
- **Single-window touch UI** with tabs/sheets/nav-stack replacing the desktop multi-window model.

This is a real product: an EmComm operator carries an iPad, leaves the Pi+radio in the go-box, and runs Winlink over the LAN — or runs pure internet CMS when RF isn't needed.

**Explicit cut-list for v1:**
- **All spawned local modems** — managed Dire Wolf, local ardopcf, local rigctld, local VARA. (Use remote equivalents.)
- **KISS-over-serial / USB CAT** — no sandbox path.
- **In-app Bluetooth Classic / RFCOMM** (UV-Pro SPP) — MFi-gated, dead.
- **CAT-PTT python bridge / close-serial keying** — no network shape.
- **MCP server + agent-runner** — UDS-bound, exclude.
- **voacapl propagation, go-pmtiles basemap extraction** — spawn-only, degrade to "unavailable" / bundled tiles.
- **gpsd subprocess + privileged Linux setup** — replaced by native Core Location (Phase 2, not v1-blocking).
- **Tray / minimize-to-tray, multi-window** — no iOS concept.

## 7. Phased plan

**Phase 0 — Prove the shell boots with radio stubbed.** On a macOS host: `tauri ios init`; set `bundle.iOS.minimumSystemVersion`; `cfg`-gate out `nix` process/signal + the managed-modem subsystem, serialport, AF_BLUETOOTH, MCP/UDS, tray, multi-window; swap keyring to `apple-native`; build `heif`-off. Get the React frontend rendering in Simulator with all RF paths stubbed/disabled and IPC working. Deliverable: it compiles for `aarch64-apple-ios-sim` and boots. *This is where every untested compile prediction in §3 gets validated or refuted.*

**Phase 1 — Network-client v1.** Internet CMS end-to-end (`telnet.rs`/B2F already portable); KISS-TCP + remote VARA + remote rigctld with `NSLocalNetworkUsageDescription` and the LAN-permission UX; native mailbox/FTS5; forms in WKWebView (native host, not child-Tauri-webview); Keychain credentials (in-process, not the cross-process Pat helper); touch UI (gesture layer, sheets/tabs, no-hover/no-Esc, large hit targets). Deliverable: a usable iPad Winlink terminal; TestFlight build. **Wire-walk the primary user flows before any "v1 done" claim** (CLAUDE.md hard gate).

**Phase 2 — Native plugins.** Core Location plugin (replaces gpsd; `NSLocationWhenInUseUsageDescription`); BLE KISS TNC plugin (CoreBluetooth↔existing KISS framer, AccessorySetupKit pairing, BT-central background mode for link persistence). Deliverable: GPS position reports + Mobilinkd-class BLE packet on the iPad itself.

**Deferred / never.** Soundcard in-app DSP modem (AVAudioSession + mic permission + audio background mode) — deferred, large. Local managed modems / serial / Classic Bluetooth / MFi-USB / DriverKit — **never** on stock iOS. MCP/agent-runner — never on iOS.

## 8. Effort & risk

**This is a re-architecture + a new touch UI + native plugins + App Store, NOT a recompile.** The portable core (protocol/network/storage/forms-logic) is the *minority* of the work; the majority is the new Swift/iOS shell, the single-window touch redesign, the CoreBluetooth/CoreLocation/Keychain plugins, the subsystem excisions (`nix` process, serialport, AF_BLUETOOTH, MCP, tray, multi-window), the macOS-host build pipeline (new for a Linux-Pi project), and Apple distribution. Plan it as a multi-sprint ground-up app that *reuses* the Rust core, not a port of the Tauri desktop app.

**Top unknowns, ranked:**
1. **Tauri-iOS maturity in practice.** First-class but young; the custom-scheme/IPC/webview edges and any multi-webview need are untested for this app. Phase 0 is explicitly the de-risking spike. *(High uncertainty — no build attempted.)*
2. **Touch-UI rework size.** The multi-window → single-window collapse plus a full gesture/hover/keyboard rework is genuinely large and easy to under-budget. *(High.)*
3. **BLE TNC plugin.** Net-new native CoreBluetooth code with no codebase precedent; pairing UX + background-link persistence + App Review of background modes. *(High.)*
4. **App Store review of a ham/EmComm app.** Local Network entitlement justification, background-mode justification, amateur-radio framing. Precedent exists (RadioMail) but is not guaranteed for a new entrant. *(Medium-high.)*
5. **Background-session limits.** A WiFi-to-Pi transfer can be suspended mid-stream with no entitlement to prevent it — a correctness/UX hazard that may force the BLE/audio path for any "keep alive during transfer" guarantee. *(Medium-high; platform limit, not fixable in code.)*

**RADIO-1 (ADR 0018):** on-air RF validation is operator-only. Agents author, test (mocks/loopback/CI), and ship the iOS transmit-path code freely, including any abort/no-runaway-TX correctness work; agents never key a real radio. The iPad's transmit path on the validated shapes is *remote* (the Pi keys the radio) or BLE — either way, on-air confirmation is the operator's act, not the agent's.

## 9. macOS vs iOS comparison

| Axis | macOS (verified recap) | iOS / iPadOS |
|---|---|---|
| **Build model** | Tauri desktop recompile; soft threshold | Tauri-iOS static-lib + Swift shell; **macOS host + Xcode required**; ground-up app |
| **External process spawning** | **Yes** (`std::process::spawn`) — drives local Dire Wolf, rigctld via Homebrew | **No** — fork/exec banned; `nix` process feature must be gated out; managed-modem subsystem excised |
| **Serial** | Works (needs `/dev/cu.*` discovery) | **Impossible** (no `/dev/tty`; serialport has no iOS backend) |
| **Bluetooth** | AF_BLUETOOTH compiles-but-fails-at-runtime (not a compile blocker) | Classic/AF_BLUETOOTH **dead**; BLE via CoreBluetooth only (net-new plugin); Classic needs MFi |
| **GPS** | (Linux gpsd path; macOS not the spawn-enabler story) | gpsd impossible → **native Core Location** plugin (`NSLocation…`) |
| **Tray / window** | Desktop tray + multi-window retained | **No tray, single-window** — collapse to nav/sheets/tabs |
| **Keyring** | `apple-native` Keychain cfg-swap | `apple-native` Keychain cfg-swap (same), but cross-process Pat helper can't work — in-process only |
| **Viable transports** | CMS-internet, KISS-TCP **incl. managed Dire Wolf (local spawn)**, VARA-TCP, **Hamlib/rigctld (local spawn)**, KISS-serial | CMS-internet, KISS-TCP (**remote only**), VARA-TCP (remote), rigctld (**remote only**), BLE KISS (plugin). **No** local spawn, **no** serial |
| **macOS cuts vs iOS cuts** | Cuts: ARDOP (no macOS ardopcf), in-app BT RFCOMM, privileged gpsd helper | Cuts: **all local-spawn modems**, serial, Classic BT, CAT-PTT bridge, MCP, voacapl/go-pmtiles, tray/multi-window |
| **Distribution** | DMG / direct (or notarization) | App Store / TestFlight; Apple review; entitlements |
| **Overall effort** | **Viable port** — mostly recompile + a few cfg-swaps; local radios still work | **Viable product, ground-up app** — re-architecture + touch UI + native plugins + App Store; network-client only |

## 10. Corrections from verification

The adversarial pass overturned or materially corrected these survey claims; the report above reflects the corrected versions:

1. **"`tauri-plugin-shell`/sidecar drives the modems, and that's the iOS spawn blocker."** *Overturned.* Modem/control spawns use `std::process::Command` / `ManagedModem::spawn` directly; `tauri-plugin-shell` is used only for `xdg-open` (`ui_commands.rs:2424`). The iOS blocker is the universal sandbox ban on *any* child process, not the plugin.

2. **"`nix` process/signal is a runtime-only constraint (compiles, just can't spawn)."** *Corrected.* The `nix` `process`/`signal` features (`Cargo.toml:72`) are plausibly a **compile-time** obstacle on iOS, and excising them means removing the whole managed-modem lifecycle, not flipping a runtime flag. Treat "no managed modems on iOS" as a subsystem excision.

3. **"`libheif` just needs `--no-default-features` to drop on iOS."** *Corrected.* `--no-default-features` gates the Rust code, but `libheif-sys ^2.x`'s `build.rs` **pkg-configs system libheif**, which doesn't exist on iOS — the build-script check is the real obstacle. The feature must be off *and* the dependency excluded for the iOS target; iOS decodes HEIC natively.

4. **"KISS-TCP / VARA-TCP / CMS work on iPad essentially unchanged."** *Corrected (code true, deployment overstated).* The *code paths* are genuinely portable and unchanged — but "works on iPad" was overstated: Tauri-iOS must first be stood up, and a **TNC/VARA on the LAN triggers iOS Local Network privacy** (`NSLocalNetworkUsageDescription` + user grant). CMS-over-internet is exempt (not LAN); loopback is exempt. The corrected framing: core unchanged, *deployment* needs the iOS shell + LAN-permission UX.

5. **"The Rust core already runs ARDOP/VARA/Dire Wolf DSP in-process, so it's a clean FFI candidate."** *Corrected.* The Rust core is *protocol + process/socket lifecycle*, not in-process modem DSP — ARDOP/VARA/Dire Wolf are external processes/daemons. The protocol/network/storage core is a clean FFI candidate; the modem *engines* are not "already in-process."

6. **"BLE KISS would be a Tauri plugin."** *Refined.* More precisely native CoreBluetooth (Swift/objc2 + FFI), net-new, with no codebase precedent — not a pure-Rust path and not a thin plugin shim.

7. **Several blanket "iOS is impossible / Tauri 2 has no iOS support" refutations were themselves wrong.** Multiple verification entries asserted "Tauri 2 has no iOS target / iOS is Tauri-4-era." *That is incorrect* and the research lane (high confidence, Tauri docs) overrides them: **iOS is a first-class Tauri 2 target since 2.0 stable (2024-10-02).** The accurate blocker is not "Tauri can't," it's "Tuxlink hasn't, and several subsystems must be excised/replaced." The report uses the research-lane facts, not the over-broad refutations.

8. **"MCP UDS layer — exclude on iOS."** *Upheld* (verification agreed). Already `#[cfg(target_os="linux")]`-gated (`mcp_connection.rs:76-81`); the agent-runner crate is transport-agnostic, only the UDS transport is Linux-bound. Correct call.

---


---

# Part III — Windows

> **Scope & provenance.** Static analysis only — no Windows build was attempted (this dev Pi has no MSVC toolchain and cannot finish a cold `cargo` build regardless; see CLAUDE.md "Testing"). Every `file:line` below is from the worktree at commit on branch `claude/nice-tu-ac3438`. Where the survey's claims were adversarially refuted, the verdict is reflected and the original framing corrected in §8. RADIO-1: on-air validation of any transmit path is operator-only (ADR 0018); the agent-side analysis below covers code reachability and compile/runtime portability, not on-air behavior.

## 1. Windows bottom line

**Can it run today? No — and unlike macOS, it won't even compile.** macOS shares Tuxlink's POSIX substrate, so the bulk of the Unix-isms (`nix`, Unix-domain sockets, `libc::getuid`) *compile* there and the threshold is "soft" (a keyring cfg-swap, HEIF, one unguarded `statvfs`). Windows is not Unix. The same code that builds on macOS is a **hard compile blocker** on `x86_64-pc-windows-msvc`: the `nix` crate (Cargo.toml:72) does not resolve for Windows targets at all, `tokio::net::UnixListener`/`UnixStream` are `cfg(unix)`-only in the standard library, and `std::os::unix::*` modules do not exist. Today's "non-Linux stub" branches (e.g. mcp_connection.rs:80-81) are **compile-only stubs that were never exercised** — and several are incomplete (the `mcp_socket_path()` function at mcp_connection.rs:29-30 is `#[cfg(target_os = "linux")]` with no Windows sibling, so a Windows build fails with "function not found", not a graceful stub).

**Is a port viable? Yes** — and the framework carries it. Tauri 2.11.2 is a tier-1 Windows target (WebView2, NSIS/MSI, Authenticode), and every Tauri plugin Tuxlink already depends on (`shell`, `fs`, `dialog`, `window-state`, `tray-icon`) supports Windows. The gate is entirely Tuxlink's own Linux-only architecture, not a Tauri capability gap.

**The defining trade-off vs macOS:** Windows is **more Rust Unix-porting work** (Unix-isms that compile on macOS become compile errors here, so they must be genuinely cfg-split or trait-abstracted, not left as compile-passing stubs) **but offers the richest native radio ecosystem of any platform** — VARA HF/FM, ardopcf, Dire Wolf, and Hamlib all ship native Windows binaries and are spawnable/TCP-reachable exactly as Tuxlink already drives them. macOS is the inverse: easier compile, thinner native modem ecosystem (VARA only under Wine). Windows is the only platform where Tuxlink could drive the **full canonical modem set, including paid native VARA, with zero emulation shim.**

## 2. Threshold: would it compile?

**No.** macOS clears the compile threshold with a handful of fixes; Windows fails on **7+ independent fronts**, several of them deep in production hot paths with zero platform abstraction. The genuine blockers:

| Blocker | Evidence (file:line) | cfg state | Effort |
|---|---|---|---|
| `nix` crate, unconditional, features signal/process/fs | Cargo.toml:72; used at winlink/modem/process.rs:28-29, basemap/commands.rs:391 | **unconditional** — `[dependencies]`, not target-gated | High |
| `nix::sys::statvfs` free-space pre-flight | basemap/commands.rs:391 | unconditional | Moderate |
| `nix::sys::signal::kill` / `unistd::Pid` (SIGINT→SIGKILL modem teardown) | winlink/modem/process.rs:28-29 | unconditional | Moderate |
| `tokio::net::UnixListener` (whole MCP server transport) | tuxlink-mcp-core/src/transport_uds.rs:36,174,199 | unconditional (`cfg(unix)` in std) | High |
| `tokio::net::UnixStream` (MCP stdio→UDS shim) | tuxlink-mcp/src/main.rs:7,19-20 | unconditional | High |
| `tokio::net::UnixStream` (d3zwe agent runner) | d3zwe/src/uds.rs:32,245 | unconditional | High |
| `std::os::unix::fs::FileTypeExt::is_socket()` (MCP socket validation) | tuxlink-mcp-core/src/transport_uds.rs:29,159 | unconditional | Moderate |
| `std::os::unix::fs::FileExt::read_at()` (lock-free PMTiles read) | basemap/mod.rs:32 | unconditional | Moderate |
| `std::os::unix::fs::PermissionsExt::mode()` (0700/0600 security checks) | lib.rs:320-323, mcp_connection.rs:47, transport_uds.rs:85,465 | unconditional | Moderate |
| `std::os::unix::fs::OpenOptionsExt::mode()` (0o600 hardening) | mcp_ports.rs:1569 | unconditional | Moderate |
| `std::os::unix::process::ExitStatusExt` (signal extraction) | ax25/direwolf_probe.rs:189, modem/process.rs:201 | unconditional | Low |
| `std::os::unix::fs::symlink` (MCP validation) | tuxlink-mcp-core/src/validate.rs:304 | unconditional | Moderate |
| `libc::getuid()` (socket-path fallback) | d3zwe/src/uds.rs:247, mcp_connection.rs:32, lib.rs:1303 | unconditional in d3zwe; **fn-gated with no Windows sibling** in mcp_connection.rs:29-30 | Moderate |
| `libc::umask()` (security-critical bind window) | tuxlink-mcp-core/src/transport_uds.rs:173,178 | unconditional | Moderate |
| `libc::getgroups()` (dialout-group probe) | logging/env_probes/serial.rs:119,124 | unconditional | Moderate |
| `libc::socket(AF_BLUETOOTH=31, BTPROTO_RFCOMM=3)` (UV-Pro RFCOMM) | winlink/ax25/rfcomm.rs:101-102,126 | unconditional | High |

**What's *already* correctly gated** (so it is **not** a Windows blocker):

- **webkit2gtk / gtk** — Cargo.toml:152-154 lives under `[target.'cfg(target_os = "linux")'.dependencies]`. On Windows these are simply not pulled; wry auto-selects WebView2. ✅
- **WebKitGTK GL env tuning** (Mesa/llvmpipe/Pi detection) — lib.rs:63-209 is `#[cfg(target_os = "linux")]` with a non-Linux no-op at :207-209. ✅
- **Free-disk guard** — free_disk_guard.rs:48-67 is `#[cfg(target_os = "linux")]`, returns `None` on non-Linux and the caller treats that as `u64::MAX` (no block). ✅
- **PDF/print** — pdf_export.rs is Linux-gated (export at :68-145, print at :156-191) with non-Linux stubs returning `UnsupportedPlatform` (:195-211). Compiles; doesn't function. ✅ *(compile)* / 🔴 *(runtime — see §3)*

**Honest quantification — Windows cfg-work vs macOS.** macOS needed roughly: keyring backend cfg-swap, HEIF disposition, and one `statvfs` guard — call it **~3-4 fix sites, mostly Cargo features**. Windows needs **~16+ distinct compile-blocker sites across 4 crates** (`tuxlink`, `tuxlink-mcp-core`, `tuxlink-mcp`, `d3zwe`), of which the MCP UDS transport alone touches 3 crates and the `nix`/`libc`/`std::os::unix` surface is woven through security validation, the modem lifecycle, PMTiles I/O, and Bluetooth. This is **not a cfg-gating afternoon** — it is a transport-abstraction + permission-model-branching exercise. Realistic order-of-magnitude: macOS is a few days of fixes; Windows compile-threshold work is **multiple weeks** before the app boots, and that's *before* any feature is wired (§6, Phase 0).

## 3. Windows gap matrix

Legend: ✅ works (no/near-zero code) · 🟡 port work · 🔴 cut for v1.

| Subsystem | Status | Why / evidence | Effort |
|---|---|---|---|
| **CMS (telnet/TLS over internet)** | ✅ | `native-tls` (Cargo.toml:94) + std TCP; pure-Rust B2F engine. No Unix dep. Authorized for agent dev testing (not a transmission). | None |
| **KISS over TCP (Dire Wolf etc.)** | ✅ | std `TcpStream`; cross-platform. | None |
| **KISS over serial + COM discovery** | 🟡 | `serialport` 4 (Cargo.toml:95) is cross-platform — opening `COM3` works. But discovery in logging/env_probes/serial.rs hand-rolls `/dev/serial/by-id`, `/dev/ttyUSB*`, `/etc/group`+`getgroups` (:119,124). Replace with `serialport::available_ports()` (SetupAPI/VID-PID) and **drop the dialout-group concept** (no Windows analog). | Low–Moderate |
| **VARA HF / VARA FM (native!)** | ✅ | VARA is a **native Windows .exe** (TCP cmd/data 8300/8301). No Linux build exists — Windows is its home. Tuxlink's existing TCP-host model reaches it unchanged. *Spawning/lifecycle: see managed-modem row.* | None (integration) |
| **ARDOP (ardopcf WIN32 native!)** | ✅ | ardopcf ships `ardopcf_amd64_Windows_64.exe` and `..._32.exe` (TCP 8515). TCP integration ports for free. | None (integration) |
| **Dire Wolf (native binary)** | ✅ | `direwolf.exe` shipped per release (64-bit; 32-bit dropped at 1.8.x). KISS-over-TCP reaches it. | None (integration) |
| **Rig control (Hamlib native)** | ✅ | `rigctld.exe` (w32/w64 installers). Tuxlink drives rigctld over TCP. | None (integration) |
| **Managed-modem spawning (works!)** | 🟡 | `std::process::Command` is cross-platform and *spawn* works. The **teardown** is the blocker: modem/process.rs:28-29 uses `nix::sys::signal::kill` SIGINT→SIGKILL + `ExitStatusExt` (:201). Replace with `TerminateProcess` / Windows `Child::kill` + a Windows exit-code branch. **Refutes "works as-is"** — spawn works, kill semantics need a Windows path. | Moderate |
| **Bluetooth (UV-Pro RFCOMM)** | 🟡 (or 🔴 v1) | rfcomm.rs is raw `AF_BLUETOOTH`/`sockaddr_rc`/libc read/write/close. Near-1:1 Winsock analog exists (`AF_BTH`=32, `BTHPROTO_RFCOMM`=3, `SOCKADDR_BTH`, `WSAStartup`, recv/send/closesocket, SDP via `WSALookupService`). **Watch the byte order:** the LE bdaddr reversal at rfcomm.rs:37 must NOT be reapplied — Windows `BTH_ADDR` is host-order. High effort; strong v1-cut candidate. | High |
| **GPS** | 🟡 | gpsd.rs:16 connects `127.0.0.1:2947` over TCP, addr overridable via `TUXLINK_GPSD_ADDR`. **Remote gpsd-over-TCP works today, zero code.** Native = WinRT `Windows.Devices.Geolocation.Geolocator` behind the existing position arbiter. Privileged setup helper bin/tuxlink-gps-fix.rs (pkexec/apt/systemd) is **dead on Windows — cut.** | Low (remote) / Moderate (native) |
| **Keyring (Credential Manager)** | 🟡 | Cargo.toml:89 pins `default-features=false, features=["sync-secret-service"]` — **Linux D-Bus only, compile-time-selected**. Wrong feature on Windows silently falls back to the in-process `mock` store (the exact failure the Cargo.toml note at :79-88 warns about). Needs target-conditional `[target.'cfg(windows)'.dependencies] keyring{features=["windows-native"]}` (DPAPI/`WinCredential`). Call sites (identity/*, winlink/secure.rs) are backend-agnostic. Mind the `tuxlink-pat` cross-process reader contract — its Windows equivalent must read Credential Manager, not Secret Service. **Verdict-corrected: not TRIVIAL; this is Moderate, with security implications.** | Moderate |
| **Forms PDF (WebView2)** | 🔴 v1 → 🟡 | Linux uses `webkit2gtk::PrintOperation`/`gtk::PrintDialog` (pdf_export.rs:68-145, 156-191); non-Linux returns `UnsupportedPlatform`. Windows replacement = `ICoreWebView2.PrintToPdf()` async. **Note:** ICS-309 export uses the pure-Rust `printpdf` crate (ui_commands.rs:235-400) and works on Windows unchanged — only the *webview* form PDF/print is affected. | High |
| **Mailbox / persistence** | ✅ | `rusqlite` bundled (Cargo.toml:97), `dirs` 6 resolves `%APPDATA%`/`%LOCALAPPDATA%` Known Folders automatically. Audit config.rs for any hardcoded `~/.local/share`/`$XDG_*` literals that bypass `dirs`. | None–Low |
| **Tray / window** | ✅ | tray.rs:35-75 uses `TrayIconBuilder`/`MenuBuilder`, no platform conditionals, PNG icon. Close-to-tray already has the Windows path: `hide()` under `#[cfg(not(target_os="linux"))]` vs `minimize()` on Linux (ui_commands.rs:6746-6750). `WebviewWindowBuilder...decorations(false)` (help/logging/compose windows) works on WebView2. | None |
| **Audio discovery (WASAPI)** | 🟡 | **Not a cpal port — Tuxlink doesn't use cpal.** It shells `arecord -L`/`aplay -L` and parses ALSA (ui_commands.rs:4355-4417), passing `plughw:` strings to modems (winlink_backend.rs:2473). Windows = WASAPI `IMMDeviceEnumerator`/`EnumAudioEndpoints` **and** the external-modem device-string format differs (no `plughw:` on Windows). Moot for the UV-Pro path (rides RFCOMM, not a soundcard). | High |
| **Privileged helpers** | ✅/🔴 | Tuxlink deliberately avoids elevation (rfcomm.rs:9-11 "no root / no rfcomm bind"; COM + AF_BTH need no elevation on Windows). Ship an `asInvoker` manifest (Tauri controls it) — **less work than Linux polkit.** The GPS pkexec helper is cut (see GPS row). | Trivial (manifest) |
| **Bundling / CI** | 🟡 | tauri.conf.json:28-91 has `targets:"all"` but only `linux.deb`/`linux.rpm` (:48-90); no `windows` block. Add `"windows": { "webviewInstallMode": "downloadBootstrapper" }` (config-only, no code). **Build ON Windows** (MSVC, not GNU): MSI needs WiX (Windows-only) + VBSCRIPT; NSIS is the low-friction first artifact. Cross-compile from Linux yields NSIS-only and is officially discouraged → add a Windows CI runner. Signing: OV minimum, EV to dodge SmartScreen. | Moderate |
| **WebKitCache uninstall cleanup** | ✅ | uninstall_cleanup.rs:262-265 deletes WebKit-named dirs unconditionally; harmless no-op on Windows. Add WebView2 (`EBWebView`) cache paths post-port if desired. | Low |
| **Frontend (React/DOM)** | ✅ | No platform-detecting JS; only Rust `cfg!()`. WebKitGTK comments (AprsPositionsMap.tsx:205,304) document quirks but the code is standard React/DOM — Chromium WebView2 likely renders **better** (native GPU, superior GL), notably for the APRS/tile map. | None |

## 4. Ecosystem reality

**Windows is the richest *native* radio-binary target of any platform** — and crucially, this upside costs Tuxlink almost no code. Distinguish two things:

- **Ecosystem (external):** every modem Tuxlink spawns or connects to runs natively on Windows — `rigctld.exe` (Hamlib w32/w64), `ardopcf_amd64_Windows_{32,64}.exe`, `direwolf.exe`, and **VARA HF/FM, which is native Windows with no Linux build at all** (Linux runs it under Wine). All speak TCP on the same ports Tuxlink already targets (VARA 8300/8301, ardopcf 8515, rigctld TCP, KISS-over-TCP for Dire Wolf). **Those TCP integrations port for free** — they are platform-agnostic by construction.
- **Tuxlink code work:** the porting burden is *entirely internal* — the `nix`/UDS/`libc`/`std::os::unix` compile blockers (§2) and the managed-modem **teardown** path. The ecosystem contributes essentially zero porting work; the OS substrate contributes all of it. This is the inverse of macOS, where the substrate is easy but the native modem ecosystem is thin.

**Net:** Windows is *confirmed-feasible* (all external deps present natively), *blocked only by internal Rust Unix-isms*, and uniquely able to drive the **full** canonical modem set including paid native VARA without a shim.

**Competitive reality:** on Windows, Tuxlink competes head-to-head with **Winlink Express on its home turf**. Express is the de-facto reference EMCOMM client — Windows-only, deeply mature, the full official HTML Forms library, PACTOR hardware-TNC support, and the client teams already train on. Tuxlink's wedge is *modern UI + open/auditable + native Rust B2F engine + one cross-platform codebase*, **not** "drop-in replacement." On Linux, Tuxlink fills a gap Express leaves empty (its defensible beachhead); on Windows it enters a much harder fight against an entrenched incumbent. That argues for sequencing Windows **after** Linux feature maturity.

## 5. Windows v1 — include vs cut

**Include (the payoff is the native modem set):**
- CMS (telnet/TLS), mailbox/persistence, tray/window, frontend, native dialogs, ICS-309 `printpdf` export — all ✅, near-zero work.
- **VARA HF/FM, ARDOP, Dire Wolf, Hamlib over TCP** — the headline capability; integrations port for free once the backend compiles.
- Managed-modem spawn **with a Windows teardown path** (TerminateProcess) — required to launch the above.
- KISS-serial + COM discovery via `serialport::available_ports()`.
- Keyring via Credential Manager (`windows-native`).
- WebView2 bundling block + Windows CI runner + NSIS artifact + code signing.
- GPS via **remote gpsd-over-TCP** (zero code) — defer native WinRT.

**Cut for v1:**
- **In-app Bluetooth RFCOMM (UV-Pro)** — Winsock rewrite is high-effort; UV-Pro users can use a Bluetooth-SPP **COM port** via `serialport` with no new BT code as an interim path.
- **Webview Forms PDF/print** — `UnsupportedPlatform` stub stays; ICS-309 export still works. Wire `PrintToPdf` in Phase 2.
- **WASAPI soundcard-modem audio discovery** — high effort; moot for UV-Pro; manual device-string entry as interim.
- **Native WinRT Geolocator** — remote gpsd covers v1.
- **MCP local-agent integration (UDS)** — the agent/MCP backbone is a Linux bonus feature, not core modem operation. Ship Windows v1 **without it** rather than building a named-pipe/TCP transport up front.
- **Privileged GPS setup helper** (tuxlink-gps-fix) — dead on Windows; cut.

## 6. Phased plan

**Phase 0 — Make it compile and boot (the hard part).**
Goal: `cargo build --target x86_64-pc-windows-msvc` succeeds and the app boots to the mailbox.
1. **Excise/abstract `nix`:** replace `statvfs` (basemap/commands.rs:391) with `GetDiskFreeSpaceEx` (or a cfg-split); replace signal-kill teardown (modem/process.rs:28-29,201) with a Windows `TerminateProcess`/`Child::kill` branch.
2. **Gate the MCP UDS layer:** cfg-gate `transport_uds` at the **library boundary** (tuxlink-mcp-core/src/lib.rs:28 — currently exported unconditionally) and the shim/d3zwe UnixStream paths; ship Windows v1 with MCP **off** (no transport rewrite yet).
3. **Branch the `std::os::unix` permission/IO surface:** `PermissionsExt`/`OpenOptionsExt`/`FileTypeExt`/`FileExt::read_at`/`symlink`/`ExitStatusExt` (§2 table) — Windows ACL/no-op branches; `read_at` → `seek_read`.
4. **`libc` calls:** give `getuid`/`umask`/`getgroups` Windows branches or cfg them out; add a Windows sibling for `mcp_socket_path()` (mcp_connection.rs:29-30) so it isn't "fn not found".
5. **Keyring:** target-conditional `windows-native` (Credential Manager).
6. **WebView2:** add the `windows` bundle block (tauri.conf.json) + `asInvoker` manifest; prove it boots and renders under Chromium.
7. **cfg-gate env_probes** (serial/audio/keyring Linux diagnostics) to no-op stubs on Windows.

**Phase 1 — Wire the native radio stack (the payoff).**
Connect VARA HF/FM, ardopcf, Dire Wolf, Hamlib over their TCP ports; make managed-modem spawn-and-teardown solid on Windows; COM discovery via `serialport::available_ports()`; keyring round-trip + `tuxlink-pat` Windows reader contract. **This is where Windows' native-VARA advantage lands.** (RADIO-1: agent validates via mocks/loopback/CI; on-air is operator-only.)

**Phase 2 — Parity polish.**
WebView2 `PrintToPdf` for forms; WASAPI audio discovery + Windows modem device-string format; native WinRT Geolocator behind the position arbiter; optional in-app Bluetooth RFCOMM (Winsock `AF_BTH`).

**Deferred / optional:**
MCP local-agent integration over Windows named pipes or loopback TCP; MSI (WiX) bundle alongside NSIS; Microsoft Store / winget (no native MSIX bundler — separate, manual effort); EV signing for SmartScreen reputation.

## 7. Effort & risk

Ranked by uncertainty (highest first):

1. **`nix` removal/replacement scope — HIGH unknown.** `nix` is woven through teardown (signals) and free-space (`statvfs`). Signals have no Windows analog; the SIGINT-grace-then-SIGKILL semantics must be re-expressed as TerminateProcess + a grace window. Risk: the modem teardown is a correctness-critical path (no runaway-TX, working abort per ADR 0018) — the Windows branch must preserve abort behavior, and we can't on-air-validate it here.
2. **UDS gating vs replacement — HIGH/structural.** The survey's "feasible without architectural rewrites" was **refuted on this point**: shipping MCP *on* Windows needs a brand-new IPC transport (named pipes / loopback TCP). Shipping MCP *off* (v1 recommendation) is a clean cfg-gate at the library boundary — much lower risk. Decision: defer the transport, gate the layer.
3. **Cross-compile vs build-on-Windows — MODERATE, well-understood.** MSI requires building on Windows (WiX + deprecating VBSCRIPT); cross-compiling from the Pi yields NSIS-only and is officially "last resort." Plan a **native Windows CI runner**; GNU target is unsupported (use MSVC).
4. **Signing — MODERATE, money/process not code.** OV cert minimum to ship downloadable binaries; OV still trips SmartScreen until reputation builds; EV gives immediate reputation. Operator/business decision, not an engineering blocker.
5. **Keyring DPAPI + cross-process contract — MODERATE.** Wrong feature → silent `mock` fallback (security-relevant); the `tuxlink-pat` reader needs a Windows Credential Manager counterpart.
6. **Bluetooth Winsock rewrite — HIGH but cuttable.** `AF_BTH`/`SOCKADDR_BTH`/`WSAStartup`/SDP + the byte-order trap (rfcomm.rs:37). v1-cut with a COM-port interim removes this from the critical path.

**RADIO-1 standing note:** all transmit-path code above is agent-writable (ADR 0018) — agents claim, write, test via mocks/loopback/CI, and ship it. The agent never runs a transmit-capable binary against real hardware; on-air validation is operator-only.

## 8. Corrections from verification

The adversarial pass **refuted** several survey claims; the section above reflects the corrected verdicts:

1. **"webkit2gtk/gtk gated + PDF stub ⇒ Windows builds" — REFUTED.** The webkit gating *is* correct, but the claim was incomplete: the build still fails on the unguarded `nix` crate (modem/process.rs:28-29) and the keyring Secret-Service-only feature. Correct framing: *those specific components are portable; the build does not compile* (§2).

2. **"Tauri 2.x supports Windows ⇒ Tuxlink is Windows-portable" — REFUTED (conflation).** Tauri's Windows support is real and first-class; Tuxlink's *application* is architecturally Linux-bound. The verdict is **requires-port**, gated entirely by Tuxlink's own Unix-isms — not a Tauri gap.

3. **"All managed-modem features run natively on Windows" — REFUTED.** The *ecosystem* binaries are native; Tuxlink's *process lifecycle* is not. Spawn works (`std::process::Command`); **teardown does not** (`nix` signals, modem/process.rs:28-29) and needs a Windows path. Ecosystem-native ≠ Tuxlink-runs.

4. **"serialport ⇒ device discovery just needs `available_ports()`" — REFUTED (framing).** Accurate that discovery needs Windows enumeration; misleading to imply a pre-existing multi-platform design with one gap. Tuxlink is intentionally Linux-only; discovery is one of many port tasks, and `serialport` was included for opening known Linux paths, not cross-platform discovery.

5. **"keyring backend swap is TRIVIAL" — REFUTED (effort).** It is a compile-time feature swap *plus* a Windows reader for the `tuxlink-pat` cross-process contract, with credential-security implications. Re-rated **Moderate**.

6. **"Windows port is feasible without architectural rewrites" (research, mixed verdict) — PARTIALLY REFUTED.** Directionally correct (the `ByteLink` / Fix-source traits do isolate OS layers), but it **omitted the MCP UDS transport** (no Windows tokio equivalent — a new IPC layer) and **understated keyring**. env_probes also can't merely "no-op": they actively probe and need Windows equivalents (or genuine cfg-stubs). §5/§6 handle this by **cutting MCP from Windows v1** rather than building the transport up front.

7. **"Net: Windows is MORE work than macOS but richest radio capability — opposite trade-off" — held with nuance.** One reviewer argued the Unix-isms are *identical* for both targets, so "more work" is unjustified. The decisive distinction the reviewer missed: **macOS is Unix, so those Unix-isms compile there and the threshold is soft; Windows is not Unix, so the same code is a hard compile blocker requiring genuine cfg-splits, not pass-through stubs.** The "more Rust work on Windows" claim therefore **holds** (verified directly: `nix` at Cargo.toml:72 won't resolve for `*-windows-msvc`; `tokio::net::UnixListener` is `cfg(unix)`). The "richest radio capability" half also holds — native VARA exists only on Windows. The trade-off framing in §1 is correct as stated.

8. **`mcp_socket_path()` gating gap — confirmed.** mcp_connection.rs:29-30 is `#[cfg(target_os = "linux")]` with **no Windows sibling**; the *call site* (:76-81) is gated, but a Windows build of the function itself is "fn not found." This is a genuine compile blocker, not a graceful stub (§2, §6 Phase 0 step 4).

---

**Files referenced (absolute paths):** `src-tauri/Cargo.toml` (72, 89, 94-96, 152-154), `src-tauri/src/mcp_connection.rs` (29-30, 47, 76-81), `src-tauri/src/winlink/modem/process.rs` (28-29, 201), `src-tauri/tuxlink-mcp-core/src/transport_uds.rs` (29, 36, 85, 159, 173-178, 199, 465), `src-tauri/src/winlink/ax25/rfcomm.rs` (9-11, 37, 101-126), `src-tauri/src/forms/pdf_export.rs` (68-145, 156-211), `src-tauri/tauri.conf.json` (28-91). Analysis is static only — no Windows build was performed.

---

# Part IV — Android

*Static analysis + platform research only. No Android build was attempted — every claim below is grounded in the Tuxlink source tree (`file:line`), the Tauri/Android platform docs, and the amateur-radio Android ecosystem. RF-path claims are authorship-only: per RADIO-1 (ADR 0018) no agent has on-air-validated any transmit path, and the dev shell has no radio.*

## 1. Android bottom line

**Can it run today? No.** Tuxlink ships only Linux bundle targets (`src-tauri/tauri.conf.json:30` `"targets": "all"` resolves to `deb`/`rpm`/`appimage` on a Linux host — lines 49/68/87), the crate self-describes as "Linux-native full-capability Winlink client" (`src-tauri/Cargo.toml:14`), and there is no Android Gradle project, NDK config, Kotlin plugin, or `aarch64-linux-android` CI. The lone `#[cfg_attr(mobile, tauri::mobile_entry_point)]` (`src-tauri/src/lib.rs:462`) is a Tauri placeholder, not a working mobile build.

**Is a port viable? Yes — as a re-architecture, not a recompile.** Tauri 2.x has first-class Android support (since 2.0 stable, 2024-10-02): `tauri android init/dev/build`, Rust core compiled per-arch to a `.so` via the NDK, UI in Android System WebView (the same architecture as the Linux desktop with WebKitGTK swapped for the Android WebView). The pure-Rust protocol core — B2F exchange (`src-tauri/src/winlink/session/mod.rs`), AX.25/KISS framing (`frame.rs`, `kiss.rs`), CMS health, handshake, LZHUF — is `~95%` Android-ready at the Rust layer with zero `#[cfg]` work. The blockers are I/O shims, native plugins, and a touch UI.

**How it compares to iOS — sandbox-similar, RF-better.** Android shares iOS's *most decisive* constraint: a third-party-app sandbox that blocks executing bundled binaries from app storage (W^X / SELinux, enforced for `targetSdk ≥ 29`; Play requires a recent target API, so you cannot dodge it by targeting an old SDK). So **the entire spawned-modem architecture dies on Android exactly as it does on iOS** — no `direwolf`, no `ardopcf`, no `rigctld`, no `python3` bridge, no `voacapl`, no `go-pmtiles` as child processes. But Android is **materially better than iOS for radio interfacing in two ways that the iOS analysis correctly called out as iOS-fatal**:

- **Bluetooth Classic SPP/RFCOMM is open to Android apps** via `android.bluetooth.BluetoothSocket` (UUID `00001101-…`). No MFi gate. This is the load-bearing difference: the large installed base of Classic-SPP KISS TNCs (Mobilinkd TNC2/TNC4, Kenwood TH-D74) is reachable on Android but **not** on iOS (iOS has no SPP — BLE/MFi only).
- **USB host (OTG) is available**, so USB-serial KISS radios, USB-CAT/PTT, and even USB-audio soundcard-TNCs are reachable through the Java USB Host API — none of which iOS offers (no general USB host, no app-level USB-serial).

Net: Android is a **better mobile RF target than iOS**, while sharing iOS's no-local-spawn and touch-UI constraints. Both are still a re-architecture; macOS remains the only non-Linux target that keeps the spawn-driven desktop advantage.

---

## 2. The defining constraint: no spawned processes (like iOS), but a real Bluetooth/USB story (unlike iOS)

Android 10 (API 29) enforces W^X: *"Untrusted apps that target Android 10 cannot invoke `execve()` directly on files within the app's home directory."* Combined with SELinux device-node policy, this kills every production `std::process::Command::spawn` of a bundled binary. The only native-code path is an in-process `.so` loaded via the NDK — which Tauri-on-Android already does for the Rust core.

**Dead spawned-modem / spawned-helper features on Android (production runtime):**

| Spawn | `file:line` | Why it dies | Recoverable? |
|---|---|---|---|
| `direwolf` (KISS soundmodem + PTT) | `src-tauri/src/winlink/ax25/managed_direwolf.rs:260` | bundled-binary exec + ALSA audio | **Yes** — KISS-over-TCP to remote Dire Wolf (`KissLinkConfig::Tcp`, `link.rs:38`) |
| `ardopcf` (ARDOP TNC, dual TCP 8515/8516) | `src-tauri/src/winlink/modem/process.rs:127` | bundled-binary exec | **Yes** — remote ARDOP over TCP |
| `rigctld` (CAT daemon) | `src-tauri/tux-rig/src/managed.rs:55` | bundled-binary exec | **Yes, zero code change** — `RigctldClient::connect(host, port)` already exists (`tux-rig/src/client.rs:24`); point at remote rigctld |
| `python3` CAT-PTT bridge (FT-710 codec-reset fix) | `src-tauri/src/winlink/modem/ardop/cat_ptt_bridge.rs:142` | no Python runtime; bundled-binary exec | Partial — companion-host TCP, or native serial keying; single-radio workaround, low loss |
| `voacapl` (propagation engine) | `src-tauri/src/propagation/engine.rs:80` | bundled-binary exec | Defer — remote HTTP service, or cut (non-essential) |
| `go-pmtiles` (basemap extraction sidecar) | `src-tauri/src/basemap/commands.rs:179` | bundled-binary exec | Defer — remote service / native map tiles, or cut |
| `lsof` (audio-fd probe) | `src-tauri/src/winlink/modem/process.rs:291` | sandbox blocks; not TX-critical | Leave out — code already soft-fails (200 ms sleep) |

**System-probe spawns** that are *not* dead but must be re-pathed to native Android APIs: `sdptool` (`rfcomm.rs:266,332`) → `BluetoothDevice.getUuids()`; `bluetoothctl devices Paired` (`ui_commands.rs:4308`) → `BluetoothAdapter.getBondedDevices()`; `arecord`/`aplay -L` (`ui_commands.rs:4409,4416`) → `AudioManager.getDevices()`; `uname -r` (`logging/manifest.rs:112`) → `android.os.Build` / `/proc/version`. All four soft-fail today, lowering adaptation cost.

**Purely Linux-desktop spawns — leave out entirely on Android** (different privilege model): `pkexec` (`position/gps_fix.rs:58`), `usermod`/`systemctl`/`apt-get` in the privileged helper (`src-tauri/src/bin/tuxlink-gps-fix.rs:30,31,32`), `getent group dialout` (`position/probe.rs:172`). **Build/test-only** (no runtime impact): `git rev-parse` + `rustc --version` (`build.rs:3,12` — CI passes a SHA env var), `mkfifo` (`forms/http_server.rs:1587`), `gpsfake`/`which`/`secret-tool` (test fixtures).

**The Android-specific openings (what iOS does NOT have):**

- **Classic SPP via Java.** The existing raw-socket path (`AF_BLUETOOTH=31`, `BTPROTO_RFCOMM=3`, `rfcomm.rs:101–102`; `KissLinkConfig::Bluetooth { mac }`, `link.rs:47`) **compiles** for bionic but is **dead at runtime** — SELinux blocks raw `AF_BLUETOOTH` sockets for normal apps. The capability survives, but only through a Kotlin/JNI plugin bridging `BluetoothSocket.createRfcommSocketToServiceRecord(SPP_UUID)`, which feeds bytes into Tuxlink's existing generic `ByteLink` trait (`link.rs`). This is the single highest-ROI Android transport and the one iOS cannot match.
- **USB host (OTG).** USB-serial KISS radios + USB-CAT/PTT via `android.hardware.usb` (`UsbManager`/`UsbDeviceConnection`), the de-facto `usb-serial-for-android` library (CDC-ACM, FTDI, CP210x, CH34x, Prolific — covering essentially every ham USB cable). Again Java-only → a Kotlin plugin owns the port and pipes bytes to Rust.

---

## 3. Threshold: would it compile / does Tauri do Android?

**Tauri Android maturity:** supported and first-class in tooling (`tauri android init/dev/build --apk|--aab`, `--target aarch64|armv7|i686|x86_64`; min SDK API 24; NDK r26d-era), but the Tauri team itself flags mobile DX as below-desktop and plugin support as uneven and per-plugin. The Rust core ships as a per-arch `.so` (NDK) loaded via `System.loadLibrary`; the React/TS frontend runs in Android System WebView (sources: `v2.tauri.app/develop/plugins/develop-mobile`, `/distribute/google-play`, `/plugin/shell`, tauri 2.0 blog). **`tauri-plugin-shell` on Android is restricted to opening URLs** — no spawn, no sidecar — which is exactly why §2's spawn architecture must move into the `.so` or a native plugin.

**Crate-by-crate, for `aarch64-linux-android` via NDK** *(refuting two over-broad survey claims — see §10):*

| Crate / facility | Compiles for Android NDK? | Notes |
|---|---|---|
| `nix 0.31` (`Cargo.toml:72`) | **Yes** — *contra the survey's "nix does NOT compile."* nix targets Unix incl. `*-linux-android` (bionic). Contrast Windows, where nix is a genuine compile-blocker. (Caveat: a few specific syscalls may be SELinux-restricted at *runtime*, but the crate builds.) |
| Unix-domain sockets (`tuxlink-mcp-core/.../transport_uds.rs`) | **Yes** — bionic has `AF_UNIX`; app-private socket path is safe. |
| `getuid` / mode-bit checks | **Yes** — bionic POSIX; app sandbox enforces UID isolation, mode checks redundant-but-harmless. |
| `rusqlite 0.40` `[bundled, modern_sqlite]` (`Cargo.toml:97`) | **Yes, with NDK toolchain** — bundled SQLite vendors C via `cc`; cross-compiles given NDK sysroot + `CC/AR/configure --host=aarch64-linux-android`. FTS5 query code is pure Rust. |
| `reqwest 0.13` + `tokio 1 [full]` (`Cargo.toml:70,71`) | **Yes** — both list `aarch64-linux-android`; tokio epoll works on bionic. |
| `native-tls 0.2` (`Cargo.toml:94`) | **Yes, with NDK link** — needs NDK OpenSSL link via build script. The telnet read/write loop (`telnet.rs`) is `BufRead + Write` only, zero Tauri. |
| `libc 0.2` `AF_BLUETOOTH` constants (`Cargo.toml:96`) | **Compiles** (hard-coded as 31/3, `rfcomm.rs:101`) but **runtime-dead** — SELinux blocks raw `AF_BLUETOOTH`; use the Java `BluetoothSocket` path. |
| `keyring 3.6.3` `[sync-secret-service]` (`Cargo.toml:89`) | **Gap** — backend is compile-time-selected; `sync-secret-service` is D-Bus/Linux-only and `keyring 3.6.3` ships **no Android Keystore backend**. Needs a custom secure-storage plugin (Keystore / EncryptedSharedPreferences via JNI). |
| `serialport 4` (`Cargo.toml:95`) | **Unusable on stock Android** — compiles against libc but the unprivileged app sandbox forbids `/dev/ttyUSB*` access (SELinux), and there is no `/dev/rfcommN`. *Not a serialport deficiency* (it works on rooted/privileged contexts) — but for a shippable app, USB serial must go through the Java USB Host API. |

**Verdict:** the Rust network + pure-logic core compiles for Android with NDK toolchain setup for `rusqlite`/`native-tls`; `keyring` and `serialport`/Bluetooth are the substantive gaps requiring native plugins.

---

## 4. Android gap matrix

Legend: ✅ works as-is · 🟡 port + touch-UI rework · 🔌 needs native (Kotlin/JNI) plugin · 🔴 impossible on Android.

| Feature | Status | Effort | `file:line` / note |
|---|---|---|---|
| CMS / Winlink telnet (TLS 8773 / plain 8772) | ✅ | moderate (NDK OpenSSL link) | `src-tauri/src/winlink/telnet.rs:1–835` — `BufRead+Write` + pure-Rust md-5; internet CMS = the most portable slice (CMS telnet is not a transmission, ADR 0018). |
| B2F / AX.25 framing / KISS / LZHUF / handshake / CMS-health | ✅ | none | `session/mod.rs`, `ax25/frame.rs`, `ax25/kiss.rs`, `winlink/lzhuf.rs`, `cms_health.rs` — pure logic, no syscalls. |
| KISS-over-TCP (networked TNC) | ✅ | low | `KissLinkConfig::Tcp` `ax25/link.rs:38,173`; generic `ByteLink` (any `Read+Write+Send`). |
| KISS-over-serial / USB | 🔌 | high | `ax25/link.rs:41,179` + `serialport` `Cargo.toml:95`; SELinux blocks `/dev/ttyUSB*`; USB Host API via `usb-serial-for-android` JNI plugin. |
| Remote VARA-over-TCP (8300/8301) | ✅ | none | `winlink/modem/vara/transport.rs:1–226` — pure `std::net` adapter; dial remote VARA host. (Native VARA on Android = 🔴, closed Windows binary — out of scope.) |
| Managed Dire Wolf (local spawn) | 🔴 | n/a | `ax25/managed_direwolf.rs:260` — bundled-binary exec blocked. Feature *survives via KISS-TCP above*; only local-spawn optimization dies. |
| ARDOP (local `ardopcf`) | 🔴 → ✅ remote | high (or use remote) | `winlink/modem/process.rs:127`; remote ARDOP-over-TCP recovers the feature. |
| Rig control (rigctld) | ✅ | none | `tux-rig/src/client.rs:24` `RigctldClient::connect(host,port)`; remote rigctld over TCP, no code change. (Local managed spawn `tux-rig/src/managed.rs:55` = 🔴.) |
| Bluetooth Classic SPP (KISS TNC) | 🔌 | high | `ax25/rfcomm.rs:99–196` raw `AF_BLUETOOTH` is SELinux-dead; Java `BluetoothSocket` SPP via Kotlin plugin → feed `ByteLink`. **Android's key advantage over iOS.** |
| Bluetooth BLE (KISS TNC) | 🔌 | high | not in current code (Linux path is Classic-only); `BluetoothGatt` via plugin (or `tauri-plugin-blec`) for TNC3/TNC4-class BLE. |
| Bluetooth pairing / SDP discovery | 🔌 | moderate | `sdptool`/`bluetoothctl` (`rfcomm.rs:266,332`; `ui_commands.rs:4308`) → `getBondedDevices()`/`getUuids()` inside the plugin. |
| GPS / position | 🔌 | high | TCP `gpsd` reader (`position/gpsd.rs`) is *technically* portable but no on-device `gpsd`; use Android Location Services (`FusedLocationProviderClient`) via plugin. Setup helper `position/gps_fix.rs:58` = 🔴 (pkexec/systemd). |
| Mailbox + FTS5 search | ✅ | moderate (NDK build) | `search/index.rs`, `native_mailbox.rs`, `rusqlite [bundled]` `Cargo.toml:97`; data in app-private `/data/data/<pkg>/files/`. |
| Forms (ICS-213/309/Position/Bulletin) | 🟡 | high | backend `forms/draft_library.rs` pure-Rust ✅; UI in separate Tauri webview window → single-Activity modal/fullscreen; PDF export uses webkit2gtk/GTK print (Linux) → Android `PdfDocument` or pure-Rust lib. Loopback `127.0.0.1` form server ✅ (`forms/http_server.rs:4`). |
| Keyring / credential storage | 🔌 | moderate | `identity/keyring_keys.rs:17`; `keyring 3.6.3` has no Android backend → Android Keystore (TEE-backed) via JNI plugin; account-string format unchanged. |
| Tray icon / close-to-tray | 🔴 | low | `tray.rs:42–130`; `lib.rs` CloseRequested intercept — no Android tray; map close-to-tray → activity `onPause`/`onResume` + a `connectedDevice` foreground service for live sessions. |
| Multi-window (help/logging/compose/stations) | 🟡 | moderate | `lib.rs:1604–1607` + per-window singletons → single Activity + fragments/sheets/back-stack. |
| Touch UI (shell) | 🟡 | high (~50 hr) | `src/App.tsx`, `src/shell/AppShell.tsx` — multi-pane assumes >1200 px, hover-to-dismiss, titlebar `getCurrentWindow()`, drag-drop; needs tab-bar/hamburger, single-pane stack, 44 px targets throughout. |
| MCP server (UDS) | ✅ | none | `mcp_connection.rs`, `tuxlink-mcp-core/.../transport_uds.rs` — tokio + `AF_UNIX` work on bionic; app-private runtime dir. (Likely *cut* on mobile regardless — no agent caller in a phone context.) |
| Audio device enumeration / soundcard | 🔌 | high | `devices.rs:449–601` reads `/proc/asound`, `/dev/snd/by-id`, sysfs USB, `/sys/class/hidraw` (CM108 PTT) — SELinux-blocked; Android Audio HAL + USB Host API via plugin. HID/CM108 GPIO PTT (`devices.rs:529–567`) = 🔴 (no `/dev/hidraw`); accept serial-RTS PTT instead. |
| Logging + redaction | ✅ | low | `tracing`/`tracing-appender` (`Cargo.toml:114–116`); logs to app cache; viewer needs an in-app panel/export instead of a window. |
| Config / wizard | ✅ | low | `wizard/` validation pure-Rust; XDG paths map to app-private dirs via Tauri's path abstraction (`config.rs:713`, `bootstrap.rs:368`) ✅; only `Wizard.tsx` is desktop-specific. |

---

## 5. Android RF-interfacing reality

Android's amateur-radio ecosystem proves the models. **WoAD (Winlink On Android)** — note: published by **Sumus Technology**, *not* F4HTB (a corrected attribution, see §10) — is a mature native Android Winlink B2F+forms client and the existence proof for the widest RF surface. **APRSdroid** established the Android Bluetooth-SPP-KISS pattern years ago; the Mobilinkd TNC was designed around it. The viable models, ranked by ROI for a Tuxlink Android port:

1. **Networked TNC over TCP (cheapest, already wired).** KISS-over-TCP to a remote Dire Wolf, remote ARDOP, remote rigctld, or remote VARA on a companion Linux/PC (a Pi at the radio). **This is pure Tuxlink code that already works** (`KissLinkConfig::Tcp` `link.rs:38`; `RigctldClient::connect` `client.rs:24`; `vara/transport.rs`). The only *Android-platform* requirement is a scoped `network-security-config` cleartext exception for the LAN host (cleartext is off by default since API 28; Android has **no** iOS-style local-network prompt — easier than iOS). This is also the **only** path to HF/VARA/ARDOP, since no native Android VARA/ARDOP build exists.

2. **Classic-SPP / BLE KISS TNC via native plugin (best self-contained field setup).** Mobilinkd TNC2/TNC4, Kenwood TH-D74 over Classic SPP; TNC3/TNC4 also BLE. VHF/UHF packet (1200/9600). This requires a **Kotlin/JNI plugin** (the *Tuxlink-code* work: bridge `BluetoothSocket` bytes into the existing `ByteLink`; the existing `rfcomm.rs` raw-socket code is dead and replaced). The *Android-platform* requirements: `BLUETOOTH_CONNECT`/`SCAN` runtime permissions (with `neverForLocation`). **Distinguishing factor vs iOS:** this whole class is reachable on Android and not on iOS.

3. **USB-OTG TNC (Android-unique).** USB-serial KISS radios (TH-D72/74 built-in TNC) and USB-CAT/PTT via `usb-serial-for-android` — moderate plugin work. A full on-device USB-audio soundcard-TNC (Digirig/SignaLink + on-phone AFSK/FSK DSP + USB-RTS/DTR/CM108 PTT) is the stretch goal and is **uniquely possible on Android** (iOS has no USB host). *Correction (see §10):* Tuxlink has **no in-process modem DSP** to "port" — it manages external modems — so a soundcard-TNC means embedding/porting an external DSP, not recompiling Tuxlink's own. *Android-platform* requirements: USB device-attach intent filter + `UsbManager.requestPermission()`.

4. **Internet-only (no radio).** CMS telnet over TLS. Lowest friction; the Phase-1 spine.

**Tuxlink-code work vs Android-platform limits:** the *code* work is (a) NDK toolchain for `rusqlite`/`native-tls`, (b) Kotlin/JNI plugins for SPP/BLE, USB-serial, Location, Keystore, (c) a touch UI. The *platform* limits are: no local binary spawn (W^X), Java-only device APIs, foreground-service requirement for long sessions, cleartext config for LAN, and Keystore as the only credential store. None of the platform limits block models 1–4; they shape *how* the code reaches the radio.

---

## 6. What an Android v1 should be vs cut

**v1 = a touch-first network-client + Bluetooth-TNC Winlink terminal.** Concretely:

- ✅ Internet CMS (telnet + TLS) — full mailbox / compose / read.
- ✅ Networked KISS-over-TCP to a companion Dire Wolf; remote rigctld; remote VARA/ARDOP over TCP (HF via companion host).
- 🔌 **Classic-SPP Bluetooth KISS TNC** (Mobilinkd-class) — the headline differentiator over an iOS v1, worth the plugin cost.
- ✅ Mailbox + FTS5 search, drafts, logging, config/wizard (touch-reflowed).
- 🔌 Location Services for position reports.
- 🔌 Android Keystore credential storage.

**Cut list (explicit) for v1:**

- 🔴 Managed/local-spawn modems (Dire Wolf, ardopcf, rigctld, python3 bridge) — recovered only via remote-TCP.
- 🔴 On-device soundcard-TNC + CM108/HID PTT — defer to a later phase (USB-audio DSP is the hardest single piece).
- 🔴 Native VARA/ARDOP on-device — remote-bridge only.
- ✂️ `voacapl` propagation + `go-pmtiles` regional basemaps — non-essential; cut or remote-service later.
- ✂️ System tray / multi-window / MCP-over-UDS — desktop paradigms with no phone analog (MCP has no mobile caller).
- ✂️ PDF export from forms — defer until `PdfDocument`/pure-Rust path lands; v1 forms compose+send without local PDF.

---

## 7. Phased plan

**Phase 0 — boot the shell, radio stubbed.** `tauri android init`; stand up the Gradle project + NDK toolchain; cross-compile the Rust core to a per-arch `.so` (resolve `rusqlite [bundled]` + `native-tls` NDK link first — the two named compile gates). Get the React UI rendering in Android System WebView with all radio transports stubbed/disabled and credentials in a temporary in-memory store. Exit criterion: app launches, mailbox opens (empty), no transport. Capture an ADR + README maturity-matrix entry marking Android research-stage with VARA explicitly out of scope.

**Phase 1 — internet + networked RF + touch UI.** Wire CMS telnet (TLS), KISS-over-TCP (existing `KissLinkConfig::Tcp`), remote VARA/ARDOP-over-TCP, and remote rigctld (existing `RigctldClient::connect`) — all reuse current Rust with no transport rewrite. Add the LAN `network-security-config` cleartext exception. Build the touch-first single-pane shell (tab/hamburger nav, 44 px targets, no hover/drag, no titlebar). Replace the tray/close-to-tray with a `connectedDevice` foreground service + notification so a live session survives Doze. This is the low-risk, high-portability spine and is agent-testable end-to-end against a CMS over the internet (not a transmission, ADR 0018).

**Phase 2 — native plugins (the hardware story).** Author Kotlin/JNI Tauri plugins (commands marshalled to `Dispatchers.IO` to avoid the documented main-thread freeze): (a) **Classic-SPP Bluetooth** — `BluetoothSocket` → `ByteLink`, `getBondedDevices()`/`getUuids()` for discovery, runtime BT permissions; (b) **Location** — `FusedLocationProviderClient` → GPS state machine; (c) **USB serial** — `usb-serial-for-android` → `ByteLink`, device-attach intent + `requestPermission`; (d) **Keystore** — TEE-backed credential get/set replacing the Linux keyring. Operator on-air validation gates the RF transports (RADIO-1).

**Deferred:** BLE KISS TNCs; on-device USB-audio soundcard-TNC + CM108/serial PTT; native VARA/ARDOP on-device; `voacapl`/`go-pmtiles` as remote services; forms PDF export; Play Store hardening (AAB signing, target-API bumps).

---

## 8. Effort & risk

This is a **re-architecture (in-process `.so` + native plugins + touch UI + Play distribution), not a recompile.** The Rust protocol core transfers; the I/O shims, secure storage, UI, and packaging do not. Phase 1's UX work (foreground service, single-Activity collapse, SAF file picker for export) is real (~2–4 weeks) on top of the protocol core — the "low-risk" framing is about *portability*, not zero effort.

**Unknowns, ranked highest-risk first:**

1. **Tauri-Android maturity.** Supported but DX flagged below-desktop; uneven per-plugin support; far fewer production Tauri-Android apps than iOS. The biggest schedule risk.
2. **Native-plugin effort for BT/USB.** Community serial/BLE Tauri plugins are immature/stale as of mid-2026 — expect *authoring from scratch* (or heavy upstreaming), not "adopt and integrate," plus JNI threading discipline Tuxlink's Rust core has never needed.
3. **Keyring → Keystore gap.** `keyring 3.6.3` has no Android backend (`Cargo.toml:89`); a custom secure-storage plugin (Keystore / EncryptedSharedPreferences) must be written and the credential flow re-pathed.
4. **Background-session limits.** Long ARQ sessions need a correctly-typed `connectedDevice` foreground service (Android 14 `foregroundServiceType` + `POST_NOTIFICATIONS`); the engine must run in the service, not the suspendable WebView.

**RADIO-1 (ADR 0018):** the agent freely authors, mocks, loopback-tests, and CI-builds all Android RF-path code; **on-air confirmation that any plugin actually keys a real radio is operator-only** — the dev shell has no radio.

---

## 9. iOS vs Android comparison

| Dimension | iOS | Android |
|---|---|---|
| Build model | Tauri-native iOS (`.so` + WKWebView), first-class | Tauri-native Android (`.so` via NDK + System WebView), first-class but younger DX |
| Local process spawning | 🔴 universal ban (no `fork`/`exec`/`posix_spawn`) | 🔴 blocked from app storage (W^X/SELinux, `targetSdk ≥ 29`) — same practical outcome for bundled binaries |
| Serial / USB | 🔴 no general USB host, no app-level USB-serial | ✅ USB Host API (OTG) — USB-serial KISS + USB-CAT/PTT via Java plugin |
| Bluetooth | 🟡 BLE only; Classic SPP needs MFi | ✅ **Classic SPP/RFCOMM open to apps** (`BluetoothSocket`) **+** BLE — no MFi gate |
| GPS | Core Location plugin | Location Services (`FusedLocationProvider`) plugin |
| Distribution | App Store; transport menu gated by Apple | Google Play (AAB) **and** sideload APK |
| Viable transports | internet CMS, remote TCP modems, BLE KISS | internet CMS, remote TCP modems, **Classic-SPP + BLE KISS, USB-OTG KISS/CAT, on-device soundcard-TNC (stretch)** |
| Overall effort | re-architecture + 0→1 touch UI; RF menu narrow | re-architecture + touch UI + more native plugins, but **widest mobile RF reach** |

The two are **opposite trade-offs, not shared constraints** (a correction, §10): they share *no-local-spawn* and *touch-UI*, but Android adds USB host + Classic SPP that iOS structurally cannot.

---

## 10. Corrections from verification

The adversarial sweep refuted several survey claims; the section above reflects the corrected positions:

- **REFUTED — "spawned-modem features are *impossible* and *must be replaced* on Android."** Overstated. Local spawn is blocked, but **the features survive via TCP fallbacks already in the codebase**: KISS-over-TCP (`link.rs:38`), `RigctldClient::connect` (`client.rs:24`), remote ARDOP/VARA-over-TCP (`vara/transport.rs`). "Local managed-modem spawn dies" ≠ "the feature is impossible." Same recovery architecture iOS uses.
- **REFUTED — "the `nix` crate does NOT compile for Android NDK."** False. `nix 0.31` targets `*-linux-android` (bionic). Unlike Windows (where nix *is* a compile-blocker), nix builds for Android; only specific syscalls may be SELinux-restricted at runtime.
- **CORRECTED — `AF_BLUETOOTH`/UDS/`getuid` "would not compile."** They **compile** on bionic; the real issue is **runtime**: SELinux blocks raw `AF_BLUETOOTH` (use Java `BluetoothSocket`); UDS and `getuid` work fine app-private. "Compile-blocker" was the wrong category for these — they are runtime/sandbox constraints.
- **CONFIRMED — Classic SPP requires the Java `BluetoothSocket` path via a native plugin;** the raw-socket `rfcomm.rs:99–196` code is effectively dead on Android. (The over-broad "this makes Tuxlink Android-feasible today" framing is rejected — it's feasible only as a re-architecture, not a swap.)
- **CONFIRMED — `serialport` is unusable on stock Android** *for the right reason*: the unprivileged-app sandbox forbids `/dev/ttyUSB*` (SELinux), not a Rust/serialport defect. The USB Host API (`usb-serial-for-android`) is the approved path.
- **CONFIRMED — keyring lacks an Android Keystore backend;** a custom secure-storage plugin is required. `rusqlite [bundled]`, `reqwest`, `tokio` cross-compile via NDK (NDK link work for `native-tls`/SQLite).
- **CORRECTED — WoAD authorship.** WoAD is published by **Sumus Technology**, not F4HTB (the brief's attribution was wrong).
- **CORRECTED — "RadioMail proves the Android SPP track record."** RadioMail is **iOS-only**; it is not an Android data point. The Android proof set is APRSdroid (+ WoAD).
- **CORRECTED — "port Tuxlink's own modem DSP" for a soundcard-TNC.** Tuxlink has **no in-process DSP** — it manages external modems (`managed_direwolf.rs`, `ardop/transport.rs`, `vara/transport.rs`). An on-device soundcard-TNC means embedding/porting an *external* DSP, not recompiling a Tuxlink DSP that doesn't exist. KISS framing (`kiss.rs`) is a frame wrapper, not a modulator.
- **CORRECTED — "Android and iOS share the no-spawned-process and touch-UI constraints" (as symmetric).** They share *those two*, but the survey's "materially better for radio" claim is right precisely because the constraints are **otherwise opposite**: Android exposes USB host + Classic SPP that iOS cannot. The corrected framing: Android is the better mobile RF target; the shared constraints are real but do not erase that asymmetry.
- **NOTE on the recurring "Tuxlink can't run on Android because it's Linux-only / Tauri-2-doesn't-do-Android" refutations.** These conflate *Tuxlink's current state* (correctly: Linux-only, no Android config) with *Tauri's capability* (Tauri 2.x **does** support Android since 2.0 stable). The brief asks for a **port assessment**, so the operative truth is: Tuxlink does not run on Android today, and a Tauri-native Android port is viable as a re-architecture — both stated explicitly in §1.

---

Files referenced (all absolute): `src-tauri/Cargo.toml`, `.../src-tauri/tauri.conf.json`, `.../src-tauri/src/lib.rs`, `.../src-tauri/src/winlink/ax25/{link.rs,rfcomm.rs,managed_direwolf.rs,frame.rs,kiss.rs}`, `.../src-tauri/src/winlink/modem/{process.rs,vara/transport.rs,ardop/cat_ptt_bridge.rs}`, `.../src-tauri/tux-rig/src/{client.rs,managed.rs}`, `.../src-tauri/src/winlink/telnet.rs`, `.../src-tauri/src/{devices.rs,ui_commands.rs,bootstrap.rs,config.rs}`, `.../src-tauri/src/position/{gpsd.rs,gps_fix.rs,probe.rs}`, `.../src-tauri/src/bin/tuxlink-gps-fix.rs`, `.../src-tauri/src/{propagation/engine.rs,basemap/commands.rs,forms/http_server.rs,identity/keyring_keys.rs,logging/manifest.rs,tray.rs}`, `.../src/{App.tsx,shell/AppShell.tsx}`.

---

# Part V — Empirical macOS build verification (2026-06-28)

Parts I–IV are analysis. This part is an **actual macOS build run on the target hardware** (Apple Silicon M5, macOS Tahoe 26.5.1). It converts the §I.2 macOS compile-blocker *predictions* into *facts* and documents exactly what the macOS build toolchain requires — for CI-runner setup and future port work. **Headline: the Rust crate graph compiles clean on `aarch64-apple-darwin` after a single one-line source fix; the only build failure is a frontend filename-case bug.**

## V.1 Host environment

- Apple Silicon **MacBook Air (M5)**, **macOS Tahoe 26.5.1**, `arm64`.
- Pre-existing: Homebrew, Xcode Command Line Tools (`xcrun`), Node 26.3.0 / npm 11.16.0 (via `fnm`).

## V.2 Toolchain installed this session

| Tool | Version | How |
|---|---|---|
| Rust (`rustup`/`cargo`/`rustc`) | **1.96.0** (rustup 1.29.0) | official rustup installer; default host `aarch64-apple-darwin`; `clippy`+`rustfmt` via default profile; PATH wired into `~/.zshenv` + `~/.profile` |
| `pnpm` | **11.9.0** | `brew install pnpm` |

## V.3 Homebrew system libraries required

- **Only for the default `heif` feature:** `brew install pkgconf libheif libde265` (pulls transitive `aom`, `x265`, `jpeg-turbo`, `libpng`, `webp`, `libtiff`, `giflib`, `xz`, `lz4`, `zstd`, `libvmaf`). `pkgconf` is mandatory because `libheif-sys`'s build script shells out to `pkg-config` — without it the build fails *before* even looking for libheif.
- **Nothing else.** Building **`--no-default-features`** (HEIF off) needs **zero** Homebrew C libraries: `webp`/`zstd` vendor their C (per ADR-0020, now confirmed), `native-tls` uses the system Security.framework, and `rusqlite` is `bundled`.

## V.4 Source change required — exactly one

`src-tauri/src/basemap/commands.rs:391-397`. `nix`'s `Statvfs::blocks_available()` returns `fsblkcnt_t`, which is **`u32` on Darwin** vs **`u64` on 64-bit Linux**, so the unguarded `let blocks: u64 = s.blocks_available();` is an `E0308` mismatched-types error on macOS:

```
error[E0308]: mismatched types --> src/basemap/commands.rs:395:31
395 |   let blocks: u64 = s.blocks_available();
    |               ---   ^^^^^^^^^^^^^^^^^^^^ expected `u64`, found `u32`
```

Fixed with a `#[cfg]`-split widening (Linux path unchanged; non-Linux path adds `.into()`), which stays clippy-clean (no `unnecessary_cast`) on both targets. This is the §I.2c "unguarded statvfs" risk landing as a concrete, trivial compile error — and it is the **only** Rust compile blocker the build found.

## V.5 Results — what compiled

| Command (`--manifest-path src-tauri/Cargo.toml`) | Result |
|---|---|
| `cargo check --no-default-features --bin tuxlink` (after the §V.4 fix) | ✅ **Finished in 8.26s** |
| `cargo check --bin tuxlink` (HEIF on, after §V.3 brew libs) | ✅ **Finished in 12.39s** |

This empirically establishes that **every dependency compiles on macOS** — `keyring` *and its D-Bus crates* (`dbus-secret-service`/`secret-service`/`zbus`), `nix` (`signal`/`process`/`fs`), `webp` + `zstd` (vendored C), `native-tls` (Security.framework), `rusqlite` (bundled). It therefore **refutes the keyring / webp / zstd / nix compile-blocker concerns** raised in §I.2a–b: none of them block a macOS compile. (One non-fatal warning: `mini_sbc 0.1.7` carries future-incompatible code that a future `rustc` will reject — a pre-existing dependency hygiene item, not macOS-specific.)

> Net macOS Rust compile threshold, **measured**: one one-line `statvfs` fix, plus `brew install pkgconf libheif libde265` *iff* you keep HEIF on. Softer than even the "soft" §I.2a prediction.

## V.6 What did NOT build — the frontend (a genuine macOS bug)

The frontend (`pnpm build` = `tsc && vite build`) does **not** build on a default macOS checkout, for two distinct reasons:

1. **pnpm-11 build-script allowlist drift (papercut).** `package.json`'s `pnpm.onlyBuiltDependencies: ["esbuild"]` is **ignored by pnpm 11**, which reads the allowlist from `pnpm-workspace.yaml`. Symptom: `ERR_PNPM_IGNORED_BUILDS: esbuild`, and pnpm's pre-run deps-check then fails `pnpm build`. The `@esbuild/darwin-arm64` binary itself *is* installed (via esbuild's optional dep). **Fix:** add `onlyBuiltDependencies: [esbuild]` to `pnpm-workspace.yaml` (done in this worktree) and re-install / `pnpm rebuild esbuild`. This is OS-agnostic (a pnpm-version migration) and should be fixed for Linux/CI too.

2. **macOS case-insensitivity bug (real portability defect).** With the toolchain working, `vite build` fails:

   ```
   src/catalog/CatalogReplyView.tsx:18 — "WeatherGlyph" is not exported by "src/catalog/weatherGlyph.ts"
   import { WeatherGlyph } from './WeatherGlyph';
   ```

   The repo git-tracks **both** `src/catalog/WeatherGlyph.tsx` (the React component) and `src/catalog/weatherGlyph.ts` (types + `resolveGlyph`) — stems differing only by case (plus a second pair, `WeatherGlyph.test.tsx` ↔ `weatherGlyph.test.ts`). On Linux's case-sensitive filesystem the import `./WeatherGlyph` resolves to `WeatherGlyph.tsx`; on macOS's **default case-insensitive APFS** the resolver matches `weatherGlyph.ts` first, which doesn't export the `WeatherGlyph` *value* → build error. **Fix:** rename one stem (e.g. `weatherGlyph.ts` → `weatherGlyphData.ts`) and update importers, or disambiguate the import. This blocks a full app bundle on a default macOS checkout and is a latent hazard on any case-insensitive FS (incl. default Windows NTFS) — worth fixing regardless of the port.

## V.7 Verified end-to-end (2026-06-28 follow-up)

After the §V.6 fixes landed on `feat/macos-build-assessment`, the full macOS path was exercised and **works start-to-finish**:

- **Frontend builds.** The `weatherGlyph.ts` → `weatherGlyphData.ts` rename resolves the §V.6 case collision; `tsc` is clean, `vite build` ✓, `vitest` 10/10. The pnpm-11 gate is cleared by `pnpm approve-builds --all`, which records `allowBuilds: { esbuild: true }` in `pnpm-workspace.yaml` — the `onlyBuiltDependencies` list alone did **not** clear a stale ignored-build state — after which `pnpm build` passes.
- **Keyring → native Keychain.** `keyring` is split per-target: macOS uses `apple-native` (security-framework / Keychain), Linux keeps `sync-secret-service`. `cargo check` confirms `security-framework` enters the macOS dep graph and `secret-service` is excluded.
- **Full debug build + LINK.** `pnpm tauri dev` ran `cargo run` to completion — `Finished dev profile … in 57.18s` (warm dep cache), then `Running target/debug/tuxlink`. The link step (which `cargo check` does not exercise) succeeds on macOS.
- **App launch + runtime + clean exit.** The window opened and **rendered its UI** (operator-confirmed), the process ran stably (~150 MB RSS, ~4% CPU), logged no panics, and **exited cleanly (code 0)** on close. The only runtime log lines were benign and non-macOS-specific: a `gpsd` connection-refused retry (no GPS attached; self-labeled normal) and a Vite resolve warning for the dev-only `dev/perf-harness/harness.tsx` (imports `maplibre-gl`, not an app dependency).

**Still not exercised (future work, not blockers):**
- ~~`pnpm tauri build` release bundle~~ — **`.app` now produced & verified (§V.10).** The `.dmg`, code-signing, and notarization remain (notarization needs a paid Apple Developer account; the `.dmg` step fails headlessly — see §V.10).
- An in-app **Keychain credential round-trip** through the identity wizard UI. (A standalone runtime smoke test of the `apple-native` backend **passed** 2026-06-28 — `set` → `get` → `delete` → confirm-gone against the real macOS Keychain, a Unicode secret round-tripping byte-faithfully, with **no interactive prompt** — using the same `keyring::Entry` calls the wizard makes. The only remaining gap is driving that round-trip through the actual UI.)
- Per RADIO-1 (ADR 0018), on-air transmit validation remains operator-only.

## V.8 Minimal reproduction recipe (CI / future macOS work)

```bash
# 1. Toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y   # rustc/cargo; host aarch64-apple-darwin
brew install pnpm                                                          # pnpm 11
xcode-select --install                                                     # Xcode CLT (xcrun) if absent
# 2. HEIF system libs — SKIP if building --no-default-features
brew install pkgconf libheif libde265
# 3. pnpm 11 build-script approval (one-time; clears ERR_PNPM_IGNORED_BUILDS)
pnpm install --frozen-lockfile
pnpm approve-builds --all          # records allowBuilds: { esbuild: true } in pnpm-workspace.yaml
# 4. statvfs cfg-split, keyring apple-native split, and WeatherGlyph rename are
#    already committed on feat/macos-build-assessment (§V.4, §V.7, §V.9)
# 5. Build + run
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features    # core   -> PASSES (~8s)
cargo check --manifest-path src-tauri/Cargo.toml                          # + HEIF -> PASSES (~12s, needs step 2)
pnpm build                                                                # frontend -> PASSES
pnpm tauri dev                                                            # full build+link+launch -> UI renders (~57s build)
```

## V.9 Changes committed by this work (branch `feat/macos-build-assessment`)

All fixes are committed on `feat/macos-build-assessment` (ahead of `origin/main`):

- `fix(basemap)` `067fab08` — statvfs `u32`/`u64` cfg-split (Linux behavior unchanged).
- `build(pnpm)` `383f98aa` + `41a9d4d0` — pnpm-11 build-script allowlist, then the working `allowBuilds` approval.
- `fix(catalog)` `8551368c` — `weatherGlyph.ts` → `weatherGlyphData.ts` rename (the §V.6 case bug), via `git mv`.
- `build(deps)` `5a9084a5` — keyring `apple-native` (Keychain) on macOS, `sync-secret-service` on Linux.
- `fix(forms)` `79e9c179` — gate `PRINT_DEADLINE_SECS` to Linux (non-Linux `dead_code`).
- `docs(design)` `066013ee` (+ a follow-up recording this §V.7 verification) — this assessment.

`dist/`, `target/`, `node_modules/` remain gitignored build output. `package.json`'s legacy `pnpm.onlyBuiltDependencies` field is left in place (a harmless warning under pnpm 11) for pnpm <11 compatibility.

## V.10 Release bundle — `.app` produced, `.dmg` fails headlessly (2026-06-28)

Executed per [docs/plans/2026-06-28-macos-app-bundle-plan.md](../plans/2026-06-28-macos-app-bundle-plan.md) (reviewed: self → Codex cross-model → self).

**Config added:** a minimal, macOS-target-scoped block in `src-tauri/tauri.conf.json` (purely additive — `bundle.linux` untouched):
```json
"macOS": { "minimumSystemVersion": "11.0" }
```

**`.app` — ✅ produced & verified.** `pnpm tauri build --ci --no-sign --bundles app` →
`src-tauri/target/release/bundle/macos/tuxlink.app` (96 MB). Headlessly verified (no window launched):

| Property | Value |
|---|---|
| `CFBundleIdentifier` | `com.tuxlink.app` |
| `CFBundleShortVersionString` | `0.78.0` |
| `LSMinimumSystemVersion` | `11.0` (proves the `bundle.macOS` block took effect — exact `plutil -extract`) |
| Architecture | **arm64-only** Mach-O (`lipo -archs` → `arm64`). NOT universal — a universal binary needs `--target universal-apple-darwin` + the `x86_64-apple-darwin` target (out of scope). |
| Bundled resources | `wle-forms/`, `ssn-forecast.json`, `basemap/` all present under `Contents/Resources/resources/` |
| Signing | inner binary **ad-hoc** (`Signature=adhoc`, flags `adhoc,linker-signed` — the linker's mandatory arm64 signature); bundle **not sealed** (`--no-sign`); `codesign --verify --strict` fails ("no resources but signature indicates they must be present" — the known ad-hoc-binary-in-unsigned-bundle state); `spctl --assess` **rejects** (expected — not Developer-ID/notarized). |

**`.dmg` — ❌ not produced (documented non-blocker).** `timeout -k 30 360 pnpm tauri build --ci --no-sign --bundles dmg` exited **1** (not a 124 timeout): Tauri's `bundle_dmg.sh` (the disk-image-layout step driving `hdiutil` + Finder/AppleScript) failed — a known macOS fragility in headless/remote-control/CI contexts. The `.app` is the deliverable; the `.dmg` is deferred to the operator / a full interactive GUI session, or a future switch to a non-Finder dmg layout. Not in scope here.

**`beforeBundleCommand` (gps-fix):** `cargo build --release --bin tuxlink-gps-fix` **compiles on macOS** (2m11s; binary produced) — its systemctl/apt/usermod paths are runtime `Path::exists()` lookups, not compile deps. It is the Linux deb/rpm payload and is **unused** in the macOS `.app`; harmless but wasteful (a future cleanup could skip it on macOS).

**Discovery — bundle identifier ends in `.app`.** Tauri warns: *"The bundle identifier `com.tuxlink.app` … ends with `.app`. This is not recommended because it conflicts with the application bundle extension on macOS."* The build still succeeded, but this is a real macOS advisory. The identifier is cross-cutting (tauri.conf, the polkit action `com.tuxlink.app.gps-fix`, etc.), so a rename (e.g. `com.tuxlink.client`) is out of scope here — flagged for a future decision.

**Net:** a launchable, arm64, unsigned macOS `.app` builds from `pnpm tauri build` with one additive config line; only the `.dmg` packaging and code-signing/notarization remain, and those are operator/Apple-account concerns, not code blockers.

---

## Appendix A — provenance

Analysis of the worktree on branch `feat/macos-build-assessment` (renamed from `claude/nice-tu-ac3438`). **Parts I–IV are static source analysis + external research** (no build). **Part V is an empirical macOS build + run** on the M5 host: `cargo check`, a full debug `cargo build` + **link**, the frontend build, and a `pnpm tauri dev` launch with the **UI rendered and a clean exit** (§V.7). Not done: a `tauri build` release bundle / notarized artifact, and per RADIO-1 (ADR 0018) no on-air run. The **Windows, iOS, and Android compile-blocker verdicts remain unverified predictions** — a cross-build (`cargo check --target …`) for each is the highest-value, cheapest first action of any port attempt, exactly as the macOS build was here, which corrected the macOS §I.2 predictions to fact (and went further: link + launch + runtime).
