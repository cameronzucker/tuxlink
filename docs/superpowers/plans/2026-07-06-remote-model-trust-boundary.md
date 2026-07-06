# Remote Model Trust Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refuse a plain-`http` model endpoint on any non-loopback host, so Tuxlink only ever drives its tool-executing agent loop from a model reached over valid TLS.

**Architecture:** Add one rule to the existing pure, CI-tested accept/reject table in `endpoint.rs` (`validate_endpoint`): a non-loopback host must use `https`. A new `EndpointError::PlaintextRemoteRefused` carries the operator-facing message. No TLS code is written — reqwest's default certificate validation (already in force in `egress.rs`, with no `danger_accept_invalid_certs` anywhere) enforces "valid" at request time; this rule only forbids the plaintext scheme. Loopback stays exempt (same trust domain). The egress SSRF gate is orthogonal and untouched. Host *form* (named vs bare IP) is deliberately not judged — cert matching is reqwest's job, so bare-IP TLS with a valid IP-SAN cert (vLLM / `llama-server` + internal CA) still works.

**Tech Stack:** Rust (`tuxlink-agent-frontend` sub-crate under `src-tauri/`), `url` crate, `thiserror`. Docs in `docs/user-guide/` (Markdown, rendered as in-app Help).

## Global Constraints

- **MSRV is 1.75.** clippy's `incompatible_msrv` is denied; use pre-1.76 idioms only (no `Result::inspect_err` etc.). Exact value: `src-tauri/Cargo.toml` `rust-version`.
- **This Pi does not finish a cold `cargo` build/test locally.** Do NOT expect to run `cargo test` here. Verification is **CI** (`.github/workflows/ci.yml`, `verify` job, amd64 + arm64: `cargo clippy … --all-targets --locked -- -D warnings` + `cargo test … --locked`). After pushing, confirm the CI run whose `headSha` matches the pushed commit reached `conclusion: success` — do not read a bare latest-run status (it latches stale).
- **Cargo invocation needs the manifest path:** `cargo … --manifest-path src-tauri/Cargo.toml` (there is no workspace-root `Cargo.toml`).
- **CHANGELOG.md is release-please-generated** from Conventional Commits (entries from `v0.0.2` onward). Do NOT hand-edit `CHANGELOG.md`. The migration note is delivered via the commit message body + a `BREAKING CHANGE:` footer.
- **Commit discipline:** every commit carries `Agent: gully-esker-mesa` and `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` trailers. Branch is `bd-tuxlink-qe6ie/remote-model-rce-reframe` (this worktree). Conventional-commit types; match type to intent.
- **Do not special-case bare IPs.** The validator gates the scheme only. Whether a bare-IP or self-signed endpoint validates is entirely reqwest's decision at request time.
- **Do not touch** `egress.rs` (the SSRF/DNS-rebind gate) or introduce any `danger_accept_invalid_certs` / `--skip-verify` escape hatch.
- **Spec:** `docs/superpowers/specs/2026-07-06-remote-model-trust-boundary-design.md` is the source of truth for this plan.

---

## File Structure

- **Modify:** `src-tauri/tuxlink-agent-frontend/src/endpoint.rs`
  - Add `EndpointError::PlaintextRemoteRefused { host: String }` variant (in the `EndpointError` enum, ~line 81-113).
  - Add the TLS rule in `validate_endpoint` (~line 177-204).
  - Update the module-level doc (top of file) and the `validate_endpoint` doc comment to state the rule.
  - Flip two existing tests that encode the closed hole; add new table tests. All in the `#[cfg(test)] mod tests` block (~line 304-610).
- **Modify:** `src-tauri/src/config.rs` — fix the stale `ElmerConfig.agent_endpoint` doc comment (~line 1478-1479) that says the endpoint "Must resolve to a loopback address" and points at `LoopbackEndpoint::parse`. Operator endpoints are validated by `AgentEndpoint::parse` (remote-https allowed). (Codex adrev NIT, 2026-07-06.)
- **Modify:** `docs/user-guide/27-settings.md` — add an "AI agent model endpoint" section documenting local / cloud / remote-self-hosted and the valid-TLS requirement.
- **Check (likely no change):** `AGENTS.md` — parity check per CLAUDE.md's AGENTS.md contract.

## Adversarial review dispositions (Codex, 2026-07-06)

