# 3. Pat owns the mailbox; no SQLite in Tuxlink v0.0.1

Date: 2026-05-05
Status: Accepted (amended by [ADR 0011](0011-fork-pat-for-tuxlink.md) — dependency target shifted from upstream `la5nta/pat` to the `tuxlink-pat` fork; the ownership-of-mailbox rule and the no-SQLite-in-tuxlink rule themselves remain operative; **superseded by [ADR 0016](0016-native-b2f-outbound-with-attachments.md) as of 2026-05-30** — native client now owns mailbox; "no SQLite" half still holds.)
Deciders: cameronzucker, lichen (during 2026-04-22 office-hours adversarial review round 2)

## Context

Tuxlink is a UX layer over the Pat Winlink client. Pat (la5nta/pat, MIT-licensed Go binary) handles the Winlink protocol, CMS authentication, telnet sessions, and mailbox storage on its own. Tuxlink wraps Pat as a managed child process and renders a desktop UI on top of Pat's HTTP API.

A natural-seeming optimization is to give Tuxlink its own SQLite database for caching the mailbox locally — better search, offline browsing, indexed reads, etc. The 2026-04-22 design discussion initially included a SQLite-backed mailbox cache for tuxlink.

Codex's round-2 adversarial review flagged this as a divergence problem:

> Two owners of the same data is a recipe for inconsistency. Pat already maintains the canonical mailbox state in its own storage. A tuxlink-side SQLite cache must either (a) refresh from Pat aggressively (in which case Pat IS the source and the cache is a write-through with sync overhead) or (b) accumulate writes locally and reconcile (in which case divergence is inevitable). Either way, a "mailbox cache" before there's a measured performance problem trades a real complexity cost for a hypothetical performance win.

## Decision

In Tuxlink v0.0.1:

- **Pat is the authoritative source of all mailbox state.** Inbox, Sent, Posted, message bodies, headers, attachments — all live in Pat's storage at `$XDG_CONFIG_HOME/pat/`.
- **Tuxlink has NO SQLite database.** No persistent state in tuxlink other than `$XDG_CONFIG_HOME/tuxlink/config.json` (wizard-set user identity) and `$XDG_RUNTIME_DIR/tuxlink/*` (PID files, sockets).
- **Tuxlink reads mailbox state by HTTP-GETting Pat's REST API** on demand. TanStack Query's caching handles short-term in-memory de-duplication of identical requests.

Tuxlink does NOT store, write, or invalidate any mailbox-related data of its own.

## Consequences

**Positive:**
- Single source of truth: Pat's storage. No invalidation logic, no sync errors, no divergence.
- The migration story for users with an existing Pat install is trivial: tuxlink reads their existing mailbox without touching it.
- Pat upstream changes to mailbox format don't require tuxlink-side migrations.
- Reduced attack surface: tuxlink doesn't store credentials, message bodies, or recipient metadata.

**Negative:**
- Every mailbox view requires a Pat HTTP request. For users with thousands of messages, list rendering depends on Pat's HTTP throughput. (Mitigation: react-virtuoso virtualizes the list; only visible rows are fetched.)
- Tuxlink cannot offer features that require local indexing — full-text search, advanced filtering, cross-account search. Those are deferred to v0.1+, conditional on a measured need.
- If Pat is not running, tuxlink's UI cannot show any mailbox content at all (graceful degradation: show "Pat is starting..." placeholder, retry).

## Alternatives considered

- **SQLite write-through cache** (Pat is source, tuxlink mirrors on every fetch): rejected for v0.0.1. Adds 30%+ implementation complexity (cache schema, invalidation triggers, error recovery on partial fetches) for no measured user benefit at the scale Tuxlink targets.
- **SQLite + lazy fetch** (tuxlink stores what it has fetched, pulls misses): rejected. The "stale data" UX is worse than the "always live" UX; Pat is a local process, so HTTP latency is sub-millisecond.
- **Custom binary serialization for mailbox cache**: rejected as over-engineering for v0.0.1.
- **Defer the decision until v0.1** (build v0.0.1 SQLite-free, revisit when there's data on user pain points): selected. This ADR locks in the v0.0.1 posture; v0.1 may revisit with measured evidence.
