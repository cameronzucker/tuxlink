# Wizard in-app Winlink account creation — design (sub-project 1)

**Date:** 2026-06-17
**Status:** approved (design); implementation pending
**Scope:** sub-project 1 of the CMS account-lifecycle expansion (tuxlink-vfb3).
**Depends on:** sub-project 0 (backend account-API command layer, CI-green `944e19ca`) —
the `cms_account_create` / `cms_account_validate_password` / availability commands it
calls. Live exercise is blocked on `tuxlink-lu7t` (the Tuxlink-issued access key).
**Backend reference:** [2026-06-17-cms-account-api-command-layer-design.md](./2026-06-17-cms-account-api-command-layer-design.md).

## The decision (operator, 2026-06-17)

Account creation is **not** presented as an up-front have/create fork. It uses the
pattern "nearly every app on earth uses": the **sign-in form is the landing screen**,
with a secondary **"Create a Winlink account"** affordance for the minority who do not
yet have one. The only tuxlink-specific branch (online CMS vs. fully offline) stays
where it already is, on Step1Welcome. Three alternatives (a dedicated fork step, a
segmented in-form toggle, three cards on Step1) were mocked and rejected for adding an
unfamiliar decision the standard pattern does not impose.

Mock (pixel-matched to `wizard.css`): `dev/scratch/vfb3-sp1-account-create-mock.html`
→ `dev/scratch/vfb3-sp1-mock-v2.png` (gitignored dev scratch).

## Flows (definition-of-done — the wire-walk gate traces these at done-time)

1. **Sign in (unchanged).** `account` → "Yes, connect to CMS" → `credentials`
   (Step2Credentials) → Continue → `wizard_persist_cms` → `cms_verify` → `location` →
   `complete`. This path is not modified except for the new affordance below.
2. **Create an account (new).** On `credentials`, a "New to Winlink? **Create a Winlink
   account**" link → new `account_create` step → fill callsign + password + confirm +
   **mandatory recovery email** → "Create account & continue" → `cms_account_create` →
   on success persist the config identity → `cms_verify` → `location` → `complete`. A
   created account joins the **existing** verify→location→complete tail with no special
   casing.
3. **Callsign already registered.** `cms_account_create` returns `Rejected` (the live
   server reports the callsign exists) → the create form shows "**{CALLSIGN} already has
   a Winlink account.** If it is yours, sign in instead." with a **Sign in with this
   callsign →** action that returns to `credentials` with the callsign prefilled.
4. **Back to sign in.** The `account_create` step has a "Back to sign in" control →
   `credentials` (reusing `RETURN_TO_CREDENTIALS`).
5. **Create unavailable (no access key).** When the CMS account-API feature is not
   configured on the build (`cms_password_change_available()` is false — no
   `TUXLINK_WINLINK_ACCESS_CODE`), the affordance degrades to today's behavior: an
   external **Register on winlink.org →** link that opens the system browser. In-app
   creation is never offered when it cannot work.

## Wizard state machine

Add a `WizardStep` value `'account_create'` and these reducer transitions:

| Action | From | To | Notes |
|---|---|---|---|
| `GO_TO_ACCOUNT_CREATE` | `credentials` | `account_create` | fired by the "Create a Winlink account" link (only when the feature is available) |
| `RETURN_TO_CREDENTIALS` *(exists)* | `account_create` | `credentials` | "Back to sign in" / "Sign in with this callsign"; clears the local password |
| `ACCOUNT_CREATE_SUCCESS` | `account_create` | `cms_verify` | clears password, `inFlight=false`; mirrors the non-skip `SUBMIT_CREDENTIALS_SUCCESS` tail |

`SUBMIT_BEGIN` / `SUBMIT_FAILURE` are reused for the in-flight + error lifecycle. The
"Sign in with this callsign" action carries the rejected callsign so `credentials`
prefills it (the reducer already keeps `state.callsign`).

## The create form (`account_create` step component)

Fields (all required except where noted), reusing `CredentialFields` for callsign +
password where practical:

- **Callsign** — validated with a **strict amateur-callsign rule** (mirrors the backend
  `looks_like_amateur_callsign` grammar: 1–2 char prefix incl. digit-led, area digit,
  1–4 letter suffix). This is deliberately stricter than the wizard's loose
  `validateCallsign` (which accepts tactical addresses) so the user gets early feedback
  instead of a backend `InvalidInput`. Tactical addresses cannot hold a CMS account.