A Codex round on the spec+plan found **no TLS-gate bypass and no validation bypass** (every model request goes through `egress::build_vetted_client` with default TLS validation; every operator endpoint string is parsed before use). Verdict `DO-NOT-SHIP-AS-IS` for three fixable items, all folded in below:

- **MED — userinfo ordering.** The new plaintext check sits inside `validate_endpoint`, which runs *before* `AgentEndpoint::parse`'s userinfo check. So `http://user:pass@<remote>` now returns `PlaintextRemoteRefused` instead of `UserinfoNotAllowed`, breaking two existing tests (`agent_endpoint_refuses_userinfo`, `agent_endpoint_refuses_username_only`). **Fix (contained):** flip both tests to `https://` (so they reach the userinfo check), add a precedence-doc test for the `http`+creds+remote case, and comment the deliberate ordering. Not moving userinfo into `validate_endpoint` (that would change `LoopbackEndpoint` behavior — scope creep). Fails closed either way; the message names only the host (no credential leak).
- **LOW — parser-edge tests.** Add a lock test for loopback lookalikes (`localhost.`, `127.0.0.1.evil.com`, `0.0.0.0`, `[::]`) → all `PlaintextRemoteRefused` over http (they are Remote, not Loopback). Guards against future `url`-parser drift.
- **NIT — stale config comment.** Fixed in `config.rs` (File Structure above).

`bare-IP TLS` claim (angle 6) confirmed correct for this repo's native-tls/OpenSSL backend (iPAddress SAN required; textual-IP dNSName SAN insufficient). Spec updated with that precision.

No other call site changes: the production callers in `src-tauri/src/elmer/config_commands.rs` (`AgentEndpoint::parse` at lines 353, 442, 513) already surface `EndpointError` as a string to the UI via `.map_err(|e| e.to_string())`, so the new variant's message reaches the operator with no wiring change. `egress.rs` parses only loopback/https endpoints in its tests, so it is unaffected.

---

### Task 1: Add the TLS rule + `PlaintextRemoteRefused`, flip the hole-locking tests

**Files:**
- Modify: `src-tauri/tuxlink-agent-frontend/src/endpoint.rs` (enum ~81-113; `validate_endpoint` ~177-204; module doc ~1-59; tests ~304-610)
- Test: same file's `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `HostClass` (`Loopback` / `LinkLocalOrMetadata` / `Remote`), `classify_host`, `url::Url` — all existing.
- Produces: `EndpointError::PlaintextRemoteRefused { host: String }` — a new refusal returned by `validate_endpoint` (and therefore by `AgentEndpoint::parse`) when a non-loopback host uses `http`. Distinct from `RemoteNotAllowed` (host is remote and `allow_remote` is false) and `UnsupportedScheme` (scheme is neither http nor https).

**Behavior table (the whole contract):**

| Host class | scheme | `allow_remote` | Result |
|---|---|---|---|
| Loopback | http or https | any | `Ok` (unchanged) |
| LinkLocal/metadata | any | any | `LinkLocalAlwaysRefused` (unchanged) |
| Remote | any | `false` | `RemoteNotAllowed` (unchanged) |
| Remote | `https` | `true` | `Ok` (unchanged) |
| Remote | `http` | `true` | **`PlaintextRemoteRefused` (NEW — was `Ok`)** |

- [ ] **Step 1: Write/flip the tests first (they encode the new contract).**

In `endpoint.rs`, **replace** the existing `lan_accepted_with_flag` test (currently ~line 345-348) with these two:

```rust
    #[test]
    fn lan_http_refused_with_flag() {
        // The hole this issue closes: a non-loopback host over plain http was
        // accepted with the remote opt-in. It is now refused for want of TLS.
        let err = validate_endpoint("http://192.168.1.50:8080/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn lan_https_accepted_with_flag() {
        // Same host over TLS is accepted (reqwest validates the cert at request
        // time; the validator only gates the scheme).
        assert!(validate_endpoint("https://192.168.1.50:8080/v1", true).is_ok());
    }
```

**Replace** the existing `agent_endpoint_accepts_rfc1918` test (~line 493-501) with these two:

```rust
    /// RFC-1918 LAN address over TLS is accepted; is_loopback() returns false.
    /// Bare-IP TLS is permitted, not special-cased — reqwest is the cert arbiter.
    #[test]
    fn agent_endpoint_accepts_rfc1918_https() {
        let ep =
            AgentEndpoint::parse("https://192.168.1.50:8080/v1/chat/completions").unwrap();
        assert!(
            !ep.is_loopback(),
            "RFC-1918 address must not classify as loopback"
        );
    }

    /// RFC-1918 LAN address over plain http is refused (qe6ie TLS rule).
    #[test]
    fn agent_endpoint_refuses_plaintext_rfc1918() {
        let err =
            AgentEndpoint::parse("http://192.168.1.50:8080/v1/chat/completions").unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "expected PlaintextRemoteRefused, got {err:?}"
        );
    }
```

