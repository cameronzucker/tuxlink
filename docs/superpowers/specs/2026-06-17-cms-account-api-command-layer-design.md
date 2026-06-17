# CMS account-API command layer — design

**Date:** 2026-06-17
**Status:** approved (design); implementation pending
**Scope:** sub-project 0 of the CMS account-lifecycle expansion (tuxlink-vfb3 follow-on).

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
POST to `/<path>?format=json` returning the common JSON envelope
(`{ "ResponseStatus": { "ErrorCode": "", "ErrorMessage": "" }, "HasError": false, … }`;
success ⇔ `HasError == false` **and** `ResponseStatus.ErrorCode == ""`). The
existing `cms_password_change` already implements this shape; this layer extends
it with the remaining operations.

### The full expansion (for context; only sub-project 0 is specified here)

0. **Backend account-API command layer** — this document.
1. Wizard in-app account creation (mandatory recovery email).
2. Status-bar identity-dialog forgot-password recovery.
3. Settings → Winlink Account full management (set recovery email; delete behind a typed-confirmation gate).

Build order 0 → 1 → 2 → 3; the backend foundation unblocks every UI surface and
needs no mockups. Sub-projects 1–3 each get their own mockup-driven design pass.

## Goals

- Expose the complete client account API as native Tauri commands: create, read
  (exists / validate-password), update (set recovery email; password change
  already shipped), delete, and recovery (send recovery email).
- One shared POST/parse path with the credential-safety guard already proven for
  password change (a partial/empty/proxy-error body is a transport error, never a
  silent success).
- Atomic keyring coupling: create writes the new credential; delete removes the
  stored credential; read/recovery ops never touch the keyring.
- No committed secret: every gated op reads the shared access code from
  `TUXLINK_WINLINK_ACCESS_CODE` at runtime; absent ⇒ the op reports unavailable.

## Non-goals

- No UI in this sub-project (the three UI surfaces are separate sub-projects).
- No gateway-sysop registration (`/sysop/add`) — out of scope for Tuxlink; it is
  the only path that collects name/address PII, which Tuxlink does not handle.
- No tactical-account create (`/account/tactical/*`) in this pass; revisit if a
  UI surface needs it.

## Commands

