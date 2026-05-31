# 11. Fork Pat as `tuxlink-pat`; refactor cred-handling first; refactor other limits as discovered

Date: 2026-05-18
Status: Accepted (amends [ADR 0003](0003-no-sqlite-pat-owns-mailbox.md) §"Pat is the authoritative source of all mailbox state"; the substance of ADR 0003 — Pat owns the mailbox, no SQLite in tuxlink, tuxlink wraps Pat's HTTP API — stays accepted, but the dependency target shifts from upstream `la5nta/pat` to a tuxlink-owned fork; **superseded by [ADR 0016](0016-native-b2f-outbound-with-attachments.md) as of 2026-05-30** — Pat is completely removed; native backend is the sole WinlinkBackend impl; the keyring refactor this ADR motivated is preserved in the native client.)
Deciders: cameronzucker, oak-fjord-swallow (drafting agent)

## Context

Tuxlink's v0.0.1 architecture (per [ADR 0003](0003-no-sqlite-pat-owns-mailbox.md)) wraps the upstream Pat Winlink client (`la5nta/pat`) as a managed child process. Pat handles the Winlink protocol, CMS authentication, telnet sessions, and mailbox storage. Tuxlink contributes the UX layer.

This architecture has been load-bearing for v0.0.1 ship velocity — Pat is battle-tested for Winlink interop and ARSFI compatibility, and reimplementing the B2F/FBB protocols from scratch in Rust would be a multi-year project with high risk of subtle interop regressions.

Two limitations of upstream Pat surfaced within a 24-hour window of v0.0.1 implementation work:

1. **2026-05-18 morning (PR #42 fixup):** Task 5's `PatClient::send` initially used JSON to talk to Pat's `/api/mailbox/out`, but Pat 1.0.0 requires `multipart/form-data`. Codex caught this at adversarial-review time. Fixed by a one-line shape change. Cost: small; surfaced cleanly.
2. **2026-05-18 evening (Task 6 brainstorm):** Pat persists WL2K passwords to `~/.config/pat/config.json` in plaintext, with no keyring integration. The April v0.0.1 spec inherited this by routing the Live-CMS smoke binary's credentials through env vars (which leak to `~/.bash_history` and `/proc/<pid>/environ`). The agent surfaced "stdin prompt → temp file → RAII delete" as a mitigation, which would minimize the on-disk window but does not eliminate it (Pat itself reads from `config.json` at startup; the temp window is at least the duration of the smoke run).

Cameron's response to the second limitation:

> No, this begs a bigger question. This is bandaid spiraling. If Pat is built like crap, should we be relying on it instead of full fat forking or recreating our own version of the functionality at all? I must be exceedingly clear: anything which stores creds to env is badly designed.

And subsequently:

> I will NOT rely on a project which was so lazy as to write creds for a real production system to .json. We'll fork it and refactor with robust agentic development as we go.

The pattern matters more than either individual finding. Every Pat limitation we encounter becomes a tuxlink user problem. Bandaging each at the tuxlink call site accumulates technical debt and surfaces as user-facing security or UX issues. The structural fix is to own the engine.

## Decision

### 1. Fork upstream Pat as `tuxlink-pat`

A new repository, `tuxlink-pat`, holds tuxlink's fork of upstream Pat. The fork is owned by the tuxlink project; the exact GitHub home (`cameronzucker/tuxlink-pat`, a `tuxlink-org/tuxlink-pat`, or a vendored subdirectory of the `tuxlink` repo itself) is an operational detail decided in the fork-setup task following this ADR.

Tuxlink's `src-tauri/` build depends on `tuxlink-pat` (not upstream `la5nta/pat`) for the Pat binary and for any Pat-side patches. The dependency declaration moves from "download a release tarball of upstream Pat" to "build from the `tuxlink-pat` source tree" — operational details follow in the fork-setup task.

### 2. First refactor target: credentials → OS keyring

The fork's first patch eliminates Pat's plaintext-cred-on-disk model:

- Pat reads the WL2K password from the OS keyring (`secret-service` on Linux Gnome/KDE, `Keychain` on macOS, `CredentialManager` on Windows) instead of from `config.json`.
- `config.json` retains the callsign + non-secret config; secrets move out entirely.
- Tuxlink's wizard (Task 9) writes the password into the keyring directly via a Rust `keyring` crate; the wizard never touches a `config.json` path that contains the password.
- The Live-CMS smoke binary (Task 6, paused pending this ADR + fork-setup) reads the password from the keyring at run time; no env vars, no temp files, no disk persistence.

This patch is the first agentic-refactor work item against `tuxlink-pat`, executed under the full `build-robust-features` pipeline.

### 3. Ongoing-refactor policy: full pipeline per fork patch

Every patch landed on `tuxlink-pat` (beyond mechanical upstream-merge work) uses the full `build-robust-features` pipeline:

1. `superpowers:brainstorming` to scope the patch
2. 5-round adversarial design review with at least one cross-provider Codex round
3. `writing-plans-enhanced` for the implementation plan (which internally runs `plan-review-cycle`)
4. TDD implementation
5. Codex round on the implementation diff
6. PR against `tuxlink-pat`

This applies to (a) the cred-handling refactor in §2, (b) any future Pat-side fixes for limitations we hit, and (c) any feature additions tuxlink needs from Pat that aren't a fit for upstream.

### 4. Upstream contribution policy

For each fork patch:

- If the patch is a bug fix or a generally-useful feature, submit a PR to upstream `la5nta/pat` after the patch ships in `tuxlink-pat`. Wait for upstream review.
- If upstream accepts: drop the fork-side patch on the next upstream-merge cycle.
- If upstream declines or the patch is tuxlink-specific by design (e.g., a tuxlink-IPC primitive Pat upstream wouldn't want): keep the patch in the fork indefinitely.

The keyring-auth refactor (§2) is a good upstream-contribution candidate — secret-service / Keychain / CredentialManager are platform-native and Pat upstream may welcome the security improvement. Pursue upstream PR after the tuxlink-pat-side patch ships.

### 5. Fork-sync discipline

`tuxlink-pat` tracks upstream `la5nta/pat` via merge-from-upstream (not rebase) at a cadence to be set in the fork-setup task. Recommended: weekly check, merge if upstream has shipped, run tuxlink's test suite against the new build. Detailed sync workflow documented in the fork-setup task's output.

### 6. Transition plan for v0.0.1

Tasks affected by the fork transition (v0.0.1 plan amendments queued post-ADR):

- **Task 5 (Pat HTTP client, SHIPPED):** No code change required initially — tuxlink's HTTP client talks to whatever Pat binary is running, fork or upstream. May need amendment if the fork's cred refactor changes Pat's startup behavior (e.g., keyring-prompt on first run).
- **Task 6 (Live-CMS smoke, PAUSED):** Brainstorm resumes after fork-setup + cred refactor land. Smoke binary uses keyring-backed cred lookup directly; no env vars, no temp files.
- **Task 9 (Wizard screen 1 — Winlink account, NOT STARTED):** Wizard writes the WL2K password to the OS keyring (not to any config file). Spec amendment captures this.
- **Task 11 (Wizard screen 3 — test send, NOT STARTED):** Test-send pulls the password from the keyring (not from a config-file path the wizard wrote).
- **v0.0.1 plan §"Tools and Dependencies":** Add `keyring` Rust crate dep; document `secret-service` system-package requirement on Debian/Ubuntu (the AppImage target).

Sequencing: ADR 0011 ships → fork-setup task ships (`tuxlink-pat` repo exists, builds, mirrors upstream) → cred-handling-refactor task ships (the first agentic patch against the fork) → v0.0.1 plan amendments ship (Tasks 5/6/9/11 updated) → Task 6 brainstorm resumes with the fork + keyring as the cred model from day one.

## Consequences

**Positive:**

- **Cred-correctness at the engine layer.** No tuxlink user ever sees WL2K passwords on disk in plaintext. The defect that triggered this ADR is fixed at its source.
- **Door open to fix other Pat limitations.** The multipart-shape surprise (PR #42) was a tuxlink-side fix because Pat 1.0.0's contract is what it is. The next equivalent surprise can be a fork-side fix instead — we own the engine.
- **Upstream contribution path is preserved.** Patches that fit upstream go upstream. The fork is not a vanity divergence; it's a workshop for changes that may flow back.
- **No `feat/v0.0.1` history rewrite.** Task 5 stays shipped; the fork transition happens via plan amendments + new tasks. The destructive-git ban remains uncompromised.

**Negative:**

- **Go codebase to maintain alongside Rust.** The agent team now needs Go competence for fork patches. Pat is ~30k LOC; we're not refactoring all of it, but each patch touches Go. Mitigations: use the full build-robust-features pipeline (caught the cred issue at brainstorm time, will catch other defects similarly); favor small surgical patches over large refactors; pursue upstream contribution to keep the divergence small.
- **Fork-sync overhead.** Every upstream Pat release requires a merge cycle + regression test against tuxlink. Cadence and tooling decisions follow in the fork-setup task. Likely weekly or per-upstream-release.
- **v0.0.1 scope expansion.** Fork-setup + cred-handling refactor + Tasks 5/6/9/11 plan amendments + the AppImage `secret-service` system-dep are all new work. The v0.0.1 timeline shifts by the duration of those items. Mitigations: parallelize where independent (e.g., fork-setup + plan amendments can run alongside the cred refactor); accept the timeline impact as the cost of fixing the cred defect at the source rather than user-side.
- **Two-codebase cognitive load for new contributors (future).** Anyone joining tuxlink needs to grok both the Rust UX layer and the Go engine layer. Mitigations: clear README boundaries; the fork's CONTRIBUTING.md points contributors at the right repo for the right kind of change.
- **Risk that the fork drifts so far from upstream that re-syncing becomes infeasible.** Mitigations: §3's "small patches, full pipeline per patch" discipline; §4's upstream-contribution policy keeps divergence small; §5's regular sync cadence catches drift early.

## Alternatives considered

- **Continue depending on upstream Pat; mitigate each limitation at the tuxlink call site.** Rejected. This is the bandaid-spiraling path Cameron explicitly named. The cred defect cannot be fully mitigated tuxlink-side — Pat reads from `config.json`, period. Every future Pat limitation accumulates the same cost shape. Pretending the dependency is sound when it has known structural defects (Cameron's words: "lazy as to write creds for a real production system to .json") is dishonest about the architecture.

- **Build a full Rust Winlink client from scratch (no Pat at all).** Rejected for v0.0.1, deferred indefinitely. Pat is ~30k LOC of Go refined over years against ARSFI compatibility and real-world transport quirks (telnet, packet, Pactor, VARA HF, VARA FM). Reimplementing the protocol stack in Rust is a multi-year project with high risk of subtle interop regressions; v0.0.1 would never ship. The fork (this ADR's decision) preserves the option of partial-or-full Rust rewrite in v0.1+ if the fork's accumulated patches reach a point where rewriting the patched modules in Rust is less work than maintaining the Go fork — but that's a future decision based on observed patch volume, not a v0.0.1 commitment.

- **Hybrid: keep depending on upstream Pat; selectively patch via a runtime config-mangling layer in tuxlink.** Rejected. This is a more elaborate bandaid — tuxlink intercepts Pat's reads/writes to `config.json` and substitutes keyring values at the syscall layer. Adds significant complexity (LD_PRELOAD shims, syscall interception, or a FUSE filesystem) for no real benefit over forking. The fork is cleaner, more debuggable, and a path that other Pat downstream consumers could share.

- **Defer the architectural decision; mitigate Task 6 with stdin-prompt + temp file + RAII delete for v0.0.1; revisit the fork question in v0.1.** Rejected. The April v0.0.1 spec was already a "v0.1 will fix this" deferral on the cred-storage layer; the current pattern of accumulating deferred problems is what produced the bandaid-spiral. Making the architectural call now, with full reasoning captured, is cheaper than another deferral cycle.

## Implementation follow-ups (queued as bd tasks)

- **Fork-setup task** (P1, blocks the cred-handling refactor): create `tuxlink-pat` repo; mirror upstream `la5nta/pat`; document build + sync workflow; update tuxlink's `src-tauri/` build to source Pat from the fork. Cameron's call on repo home (personal account vs new org vs vendored subdirectory).
- **Cred-handling refactor task** (P1, blocks Task 6 resume): the first agentic patch against `tuxlink-pat`. Move WL2K password reads from `config.json` to OS keyring. Full build-robust-features pipeline. Pursue upstream PR after fork-side patch ships.
- **v0.0.1 plan amendment task** (P2, can run parallel to the cred refactor): update Tasks 5/6/9/11 specs to reflect the fork + keyring model. Bundle as one AMD-* commit.
- **AppImage system-dep task** (P2): document `secret-service` requirement for the Debian/Ubuntu AppImage target.

These follow-ups are queued as bd issues post-ADR-merge with deps drawn to enforce the sequencing above.