**Add** these new tests to the `mod tests` block (place after `lan_https_accepted_with_flag`):

```rust
    #[test]
    fn remote_named_http_refused_with_flag() {
        // A named non-loopback host over http is refused just like a bare IP.
        let err = validate_endpoint("http://model.internal.example/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn remote_named_https_accepted_with_flag() {
        assert!(validate_endpoint("https://model.internal.example/v1", true).is_ok());
    }

    #[test]
    fn loopback_http_still_accepted_after_tls_rule() {
        // Loopback is exempt from the TLS rule (same trust domain). Regression
        // lock for first-class local operation.
        assert!(validate_endpoint("http://127.0.0.1:11434/v1", false).is_ok());
        assert!(validate_endpoint("http://localhost:11434/v1", false).is_ok());
        assert!(validate_endpoint("http://[::1]:11434/v1", false).is_ok());
    }

    #[test]
    fn link_local_https_still_refused() {
        // The TLS rule does NOT relax the always-refuse link-local/metadata rule.
        let err = validate_endpoint("https://169.254.169.254/v1", true).unwrap_err();
        assert!(
            matches!(err, EndpointError::LinkLocalAlwaysRefused { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn plaintext_remote_refusal_is_distinct() {
        // PlaintextRemoteRefused (remote + http + allow_remote) must not be
        // confused with RemoteNotAllowed (remote + !allow_remote) or
        // UnsupportedScheme (neither http nor https).
        let plaintext = validate_endpoint("http://192.168.1.50/v1", true).unwrap_err();
        let not_allowed = validate_endpoint("http://192.168.1.50/v1", false).unwrap_err();
        let bad_scheme = validate_endpoint("ftp://192.168.1.50/v1", true).unwrap_err();
        assert!(matches!(plaintext, EndpointError::PlaintextRemoteRefused { .. }));
        assert!(matches!(not_allowed, EndpointError::RemoteNotAllowed { .. }));
        assert!(matches!(bad_scheme, EndpointError::UnsupportedScheme(_)));
        // The operator-facing message names the offending host and mentions https.
        let msg = plaintext.to_string();
        assert!(msg.contains("192.168.1.50"), "message must name the host: {msg}");
        assert!(msg.contains("https"), "message must point at https: {msg}");
    }
```

**Also flip the two existing userinfo tests (Codex MED)** — they currently use `http://user:pass@…` (remote http), which now returns `PlaintextRemoteRefused` before the userinfo check. Replace `agent_endpoint_refuses_userinfo` (~line 516-524) and `agent_endpoint_refuses_username_only` (~line 602-609) with:

```rust
    /// Userinfo (user:pass@host) is refused. Uses https so the endpoint clears
    /// the TLS gate and reaches the userinfo check. (A remote *http* endpoint is
    /// refused as plaintext BEFORE userinfo is examined — see
    /// agent_endpoint_plaintext_refused_before_userinfo.)
    #[test]
    fn agent_endpoint_refuses_userinfo() {
        let err = AgentEndpoint::parse("https://user:pass@api.openai.com/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::UserinfoNotAllowed { .. }),
            "expected UserinfoNotAllowed, got {err:?}"
        );
    }

    /// Username-only (no password) is also refused. https so it reaches the check.
    #[test]
    fn agent_endpoint_refuses_username_only() {
        let err = AgentEndpoint::parse("https://user@api.openai.com/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::UserinfoNotAllowed { .. }),
            "username without password must also be refused; got {err:?}"
        );
    }
```

**Add the precedence-doc test and the loopback-lookalike lock test (Codex MED + LOW)** — place in the `mod tests` block:

