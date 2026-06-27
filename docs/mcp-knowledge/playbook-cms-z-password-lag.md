# Playbook: new Winlink account, correct password rejected

## Symptom

A brand-new Winlink account cannot log in to the CMS even though the password
is known to be correct. The login is rejected, often within a day of the
account being created. Re-typing the password, resetting it, or reinstalling
the client does not help.

## Cause

This is a **server-side replication lag, not a client bug and not a wrong
password.** New Winlink accounts are created on the Winlink production
infrastructure and then replicate out to secondary and development servers.
Replication to the `cms-z` server takes roughly 24 hours. During that window
the account exists on production but not yet on `cms-z`, so a login attempt
against `cms-z` is rejected with what looks like a credentials failure even
though the credentials are correct.

This is the actual cause behind the recurring "what is wrong with my
password?" reports. The client is behaving correctly; the account simply has
not propagated yet.

## Resolution

1. Confirm the password works against the **primary** Winlink infrastructure
   first (for example by logging in through the official web interface or a
   known-good client pointed at production). If it works there, the
   credentials are correct and the problem is propagation.
2. Wait approximately 24 hours from account creation for replication to
   reach `cms-z`.
3. Retry the login. No client-side change is required; the same password will
   succeed once the account has replicated.

If the password still fails against the primary after waiting, that is a
genuine credentials problem and is handled separately from this propagation
case.

## What not to do

Do not chase this as a Tuxlink authentication defect. The secure-login
exchange is confirmed working; the symptom is timing, not code. Re-diagnosing
the client wastes the operator's time and was the wrong path the last several
times this came up.
