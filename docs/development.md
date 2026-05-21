# Development guide

This document covers building tuxlink from source.

**End-users:** If you just want to use tuxlink, download the prebuilt AppImage from the Releases page. The AppImage bundles all build-time toolchain deps (Go runtime, Pat binary, libax25) — you do NOT need any of the toolchain setup below. End-users on Linux DO need a secret-service-compatible keyring daemon running for the Winlink credential-storage path; see [Runtime prerequisites for end-users](#runtime-prerequisites-for-end-users) below for the (short) details.

## Build prerequisites (source builds only)

Tuxlink wraps the [Pat Winlink client](https://github.com/la5nta/pat) (via the [tuxlink-pat](https://github.com/cameronzucker/tuxlink-pat) fork per [ADR 0011](adr/0011-fork-pat-for-tuxlink.md)). Building tuxlink from source requires:

| Dep | Version | Purpose |
|---|---|---|
| Rust | stable (1.75+) | Tuxlink's Tauri app |
| Go | 1.24+ (per Pat's `go.mod`) | Builds Pat from `external/tuxlink-pat/` submodule via `bash make.bash` |
| libax25-dev | any | Optional but recommended on Linux: enables Pat's AX.25 hardware modem support. Without it, Pat builds but AX.25 features are absent. |
| libsecret-1-dev | any | Linux only: development headers for the secret-service D-Bus interface. Tuxlink's wizard writes the Winlink CMS password to the OS keyring via the Rust `keyring` crate (per [AMD-14](plans/2026-04-22-tuxlink-v0.0.1-plan.md) of the v0.0.1 plan) and Pat (via the fork) reads it back via `zalando/go-keyring`. macOS uses native Keychain frameworks and Windows uses native CredentialManager — neither needs an additional build-time dep on those platforms. |
| Tauri 2.x system deps | per Tauri docs | webkit2gtk, GTK dev libs, etc. |

### Debian / Ubuntu

```bash
sudo apt update
sudo apt install -y rustc cargo golang-go libax25-dev libsecret-1-dev \
  libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

### Clone with submodules

```bash
git clone --recurse-submodules https://github.com/cameronzucker/tuxlink.git
cd tuxlink

# If you cloned without --recurse-submodules:
git submodule update --init --recursive
```

### Build

```bash
cd src-tauri
cargo build --release   # triggers build.rs which invokes 'bash make.bash'
                        # in external/tuxlink-pat/ and produces the Pat sidecar
                        # at src-tauri/sidecars/pat-<target-triple>
```

Debug builds + `cargo test` skip the Pat build entirely (release-only gate per [spec §3.2](superpowers/specs/2026-05-18-fork-setup-design.md)). That means you do NOT need Go installed to run `cargo test`.

### AppImage release build

To build an AppImage locally, install `cargo-tauri` and run `cargo tauri build --bundles appimage` from `src-tauri/` — this requires the same Go + libax25 deps as above.

CI scope today (per `.github/workflows/release.yml`) is a **release-profile Pat-build smoke**: on PRs touching the integration surface and on `v*` tags, CI runs `cargo build --release` from `src-tauri/`, which triggers `build.rs`'s release-only Go-build path and produces the Pat sidecar binary. This validates the end-to-end build path (Go toolchain + submodule + sidecar production) on every relevant PR. CI does NOT yet bundle an AppImage or upload a release artifact — that's deferred to Task 17 (`tuxlink-cs7`).

## Runtime prerequisites for end-users

On Linux, tuxlink stores the Winlink CMS password in the OS keyring via the secret-service D-Bus interface (per [ADR 0011](adr/0011-fork-pat-for-tuxlink.md) and [AMD-13](plans/2026-04-22-tuxlink-v0.0.1-plan.md) of the v0.0.1 plan). This requires a secret-service-compatible keyring daemon to be running. The AppImage cannot bundle this — it's an OS service.

- **GNOME / GNOME-derived desktops** (Ubuntu, Fedora Workstation, Debian GNOME): `gnome-keyring-daemon` ships with the desktop and is usually already running. No action needed.
- **KDE Plasma**: `kwalletd5` (or `kwalletd6` on Plasma 6) ships with the desktop, configured to provide the secret-service interface to non-KDE apps via `kwalletmanager` settings. Usually no action needed.
- **Minimal / non-desktop installs** (e.g., a server, a window-manager-only install like i3 / sway): you must install and start a secret-service provider yourself. Easiest path: `sudo apt install gnome-keyring libsecret-1-0` (or distro-equivalent) and ensure `gnome-keyring-daemon` is started in your session (typically via `pam_gnome_keyring.so` in `/etc/pam.d/login` or by running `gnome-keyring-daemon --daemonize --components=secrets` from your session startup).
- **macOS**: native Keychain Services — always available, no action needed.
- **Windows**: native CredentialManager — always available, no action needed.

If you launch tuxlink and the wizard reports "keyring backend unavailable" or "secret-service not running," install and start one of the above. The AppImage bundles `libsecret-1-0` (the library) so the binary loads, but it cannot start the daemon process for you. macOS and Windows builds skip libsecret entirely — they use native frameworks.
