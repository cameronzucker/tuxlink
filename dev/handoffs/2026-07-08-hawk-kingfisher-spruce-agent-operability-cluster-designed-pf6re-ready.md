# Handoff — 2026-07-08 — hawk-kingfisher-spruce

**This session designed the entire agent-operability cluster and produced a
build-ready, adrev-hardened plan for the lead item (pf6re). No product code was
written yet — the build is the next session's job, and the plan is written to be
executed directly.** Two live agent-mode tests (the earlier VARA HF plumbing bug +
a second armed test whose transcript the operator pasted this session) drove the
work.

Branch: `bd-tuxlink-7ppfq/session-handoff` (all work committed + pushed).

## What happened this session

1. **Brainstormed 4 design contracts** with the operator for the original cluster
   (7ppfq reachability, active-modem SoT, z2nwx print, 77seh audio). North star the
   operator set: *get VARA working no matter what; agent capability = shell-equivalent,
   gated only by real safety boundaries (RADIO-1, egress arm/taint).*
2. **Ran a 5-round adrev on 7ppfq** (Codex + 4 Claude). Result: `vara_start` is a
   different, riskier problem (agent on Pi, VARA on remote R2; launch primitive
   shell-only + box64-refused) → **split to tuxlink-u269g**. 7ppfq re-scoped to
   **perception-only**. Many design corrections folded into the spec (try_lock probe,
   read-only deep probe, ARDOP running-source, reuse activeConnection, config schema
   bump, etc. — see spec + 7ppfq bd notes).
3. **The operator pasted a live armed-test transcript** exposing a NEW, more acute
   defect: an egress denial (send authority expired) returned a raw `-32600` that
   **killed the agent turn and clobbered its output**. Diagnosed to `runner.rs:163-165`
   (`RunOutcome::ToolDenied` terminal) — deliberate injection defense (2ouqf), but
   terrible UX. **Filed tuxlink-pf6re (P1) and made it the cluster LEAD.**
4. **Ran a 2nd 5-round adrev on pf6re** (security-focused; Codex + 4 Claude). Verdict:
   no P0 transmit regression (egress lock holds structurally). Convergent design +
   R4's P0 (the flattened denial string lies about the taint remedy) + a verified fact
   (`session.rearm` = quarantine-that-discards-the-conversation) → cause-split denial
   strings.
5. **Wrote a subagent-proof 7-task pf6re plan**, self-reviewed 3 rounds (fixed a
   latent contradiction: derive DenialKind from the reason string, do NOT reshape
   `ToolOutcome::Denied` — keeps `injection_tests.rs` untouched).
6. **Filed + linked all cluster issues.**

## Artifacts (all committed + pushed on this branch)

- `docs/superpowers/specs/2026-07-08-agent-operability-cluster-design.md` — cluster
  design, adrev-hardened. Contract 0 (pf6re) + 1-4.
- `docs/plans/2026-07-08-pf6re-graceful-denial-plan.md` — **the build-ready plan.**
- `dev/adversarial/2026-07-08-{7ppfq,pf6re}-design-codex.md` — raw Codex (gitignored,
  local only).

## bd tracker state

- **tuxlink-pf6re** (P1, LEAD, in_progress) — graceful denial + arm/taint perception.
  **Design + plan done; BUILD NEXT.** bd notes carry 4 implementation nuances found
  while starting Task 1 (SessionLog 4th TaintReason variant; keep tuxlink-security
  dependency-free — TaintReason as plain enum, serialize in mcp-core; ~25 taint()
  call-sites; the ONE allowed injection_tests.rs edit is a mechanical arg-add at :597).
- **tuxlink-7ppfq** (P1) — perception-only (reachable + vara_probe + SoT). Adrev-hardened
  notes recorded. Build after pf6re.
- **tuxlink-u269g** (P2) — vara_start (local-vs-remote launch), depends on 7ppfq.
- **tuxlink-etjp9** (P2) — predict_path runaway (~40+ calls in the transcript).
- **tuxlink-iicsh** (P2) — agent clear-channel listen + duty-cycle cooldown pacing.
- **tuxlink-z2nwx** (P2) — print (CUPS) + report export. Forward flag from adrev: a
  NETWORK printer is a real egress side-channel — gate it when built.
- **tuxlink-77seh** (P2) — audio surface. Hardware grounding: FT-710 = DRA-100 audio +
  separate FT-710 USB for CAT/RTS PTT (digital preset; CAT readable for state).

## Build order

pf6re (lead) → 7ppfq perception-only → z2nwx + 77seh. Split-outs (u269g/etjp9/iicsh)
have their own brainstorms.

## The pf6re build in one paragraph (for the next session)

Execute `docs/plans/2026-07-08-pf6re-graceful-denial-plan.md` tasks 1-7 via TDD.
Core: make an egress denial NON-terminal — the runner pushes the denial as a tool
result, breaks the batch, grants exactly ONE narration turn, then terminates
(`ToolDenied` for authority/expiry; `NeedsOperator` for taint) — while keeping the
egress lock absolute and `injection_tests.rs` untouched. Add a durable denial
`RunEvent` (chip → `denied`) so the security signal survives. Cause-split the denial
strings (taint must NOT say "resume"). Extend `server_info` with a content-free
`taint_reason`. Fix `agents-guide.md`'s stale "re-arm clears taint" wording. **Pi
can't compile Rust — build on the PR, let CI (amd64+arm64) compile/test.** End at the
wire-walk gate (3 flows in the plan).

## Working-tree / worktree state

- Clean; all commits pushed. No stashes.
- This branch also carries the prior session's handoff + the hamlib work already
  merged. Disposal candidates (merged-dead branches) noted in the prior handoff are
  unchanged.

## Pending decisions

- None blocking the build — the plan resolves the design. The only judgment left is
  inside Task 6: whether the Elmer re-arm UI already states the conversation-discard
  truth when tainted (verify, then tweak copy or add the warning).