```rust
    /// Deliberate precedence: a remote http endpoint WITH userinfo reports the
    /// plaintext refusal first (validate_endpoint runs before the userinfo check
    /// in AgentEndpoint::parse). Still fails closed; the message names only the
    /// host, never the credentials.
    #[test]
    fn agent_endpoint_plaintext_refused_before_userinfo() {
        let err = AgentEndpoint::parse("http://user:pass@192.168.1.50/v1").unwrap_err();
        assert!(
            matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
            "remote http+creds must report plaintext refusal first; got {err:?}"
        );
        assert!(!err.to_string().contains("pass"), "creds must not leak: {err}");
    }

    /// Hosts that superficially resemble loopback but are NOT must classify as
    /// Remote and be refused over plain http — they cannot ride the loopback
    /// plaintext exemption. Locks the boundary against future url-parser drift.
    #[test]
    fn loopback_lookalikes_refused_over_http() {
        for raw in [
            "http://localhost./v1",         // trailing dot: not literal "localhost"
            "http://127.0.0.1.evil.com/v1", // loopback IP as a subdomain label
            "http://0.0.0.0:8080/v1",       // unspecified v4, not loopback
            "http://[::]:8080/v1",          // unspecified v6, not loopback
        ] {
            let err = validate_endpoint(raw, true).unwrap_err();
            assert!(
                matches!(err, EndpointError::PlaintextRemoteRefused { .. }),
                "{raw} must be refused as plaintext remote; got {err:?}"
            );
        }
    }
```

Note: the existing `userinfo_check_runs_before_remote_accept` test (~line 582) uses a **loopback** creds URL (`http://u:p@127.0.0.1`), which is exempt from the TLS rule, so it still returns `UserinfoNotAllowed` — leave it unchanged.

- [ ] **Step 2: Add the `PlaintextRemoteRefused` variant.**

In the `EndpointError` enum, immediately after the `RemoteNotAllowed` variant (after ~line 97), add:

```rust
    /// A non-loopback host reached over plain `http`. A remote model endpoint is
    /// an untrusted-instruction channel: Tuxlink executes the tool-calls the
    /// model returns, so over plaintext an on-path device can rewrite them
    /// (qe6ie trust boundary). Require TLS for any non-loopback host. reqwest's
    /// default validation enforces a *valid* certificate at request time; this
    /// variant only refuses the plaintext scheme. Host form (named vs bare IP)
    /// is not judged here — cert matching is reqwest's job.
    #[error(
        "endpoint host `{host}` uses plain http; a remote (non-loopback) model \
         endpoint must use https. Point Tuxlink at an https:// URL backed by a \
         valid TLS certificate, or use a local (loopback) or cloud endpoint. \
         See Help > Settings > 'AI agent model endpoint'."
    )]
    PlaintextRemoteRefused { host: String },
```

- [ ] **Step 3: Add the rule in `validate_endpoint`.**

Replace the scheme match and the `HostClass::Remote` arm. The current function body (~line 178-204) becomes:

```rust
    let url = Url::parse(raw).map_err(|e| EndpointError::Unparseable(format!("{raw}: {e}")))?;

    // Validate scheme and remember whether it is TLS. The scheme is constrained
    // to http|https here; the TLS rule below applies only to non-loopback hosts.
    let is_https = match url.scheme() {
        "https" => true,
        "http" => false,
        other => return Err(EndpointError::UnsupportedScheme(other.to_string())),
    };

    let host = url
        .host()
        .ok_or_else(|| EndpointError::MissingHost(raw.to_string()))?;

    match classify_host(&host) {
        HostClass::Loopback => Ok(url),
        HostClass::LinkLocalOrMetadata => Err(EndpointError::LinkLocalAlwaysRefused {
            host: host.to_string(),
        }),
        HostClass::Remote => {
            if !allow_remote {
                return Err(EndpointError::RemoteNotAllowed {
                    host: host.to_string(),
                });
            }
            // qe6ie trust boundary: a non-loopback model endpoint is an
            // untrusted-instruction channel, so it MUST use TLS. Loopback is
            // exempt (handled above): same trust domain, the OAuth-2.1 loopback
            // carve-out. Host form is deliberately not judged — reqwest decides
            // whether the cert validates at request time.
            if !is_https {
                return Err(EndpointError::PlaintextRemoteRefused {
                    host: host.to_string(),
                });
            }
            Ok(url)
        }
    }
```

- [ ] **Step 4: Update the doc comments to state the rule.**

At the end of the module-level doc block (after ~line 59, before `use std::net…`), add:

