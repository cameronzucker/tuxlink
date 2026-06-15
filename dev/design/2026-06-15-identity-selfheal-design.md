# Design: bootstrap self-heal for orphan-v2 missing activation secret (tuxlink-nx3g)

P0 transmit un-brick. Child of `tuxlink-6wz3`. Security-implicated (auto-provisioning
a credential), so this design is the target of cross-provider Codex convergence BEFORE
any code.

## The bug (verified on origin/main @ 0.65.0)

A FULL identity authenticates by comparing the operator-supplied credential against an
**activation secret** in the OS keyring (`SERVICE="tuxlink"`, account
`tuxlink-identity-activation:<CALL-UPPER>`). By the lockstep invariant that
`winlink::credentials::write_password` establishes (credentials.rs:288-321), the
activation secret EQUALS the stored CMS password (`SERVICE="tuxlink"`, account `<CALL>`).

`authenticate()` (identity/service.rs:53) returns:
- `Ok(handle)` — secret present and matches,
- `Err(NoSecretSet)` — keyring `NoEntry` (the secret was never written),
- `Err(CredentialMismatch)` — secret present but the supplied credential differs.

**Orphan-v2 state:** a user who migrated config `v1 -> v2` during ~0.55-0.57 (before the
migration backfill at bootstrap.rs:160 existed) has the CMS password but NO activation
secret. The migration is one-shot (`schema_version == 1` only), so re-upgrading never
re-runs it — the secret is missing forever.

`resolve_auto_identity` (bootstrap.rs:514) currently:
```rust
let pw = read_pw(full.as_str())?;          // stored CMS password
svc.authenticate(full, &pw).ok().map(SessionIdentity::full)   // None on NoSecretSet
```
On `NoSecretSet` it returns `None` -> auto-auth silently fails every launch
(bootstrap.rs:454-472 logs a warn). Worse, MANUAL unlock via the switcher
(`identity_authenticate`) ALSO calls `authenticate()` and ALSO fails `NoSecretSet`, so the
user is HARD-bricked: cannot transmit by any path. Only re-running the first-run wizard
(which calls `write_password` -> `set_activation_secret`) heals it.

## Proposed fix

In `resolve_auto_identity`, when authentication fails with **`NoSecretSet`** (and only
then), provision the activation secret FROM the stored CMS password — restoring the
lockstep invariant the wizard/migration would have established — then retry once:

```rust
fn resolve_auto_identity(svc, full, read_pw) -> Option<SessionIdentity> {
    let pw = read_pw(full.as_str())?;            // no stored CMS pw -> nothing to heal
    match svc.authenticate(full, &pw) {
        Ok(h) => Some(SessionIdentity::full(h)),
        Err(IdentityError::NoSecretSet) => {
            // Orphan-v2: CMS password exists, activation secret was never written.
            // Restore the lockstep (secret := stored CMS pw) and retry ONCE.
            svc.set_activation_secret(full, &pw).ok()?;
            svc.authenticate(full, &pw).ok().map(SessionIdentity::full)
        }
        Err(_) => None,   // CredentialMismatch / other: do NOT heal.
    }
}
```

The heal is invisible (auto-auth just starts working on next launch) and one-shot per
host (once written, subsequent launches hit the `Ok` arm).

## Security analysis

- **Trust root.** The OS keyring is already the trust root for both the CMS password and
  the activation secret. The heal only COPIES one keyring entry the operator wrote
  (`<CALL>`) into another (`tuxlink-identity-activation:<CALL>`). An attacker who can
  write the keyring already owns the user's secrets; the heal adds no attack surface.
- **Why never heal `CredentialMismatch`.** A present-but-mismatched secret is ambiguous:
  it could be a CMS-password rotation the operator did out-of-band, OR a tamper signal.
  Overwriting it from the (possibly-rotated) CMS password would silently change the
  activation gate and could mask the signal. Mismatch stays `None` -> operator resolves
  explicitly (re-run wizard / switcher). Healing is confined to the unambiguous
  "secret never existed" case.
- **No network.** The heal does not verify the CMS password against the CMS server; the
  activation secret is a LOCAL gate and the invariant is purely local (secret == stored
  CMS pw). A wrong-but-present CMS password would let the local identity gate pass but the
  CMS connection then fails with its own clear error — strictly better than a silent
  identity brick, and no worse than today.
- **Idempotent + fail-safe.** If `set_activation_secret` errors (keyring write failure),
  return `None` (stay as today: bricked-but-not-worse, warn logged). If the retry
  `authenticate` somehow fails, return `None`.

## Open questions for Codex (attack these)

