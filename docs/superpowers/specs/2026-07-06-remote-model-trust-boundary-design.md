# Remote Model Trust Boundary — Require Valid TLS for Non-Loopback Endpoints (Design Spec)

**bd:** tuxlink-qe6ie · **Date:** 2026-07-06 · **Author agent:** gully-esker-mesa
**Status:** approved design (brainstorm signed off by operator 2026-07-06); pending Codex adversarial review → plan → build.

## Goal

Close the trust boundary on the Elmer model-instruction channel. A remote model
backend is an **untrusted instruction source**: Tuxlink ships the conversation to
the model and then **executes the tool-calls the model returns**. When the model
endpoint is reached over an unauthenticated, unencrypted channel (plain HTTP to a
non-loopback host), any on-path device can rewrite the instruction stream and
thereby choose which tools run on the operator's host — RCE-class as the tool
surface grows. The fix is to require that every non-loopback model endpoint be
reached over **valid TLS**, and to **refuse** anything less. Loopback stays a
first-class, plaintext-exempt mode because it shares Tuxlink's trust domain.

This is an appsec / channel-integrity change. It is **not** an amateur-radio /
transmit-authority concern; the transmit gates (arm/taint/RADIO-1) are unrelated
and untouched.

## The reframe that scoped this (settled with operator, 2026-07-06)

- The threat is **untrusted instructions**, not egress. The SSRF egress gate keeps
  our *outbound* requests off the metadata IP; it does nothing about trusting the
  *instructions that come back*. The two are orthogonal and both remain.
- **Cloud (valid TLS + API key) and plain HTTP to an arbitrary host are different
  risk classes.** TLS gives server-authentication + integrity; plain HTTP gives
  neither. The API key is *client→server authorization* (usage/billing — omit it
  and the provider returns 401) that rides **inside** the TLS boundary; it is not
  the boundary.
- **The security boundary is valid TLS**, enforced by Tuxlink refusing plaintext to
  non-loopback hosts — using the OS trust store the normal way. Tuxlink does **not**
  pin certs, does **not** add anything to the system trust root, and does **not**
  ship a far-side component. The earlier explorations of cert-pinning UX and a
  paired far-side "gateway" component are **rejected**: people are bad at cert auth,
  and an app that mutates the OS root store or ships cross-platform endpoints is a
  larger, worse surface than the problem it solves.
- **Scope-minimization / capability-gating of the tool set is out of scope for this
  issue.** It is a blast-radius limiter that only has a job *after* an untrusted
  party is on the channel — it is not the gate. The one residual it addresses (a
  *properly authenticated* box whose model emits a hostile tool-call because it was
  indirectly prompt-injected by content it processed) is **identical for the local
  and cloud backends** and is therefore a backend-agnostic agent-safety concern, not
  a property of the remote channel. It gets its own issue (see Follow-ups).

## Context (verified against shipped code, this worktree)

- **Endpoint validation is a pure, CI-unit-tested accept/reject table.**
  `validate_endpoint(raw, allow_remote)`
  (`src-tauri/tuxlink-agent-frontend/src/endpoint.rs:~180`) already:
  - rejects non-`http`/`https` schemes (`EndpointError::UnsupportedScheme`);
  - **always** rejects link-local / cloud-metadata hosts, even with `allow_remote`
    (`EndpointError::LinkLocalAlwaysRefused`);
  - rejects credentials-in-URL (`EndpointError::UserinfoNotAllowed`);
  - gates non-loopback hosts behind `allow_remote` (`EndpointError::RemoteNotAllowed`);
  - exempts loopback (`127.0.0.0/8`, `::1`, literal `localhost`) from the remote gate.
  `AgentEndpoint::parse` calls it with `allow_remote = true`, so **today a
  non-loopback host over plain `http://` is accepted** (tests:
  `lan_rejected_without_flag` passes only *without* the flag; the config path uses
  the flag). This is the hole.
- **Valid-TLS enforcement is already the default and needs no new code.** There is
  **no `danger_accept_invalid_certs` / accept-invalid anywhere** in
  `tuxlink-agent-frontend`. Both `reqwest::Client::builder()` call sites live in
  `egress.rs` (`build_vetted_client`) and use reqwest's **default** certificate
  verification against the OS trust store. So an `https://` connection is *already*
  validated; the only gap is that we still *allow* `http://` to non-loopback.
- **The egress module is an orthogonal SSRF / DNS-rebind gate** (where the socket is
  allowed to go), not a transport-authenticity gate (`egress.rs`). It pins the
  vetted resolved-IP set, forbids redirects and ambient proxies, and permits both
  public and RFC1918 IPs for Elmer. It stays exactly as-is; the new rule composes
  with it (a non-loopback endpoint must now pass **both** the new https rule **and**
  the existing resolved-IP gate).
- `is_loopback()` classifies by the literal/name, not the resolved IP. A *named*
  non-loopback host is `is_loopback() == false` and so is subject to the https rule;
  the canonical `localhost` / `127.0.0.1` forms remain exempt.

## Operator decisions (locked during brainstorming, 2026-07-06)