```rust
//! ## qe6ie: TLS required for non-loopback hosts
//!
//! A non-loopback model endpoint is refused unless the scheme is `https`
//! (`EndpointError::PlaintextRemoteRefused`). The model is an
//! untrusted-instruction channel — Tuxlink executes the tool-calls it returns —
//! so a plaintext, MITM-rewritable channel to a non-loopback host is refused by
//! default. "Valid TLS" itself is reqwest's default certificate validation at
//! request time (see `egress::build_vetted_client`); this module only forbids
//! the plaintext scheme and never judges host form (named vs bare IP). Loopback
//! is exempt (same trust domain).
```

In the `validate_endpoint` doc comment (the `///` block ~line 170-176), add one bullet:

```rust
/// * A non-loopback host over plain `http` is refused (`PlaintextRemoteRefused`);
///   remote endpoints must use `https`. Loopback is exempt.
```

- [ ] **Step 4b: Fix the stale `ElmerConfig` doc comment (Codex NIT).**

In `src-tauri/src/config.rs`, replace the `agent_endpoint` doc comment (~line 1478-1479):

```rust
    /// The chat-completions endpoint URL. Operator-configured endpoints are
    /// validated by `AgentEndpoint::parse`: a loopback host (`127.0.0.0/8` /
    /// `::1` / `localhost`) may use `http`; a non-loopback host MUST use `https`
    /// (qe6ie trust boundary); link-local/metadata ranges and credentials-in-URL
    /// are always refused. The default is loopback.
    ///
    /// Default: local Ollama (`http://127.0.0.1:11434/v1/chat/completions`).
    pub agent_endpoint: String,
```

- [ ] **Step 5: Verify in CI (the Pi cannot cold-build cargo).**

Commit (Step 6) and push, then confirm the `verify` job for the pushed `headSha` is green on both arches. Do not run `cargo test` locally — it will not finish on this Pi. Expected: `clippy` clean (no MSRV-1.76 idioms introduced; the change uses only `match`, `!=`, and struct construction), and all `endpoint.rs` tests pass, including the flipped `lan_http_refused_with_flag` / `agent_endpoint_refuses_plaintext_rfc1918` and the new table tests.

Command to check CI after push (match SHA):
```bash
gh run list --branch bd-tuxlink-qe6ie/remote-model-rce-reframe --limit 5 \
  --json headSha,status,conclusion,workflowName
# Confirm the run whose headSha == the pushed commit has conclusion=success.
```

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/tuxlink-agent-frontend/src/endpoint.rs src-tauri/src/config.rs
git commit -F - <<'EOF'
fix(elmer)!: require valid TLS for non-loopback model endpoints

A remote model backend is an untrusted-instruction source: Tuxlink executes the
tool-calls the model returns, so a plain-http channel to a non-loopback host lets
an on-path device rewrite which tools run on the host (RCE-class as the tool
surface grows). validate_endpoint now refuses a non-loopback host unless the
scheme is https (new EndpointError::PlaintextRemoteRefused). Loopback stays
exempt (same trust domain). No TLS code is added: reqwest's default validation
already enforces a valid certificate, and host form (named vs bare IP) is not
judged — bare-IP TLS with a valid IP-SAN cert still works. Egress SSRF gate
untouched. Spec: docs/superpowers/specs/2026-07-06-remote-model-trust-boundary-design.md

BREAKING CHANGE: An Elmer/agent endpoint set to http://<non-loopback> (e.g.
http://192.168.1.50:11434/...) is now refused. Use an https:// URL backed by a
valid certificate, a loopback endpoint, or a cloud endpoint. See the user guide
"Settings > AI agent model endpoint".

Agent: gully-esker-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
git push origin bd-tuxlink-qe6ie/remote-model-rce-reframe
```

---

### Task 2: Operator documentation + AGENTS.md parity

**Files:**
- Modify: `docs/user-guide/27-settings.md` (add a section; the error message in Task 1 references it)
- Check: `AGENTS.md` (parity check per CLAUDE.md; likely no change)

**Interfaces:**
- Consumes: the `PlaintextRemoteRefused` message from Task 1 (which names "Settings > 'AI agent model endpoint'"). The doc section title must match that reference.

- [ ] **Step 1: Add the settings-doc section.**

In `docs/user-guide/27-settings.md`, add a new `##` section (place it after the `## Connection settings` block, before `## Pending inbound review`). Use the file's existing formal, present-indicative voice (no first person):