1. Is copying the CMS password into the activation-secret slot on `NoSecretSet` ever
   UNSAFE in a way the trust-root argument misses? (keyring backends that silently
   succeed-without-persisting; `NoEntry` vs `NoStorageBackend` conflation; multi-user
   hosts.)
2. Is `NoSecretSet`-only correct, or are there real states where `CredentialMismatch`
   SHOULD heal (e.g. the operator rotated their CMS password in the wizard but the secret
   write failed, leaving a stale secret)? Argue both sides.
3. **Manual-unlock parity.** Auto-auth only fires for `sole_full`. A multi-FULL orphan-v2
   user (auto-auth skipped) still can't manual-unlock (switcher `authenticate` -> NoSecretSet).
   Should the heal live deeper (in `authenticate`, or in `identity_authenticate`) so BOTH
   paths heal? Trade-off: centralizing in `authenticate` makes a security primitive WRITE
   on a read — is that acceptable, or worse than the bootstrap-only scope?
4. Failure-mode visibility: today auto-auth failure is a `tracing::warn!` only. Should a
   heal attempt (success or fail) emit a visible session-log line? Does silent healing
   hide a real problem the operator should see?
5. Any way the retry can loop, double-write, or race a concurrent identity op at launch?
6. Test surface: the `with_memory_keyring` seam (service.rs:114) + the injected `read_pw`
   closure. What states MUST be covered (no CMS pw; CMS pw + no secret; CMS pw + matching
   secret; CMS pw + mismatched secret; set_activation_secret failure)?

## Test plan (TDD, headless via memory-keyring)

- `resolve_auto_identity` heals when CMS pw present + secret absent -> `Some` + secret now set.
- does NOT heal on `CredentialMismatch` (secret present, wrong) -> `None`, secret unchanged.
- does NOT act when no CMS pw -> `None`.
- already-authenticatable (secret matches) -> `Some`, no extra write.
- `set_activation_secret` failure -> `None` (fail-safe).
- (pending Codex Q3) manual-unlock parity test if the heal moves/duplicates.

---

# Revision v2 — after Codex round 1 (dispositions)

Codex R1 found a real auth-bypass + a scope error. All findings accepted (verified
`.first()` at bootstrap.rs:429 and authenticate-before-tactical-validation at
commands.rs:126). Revised design:

## v2 heal model — split by path, never inside `authenticate()`

`IdentityService::authenticate()` STAYS read-only (no write). A new narrowly-scoped
repair helper does the provisioning, and the two callers gate it differently:

```rust
/// Provision a MISSING activation secret from a TRUSTED CMS password (orphan-v2 only).
/// The caller must have established trust in `cms_pw` (bootstrap: read from keyring;
/// manual: proven == stored CMS pw). Refuses empty pw; never overwrites an existing
/// secret (so it cannot touch a CredentialMismatch). Returns whether it healed.
fn heal_activation_secret(svc, full, cms_pw) -> Result<bool, IdentityError> {
    if cms_pw.is_empty() { return Ok(false); }
    if svc.has_activation_secret(full) { return Ok(false); }   // never overwrite (mismatch/race)
    svc.set_activation_secret(full, cms_pw)?;
    Ok(true)
}
```