1. **Require valid TLS for all remote (non-loopback) endpoints.** Mirror the cloud
   model: `https://` with standard OS-trust-store validation. No pinning, no custom
   trust store, no OS-root manipulation.
2. **Local loopback is the first-class mode.** Most operators run no secondary model
   host; loopback (`http://127.0.0.1:11434/...`) stays plaintext-exempt as
   same-trust-domain. This is the OAuth-2.1 §1.5 loopback carve-out, not a bespoke
   exception.
3. **Document, do not facilitate, the self-hosted-remote path.** An operator who
   runs a remote model box is responsible for presenting a certificate their own
   client OS already trusts (an internal domain CA, or a public cert). Tuxlink does
   not help them mint or install certs and ships no far-side component. If they have
   not secured the channel, remote-native is refused and they use loopback or cloud.
4. **Refuse by default, no escape hatch.** There is no `--skip-verify`, no "trust
   anyway" button, and no self-signed acceptance in the product. "Valid" means the
   presented chain validates against the OS trust store.

## Design

### The rule (one addition to the existing reject table)

Extend `validate_endpoint` with a single composed rule, evaluated after the
existing scheme / link-local / userinfo / remote-gate checks:

> **If the host is not loopback and the scheme is not `https`, reject.**

- Loopback host (`is_loopback() == true`): `http` and `https` both accepted
  (unchanged).
- Non-loopback host + `https`: accepted (subject to the existing `allow_remote`
  gate and, at request time, the egress resolved-IP gate and reqwest TLS
  validation).
- Non-loopback host + `http`: **rejected** with a new operator-facing error.

No TLS code changes: `https` acceptance here relies on reqwest's existing default
validation in `build_vetted_client`. The design's job is purely to *stop allowing
`http` to a remote host*.

### What presents the cert (Ollama does not — this is deployment, not a Tuxlink feature)

Ollama serves **plain HTTP on `:11434` and has no TLS support**; `https://<host>:11434`
against *raw* Ollama is impossible. For a remote endpoint to present valid TLS,
some process the operator runs must own the TLS listener and bind the cert:

- **A reverse proxy / TLS terminator in front of Ollama** — Caddy (auto-HTTPS),
  nginx + certbot, Traefik, or a TLS-terminating tunnel. It listens on a TLS port
  with a cert and forwards cleartext to `127.0.0.1:11434` on the model box; Ollama
  itself stays on loopback. This is the Ollama path.
- **A TLS-capable inference server** — `llama-server` (`--ssl-cert-file` /
  `--ssl-key-file`) or vLLM (`--ssl-certfile` / `--ssl-keyfile`) bind the cert
  themselves. Ollama is not in this group.

Two consequences for the endpoint operators will actually enter:

1. **Usually a *named* host — but bare-IP TLS is permitted, not special-cased.**
   Standard TLS validation matches the cert SAN against whatever you connected to.
   Public CAs will not issue for private IPs, so the *common* realistic endpoint is
   `https://ollama.internal.example/…` (internal DNS + a cert whose SAN matches that
   name — the "internal domain CA" path in decision 3). **But a bare-IP endpoint is
   allowed to work:** a server that presents a cert with a matching **IP-SAN**
   (issued by a CA the operator's OS trusts — an internal CA, or a self-signed root
   they installed) validates fine, and `https://192.168.x.y/…` is then accepted. The
   design does **not** discourage or refuse bare IPs — the pure scheme rule passes
   them through and **reqwest's standard validation is the sole arbiter**. A bare IP
   is refused *only* when no valid cert backs it, which is the same rule applied to
   everything. (People really do run IP-SAN TLS in prod; we neither bless nor block
   the host form — only the transport.)
2. **The port is arbitrary** — whatever the terminator listens on (commonly 443).
   `:11434` in an endpoint URL implies raw Ollama and therefore cleartext; it is not
   the TLS port.

None of this is Tuxlink's to build or facilitate — it is the documented operator
deployment. Tuxlink's contribution remains: require valid TLS, refuse otherwise.

### Error surface

Add `EndpointError::PlaintextRemoteRefused { host }` (or equivalently-named
variant) with an operator-facing message that:

- states the endpoint was refused because a remote model host must use `https`;
- names the host;
- points at the docs section for securing a self-hosted remote model (below).

The message must be actionable and non-scary — the operator most likely typed
`http://` out of habit, or is pointing at a raw LAN Ollama. The remedy is "use
`https://` (see docs)" or "use a loopback / cloud endpoint," not a security alarm.

### Data flow (unchanged shape, one new refusal point)

```
operator sets endpoint
   → AgentEndpoint::parse            (validate_endpoint, allow_remote = true)
       → scheme ∈ {http,https}?      (existing)
       → link-local/metadata?        (existing, always-refuse)
       → userinfo present?           (existing, refuse)
       → non-loopback & !allow_remote? (existing)
       → NON-LOOPBACK & scheme != https?  ← NEW: refuse (PlaintextRemoteRefused)
   → per-turn request via build_vetted_client
       → resolved-IP egress gate     (existing, orthogonal)
       → reqwest default TLS validation (existing; enforces "valid")
```

