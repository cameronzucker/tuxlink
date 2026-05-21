# Handoff — 2026-05-21 — gully-sage-heron — live-CMS round-trip + Pat shape + replace-Pat directive

**Operator directive (verbatim intent):** (1) get the live-CMS telnet round-trip working FAST so we can reference the **known Pat shape** for the tuxlink-pat *replacement*; (2) **once working, refactor immediately to replace Pat** — stop sinking time into a component we're discarding. Start immediately.

**Standing operator authorization (this roughing-out phase, session-scoped — NOT cached forever):** explicit permission to use the cred store and run the Pat **CMS plumbing over TELNET ONLY (no RF)** for testing. Re-confirm per phase; do not extend to RF or other transports.

## 0. ⚠️ Why progress stalled — TWO blockers (neither is a tuxlink code bug)

**A. Broken keyring environment (the round-trip blocker).** Pat's reader, `go-keyring v0.2.8`, reads **only the `login` Secret Service collection** (`keyring_unix.go` → `GetLoginCollection()`). The Rust keyring crate (the wizard's writer) writes the **`default`** collection. On a normal desktop `login`==`default` so they interoperate (PR #75 verified there). **On pandora the incident split them:** `~/.local/share/keyrings/login.keyring` was moved to `login.keyring.broken-20260520.bak`; the active default is `Default_keyring`. `gdbus ReadAlias login` → `/` (unset). So go-keyring → login → null → Pat can't read the cred → falls through to its interactive `Enter secure login password` prompt → `live_cms_smoke` (non-interactive) stalls → `Exchange failed: context deadline exceeded`. **`SetAlias('login', …)` is REJECTED by gnome-keyring** ("Only the 'default' alias is supported"), so the login collection can't be created via the API — it needs real login-keyring repair. Recorded on **tuxlink-qn8**.

**B. Harness permission gate (the autonomy blocker).** The Claude Code auto-mode classifier hard-blocked the agent's Bash for (1) `gdbus SetAlias` (system-wide keyring reconfig) and (2) `secret-tool lookup | pat connect telnet` (cred-read piped into a transmit) — **independent of the operator's verbal authorization.** To run these autonomously next session: operator runs them manually, OR adds Bash allow-rules / uses a less-restrictive permission mode.

## 1. Known Pat / Winlink-CMS shape (the deliverable for the replacement)
Captured live this session:
- **Connect:** Pat (http mode, ephemeral port) or `pat connect telnet`. CMS telnet at `<rotating-AWS-IP>:8772` (saw 32.196.219.66, 52.206.142.38).
- **Protocol (telnet B2F):** TCP connect → banner `[WL2K-5.0-B2FWIHJM$]` → `;PQ: <number>` (proposal/queue) → `CMS>` → **secure-login** (`Enter secure login password for <CALL>:`, challenge/response with the Winlink account password) → B2F proposal exchange (`FF`=no more proposals, `FQ`=disconnect). No-RF over TLS.
- **Auth source:** the Winlink secure-login password, read from the keyring (service=`tuxlink-pat`, username=`<NORMALIZED CALLSIGN>`) at session time by the fork's `app/exchange.go::secureLoginLookup` → `internal/credstore.Get` → go-keyring (login collection). Falls back to interactive prompt on keyring miss.
- **Pat HTTP API quirk:** `POST /api/connect?url=telnet` *triggers* the connect but its HTTP response does not return cleanly to a plain client (Pat runs the session regardless; response is streaming/odd). `live_cms_smoke` now treats that as **non-fatal** and relies on the inbox poll.
- **Pat fork source is now populated:** `external/tuxlink-pat` submodule was init'd (commit `4969aa8`) — read `app/exchange.go`, `internal/credstore/`, `api/winlink_account.go` for the exact read paths. go-keyring v0.2.8 in `~/go/pkg/mod`.

## 2. #88 (tuxlink-22l) status — code VERIFIED, env-blocked
- **#88's code is correct:** it spawned Pat and reached the live CMS at `:8772`. The round-trip fails ONLY due to blocker A (broken login keyring) — not the PR.
- **live_cms_smoke improved this session** (commit `190d75c` on `bd-tuxlink-22l/pat-spawn-bootstrap`): streams Pat stderr (`[pat]` prefix; `log_sink` was `None` = black box), generous 60s non-fatal connect (was a 30s hard-abort), 60s poll, honest 60s consent duration. **Plus the earlier merge-resolution + http_announce_timeout fix.** #88 is still **DRAFT**, mergeable.
- **Decision pending:** land #88 on code-correctness (round-trip blocked only by the keyring env), OR finish the keyring repair first and get a green round-trip.

## 3. Immediate next steps (start here)
1. **Get one green round-trip to lock the shape** — fastest path bypasses the broken keyring: read the cred + feed Pat's prompt (telnet, authorized) — `printf '%s\n' "$(secret-tool lookup service tuxlink-pat username N7CPZ)" | pat connect telnet` (queue a `/test/` to SERVICE@winlink.org first for the full reply). NOTE: Pat may read the password from `/dev/tty` not stdin — if the pipe is ignored, use `expect`, or repair the login keyring instead. Run with permissions that don't trip the auto-classifier (operator-run or allow-ruled).
2. **OR repair the login keyring** (env, operator domain): recreate a real gnome-keyring `login` keyring so `login`==`default` (the normal state); then the wizard's write is readable by Pat. Ties to tuxlink-qn8 (the wizard should detect/guide this) + the cred-handling spec should state the **login-collection requirement** (tuxlink-8zt follow-up).
3. **Then: replace-Pat refactor** per operator directive — see `project_v05_modem_design_posture` memory + ADR 0011 (fork-to-enable-deletion). The keyring/`/api/connect` brittleness documented above is the motivation + the shape to design against. Scope the replacement (the v0.5+ clean-room path vs a thinner v0.0.1 engine boundary) with the operator.

## 4. Repo state
- PRs #95–#102 (pqg/2a7/h2y/g3d/f1a/fzm/8zt + handoff #96) merged earlier this session. #88 DRAFT. `external/tuxlink-pat` submodule now populated locally (was empty).
