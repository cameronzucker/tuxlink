# Handoff — remote-model backend reframed as an RCE-class trust boundary (tuxlink-qe6ie)

- **Agent:** poplar-oriole-butte
- **Date:** 2026-07-06
- **Scope:** short, security-sensitive session. Started tuxlink-xnenf (remote native Ollama) via brainstorming; the operator surfaced a threat-model reframe that supersedes the original framing. All work this session was **read-only investigation + tracker updates**. No code changed.

## The one thing the next session must not miss

**tuxlink-xnenf was scoped as an SSRF-egress-allowlist problem. That is the wrong security frame.** The real risk is that a **remote model backend is an untrusted _instruction_ source** — Elmer's "agent" IS the model, and Tuxlink executes the tool-calls the model returns. A remote, unauthenticated/plain-HTTP model channel means a rogue host or an on-path MITM decides which tools run on our host = **RCE-class** (and grows to literal RCE as the tuxlink-mcp hands-off execution harness adds file/process tools). The SSRF egress gate is **orthogonal**: it keeps our outbound requests off the metadata IP but does nothing about trusting the instructions that come back.

New blocker issue **tuxlink-qe6ie** (P1) holds the full grounded write-up + design direction and now **blocks tuxlink-xnenf**. Do the qe6ie trust-boundary design FIRST.

## Threat-model reframe (operator, 2026-07-06 — take these as settled)

- The operator explicitly said: **stop thinking in amateur-radio terms**; the concern is RCE / unauthenticated remote control of the host, not transmit/Part-97.
- Two dodges the operator rejected — **do not reuse them**:
  1. **arm/taint/RADIO-1 guard transmit authority, not host integrity** — irrelevant to RCE.
  2. **"cloud models already drive the tool loop" is not a license** — TLS-to-known-provider ≠ plain-HTTP-to-self-hosted-MITMable box; never justifies a strictly-worse variant. (Corollary: the existing **cloud** path may already under-appreciate this surface — worth its own review, separate scope.)

## Grounded investigation (verified on `origin/main`, read-only)

- **MCP server is a LOCAL Unix-domain socket ONLY** — `src-tauri/tuxlink-mcp-core/src/transport_uds.rs`: `0600`, uid-owned, umask `0o077`, single-session, refuses group/world-writable parent. **No TCP/SSE/HTTP MCP listener anywhere.** The standalone `tuxlink-mcp` binary (`src-tauri/tuxlink-mcp/src/main.rs`) is a dumb stdio↔UDS byte-pump for local `claude mcp`; opens no port.
- **Enumerated every network bind in the src-tauri Rust tree** — all loopback or UDS. `forms/http_server.rs:195` → `127.0.0.1:0`; `managed_direwolf` → `127.0.0.1`; gpsd is a client. **No `0.0.0.0`, no routable inbound listener.** ⇒ there is **no separate inbound hole**; the RCE surface **is** the model tool-execution loop.
- **Current MCP tool surface** (`tuxlink-mcp-core/src/router.rs`) is read-only-ish: `server_info`, `backend_status`, `modem_get_status`, `vara_status`, `position_status`, `platform_info`, `rig_status`, `get_wizard_completed`, `p2p_peer_password_status`, `mailbox_list`/`message_read` (these two **taint** the session), `user_folders_list`. Even at this surface, a MITM on the model channel siphons full conversation context (mailbox contents, position, backend state) and can drive disclosure. Recent commits show the surface is **growing** (Plan 3 hands-off execution harness).
- **The `is_loopback` gate at `src-tauri/src/elmer/provider.rs:246` is a provider-SELECTION gate**, not an egress gate. `build_vetted_client` (`src-tauri/tuxlink-agent-frontend/src/egress.rs`) runs on BOTH branches and already **permits** public/RFC1918/ULA/CGNAT IPs for Elmer, denying only SSRF-magnet ranges (loopback-without-opt-in, `169.254/16` metadata, link-local, multicast). So the operator's tailnet Ollama is **already reachable via compat today** — it just doesn't get the native `/api/chat` path (num_ctx + context meter).

## Design direction for qe6ie (open — decide with operator next session)

Treat the remote model as an **untrusted instruction source** and require **both**:
1. **Authenticated + encrypted transport** — real TLS to the remote, OR restrict "remote native" to a Tailscale host and treat the WireGuard tunnel as the auth+crypto boundary (plain-HTTP-over-tailnet = encrypted+peer-authenticated; plain-HTTP-over-LAN refused).
2. **Capability-gate remote-driven sessions** — constrained/allowlisted tool set; powerful execution-harness tools stay **local-model-only**.

**Open question to put to the operator first:** is a remote model backend even worth this surface vs keeping the agent loop local-model-only? The answer scopes everything downstream.

