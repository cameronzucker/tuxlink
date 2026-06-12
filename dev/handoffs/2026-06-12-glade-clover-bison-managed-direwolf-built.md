# Session handoff — glade-clover-bison — 2026-06-12

Execution session: built **managed Dire Wolf (Slice B, tuxlink-yq3l)** end-to-end from the committed plan via subagent-driven-development. The accessibility centerpiece — the operator picks sound-card + PTT + callsign in the UI and never authors a `direwolf.conf`. **P1–P7 + P9 all built, committed, pushed; P1–P6+P9 CI-green.** PR #628 stays **DRAFT** pending the mandatory Codex round (quota-gated to tomorrow) + the operator on-air smoke.

## ⚠️ Read first — state

- **Branch `bd-tuxlink-yq3l/managed-direwolf`** (worktree `worktrees/bd-tuxlink-yq3l-managed-direwolf`, off origin/main). Working tree clean; all commits pushed. **PR #628** (draft).
- **Main checkout** is on `bd-tuxlink-xygm/recover-handoffs`, far behind origin/main — read code via `git show origin/main:<path>` or the worktree, never the main working tree. This handoff is committed on the **yq3l branch** (the main-checkout-race hook blocks main commits while other sessions are live); the next session reads it by the path in the starting prompt.
- **No cold cargo on this Pi** — the entire Rust build was validated by GitHub CI on the draft PR, never locally. Keep that posture.

## What was built (15 commits: `eeeafb3d`…`d7b9c9bd`)

A managed VHF-FM-packet path where tuxlink owns the Dire Wolf lifecycle (finishes ADR-0015 decision #1; ardopcf was already managed, Dire Wolf was the gap):

- **P1** `winlink/ax25/devices.rs` — stable audio-device id (by-id → vid:pid:serial → FNV-1a, never card index) + ranked PTT discovery (CM108-HID same-USB-parent first, serial-RTS second). Pure, fixture-tested.
- **P2** `direwolf_conf.rs` — generates exactly 6 directives; TXDELAY/persist/slot + FX.25/IL2P deliberately ABSENT (tuxlink pushes timing as KISS param frames). MYCALL = base call (SSID set by tuxlink's AX.25 layer).
- **P3** `direwolf_probe.rs` — presence/version (numeric compare), conf classifier, `/proc/asound` device-busy probe. **Grounding correction:** Dire Wolf 1.7 has **no config-dry-run flag** (`-t` is text-color, `-c` opens audio) — so `validate_conf` is a typed wiring point NOT run against the real binary; pre-spawn safety = construction-correctness (template conf) + the device-busy probe. *(The design doc's "`direwolf -t 0 -c` parse gate" premise was wrong — corrected in code; worth a one-line design-doc amend.)*
- **P5** `KissLinkConfig::ManagedDireWolf { audio_device, ptt }` + `PacketConfigDto` round-trip (nested structured fields, lossless; lenient deser keeps back-compat).
- **P4** `managed_direwolf.rs` — `ManagedDireWolf` wraps the existing `ManagedModem` (reused, not reimplemented): generate conf → device-busy probe → spawn `direwolf -t 0 -c` → bind-wait the single KISS port (TcpListener-EADDRINUSE) → no-leak-on-timeout; `shutdown()` = stop(5s grace) → confirm card released → restore-on-failure retry. **RADIO-1-reviewed** (2 independent reviewers + fixes).
- **P6** backend wiring (`winlink_backend.rs` `native_packet_connect`, clean 90-line insertion) — intercepts the managed variant, resolves the persisted stable id against the live system (`read_sys_snapshot` now real + `resolve_managed_device`), picks a free loopback port, spawns, rebinds `link` to Tcp. A **`ManagedDireWolfGuard`** held for the whole fn scope runs the explicit **5s `shutdown()` (clean PTT de-key) on EVERY exit — normal / `?`-error / panic / abort**. Covers dial + listen. `DireWolfNotInstalled` → install-or-BYO message. **RADIO-1-reviewed** (no spawned-but-unguarded window; the inter-spawn window is covered by `ManagedModem::Drop`'s reap).
- **P7** UI (`PacketRadioPanel.tsx` + `ManagedModemSection.tsx` + `packet_list_audio_devices` Tauri cmd) — Connection toggle **Managed (recommended) vs Bring-your-own KISS**; managed shows sound-card picker + PTT picker (auto-default + override) + read-only callsign. Fresh panel defaults to Managed; existing Tcp/Serial/BT shows BYO (no clobber); empty-list → plug-in+Refresh affordance. tsc clean; 35 + 63 vitest green incl. an App-level production-mount-path test.
- **P9** packaging — `.deb`/`.rpm` **Recommends** `direwolf (>= 1.6)` (NOT Depends — no first-run brick), verified against the locked tauri-utils schema; user-guide note added.

**RADIO-1 posture:** clean SIGINT de-keys; the **SIGKILL-residual-PTT** risk (a hard-killed modem can leave PTT asserted) is honestly documented, with **no tuxlink-added airtime cap/TOT/watchdog** (per `feedback_no_tuxlink_added_safeguards`). The agent never transmitted.

## CI

- **P1–P6 + P9: GREEN** (run 27408536190 — `clippy --all-targets --locked -D warnings` clean + full vitest + both arches). Cleared 4 CI-only issues the cold-no-local-clippy posture surfaced: Debug-derive E0277, needless_update, items_after_test_module, question_mark.
- **P7 CI: in-flight** at handoff (run for `d7b9c9bd`). The new Rust command is a thin wrapper verified sound (partial-move ordering checked); vitest green locally. **Next session: confirm P7 CI went green** (`gh pr checks 628`) before the Codex round.

## Remaining before PR #628 → ready (in order)

1. **Confirm P7 CI green** (above).
2. **MANDATORY cross-provider Codex adversarial round** on the full diff — **quota-blocked until ~2026-06-13 1:49 PM**. Do NOT substitute Claude (`feedback_no_carveout_on_cross_provider_adrev`). Run per CLAUDE.md's stdin pattern on `git diff origin/main..HEAD`; tee to `dev/adversarial/`. Address findings, then mark PR ready.
3. **Operator on-air smoke** — DRA-100 → CDM-1550LS+, VHF FM packet. The agent cannot transmit (RADIO-1); this is the operator's. The key thing to verify on air: **clean disconnect de-keys the transmitter** (the documented SIGKILL residual is the watch item).
4. `tuxlink-dn5b` (P8 readiness chip) — optional polish, reframed from live-health to pre-connect readiness; not required for alpha.
5. Minor: amend the design doc's `direwolf -t 0 -c` parse-gate line per the P3 grounding (one line).

## Worktree

`worktrees/bd-tuxlink-yq3l-managed-direwolf` — tracked clean, all pushed. Untracked: `node_modules/` (gitignored; installed for the pre-push doc-link hook). No gitignored-stateful content of concern. **Do NOT dispose** — active build worktree until PR #628 merges (then ADR-0009 ritual).

Agent: glade-clover-bison