### Migration (this is a tightening, not a purely additive change)

An operator currently pointing Elmer at `http://<non-loopback>` (e.g.
`http://192.168.1.50:11434/...`) will, after this change, be **refused** at config
parse. Given the audience reality (near-nobody runs a secondary model host, and
those who do can present a real cert), this is acceptable, but the spec commits to:

- a clear refusal message (above) rather than a silent fallback;
- a short **CHANGELOG / release-note** entry describing the tightening and the
  remedy;
- a **docs** section (operator-facing) on securing a self-hosted remote model:
  put a valid cert in front (internal CA already trusted by the client OS, or a
  public cert), or bind the model to loopback and run Tuxlink on the same host.

## Documentation

- Operator docs: "Using a remote (self-hosted) model with Tuxlink" — the valid-TLS
  requirement, the internal-CA / public-cert options, the loopback and cloud
  alternatives, and an explicit statement that Tuxlink does not manage certificates.
- CHANGELOG entry noting the tightening + migration remedy.
- AGENTS.md parity check: this is a code + policy change local to the Elmer endpoint
  validator; verify whether any AGENTS.md summary line references endpoint
  validation and update in the same PR if so (likely no change needed).

## Testing

Pure-function table tests in `endpoint.rs` (the accept/reject table is already the
CI-tested unit surface — extend it):

- `http://192.168.1.50:11434/...` (non-loopback IP + http — the raw-Ollama case)
  → **refused** (`PlaintextRemoteRefused`), where today it is accepted. This is the
  regression lock for the hole.
- `https://model.internal.example/...` (named non-loopback + https — the realistic
  proxy-fronted endpoint) → accepted.
- `http://model.internal.example/...` (named non-loopback + http) → **refused**.
- `https://192.168.1.50/...` (bare-IP + https) → accepted by *this* pure rule (host
  is non-loopback, scheme is https). Whether the request then succeeds is **reqwest's**
  call at connect time: it succeeds if the server presents a valid **IP-SAN** cert
  the OS trusts (a real non-rev-proxy pattern — vLLM / `llama-server` with an internal
  CA), and fails otherwise. The validator enforces the scheme rule *only*; it must not
  special-case, discourage, or block bare IPs — cert matching is reqwest's job.
- `http://127.0.0.1:11434/...`, `http://localhost:8080/...`, `http://[::1]/...`
  (loopback + http) → accepted (unchanged — regression lock for first-class local).
- `https://127.0.0.1/...` (loopback + https) → accepted (unchanged).
- Link-local / metadata over `https` (e.g. `https://169.254.169.254/...`) → still
  refused by the always-refuse rule (the new rule does not relax it).
- Message-shape assertion: the refusal for a plaintext remote names the host and is
  distinct from `RemoteNotAllowed` / `UnsupportedScheme`.

No new integration test is required for TLS validation itself — it is reqwest's
default behavior and is not code we own — but a note in `testing-pitfalls.md` should
record that "valid TLS" is the reqwest default and that adding
`danger_accept_invalid_certs` anywhere would silently reopen this boundary.

## Out of scope (explicit)

- **Capability-gating / tool scope-minimization** — separate cross-cutting
  agent-safety concern; see Follow-ups.
- **The transmit gates** (arm/taint/RADIO-1) — unrelated; untouched.
- **Cert pinning, custom trust stores, OS-root manipulation, a far-side gateway
  component** — rejected during brainstorming.
- **The existing egress SSRF gate** — stays as-is.

## Downstream / follow-ups

- **tuxlink-xnenf re-scopes onto this gate** rather than dying: "native `/api/chat`
  path for non-loopback hosts" becomes "…for non-loopback hosts *that present valid
  TLS*." The `providerKind`-visibility fixes (handoff BUG 2/3 — surface `ProviderKind`
  into `ConfigReadDto`, move num_ctx gating off string `isLoopback`) still ride on
  top and are unaffected by this change.
- **New issue (to file): backend-agnostic agent tool-execution safety** — indirect
  prompt injection / excessive-agency (OWASP LLM08) mitigations applied uniformly to
  local, cloud, and remote backends: least-privilege tool scoping and risk-based
  human confirmation for high-impact tools. Explicitly *not* part of the remote
  trust boundary.
- **Separate review of the existing cloud path** for whether it under-appreciates
  this surface (flagged in the qe6ie investigation; separate scope).

## References

- OAuth 2.1 §1.5 and MCP Security Best Practices: require HTTPS for all non-loopback
  URLs; `http://` acceptable only for loopback. This design is that rule, applied to
  the model endpoint.
- OWASP Top 10 for LLM Applications — LLM08 Excessive Agency (governs the deferred
  capability-gating follow-up, not this issue).
- Prior art for self-hosted remote inference security (informational, not adopted
  here): Ollama-behind-reverse-proxy hardening guides; Tailscale + LM Studio "LM
  Link" (tailnet-as-boundary). Both are operator-side deployment patterns; Tuxlink's
  product contribution is refusing the unauthenticated channel, not facilitating
  these.
