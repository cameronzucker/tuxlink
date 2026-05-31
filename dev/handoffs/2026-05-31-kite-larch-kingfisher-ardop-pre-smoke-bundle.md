# Handoff — kite-larch-kingfisher — ARDOP pre-smoke bundle PR open

> **Date:** 2026-05-31 · **Agent:** kite-larch-kingfisher · **Machine:** pandora (Pi 5)
> **Session arc:** continued from the 2026-05-30 ARDOP HF UI sweep. Operator asked me to ship the three P1/P2/P3 items from the 2026-05-31 gap audit before the next on-air smoke, and to surface a test command they could run *without* a radio.
>
> **Status:** Three commits landed on `bd-tuxlink-o3f2/ardop-abort-connect`, pushed, bundled into PR [#164](https://github.com/cameronzucker/tuxlink/pull/164). **Codex adrev NOT YET RUN — next session must do it before merge.** Test command surfaced below.

---

## 0. Where to start (next session)

1. **Read this handoff first**, then PR #164's body.
2. **CRITICAL FIRST ACTION:** run Codex adrev on PR #164. Hit the context wall before I could. Codex has caught 5 P1 bypasses across PR #153 / #157 / #160 / #162 in this session arc — assume it'll find something here too. Use the custom-prompt CLI pattern from CLAUDE.md (`cat prompt.txt | codex review -`). Attack angles in the PR body.
3. Fold any Codex findings into fix-up commits on the same branch.
4. Merge PR #164 after Codex clean.
5. THEN the operator can run the no-radio integration test (recipe in §6).

---

## 1. Today's session arc

Five-step journey today:

1. **Operator asked about hamexandria.** I found it (gitignored at `dev/scratch/ham-knowledge-store/`, never pushed — separate `.git` with zero commits). Saved a reference memory `reference_hamexandria.md`. Searched for ARDOP demo videos; corpus is thin (one oblique demo: The Tech Prepper CIQusjng37I).

2. **I extrapolated VARA UX → "ARDOP UX expectations"** memory; operator rightly pushed back ("grossly insufficient evidence"). I retracted the memory file and stuck to ARDOP-specific primary sources (AE8Q + Winlink spec).

3. **Operator asked: does our implementation match what Winlink Express ARDOP exposes?** Honest answer: no systematic audit had been done. Performed full audit against ardopcf upstream docs + AE8Q guide. Report at `dev/scratch/2026-05-31-ardop-gap-audit.md`. Filed 5 follow-up bd issues:
   - `tuxlink-o3f2` (P1 bug) — abort during connect
   - `tuxlink-j0ij` (P2 feat) — ARQBW bandwidth selection
   - `tuxlink-1637` (P2 feat) — PINGACK structured S/N parsing
   - `tuxlink-60wh` (P3 task) — ardopcf WebGUI link
   - `tuxlink-mzr7` (P3 feat) — TWOTONETEST pre-flight

4. **Operator: "ship 1-3."** Bundled into PR #164 with three commits:
   - `22cfe80` o3f2 ABORT during connect — **P1 safety fix** — 7 new tests including end-to-end TcpListener integration verifying ABORT works in <2s vs 120s deadline.
   - `85a6d90` j0ij ARQBW bandwidth selector — bandwidth_hz threaded through ArdopUiConfig → InitConfig → ARQBW host command; Settings dropdown.
   - `11f444d` 60wh WebGUI link — ardopcf spawned with `-G <cmd_port-1>`; dock "Open WebGUI" button opens `http://localhost:8514/`.

5. **Operator asked for a no-radio test command.** Recipe in §6 below.

Quality gates at HEAD `11f444d`:
- `cargo test --lib`: 465 passed (+21 from baseline)
- `cargo clippy --lib -- -D warnings`: clean
- `pnpm vitest run`: all green (+9 new tests across SettingsPanel + ArdopDock)
- `pnpm exec tsc --noEmit`: clean

---

## 2. Branch + worktree + PR state

- **Branch:** `bd-tuxlink-o3f2/ardop-abort-connect` (poorly-named in retrospect — carries all 3 bd issues; can rename if desired).
- **Worktree:** `worktrees/bd-tuxlink-o3f2-ardop-abort-connect/`.
- **HEAD:** `11f444d` (60wh WebGUI commit).
- **Parents:** `85a6d90` (j0ij) ← `22cfe80` (o3f2) ← origin/main at `5df177d`.
- **PR:** [#164](https://github.com/cameronzucker/tuxlink/pull/164), open, mergeable (modulo Codex review).
- **Origin tracking:** branch pushed; remote up to date.

Other live worktrees from this session:
- `worktrees/bd-tuxlink-ecth-ardop-send-receive/` — PR #160 (Send/Receive button) merged earlier this session; worktree disposable per ADR 0009 ritual.
- `worktrees/bd-tuxlink-n2uz-ardop-numeric-meters/` — PR #162 (numeric meters) merged earlier this session; worktree disposable.
- `worktrees/bd-tuxlink-ytg-ardop-b2f-transport/` — PR #159 (B2F over ARDOP) merged earlier this session; worktree disposable.
- `worktrees/bd-tuxlink-926y-ardop-live-meters/` — PR #156 (broadcaster polling) merged; worktree disposable.
- `worktrees/bd-tuxlink-qvl-ardop-polish-bundle/` — PR #155 (polish) merged; worktree disposable.
- `worktrees/bd-tuxlink-4ek-ardop-ui/` — PR #153 (dial) merged in v0.4.0; worktree disposable.

Disposal of all 6 stale worktrees is a separate hygiene task — they're gitignored under `worktrees/` so they don't pollute the index, but they're consuming disk.

---

## 3. What this PR ships (per commit)

See PR #164 body for the operator-facing summary. Three commits, all reviewed by per-task Claude subagents, ALL three need Codex adrev still.

---

## 4. Codex adrev — REQUIRED NEXT STEP

PR #157 (B2F) and earlier work had Codex find P1 issues that Claude reviewers missed. This PR has not yet been Codex-reviewed. **The custom-prompt invocation works:**

```bash
cat > /tmp/codex-164.txt <<'EOF'
You are doing adversarial code review of the diff against origin/main in this
worktree. Run `git diff origin/main..HEAD` to see all three commits.

Context: pre-smoke ARDOP bundle —
1. ABORT during in-flight connect (side-channel writer to cmd socket)
2. ARQBW bandwidth selection (200/500/1000/2000 Hz, validated)
3. ardopcf WebGUI link (-G flag at spawn + window.open in dock)

Audit P0/P1 only. Specifically:
1. RADIO-1 invariants — verify ABORT side channel can't trigger TX without
   an existing consent-gated connect; consume_consent_token property
   preserved.
2. Concurrency — abort_in_flight writes to TcpStream from a thread separate
   from the reader thread reading the same stream. POSIX atomic-write
   semantics + flush bounds — any deadlock or partial-write risk?
3. ARQBW negotiation — we send "ARQBW <hz> FORCED" between LISTEN and
   MYCALL. Does forced-vs-negotiated semantics carry any safety concern?
   What if ardopcf rejects FORCED with FAULT mid-init?
4. WebGUI URL injection — window.open with constructed URL based on
   user-configured cmd_port. Could a hostile config trigger URL injection
   in the WebView's external-browser dispatch?
5. Bandwidth validation — validate_arq_bandwidth_hz drops invalid values
   to None. Any path where hand-edited config JSON could smuggle a value
   through? Edge cases at 200 / 500 / 1000 / 2000 boundaries?

Read modem_commands.rs, modem_status.rs, transport.rs, session.rs,
ArdopDock.tsx, SettingsPanel.tsx.

Format findings as:
## Findings
### P0
- ...
### P1
- ...
NO P0/P1 ISSUES FOUND. — if clean.
EOF
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-o3f2-ardop-abort-connect
cat /tmp/codex-164.txt | npx --yes @openai/codex review - 2>&1 | tee dev/adversarial/2026-05-31-pr164-codex.md
```

If Codex finds P1s, fold them into fix-up commits on the same branch before merge.

---

## 5. Bd state

Issues to close on PR #164 merge (these PR commits close them):
- `tuxlink-o3f2` (in_progress) — ABORT during connect
- `tuxlink-j0ij` (in_progress) — ARQBW bandwidth
- `tuxlink-60wh` (in_progress) — WebGUI link

Still open from this session's audit (not in this PR):
- `tuxlink-1637` (P2) — PINGACK structured S/N parsing — next-batch work
- `tuxlink-mzr7` (P3) — TWOTONETEST pre-flight — subsumed by WebGUI's built-in test-tone button (verify before closing)

---

## 6. No-radio test command (the operator-promised deliverable)

After PR #164 merges, the operator can run a full integration smoke without a radio. ardopcf will spawn, init, attempt connect, and time out (since no radio is transmitting). All UI surfaces, the consent gate, RADIO-1 modal, ABORT button, WebGUI link, and state-machine transitions can be validated locally.

### Prereqs (one-time)

```bash
# Install ardopcf if not already present:
#   (Pi 5: build from source per pflarue/ardop USAGE_linux.md — armhf prebuilts available)
# Verify ardopcf is reachable at the configured path (default ardopcf in PATH or /usr/local/bin/ardopcf).
which ardopcf
# Or set the binary path in Settings → ARDOP HF after launch.

# Identify a non-existent or null ALSA device for the no-radio test
# (so ardopcf doesn't try to actually output to a real soundcard / key
# a real PTT). Two options:
#   (a) Use a PulseAudio null sink:
pactl load-module module-null-sink sink_name=ardop_null
#   That gives you "null" as a capture+playback device name.
#   (b) Or just use a device that exists but isn't connected to a radio:
arecord -l    # list ALSA capture devices
aplay -l      # list ALSA playback devices
```

### Launch + walkthrough

```bash
# 1. Start tuxlink in dev mode.
pnpm -C /home/administrator/Code/tuxlink tauri dev
#    The dev server binds Vite at :1420 (strictPort — kill any other
#    Vite first). Webview opens automatically.

# 2. Complete the wizard if it appears (callsign, grid, etc).
# 3. Settings → ARDOP HF:
#    - Binary:    ardopcf  (or absolute path like /usr/local/bin/ardopcf)
#    - Capture:   plughw:0,0  (or "null" for the PulseAudio null sink)
#    - Playback:  plughw:0,0  (same)
#    - PTT:       (leave blank — no radio = no PTT keying)
#    - Cmd port:  8515
#    - ARQ bandwidth:  500 Hz  (test the new dropdown — try different values)
#    Save (auto-blur).

# 4. Sidebar → Winlink (CMS) → ARDOP HF.
#    Dock should appear on right side once status leaves Stopped.

# 5. In the dock's Connect form, type a fake target callsign (e.g. "TEST-10").
# 6. Click Connect.
# 7. RADIO-1 modal appears. Ack the checkbox, click Connect.
# 8. Modem spawns ardopcf, transitions Spawning → Initializing → Connecting.
#    ardopcf is now actually running with -G 8514. While in Connecting state:

# 9. Click "Open WebGUI" in the dock.
#    Default browser opens http://localhost:8514/ — ardopcf's built-in
#    Spectrum + Waterfall view. You should see audio inputs (probably noise
#    from the null sink) + the test-tone button + status indicators.

# 10. NEW (tuxlink-o3f2): mid-Connecting, click Disconnect in the dock.
#    Expected: status goes to Error within ~2 seconds (NOT the 120s
#    CONNECT_DEADLINE). The dock's connectError should show a message
#    referencing "consent token cleared" or similar. ardopcf process
#    should exit cleanly.

# 11. Verify ardopcf process cleanup:
pgrep -fa ardopcf    # should return nothing after the dock returns to Stopped

# 12. (Optional) Try changing ARQ bandwidth in Settings between connects.
#    ardopcf's WebGUI should show the FORCED bandwidth value in its status
#    line during the next connect attempt's init phase.
```

### What you're validating

| Surface | What it proves |
|---|---|
| RADIO-1 modal appears + token consume | Consent gate from PR #153 + #159 |
| ardopcf actually spawns | ManagedModem supervisor from PR #138 + tuxlink-4ek wiring |
| WebGUI is reachable | tuxlink-60wh's `-G` flag is plumbed |
| ARQ bandwidth dropdown persists + drives the next connect | tuxlink-j0ij's ARQBW wiring |
| Disconnect mid-connect returns promptly (NOT 120s) | tuxlink-o3f2's ABORT side channel |
| ardopcf exits cleanly on disconnect | `reset_to_stopped` + transport.disconnect + ManagedModem.stop |
| Dock UI state machine transitions | useModemStatus + ModemStatusBroadcaster (PRs #156, #162) |

### What this does NOT validate

- Any real on-air behavior — no radio TX, no real ARQ handshake, no real B2F mail flow. Those require a real radio + remote gateway and are gated by RADIO-1 (operator-only, not agent-runnable).

---

## 7. Cleanup chores for next session (low priority)

- Worktree disposal ritual (ADR 0009) for the 6 stale worktrees from this session's merged PRs. They're consuming disk but otherwise harmless.
- Verify `tuxlink-mzr7` (TWOTONETEST) is actually subsumed by the WebGUI button. If yes, close it.
- Consider filing one more bd issue: "iframe-embed the ardopcf WebGUI inside the dock" — would close the spectrum/waterfall UX gap fully without external-browser context switching.

---

## 8. Files changed in this PR

```
src-tauri/src/config.rs                        | bandwidth_hz field + tests
src-tauri/src/modem_commands.rs                | abort wiring + bandwidth plumbing + extra_args helper + WebGUI -G flag + tests
src-tauri/src/modem_status.rs                  | abort_writer field + install_abort_writer + abort_in_flight + tests
src-tauri/src/winlink/modem/ardop/session.rs   | try_clone_writer + InitConfig.arq_bandwidth_hz + ARQBW init send + tests
src-tauri/src/winlink/modem/ardop/transport.rs | try_clone_abort_writer override + tests
src-tauri/src/winlink/modem/mod.rs             | trait method default
src/modem/ArdopDock.tsx                        | Open WebGUI button
src/modem/ArdopDock.test.tsx                   | WebGUI button tests
src/shell/SettingsPanel.tsx                    | ARQ bandwidth dropdown
src/shell/SettingsPanel.test.tsx               | bandwidth dropdown tests
dev/scratch/2026-05-31-ardop-gap-audit.md      | audit report (gitignored, won't ship)
dev/handoffs/2026-05-31-...-pre-smoke-bundle.md| this handoff
```

3 commits, +1207 lines, +21 lib tests, +9 frontend tests.
