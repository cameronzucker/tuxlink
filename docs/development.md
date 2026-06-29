# Development guide

This document covers building tuxlink from source.

**End-users:** If you just want to use tuxlink, download the prebuilt AppImage from the Releases page. End-users on Linux DO require a secret-service-compatible keyring daemon running for the Winlink credential-storage path; see [Runtime prerequisites for end-users](#runtime-prerequisites-for-end-users) below for the (short) details.

## Build prerequisites (source builds only)

Tuxlink is a native Rust + Tauri application. Tuxlink requires no Go toolchain. Building from source requires:

| Dep | Version | Purpose |
|---|---|---|
| Rust | stable (1.75+) | Tuxlink's Tauri application |
| libax25-dev | any | Optional but recommended on Linux: enables AX.25 hardware modem support (KISS TNC). Without it, tuxlink builds but AX.25 features remain absent. |
| libsecret-1-dev | any | Development headers for the secret-service D-Bus interface. Tuxlink's wizard writes the Winlink CMS password to the OS keyring via the Rust `keyring` crate. |
| Tauri 2.x system deps | per Tauri docs | webkit2gtk, GTK dev libs, etc. |

### Debian / Ubuntu

```bash
sudo apt update
sudo apt install -y rustc cargo libax25-dev libsecret-1-dev \
  libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

### Clone

```bash
git clone https://github.com/cameronzucker/tuxlink.git
cd tuxlink
```

### Build

```bash
cd src-tauri
cargo build --release
```

### AppImage release build

To build an AppImage locally, install `cargo-tauri` and run `cargo tauri build --bundles appimage` from `src-tauri/`.

CI scope today (per `.github/workflows/release.yml`): on PRs touching the integration surface and on `v*` tags, CI runs `cargo build --release` from `src-tauri/`. CI does NOT yet bundle an AppImage or upload a release artifact; that work is deferred to Task 17 (`tuxlink-cs7`).

### Linux taskbar icon (dev mode)

The Tuxlink icon ships with the app, but Linux window managers (GNOME / KDE / labwc / Sway) require a `.desktop` entry in the user's XDG paths to map the running window to its icon. Production `.deb` builds install this automatically; for `tauri dev` from source, run once after first clone:

```bash
bash scripts/install-desktop-entry.sh
```

This copies `src-tauri/icons/*` into `~/.local/share/icons/hicolor/<size>/apps/com.tuxlink.app.png` and writes `~/.local/share/applications/com.tuxlink.app.desktop`. Idempotent; safe to re-run after `git pull`. Linux-only — tuxlink targets Linux (x86_64 / arm64); there is no macOS or Windows build.

## Runtime prerequisites for end-users

On Linux, tuxlink stores the Winlink CMS password in the OS keyring via the secret-service D-Bus interface (per [ADR 0016](adr/0016-native-b2f-outbound-with-attachments.md)). This requires a running secret-service-compatible keyring daemon. The AppImage cannot bundle one; it is an OS service.

- **GNOME / GNOME-derived desktops** (Ubuntu, Fedora Workstation, Debian GNOME): `gnome-keyring-daemon` ships with the desktop and is usually already running. No action needed.
- **KDE Plasma**: `kwalletd5` (or `kwalletd6` on Plasma 6) ships with the desktop, configured to provide the secret-service interface to non-KDE apps via `kwalletmanager` settings. Usually no action needed.
- **Minimal / non-desktop installs** (e.g., a server, a window-manager-only install like i3 / sway): install and start a secret-service provider yourself. Easiest path: `sudo apt install gnome-keyring libsecret-1-0` (or distro-equivalent) and ensure `gnome-keyring-daemon` starts in your session (typically via `pam_gnome_keyring.so` in `/etc/pam.d/login` or by running `gnome-keyring-daemon --daemonize --components=secrets` from your session startup).
If you launch tuxlink and the wizard reports "keyring backend unavailable" or "secret-service not running," install and start one of the above. The AppImage bundles `libsecret-1-0` (the library) so the binary loads, but it cannot start the daemon process for you.