**Bootstrap (auto-auth) — `resolve_auto_identity`:**
- Gate the WHOLE auto-auth + heal on **exactly one FULL** (`full().len() == 1`), not
  `.first()` (R1 P0 #1). Multi-FULL ⇒ no auto-auth, no heal (operator uses the switcher).
- `pw` is the stored CMS password read from the keyring (the trust root — no user input,
  so no bypass). On `NoSecretSet` and `!pw.is_empty()` ⇒ `heal_activation_secret` ⇒ retry.
- Return a 4-state outcome (`Authenticated` / `Healed` / `HealFailed` / `Unavailable`)
  so the caller emits the right VISIBLE session-log line.

**Manual unlock — `authenticate_inner` (NEW scope, R1 P0 #3):**
- On `authenticate() == NoSecretSet`: heal ONLY if the user-supplied credential EQUALS
  the stored CMS password (`read_password(full)`, non-empty) — proof-of-knowledge, which
  is what stops "any typed string becomes the secret" (R1 P0 #2). 
- Validate the target FIRST (R1 P2): the FULL is in the store, and if `tactical_label`
  is present the tactical exists under that FULL — BEFORE writing the secret. Then heal
  the FULL's secret, re-authenticate, proceed. Else ⇒ `AuthFailed` (unchanged).

## Other R1 dispositions

- **Never auto-heal `CredentialMismatch`** (R1 P1) — `has_activation_secret` guard makes
  the helper structurally incapable of overwriting an existing secret. Explicit operator
  "repair from CMS credential" is a possible future affordance, out of scope here.
- **Empty CMS pw rejected** before any heal (R1 P1) — in the helper.
- **CMS keyring read diagnostics** (R1 P1): the bootstrap `read_pw` reader distinguishes
  `NoEntry` (no stored pw — nothing to heal, expected) from backend-unavailable / locked
  (a real environment problem) in the log line; both still fail-closed.
- **Visible heal log line** (R1 P1): one Warn-level session-log line on heal success AND
  failure — callsign + reason class only, NEVER the secret value.
- **Race** (R1 P2): bootstrap runs single-threaded pre-UI (no race). The manual path
  re-reads + the `has_activation_secret` guard make a double-write a no-op; a concurrent
  rotation at worst leaves the operator to retry. No lock added (KISS); documented.
- **Tests** (R1 P2): add a custom `EntryFactory` double (beyond `with_memory_keyring`)
  to exercise set-failure, set-Ok-without-persist, activation-read backend error, and
  write-COUNT assertions (no write on mismatch / multi-FULL / empty-pw / already-set).
- **Trust boundary doc** (R1 P3): note that the identity gate assumes the OS user account
  is the security boundary; no protection between operators sharing one OS login.

## Revised scope (files)

- `src-tauri/src/identity/service.rs` — `heal_activation_secret` helper (+ `has_activation_secret` reuse).
- `src-tauri/src/bootstrap.rs` — `sole_full` ⇒ exactly-one gate; `resolve_auto_identity` heal + 4-state outcome; visible log line.
- `src-tauri/src/identity/commands.rs` — `authenticate_inner` manual self-heal with proof-of-knowledge + validate-before-write.

---

# Revision v3 — after Codex round 2 (dispositions)

R2 confirmed the auth-bypass is closed and the core choices sound (len==1, heal-out-of-
authenticate, manual proof-of-knowledge, warn visibility). No new P0. Dispositions:

1. **One `exactly_one_full(store) -> Option<Callsign>` helper, used for BOTH
   `with_default_identity` (bootstrap.rs:437) AND auto-auth/heal** (R2 P1, scoping half).
   Multi-FULL ⇒ `None` ⇒ no default + no auto-auth. With no default, a multi-FULL user's
   queued mail is UNTAGGED (drains for any active identity per winlink_backend.rs:316-330),
   so it is not mis-tagged to the wrong FULL.
2. **Deeper multi-FULL Outbox tagging (store via `for_identity(active)`) is OUT OF SCOPE**
   — filed as a separate P2 bd issue (pre-existing, reachable only with multiple FULLs,
   a "coming soon" feature). The P0 here is the auth un-brick; sole-FULL (Orv + the common
   case) is fully fixed.
3. **Manual heal resolves the canonical stored FULL** (R2 P2): load the store, match the
   typed callsign by `eq_ignore_ascii_case`, and use that canonical callsign (+ the
   project's CMS-credential casing convention) for `read_password` / `set_activation_secret`
   / re-auth, so `w1abc` heals the stored `W1ABC`.
4. **Strict never-overwrite, fail-closed** (R2 P2): replace the boolean `has_activation_secret`
   guard with a fallible existence check that distinguishes `NoEntry` from a backend read
   error; write the secret ONLY on confirmed `NoEntry`; on a backend read error, do NOT
   write (fail-closed, log). The non-atomic guard→set window is accepted (no lock): bootstrap
   is single-threaded pre-UI; in the manual path the worst case is a redundant same-value
   write or a benign no-op. Documented, with a test for the backend-read-error path.

---

# Codex round 3 — VERDICT: READY TO IMPLEMENT (no P0/P1 blockers)

R3 confirmed all v3 dispositions sound. Implementation notes locked in:
- `read_password`/`write_password` use the RAW account string (no uppercasing); the
  activation account uppercases via `activation_account`. So: match typed callsign to the
  store entry by `eq_ignore_ascii_case`, then use the canonical stored `Callsign` for the
  CMS read + activation set + re-auth.
- Replace `has_activation_secret` (collapses errors to false) with a fallible existence
  check; write only on confirmed `NoEntry`; fail closed on backend error.
- Multi-FULL "no default ⇒ untagged mail drains for any identity" CONFIRMED via the drain
  filter (winlink_backend.rs:324). Do NOT claim multi-FULL Outbox isolation is solved (filed P2).
- Complete test list: 0/1/2-FULL helper; multi-FULL no-default+no-auto-auth; heal
  success/failure/no-write; backend-read-error fail-closed; manual proof-of-knowledge;
  canonical lowercase typed call; tactical-validation-before-write.