```markdown
## AI agent model endpoint

Tuxlink's optional AI agent (Elmer) talks to a model over an HTTP endpoint set in
**Tools → Connect an AI Agent…**. The agent is the model: Tuxlink sends the
conversation to the endpoint and then executes the tool-calls the model returns.
The endpoint is therefore a trusted control channel, and its transport is gated
accordingly.

- **Local (default, recommended).** A loopback endpoint such as
  `http://127.0.0.1:11434/v1/chat/completions` (a local Ollama or llama.cpp
  server) runs in the same trust domain as Tuxlink and is accepted over plain
  HTTP. This is the first-class path; most operators need nothing else.
- **Cloud.** A provider endpoint such as `https://api.openai.com/v1/chat/completions`
  is reached over TLS. An API key (stored in the OS keyring, never in the URL)
  authorizes usage; TLS authenticates the provider.
- **Remote self-hosted.** A model on another machine on the network **must** be
  reached over `https` with a certificate the running machine already trusts.
  Plain `http` to a non-loopback host is refused: an on-path device could rewrite
  the tool-calls Tuxlink executes. Front the model with a TLS terminator (Caddy,
  nginx, Traefik) or run a server that serves its own certificate (vLLM,
  `llama-server` with `--ssl-cert-file`), issued by an internal CA the client
  machine trusts. Tuxlink does not issue or install certificates and does not
  accept self-signed certificates that no trusted CA vouches for. An operator who
  has not secured the channel uses a local or cloud endpoint instead.

A refused endpoint reports why (unsupported scheme, non-loopback over plain http,
link-local/metadata range, or credentials in the URL). Correct the endpoint and
retry.
```

- [ ] **Step 2: AGENTS.md parity check.**

```bash
grep -ni "endpoint\|SEC-5\|loopback\|elmer" AGENTS.md
```
If a summary line describes the endpoint-validation policy in a way this change makes inaccurate, update it in this commit. If AGENTS.md does not summarize endpoint validation (expected), no change — the substantive rule lives in the spec + code per the propagation contract.

- [ ] **Step 3: Commit.**

```bash
git add docs/user-guide/27-settings.md
# add AGENTS.md too only if Step 2 required a change:
# git add AGENTS.md
git commit -F - <<'EOF'
docs(user-guide): document the AI agent model endpoint transport rules

Add a Settings section covering the three endpoint modes (local loopback,
cloud TLS, remote self-hosted) and the valid-TLS requirement for non-loopback
hosts introduced by the qe6ie trust boundary. The PlaintextRemoteRefused error
message points operators here.

Agent: gully-esker-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
git push origin bd-tuxlink-qe6ie/remote-model-rce-reframe
```

- [ ] **Step 4: Verify docs lint (pre-push hook) passed.** The `lint:docs` hook runs on push (requires `pnpm install` first in a fresh worktree). Confirm the push succeeded; if the hook blocked on a broken link, fix the referenced link and re-push.

---

## Self-Review

**1. Spec coverage:**
- "The rule" (non-loopback + !https → reject) → Task 1 Step 3. ✓
- `PlaintextRemoteRefused` error + operator message (names host, mentions https, points at docs) → Task 1 Step 2 + the `plaintext_remote_refusal_is_distinct` message assertions. ✓
- "No TLS code changes; reqwest default validation" → no egress/TLS task exists by design; Global Constraints forbid touching it. ✓
- "Bare-IP TLS not special-cased" → `agent_endpoint_accepts_rfc1918_https` + `lan_https_accepted_with_flag` (bare-IP https accepted). ✓
- Migration (tightening; refuse existing http remote configs) → `BREAKING CHANGE:` footer in Task 1 Step 6; the two flipped tests lock the behavior change. ✓
- Testing list (http IP refused, https named accepted, http named refused, loopback http accepted, link-local https still refused, message distinctness) → Task 1 Step 1. ✓
- Documentation (operator docs; AGENTS.md parity) → Task 2. ✓
- CHANGELOG note → delivered via commit footer (release-please owns CHANGELOG); Global Constraints. ✓
- Out-of-scope (capability-gating, egress gate, arm/taint) → no tasks touch them. ✓

**2. Placeholder scan:** No TBD/TODO/"add error handling"/"similar to". All code is complete and exact. ✓

**3. Type consistency:** `PlaintextRemoteRefused { host: String }` is defined once (Step 2) and matched identically in every test and in `validate_endpoint` (Step 3). `is_https` is a local `bool`. `EndpointError::RemoteNotAllowed` / `UnsupportedScheme` / `LinkLocalAlwaysRefused` names match the existing enum. The doc-referenced Help title "AI agent model endpoint" matches the Task 2 section heading and the error-message string. ✓