All commands take a raw callsign and normalize it to the **base** callsign
(uppercase, drop `.`-qualifier, strip SSID) via the existing `account_callsign()`
before sending, matching WLE's `BaseCallsign`. All form-POST to
`https://api.winlink.org/<path>?format=json`. The transport auto-adds
`Requester=<callsign>` (mirroring WLE's `JsonCommand`). TLS cert validation stays
on for every call.

| Command | Path | Form params | Returns | Keyring on success |
|---|---|---|---|---|
| `account_create` | `/account/add` | `Callsign`, `Password`, `WebServiceAccessCode` | `()` | write password (atomic) |
| `account_exists` | `/account/exists` | `Callsign` | validation code | none |
| `account_validate_password` | `/account/password/validate` | `Callsign`, `Password`, `WebServiceAccesscode` | validation code | none |
| `account_set_recovery_email` | `/account/password/recovery/email/set` | `Callsign`, `Password`, `RecoveryEmail`, `WebServiceAccessCode` | `()` | none |
| `account_send_recovery` | `/account/password/send` | `Callsign` | `()` | none |
| `account_remove` | `/account/remove` | `Callsign`, `WebServiceAccessCode` | `()` | **delete** the keyring entry |
| `cms_password_change` *(shipped)* | `/account/password/change` | `Callsign`, `OldPassword`, `NewPassword`, `WebServiceAccesscode` | `()` | write new password (atomic) |

### Load-bearing wire detail — access-code parameter casing varies per endpoint

The parameter NAME carrying the access code differs by endpoint and MUST be sent
verbatim:

- `WebServiceAccessCode` (capital `C`): `add`, `remove`, `recovery/email/set`.
- `WebServiceAccesscode` (lowercase `c`): `password/validate`, `password/change`.
- **No access-code parameter at all:** `exists`, `password/send`.

A single env var (`TUXLINK_WINLINK_ACCESS_CODE`) supplies the value for every
gated op; only the parameter NAME's casing changes. The casing is the kind of
detail that silently fails the live call while passing every offline test, so it
is a primary target for the adversarial review round.

## Shared infrastructure (refactor)

Extract from the existing `change_password` body a single helper:

```
post_account_form(path: &str, params: Vec<(&str, String)>) -> Result<AccountEnvelope, AccountApiError>
```

- Builds the form body, POSTs with the existing reqwest client + the 30s timeout,
  TLS on.
- Parses the common envelope with the **required-fields guard** (HasError,
  ResponseStatus, ErrorCode all required; a body missing any ⇒ `Network`
  transport error, never success). This is the Codex-adrev-2026-06-17 P1
  invariant, now shared by all ops instead of living only in the change path.
- Maps `HasError == true || ErrorCode != ""` ⇒ `Rejected{ message = ErrorMessage
  verbatim }` (with a coded fallback when the server gives no message).

`change_password` is refactored to call `post_account_form` so there is one
parse/guard implementation. Read ops (`exists`, `validate_password`) additionally
deserialize the `AccountValidationCodes` result field and return it.

### Error type

Generalize the existing `PasswordChangeError` into `AccountApiError` with the same
variants — `NotConfigured`, `Network{reason}`, `Rejected{message}`, and
`KeyringDesync{reason}` (for the create/change/remove paths that touch the
keyring) — serialized to the frontend with the `kind` tag the UI already switches
on. `change_password` returns this generalized type; the existing frontend error
mapping is unaffected (same variant names).

### Keyring coupling rules

- `account_create`: on confirmed success, write the new password to the keyring
  (service `tuxlink`, account `<base callsign>`) atomically, identical to the
  change path's snapshot-and-restore discipline. A keyring failure after a
  confirmed server create surfaces `KeyringDesync` (account exists at the CMS;
  local store out of sync) — never a silent success.
- `account_remove`: on confirmed success, delete the keyring entry for that
  callsign (the account no longer exists, so the stored credential is dead).
  A keyring-delete failure after a confirmed server remove is logged and surfaced
  as `KeyringDesync` — the account is gone regardless; the stale local secret is
  the lesser, recoverable problem.
- `account_exists`, `account_validate_password`, `account_set_recovery_email`,
  `account_send_recovery`: read/auxiliary ops; never touch the keyring.

## Security & safety

- **No committed access-code literal.** Same posture as the shipped change path:
  the value is read from `TUXLINK_WINLINK_ACCESS_CODE` at runtime; when absent,
  every gated command returns `NotConfigured` and the layer reports unavailable.
  Source builds ship no literal.
- **TLS mandatory.** This API path is genuinely TLS; cert validation stays on for
  every call (no `danger_accept_invalid_certs`).
- **RADIO-1:** these are internet HTTPS calls to the account API, not
  transmissions — no on-air consent gate. They DO mutate real accounts, so live
  exercise is operator-run, never CI.
- **Destructive `account_remove`** irreversibly deletes a Winlink account. The
  backend command performs no extra confirmation (that is the UI's job — the
  Settings surface gates it behind typed confirmation in sub-project 3). The
  command itself simply must not fire except on an explicit caller request.

## Testing

- **Unit (TDD, CI):** for each command, the pure form-encode (exact parameter
  names + per-endpoint access-code casing + base-callsign normalization) and the
  response-parse (representative success, server rejection with `ErrorMessage`,
  and the malformed/partial-body guard returning a transport error). Read ops
  additionally test the validation-code mapping.
- **Live (operator-run, never CI):** there is **no dev instance** for
  `api.winlink.org` (unlike the telnet CMS at cms-z). Create/remove mutate a real
  account, so the live lifecycle (create → validate → set recovery → change →
  remove) is exercised by the operator against a throwaway test callsign. Agents
  do not run it.
- **Adversarial:** ≥1 Codex round on the wire encoding (per-endpoint param-name
  casing, base-callsign edge cases) and the `account_remove` keyring-delete
  ordering, before the layer is considered done.

## Source of truth

The wire contracts are mirrored from the legally-possessed Winlink Express
decompile via the private design note
(`library-of-hamexandria/.../2026-06-16-tuxlink-vfb3-cms-account-api.md`); the
Tuxlink implementation is clean-room forward code. The access-code value and the
raw decompile remain private (not in this public repo), consistent with the
existing `cms_password_change` posture.
