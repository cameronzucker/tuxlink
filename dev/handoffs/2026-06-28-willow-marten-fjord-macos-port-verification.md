# Handoff — macOS port verification + 4-platform assessment

- **Agent:** willow-marten-fjord
- **Date:** 2026-06-28
- **Branch:** `feat/macos-build-assessment` (renamed from `claude/nice-tu-ac3438`; tracks `origin/main`; **ahead 16; NOT pushed**)
- **Host:** Apple Silicon MacBook Air (M5), macOS Tahoe 26.5.1 — i.e. the real macOS target, not the Pi.
- **Working tree:** clean.

## TL;DR

Empirically verified that **Tuxlink builds, links, runs, bundles, and serves its MCP interface on macOS** — and wrote a 4-platform (macOS / iOS / Windows / Android) static portability assessment. Everything is committed on `feat/macos-build-assessment`. **Push is your call** (you asked to confirm origin/branch before pushing) — see Pending.

Canonical detail lives in **[docs/design/2026-06-28-macos-ios-portability-assessment.md](../design/2026-06-28-macos-ios-portability-assessment.md)** (Parts I–V) and the app-bundle plan **[docs/plans/2026-06-28-macos-app-bundle-plan.md](../plans/2026-06-28-macos-app-bundle-plan.md)**. This handoff is the index.

## Verified on macOS (empirical, this host)

- **Toolchain** (assessment §V.2–V.3): rustup/cargo 1.96, pnpm 11.9, Homebrew `pkgconf`/`libheif`/`libde265` (HEIF only; `--no-default-features` needs none).
- **Rust:** `cargo check` (core + HEIF) ✅ · full debug build + **link** ✅ · `clippy --bin tuxlink -- -D warnings` ✅ **clean** · **lib tests 2615/2615** ✅.
- **App runtime** (§V.7): `pnpm tauri dev` → build+link (57s) → window launched, **UI rendered** (operator-confirmed) → clean exit. **Keychain `apple-native` runtime round-trip** (set/get/delete, unicode, no prompt) ✅.
- **Release bundle** (§V.10): `.app` built + headlessly verified (arm64, identifier/version/`LSMinimumSystemVersion 11.0`, resources bundled, **unsigned** `--no-sign`, `spctl` rejects as expected). `.dmg` **fails headlessly** (`bundle_dmg.sh` exit 1 — hdiutil/Finder/AppleScript) — documented non-blocker, deferred.
- **Frontend** (§V.12): **vitest 3320/3320 under Node 20** ✅. Under the host default **Node 26.3.0**: 501 failures — a jsdom/undici environment artifact (`localStorage`/`AbortSignal`), **not macOS or code**.
- **MCP** (§V.11): built `tuxlink-mcp` (stdio shim) + `tuxlink-mcp-testserver`; drove the **real rmcp 0.8.5 router** over UDS via the stdio shim on macOS — `initialize`, **50 tools**, read tools return data, and the **arm/taint security gate verified end-to-end** (armed write OK; `mailbox_list` taints; post-taint write/egress denied; **taint persists across reconnect — no bypass**). Socket hardening (reject world-writable `/tmp`; 0600 socket under 0700 dir) works on Darwin.

## Code changes on the branch (all Linux-safe — additive or cfg-gated)

| Commit kind | What | Why |
|---|---|---|
| `fix(basemap)` | statvfs `u32`/`u64` cfg-split | `nix` field width differs on Darwin (only macOS compile error found) |
| `fix(forms)` | gate `PRINT_DEADLINE_SECS` to linux | non-Linux `dead_code` under `-D warnings` |
| `fix(catalog)` | `weatherGlyph.ts`→`weatherGlyphData.ts` | macOS/Windows case-insensitive import collision broke `vite build` |
| `fix(logging)` | canonicalize tempdir in state_dir test | macOS `/var`→`/private/var` symlink (test artifact, prod correct) |
| `build(deps)` | keyring `apple-native` on macOS (+ Cargo.lock `security-framework`) | macOS Keychain vs Linux D-Bus Secret Service |
| `build(bundle)` | `bundle.macOS.minimumSystemVersion = 11.0` | proper macOS bundle; purely additive (linux untouched) |
| `build(pnpm)` | `pnpm-workspace.yaml` esbuild allowlist | pnpm 11 ignores `package.json` `onlyBuiltDependencies` |
| `docs` | the assessment (§I–V) + app-bundle plan | the deliverable |

Linux impact: the Rust changes are cfg-split (Linux path unchanged) or additive; the catalog rename works on case-sensitive FS; the config/pnpm changes are additive. **I could not run Linux here** — CI (Linux-only) is the authoritative cross-check.

## Pending / operator decisions

1. **PUSH** — your call. You asked to confirm pushing to `origin` as `feat/macos-build-assessment`. Branch is push-ready; pushing + a draft PR gets **Linux CI** to validate the primary platform (clippy `-D warnings`, `--locked`, tests) which I can't run on this Mac.
2. **Node pin** — repo has no `engines`/`.nvmrc`; the suite needs **Node 20** (host defaults to 26 → 501 false failures). Recommend adding `.nvmrc`=20 + `engines.node`. Not done (cross-platform DX change, broader than this branch).
3. **Bundle identifier** `com.tuxlink.app` ends in `.app` → Tauri macOS advisory; cross-cutting rename (tauri.conf + polkit action) deferred (§V.10).
4. **`.dmg`** — fails headlessly; build in an interactive GUI session, or switch off Finder/AppleScript dmg layout (§V.10).
5. **App-side MCP on macOS** — the server start is `#[cfg(linux)]`-gated (`lib.rs:1371-1443`); the standalone testserver works, but exposing MCP in the macOS *app* needs un-gating + a macOS socket path (§V.11) — **substantive; plan first** per the writing-plans rule.
6. **Blocked on Screen Recording** (perm added but Claude not yet restarted): in-app Keychain wizard UI test; visual screenshots of the running app.
7. **Not run / deferred:** Rust integration + `--all-targets` tests on macOS (some Linux-specific, e.g. the D-Bus `wizard_integration_test` `#[ignore]`); ARDOP / in-app-Bluetooth / Core-Audio macOS work (assessment §I.5 cut-list); **iOS / Windows / Android remain static predictions** (Parts II–IV) — cheapest next step per platform is a `cargo check --target …`.

## Disposable state (not in git)
- `dev/adversarial/2026-06-28-macos-bundle-plan-codex.md` — Codex plan review (gitignored per CLAUDE.md).
- Scratchpad: the MCP smoke driver (`mcp_drive.py`), build/test logs — session-local, disposable. The MCP testserver daemon was SIGINT-stopped (socket unlinked).