## The xnenf plumbing (already mapped — rides on top of whatever qe6ie settles)

Backend architecture is fully traced; capture so it isn't re-derived:
- `ElmerConfig` (`src-tauri/src/config.rs:1477`) — add a `#[serde(default)]` bool (e.g. `treat_as_native_ollama`); include it in `is_default()`.
- Thread it through `ElmerProvider::new_vetted` → `new_vetted_with_resolver_and_probe` (`provider.rs`) as `should_probe_native = is_loopback || opted_in`. **Leave `build_vetted_client`, the deny-set, and `allow_loopback = endpoint.is_loopback()` UNCHANGED** — a named remote host still cannot rebind to `127.0.0.1`.
- `ProviderKind` is **already computed** in that constructor — surface it into `ConfigReadDto` (`config_commands.rs:476`) to fix **BUG 3** (frontend-predicts-with-string-`isLoopback` vs backend-probes mismatch) + render a "Provider: Ollama (native)" badge.
- **BUG 2 (num_ctx gating):** frontend `ElmerPane.tsx:616/781` + `GetKeyCard.tsx:200` gate num_ctx on string `isLoopback`; move them onto the authoritative `providerKind` from `ConfigReadDto`.
- **Test gap** to close: no end-to-end test that a provider emitting nothing → meter absent (the isolated ContextMeter test uses a stand-in wrapper).
- **Host-designation UX** was mid-decision when we pivoted: dedicated "Remote Ollama (your network)" tile (recommended) vs Custom-tile + checkbox vs both. Defer until qe6ie settles the trust model — the UX must carry the transport-security posture (e.g. tailnet-only warning).

## Original workstreams still pending (unchanged from delta-basil-fen 2026-07-06 handoff)

- **tuxlink-cnz5o** — PR #1019, CI green both arches, wire-walked. **Mergeable on operator say-so.** Live worktree `worktrees/bd-tuxlink-cnz5o-sim-harness-poc`. ⚠️ DEAD worktree `worktrees/bd-tuxlink-cnz5o-tuxlink-as-sim-harness` still holds kingfisher-cove-yew's **uncommitted staged edit to `docs/notes/2026-07-05-choosing-a-distillation-teacher.md` (192+/146−)** — rescue before disposing (ADR 0009).
- **tuxlink-jfpj2** (P0, Elmer Stop/OOM) — root-caused: Ollama slow to abort on disconnect; Stop looks done, a repeat Send **stacks** a second gen → OOM. Fix direction (not implemented): bound `num_predict` + block a new Send while a gen is in-flight (poll `/api/ps`); operator checks `ollama --version` on the Framework. **Do NOT re-run cnz5o A/Bs (done).**
- **tuxlink-lubim** (tile-switch stale fields) — FIXED, PR #1022 (commit 97a18386).
- **tuxlink-1p2as** — resolve via xnenf (now gated behind qe6ie).

## Branch / worktree / push state

- **This session:** worktree `worktrees/bd-tuxlink-qe6ie-remote-model-rce-reframe`, branch `bd-tuxlink-qe6ie/remote-model-rce-reframe` (off `origin/main`), claims tuxlink-qe6ie. This handoff is committed here + pushed.
- **Main checkout:** operator state (`bd-tuxlink-ant8s/ardop-connect-fixes`, 88 uncommitted) — **untouched.**
- **bd:** created tuxlink-qe6ie (P1, blocks xnenf); added dep edge; reframed xnenf notes. Pushed via `bd dolt push`.
- No processes started; nothing on the Framework or R2. No code changed this session.

## Next-session starting prompt

```
Continue Tuxlink. Last session (poplar-oriole-butte) was a security reframe:
tuxlink-xnenf (remote native Ollama) is NOT an SSRF-egress problem — a remote
model backend is an untrusted INSTRUCTION source (RCE-class), because Tuxlink
executes the tool-calls the model returns. New blocker tuxlink-qe6ie (P1) holds
the full grounded investigation.

FIRST: read the handoff before anything —
  dev/handoffs/2026-07-06-poplar-oriole-butte-remote-model-rce-reframe.md
Then `bd show tuxlink-qe6ie`.

PRIMARY TASK: with the operator, DESIGN the trust boundary in qe6ie (brainstorm,
do not code yet). The open question to settle first: is a remote model backend
worth the RCE surface at all vs keeping the agent loop local-model-only? Then
decide transport (TLS or Tailscale-only) + capability-gating of remote-driven
sessions. Frame it as appsec/RCE, NOT amateur radio — the operator was explicit.
xnenf plumbing is already mapped in the handoff and rides on top of whatever
qe6ie settles.

Do NOT reuse the rejected dodges (arm/taint/RADIO-1; "cloud already does it").
Original pending work (cnz5o PR #1019 mergeable, jfpj2 P0) is in the handoff.
```
