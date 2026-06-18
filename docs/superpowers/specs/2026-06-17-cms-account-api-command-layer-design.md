# CMS account-API command layer — design

**Date:** 2026-06-17
**Status:** approved (design, v2); implementation pending a valid access key for LIVE use
**Scope:** sub-project 0 of the CMS account-lifecycle expansion (tuxlink-vfb3 follow-on).

## Revision note (v2.1)

Second Codex adversarial round (2026-06-17, agent `tamarack-slate-fern`) on the
corrected wire encoding, plus a read-only metadata probe of the live ServiceStack
DTOs (`api.winlink.org/types/typescript`, no auth required). Folded in:

1. **`account_validate_password` contract verified + built.** `AccountPasswordValidate`
   (route `/account/password/validate`, POST-only) takes `Callsign`+`Password` and its
   response is **envelope-only** (`AccountPasswordValidateResponse` extends the bare
   `WebServiceResponse` — no payload field). The earlier "validation code" payload
   assumption was wrong: the result is the ServiceStack verdict (success = correct;
   HTTP-400 `ResponseStatus` = incorrect/no-account). Implemented as a typed
   `PasswordValidation { Valid | Invalid{code,message} }`; a server rejection is the
   *answer* (Invalid), while transport/config/access-key failures stay `Err` so the UI
   never shows "password invalid" for an unreachable server.
2. **Fail closed on malformed `ResponseStatus` (Codex P2).** A present-but-malformed
   `ResponseStatus` (non-object, non-string `ErrorCode`, or a non-empty `Errors[]` whose
   first entry lacks a usable code) is now a transport error, never a silent success —
   closing a path where an odd HTTP-200 body could trigger an unwanted keyring write.
   A nested `Errors[]` carrying the real code (top-level empty) is classified, not dropped.
3. **`UnknownOutcome` for in-flight mutation failures (Codex P1).** A timeout (or a lost
   response body) now maps to `UnknownOutcome` rather than `Network`, so a mutation —
   `account_remove` especially — never falsely reports "nothing happened"; a connection
   refused / DNS / TLS failure (definitely pre-send) stays `Network`. `Rejected` gained a
   `code` field to carry the machine-readable `ResponseStatus.ErrorCode`.
4. **Callsign grammar tightened (Codex P2).** The has-a-digit heuristic accepted tactical
   labels (`RELAY1`, `EOC1`, `TEST123`); replaced with a real amateur-callsign grammar
   (1–2 char prefix incl. digit-led, area digit, 1–4 letter suffix) so a tactical/garbage
   string is never sent as `Callsign` to a full-account mutation.
5. **No public `/account/remove` route** in the live metadata (only `/account/tactical/remove`),
   consistent with the decompile's plain-remove being privilege-gated/hidden — reinforces
   keeping delete UNWIRED until the issued key proves it invocable. Also confirmed: "blocked"
   is a separate op (`/account/lockedOut/get`), and the API enforces **password length 6–12**
   (a client-side validation rule for the wizard). `{}` on HTTP-200 is a legitimate VOID-op
   success (Codex confirmed the spec's earlier "bare `{}` = transport error" wording was the
   wrong part, not the code).

The raw round-2 transcript is local-only (`dev/adversarial/`, gitignored).

## Revision note (v2)

v1 derived the wire contract from the Winlink Express 1.8.2.0 decompile. A Codex
adversarial round plus a live probe of `api.winlink.org` (CMS v5.0.9649) proved the
decompile contract is **stale** in three ways, all corrected here:

1. The auth parameter is **`Key`** (uniform across every op), not the decompile's
   per-endpoint `WebServiceAccesscode`/`WebServiceAccessCode`. The per-endpoint
   casing complication is gone.
2. The success/error envelope is **ServiceStack-style**: payload fields are
   top-level; errors live in `ResponseStatus { ErrorCode, Message, Errors[] }`;
   there is **no `HasError` field** and the error text is **`Message`**, not
   `ErrorMessage`. Success ⇔ empty/absent `ResponseStatus.ErrorCode` (HTTP 200);
   errors return HTTP 400.
