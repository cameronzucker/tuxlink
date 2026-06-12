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

- **P1–P7 + P9: GREEN** (run 27409525920 + Release build — `clippy --all-targets --locked -D warnings` clean + full vitest incl. the P7 UI tests + the Rust device-list command, both arches). Cleared 4 CI-only issues the cold-no-local-clippy posture surfaced: Debug-derive E0277, needless_update, items_after_test_module, question_mark.
- **Device-enum smoke-blocker fix (`563884ea`): CI re-verifying** at handoff. Confirm green (`gh pr checks 628`).

## Self-adrev outcome (2026-06-12, OPERATOR DECISION: substituted Claude self-adrev for Codex — "we don't have time; anything really broken surfaces in smoke")

Two parallel adversarial passes on the full diff. **Caught 2 CRITICAL real-hardware bugs, confirmed against this Pi's live `/sys`** — `read_sys_snapshot` read USB `idVendor` at the interface node (none there) → DigiRig/DRA-100 filtered out of the picker entirely; and the card-vs-PTT `usb_parent` join key was at the wrong tree depth → PTT never resolved / cross-matched on a shared hub. **FIXED in `563884ea`** (climb to the USB device node on all three walks; join key = device node, not hub; hardened the CardIdHash fallback since `/dev/snd/by-id` is absent here). New regression tests incl. two-dongles-one-hub. The RADIO-1 never-keyed-PTT invariant, abort path, conf syntax, config/DTO, and TS↔Rust wire shapes all reviewed CLEAN end-to-end. **The self-adrev is the review gate (operator's call); switch back to Codex only if it's back and there's time.** Raw findings are in the session transcript (not tee'd — the passes were Agent subagents, not the Codex CLI).

## Remaining before PR #628 → ready (in order)

1. **Confirm the `563884ea` CI run is green** (`gh pr checks 628`).
2. **Operator on-air smoke** — DRA-100 → CDM-1550LS+, VHF FM packet. The agent cannot transmit (RADIO-1); this is the operator's. Key checks: (a) the device picker now shows the cards (the `563884ea` fix), (b) **clean disconnect de-keys the transmitter** (documented SIGKILL residual is the watch item). Suggest `ls /dev/snd/by-id` on the reference Pi first — if empty (as on this Pi), the hardened CardIdHash fallback is what disambiguates the two cards.
3. **`tuxlink-sr86` (P2) — bring the branch up to origin/main BEFORE merge** (34 commits behind; the `origin/main..HEAD` diff shows phantom reverts — review the feature against fork point `63315e63..HEAD`, not main). Merge-time blocker, NOT a smoke blocker.
4. Residual self-adrev follow-ups (non-blocking): `tuxlink-331c` (I1, gate Connect on a configured managed link), `tuxlink-opl8` (M2, conf control-char guard), `tuxlink-kyh1` (M3, PTT display drift).
5. `tuxlink-dn5b` (P8 readiness chip) — optional polish.
6. Minor: amend the design doc's `direwolf -t 0 -c` parse-gate line per the P3 grounding (one line).

## Worktree

`worktrees/bd-tuxlink-yq3l-managed-direwolf` — tracked clean, all pushed. Untracked: `node_modules/` (gitignored; installed for the pre-push doc-link hook). No gitignored-stateful content of concern. **Do NOT dispose** — active build worktree until PR #628 merges (then ADR-0009 ritual).

Agent: glade-clover-bison
