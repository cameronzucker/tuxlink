# Development guide

This document covers building tuxlink from source.

**End-users:** If you just want to use tuxlink, download the prebuilt AppImage from the Releases page. The AppImage bundles all dependencies (Go runtime, Pat binary, libax25) — you do NOT need any of the toolchain setup below.

## Build prerequisites (source builds only)

Tuxlink wraps the [Pat Winlink client](https://github.com/la5nta/pat) (via the [tuxlink-pat](https://github.com/cameronzucker/tuxlink-pat) fork per [ADR 0011](adr/0011-fork-pat-for-tuxlink.md)). Building tuxlink from source requires:

| Dep | Version | Purpose |
|---|---|---|
| Rust | stable (1.75+) | Tuxlink's Tauri app |
| Go | 1.24+ (per Pat's `go.mod`) | Builds Pat from `external/tuxlink-pat/` submodule via `bash make.bash` |
| libax25-dev | any | Optional but recommended on Linux: enables Pat's AX.25 hardware modem support. Without it, Pat builds but AX.25 features are absent. |
| Tauri 2.x system deps | per Tauri docs | webkit2gtk, GTK dev libs, etc. |

### Debian / Ubuntu

```bash
sudo apt update
sudo apt install -y rustc cargo golang-go libax25-dev \
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

The release CI workflow at `.github/workflows/release.yml` handles the full AppImage build. To run locally, install `cargo-tauri` and run `cargo tauri build --bundles appimage` from `src-tauri/` — but this requires the same Go + libax25 deps as above.
