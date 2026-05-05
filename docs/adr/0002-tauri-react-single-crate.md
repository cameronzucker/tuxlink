# 2. Tauri 2 + React + single-crate architecture for v0.0.1

Date: 2026-05-05
Status: Accepted
Deciders: cameronzucker, lichen (during 2026-04-22 office-hours), alder (recording)

## Context

Tuxlink is a Linux-native desktop Winlink client. The 2026-04-22 office-hours session converged on a v0.0.1 scope of "telnet-only Winlink, bundled Pat, AppImage distribution, single-crate Rust binary, native menu + tray, three-screen wizard, inbox/sent/compose UI, session log, status bar." The architecture had to support that scope without overcommitting to v0.1+ futures (VARA, AX.25, native protocol implementations, P2P, RMS Relay).

Several technology choices were on the table:

- **GUI framework:** Tauri (Rust-native, web-tech rendering) vs Electron (Node-native, web-tech rendering) vs egui (immediate-mode Rust) vs GTK4 (native widgets via gtk-rs).
- **Frontend stack:** React vs Solid vs Svelte vs vanilla JS.
- **Crate organization:** single binary vs workspace split (e.g., `tuxlink-core`, `tuxlink-protocol-native`, `tuxlink-ui`).
- **Mailbox storage:** SQLite-in-tuxlink vs Pat-owns-mailbox (deferred to ADR 0003).

The Codex round-2 adversarial review surfaced specific risks:

- A pre-emptive workspace split with `tuxlink-protocol-native` would lock in a trait shape before the v0.5+ native VARA backend exists to validate it.
- Two storage owners (Pat's mailbox + tuxlink's SQLite cache) creates a divergence problem with no single source of truth.

## Decision

Tuxlink v0.0.1 uses:

- **Tauri 2.x** as the desktop framework. WebKitGTK 4.1 renders the React frontend; Rust handles the backend (Pat lifecycle, HTTP client, IPC).
- **React 18 + TypeScript 5** as the frontend. Vite as the dev server. TanStack Query 5 for server state. Radix UI for accessibility primitives. react-virtuoso for list virtualization.
- **A single Rust crate** at `src-tauri/`. No workspace split in v0.0.1. The protocol abstraction is a single trait inside the main binary.
- **No `tuxlink-protocol-native` stub crate.** Defer extraction to v0.5+ when the native VARA backend lands and validates the trait shape.

## Consequences

**Positive:**
- Tauri 2 produces a small AppImage (~15-25 MB) compared to Electron (~120 MB+) — important for distribution over slow / mesh networks where amateur radio operators may be deployed.
- React + TypeScript is the most-staffed frontend ecosystem; future contributors (and AI subagents) need no exotic framework knowledge.
- Single-crate keeps the build simple, the dependency graph small, and the cognitive overhead low for v0.0.1's scope.
- WebKitGTK 4.1 is bundled with most current desktop Linux distros, so the AppImage avoids shipping its own webview runtime.

**Negative:**
- WebKitGTK 4.1 is a system dependency: distros stuck on 4.0 (older Debian / RHEL) cannot run Tuxlink v0.0.1 without a backport. Documented in [README.md](../../README.md) install requirements.
- Single-crate means the protocol trait will eventually need to be extracted (probably v0.5). At that point the trait will have crystallized through use, so the extraction is informed rather than speculative.
- Tauri 2 is a younger ecosystem than Electron; some integrations (e.g., specific code-signing tooling) may need workarounds.

## Alternatives considered

- **Electron + Node.js**: rejected. Larger AppImage, larger memory footprint, requires Node toolchain at build time, less Rust-native.
- **GTK4 (no webview)**: rejected. Native-widget UI would be slower to iterate on, less familiar to Web-trained AI agents, and constrains future migration paths (e.g., to a web-deployed companion).
- **egui (immediate mode)**: rejected. Excellent for rapid prototyping but the Mailbox / Compose UX doesn't fit immediate-mode patterns well; persistent state and form-heavy interactions favor a retained-mode framework.
- **Multi-crate workspace from day 1** (`tuxlink-core`, `tuxlink-ui`, `tuxlink-protocol-native`, `tuxlink-pat`): rejected. The v0.0.1 scope is small enough that crate boundaries would be premature; the boundaries that matter (trait shape between protocol and UI) aren't yet validated by a second backend. Office-hours adversarial review specifically flagged this as scope-creep.
- **Solid or Svelte instead of React**: rejected. Smaller ecosystems, less AI-agent training data, no clear win at v0.0.1's scope. Revisit if React's overhead becomes a measured problem.