3. **The shared WLE 1.8.2.0 access code is REJECTED by the current server**
   (`InvalidAccessKey`, HTTP 400) — verified with the real code value. The whole
   account API is therefore auth-blocked until Tuxlink holds a **Tuxlink-issued
   access key** (the sanctioned path: keys are issued per-application by a Winlink
   administrator — the same CMS team gating prod approval). Republishing WLE's key
   was never the plan and is now doubly invalid (stale + rejected).

**Consequence for the already-shipped `cms_password_change`:** it is built to the
v1 (decompile) contract and would fail live on BOTH the wrong auth param and the
wrong response parse. It is corrected to the v2 contract as part of this work
(see "Shipped-path correction" below). Nothing is user-visible-broken today (the
feature is access-key-gated and was never live-exercised).

**Build posture (operator decision 2026-06-17):** fix the contract now and build
sub-project 0 **offline-correct** with full unit-test coverage; it stays
access-key-gated via the `TUXLINK_WINLINK_ACCESS_CODE` env var. LIVE validation is
deferred until a valid Tuxlink-issued key exists. The offline tests need no real
key; only the operator's live integration check does.

**Open assumption:** the issued key is treated as a **static per-application code**
(env-injectable, matching this architecture), per the design-note framing. If it
turns out to be a **per-session token** obtained via a login handshake, sub-project
0's command shape changes and this spec is revisited before live use.

## Context

`tuxlink-vfb3` shipped the *update* operation of the Winlink account lifecycle —
changing a known CMS password — reachable from Settings → Winlink Account. A
wire-walk against the operator's definition-of-done flows (full account
create/read/update/delete, in-app account creation during onboarding, and
forgot-password recovery) showed those flows require the rest of the account
lifecycle, which does not yet exist.

The Winlink account lifecycle is a separate HTTPS REST/JSON API at
`https://api.winlink.org` — **not** the telnet CMS (`:8772`/`:8773`) and not the
secure-login challenge. Every operation is one `application/x-www-form-urlencoded`
POST to `/<path>?format=json`.

### The full expansion (for context; only sub-project 0 is specified here)

0. **Backend account-API command layer** — this document.
1. Wizard in-app account creation (mandatory recovery email).
2. Status-bar identity-dialog forgot-password recovery.
3. Settings → Winlink Account full management (set recovery email; delete behind a typed-confirmation gate).

Build order 0 → 1 → 2 → 3; the backend foundation unblocks every UI surface and
needs no mockups. Sub-projects 1–3 each get their own mockup-driven design pass.

## The verified wire contract (live, CMS v5.0.9649)

**Request:** form-encoded POST to `https://api.winlink.org/<path>?format=json`.
Every op carries **`Key=<access key>`** plus its op-specific params. TLS cert
validation stays on. (The decompile's `Requester`/`WebServiceAccesscode` params are
not part of the current contract; `Key` is the sole auth param.)

**Response envelope (ServiceStack):**

```jsonc
// success (HTTP 200): payload fields top-level, ResponseStatus empty or absent
{ "CallsignExists": true, "Blocked": false }                 // e.g. AccountExists
// error (HTTP 400):
{ "ResponseStatus": { "ErrorCode": "InvalidAccessKey",
                      "Message": "Invalid access key for this operation",
                      "Errors": [ { "ErrorCode": "...", "FieldName": "Key",
                                    "Message": "...", "Meta": { } } ] } }
```

- **Success ⇔** `ResponseStatus` absent OR `ResponseStatus.ErrorCode` empty.
- **Error ⇔** `ResponseStatus.ErrorCode` non-empty; surface `ResponseStatus.Message`
  verbatim; HTTP status is 400.
- **There is no `HasError` field.** Any parser requiring one is wrong.
- **Safety guard (preserved from the shipped P1):** a body that is not valid JSON,
  or that on an HTTP-200 lacks BOTH a recognized payload field AND a well-formed
  (even if empty) `ResponseStatus`, is a **transport error, never a silent
  success** — so a bare `{}`, a proxy error page, or a future shape change can
  never be read as a confirmed mutation that triggers a keyring write.

## Commands

All commands take a raw callsign, **reject tactical/hyphenated inputs** (these are
full-account ops; see normalization below), normalize to the base callsign
(uppercase), and form-POST with `Key` + op params.