- **Password** — **6 to 12 characters** (the live API rule, verified from the
  ServiceStack metadata), show/hide. The existing `validatePassword` only enforces ≥6;
  add a create-specific validator (or a max-length parameter) for the 6–12 bound.
- **Confirm password** — must match.
- **Recovery email** — **mandatory**, non-empty + a light email-shape check. Hint copy:
  "Required. If you forget your password, Winlink emails it to this address — so use one
  you control and a password you reuse nowhere else." (Mandatory-recovery is the locked
  hamexandria support-burden decision from sub-project 0.)

Submit is gated until all client validations pass. On submit: `SUBMIT_BEGIN` →
`invoke('cms_account_create', { rawCallsign, password, recoveryEmail })`.

## Persisting the created identity

`cms_account_create` creates the CMS account and writes the **password** to the keyring,
but does **not** persist the config identity (callsign / MBO) — that is the wizard's job
(`wizard_persist_cms`). On `cms_account_create` success the create flow therefore also
persists the identity so the app knows who it is:

1. `invoke('cms_account_create', …)` — creates account + writes keyring password.
2. `invoke('wizard_persist_cms', { rawCallsign, password, grid: '', mboAddress })` —
   writes `config.json` identity (+ an idempotent keyring re-write of the same value).
   MBO auto-fills from the callsign exactly as Step2Credentials does; grid is collected
   later in the Location step.
3. `dispatch(ACCOUNT_CREATE_SUCCESS)` → `cms_verify`.

**Partial-state note:** if step 1 succeeds but step 2 fails (config-write error), the CMS
account exists and the keyring holds the password, but the config identity is unwritten.
Surface the `wizard_persist_cms` error; the user is now effectively in the "have an
account" state and can complete via the sign-in path (a retry of *create* would
correctly fail "callsign exists"). No silent success, no data loss.

## Error UX

- `Rejected{code,message}` from `cms_account_create` → flow 3 (callsign-exists banner +
  sign-in offer) when the message indicates the callsign exists; otherwise surface the
  server `message` verbatim in an error banner.
- `InvalidKey` / `NotConfigured` → should not occur (the affordance is gated on
  availability) but, defensively, map to "account creation is unavailable on this build".
- `Network` → "Could not reach the Winlink account service. Check your connection and try
  again." `UnknownOutcome` → "The request timed out before we could confirm the result.
  Your account may or may not have been created — try signing in before creating again."
  `InvalidInput{field}` → inline field error (should be pre-empted by client validation).

## Testing (TDD, CI — no real key required)

- **Validators (unit):** the create-password 6–12 bound (reject 5, accept 6/12, reject
  13); the strict callsign rule (accept `KK7ABC`/`W1AW`/`2E0AAA`, reject `RELAY1`/`EOC1`/
  `ARES`); recovery-email required + shape; confirm-match.
- **Reducer (unit):** `GO_TO_ACCOUNT_CREATE` only from `credentials`; `ACCOUNT_CREATE_SUCCESS`
  → `cms_verify` clearing the password; `RETURN_TO_CREDENTIALS` from `account_create`.
- **Component (RTL):** the create affordance shows on `credentials` only when available
  and degrades to the external link when not; the create form's submit gating; the
  success path invokes `cms_account_create` then `wizard_persist_cms`; the callsign-exists
  rejection renders the sign-in offer and prefills the callsign on return.
- **App-level mount (production path):** the wizard renders `account_create` through the
  real provider tree (per the "test the production mount path" pitfall), not just the
  component in isolation.
- **Live (operator, deferred):** real account creation against a throwaway callsign,
  blocked on `tuxlink-lu7t`.

## Out of scope (later sub-projects)

- **2** — status-bar identity-dialog forgot-password recovery (`account_send_recovery` /
  change-password).
- **3** — Settings → Winlink Account management (set recovery email; delete behind a
  typed-confirmation gate, only once `account_remove` is live-proven invocable).

## Source of truth

The operator's 2026-06-17 decision (login pattern, not a fork) and the verified live
contract in the backend spec. The mock is illustrative; this document is canonical for
the flows and state machine.
