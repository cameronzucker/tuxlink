# Handoff — Node 20→24 remediation SHIPPED; README repositioning pass PARKED

- **Agent:** poplar-mink-chasm
- **Date:** 2026-07-17
- **Ended:** operator asleep, delegated recognizance to resolve `tuxlink-niiug` to completion and touch no other bd work. Both honored.

## 1. Node 20 → 24 LTS: DONE (tuxlink-niiug, GHSA-j23x-pffj-fxpv)

**Merged to main: PR #1136 (`d16951e5`).** ci.yml / release.yml / ect-build.yml now pin Node 24.

**It was never a shipped-product vulnerability.** Verified by inspecting the built `.deb` + `dist/`: the CVE'd Node runtime and its bundled deps (OpenSSL 3.0.19, undici, nghttp2, V8) do not ship; the frontend build output that ships is clean browser JS. The real item was EOL-runtime build-toolchain hygiene under the `contents: write` release job. Low urgency, no active exploit.

**Verified properly, because the filer's single container run was inadequate for a 4523-test, no-retry, dual-arch pipeline:**
- R2 deterministic gate (x86, faithful pnpm 10): 4523/4523, no hard break.
- Two differential flake-fingerprint matrices, 18 samples/cell × {Node 20 control, Node 24} × {amd64, arm64}: **zero new deterministic failures** under Node 24; amd64 strictly better (18/18); the arm64 wobble is entirely pre-existing, Node-**independent** flakes (ConsentGate journal-park flaked under Node 20 amd64 AND arm64 too). Verdict: safe.
- The fingerprint instrument (`node-flake-fingerprint.yml`, PR #1135) is **kept on main**, reusable for future runtime bumps: `gh workflow run node-flake-fingerprint.yml -f iterations=N`.

**Cost of verifying the "3-line change"** (per your directive to document that "trivial diff" ≠ "trivial risk"): ~2h24m wall + ~408 CI job-minutes (24 dual-arch jobs, 36 full suite executions) + R2. A full work-stoppage over a non-shipped-product item. Details: `dev/scratch/niiug-cost-ledger.md` in the (now disposed) niiug worktree — archived to `.claude/worktree-archives/`.

**Caught en route:** the pnpm-11/corepack fidelity trap. Node 24's corepack defaults to pnpm 11, which rejects the pnpm-10 lockfile under `--frozen-lockfile`. CI is safe (it pins pnpm 10 via action-setup), and my first-draft instrument reproduced the bug, which would have produced a FALSE failure — a concrete example of why an unfaithful single run proves nothing.

**Filed, NOT worked (per scope):**
- **`tuxlink-2h16p`** (P2) ConsentGate arm64 load flake + **recommendation to add vitest `retry` to ci.yml** — this is the actual driver of the "flaky CI costs hours" pain, orthogonal to Node. Fixing it is the highest-leverage follow-up.
- (P3, `bd list | grep -i pnpm`) add a `packageManager` field to package.json so corepack users get pnpm 10.

**GHSA-j23x-pffj-fxpv** (draft, another agent's) is now remediated by #1136 and can be dismissed/closed at your discretion. I did not touch it (it is a security-tab artifact and not mine to publish or close).

## 2. README repositioning pass: PARKED, now unblocked (tuxlink-d8f3l)

Not resumed tonight (you scoped me to niiug only). State:
- **Tasks 1–3 DONE and reviewed** on branch `bd-tuxlink-d8f3l/readme-elmer-pass` (worktree `worktrees/bd-tuxlink-d8f3l-readme-elmer-pass`): fact ledger; `docs/ELMER.md` (approved); full README rewrite in your voice profile (approved). All committed + pushed.
- **Task 4 (screenshots) was blocked** by the scheduler launch panic. **#1132 merged that fix**, so it is now unblocked. The d8f3l worktree also has an uncommitted cherry-pick of the scheduler fix and a patched arm64 build that launches — that local patch should be dropped and the worktree rebased onto current main (which now has #1132) before capturing.
- Remaining: Task 4 (real-app screenshots, receive-only), Task 5 (Codex adrev), Task 6 (ship). The voice profile is at `~/.claude/cameron-writing-voice-profile.md` (memory updated).

## 3. Housekeeping done
- bd persisted (`bd dolt push`). niiug worktree disposed per ADR 0009 (cost ledger archived). This handoff worktree disposed after push.
- No other session's PR touched. No other bd work started.