| Command | Path | Op params (+ `Key`) | Returns | Keyring on success |
|---|---|---|---|---|
| `account_create` | `/account/add` | `Callsign`, `Password`, `RecoveryEmail` | `()` | write password (atomic) |
| `account_exists` | `/account/exists` | `Callsign` | `{ exists, blocked }` | none |
| `account_validate_password` | `/account/password/validate` (POST) | `Callsign`, `Password` | `Valid` / `Invalid{code,message}` (envelope-only; no payload field) | none |
| `account_set_recovery_email` | `/account/password/recovery/email/set` | `Callsign`, `Password`, `RecoveryEmail` | `()` | none |
| `account_send_recovery` | `/account/password/send` | `Callsign` | `()` | none |
| `account_remove` | `/account/remove` | `Callsign` | `()` | **delete** the keyring entry |
| `cms_password_change` *(shipped; corrected)* | `/account/password/change` | `Callsign`, `OldPassword`, `NewPassword` | `()` | write new password (atomic) |

**`RecoveryEmail` is a direct param of `/account/add`** (confirmed against live
metadata), so account creation is a **single atomic call** — no create-then-set
two-step with a bad partial state. The wizard's mandatory-recovery-email rule (a
hamexandria support-burden decision: missing recovery email → manual reset = a
large fraction of support posts) is satisfied by requiring `RecoveryEmail`
non-empty at the `account_create` boundary.

### `account_remove` is privilege-gated — do not ship delete until live-proven

`/account/remove` exists in the WLE decompile, but its live metadata returns **403
(privileged)** while `AccountTacticalRemove` returns 200. The client access key may
**not** be authorized to invoke it. Therefore:

- The command is implemented + unit-tested, but **delete must not be wired into any
  UI (sub-project 3) until an operator live-test proves the issued key can actually
  invoke `/account/remove`.**
- Partial-state handling: after a server-confirmed removal, a keyring-delete failure
  is `KeyringDesync` (the account is gone; the stale local secret is the lesser,
  recoverable problem). A network timeout *after* the request is an **`UnknownOutcome`**
  — the caller reconciles via `account_exists` before deciding whether to delete the
  local credential, rather than assuming either result.

## Shared infrastructure

Extract from the existing `change_password` body a single helper used by every op:

```
post_account_form(path: &str, params: Vec<(&str, String)>) -> Result<Value, AccountApiError>
```

- Builds the form body (auto-appends `Key` from `access_code()`), POSTs with the
  existing reqwest client + 30s timeout, TLS on.
- Parses the ServiceStack envelope: maps a non-empty `ResponseStatus.ErrorCode` to
  `Rejected{ code, message }` (message = `ResponseStatus.Message`), with
  `InvalidAccessKey` mapped to a distinct `AccountApiError::InvalidKey` so the UI
  can tell the operator the access key is missing/invalid (vs a normal rejection).
- Applies the safety guard above (unparseable / payload-less-and-ResponseStatus-less
  body ⇒ `Network` transport error, never success).
- Returns the parsed `Value` (or a typed payload struct) so read ops read their
  top-level fields **through** the guard, never by reparsing the raw body. Read-op
  unit tests include a "ResponseStatus-only / payload-missing" case that must fail
  closed.

`cms_password_change` is refactored to call `post_account_form`, giving one
parse/guard implementation for all ops.

### Per-base-callsign serialization (mutating ops)

Server-confirm-before-keyring is necessary but not sufficient: interleaved
mutations on the same callsign can desync (e.g. `remove(N0CALL)` server-succeeds →
`create(N0CALL)` server-succeeds + writes keyring → `remove` resumes and deletes
the keyring ⇒ account exists, credential gone). All mutating ops
(`account_create`, `cms_password_change`, `account_remove`,
`account_set_recovery_email`) take a **per-base-callsign async mutex** spanning the
server call + the keyring mutation, so operations on one account serialize.

### Error type

Generalize `PasswordChangeError` → `AccountApiError`:

- `NotConfigured` — no access key in env (feature unavailable).
- `InvalidKey` — server returned `InvalidAccessKey` (the issued key is missing/wrong).
- `Network{reason}` — transport / unparseable-or-shapeless body.
- `Rejected{ code, message }` — server `ResponseStatus.ErrorCode` + `Message` verbatim.
- `KeyringDesync{reason}` — server mutation confirmed but the local keyring write/delete failed.
- `UnknownOutcome` — a mutating call's outcome is indeterminate (timeout after send);
  caller reconciles.

Variant names that the shipped frontend already switches on
(`NotConfigured`/`Network`/`Rejected`/`KeyringDesync`) are preserved, so the
existing `CmsPasswordChange` mapping keeps working; new variants are additive.

### Read-op typed outcomes (frontend contract)

Read ops do NOT route through generic `Rejected`. They return typed results the UI
can branch on without reusing password-change copy:

- `account_exists` → `{ exists: bool, blocked: bool }`.
- `account_validate_password` → a validation code (`Valid` / `BadPassword` /
  `NoAccount` / …) mirroring `AccountValidationCodes`.
- `account_send_recovery` → `Ok` vs a distinct "no recovery address on file" outcome
  (the server returns an error when no recovery email is set; surface it as its own
  case so the UI can guide the user to set one).

### Callsign normalization

The existing `account_callsign()` reuses `base_callsign_for_post_office` (RMS
post-office login behavior; strips on `-`). For these full-account ops that is the
correct base form for a licensed callsign, but it is **destructive for
tactical/hyphenated identifiers**. Since tactical account create/remove is a
non-goal here, these commands **reject** a tactical/hyphenated input with
`InvalidInput` rather than silently stripping it.

## Shipped-path correction (`cms_password_change`)

In the same change, the merged `cms_password_change` is corrected to the v2
contract: send `Key` (not `WebServiceAccesscode`), drop `Requester`, and parse the
ServiceStack envelope (success ⇔ empty `ResponseStatus.ErrorCode`; no `HasError`;
error text from `Message`). Its existing tests are updated to the live shapes
(including an `InvalidAccessKey` case) and it adopts the shared `post_account_form`
+ the per-callsign mutex. This is a correctness fix to never-live-exercised code,
not a behavior change users have seen.

## Security & safety

- **No committed access-key literal.** Read from `TUXLINK_WINLINK_ACCESS_CODE` at
  runtime; absent ⇒ every gated command returns `NotConfigured` and the layer
  reports unavailable. Source builds ship no key. (The issued Tuxlink key is an
  operator-managed secret; the agent never handles it.)
- **TLS mandatory** on every call (no `danger_accept_invalid_certs`).
- **RADIO-1:** internet HTTPS to the account API, not a transmission — no on-air
  consent gate. Mutating ops change real accounts, so live exercise is operator-run,
  never CI.
- **Destructive `account_remove`** does no extra backend confirmation (the UI gates
  it, sub-project 3) and must not fire except on an explicit caller request — and is
  not wired to UI until live-proven invocable.

## Testing

- **Unit (TDD, CI; needs NO real key):** per command, the pure form-encode (`Key`
  present, op params, base-callsign normalization, tactical-input rejection) and the
  response-parse against **live-shaped** JSON: success (top-level payload, empty/absent
  `ResponseStatus`), rejection (`ResponseStatus.ErrorCode`+`Message`, HTTP 400),
  `InvalidAccessKey` → `InvalidKey`, and the malformed/shapeless-body guard ⇒
  transport error. Read ops test their typed outcomes incl. a fail-closed
  payload-missing case.
- **Live (operator-run, deferred):** blocked until a valid Tuxlink-issued key
  exists; there is no dev instance for `api.winlink.org`. Lifecycle
  (create → validate → set-recovery → change → remove) is exercised by the operator
  against a throwaway test callsign. `account_remove` live-invocability is a
  prerequisite for wiring delete into the UI.
- **Adversarial:** a Codex round on the corrected wire-encoding + the
  per-callsign-mutex / remove-reconciliation logic before the layer is "done."

## Source of truth

The wire contract is the **live `api.winlink.org` server** (verified via metadata +
a read-only probe, 2026-06-17), which supersedes the legally-possessed WLE 1.8.2.0
decompile where they differ. The decompile remains a useful cross-reference for op
existence (e.g. it proves `/account/remove` is a real route). The access-key value
and the raw decompile stay private (not in this public repo).
