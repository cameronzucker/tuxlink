# Handoff — 2026-05-22 — spruce-marten-cardinal — product audit + remediation

## TL;DR
A post-merge smoke of #113 turned into a **full product audit + trust rebuild**. The
operator established that **"done" here has meant "compiles + tests pass + reviewed",
never "a human walked it as a product"** — so many surfaces shipped as *compiled mocks*.
The backend engine is real (the client **received + rendered a real Winlink message**
live this session), but multiple user-facing surfaces were unstyled / dead-ended /
crippled, and the shipped CMS connect default **cannot connect by construction**.

Three **draft** PRs off `main` (none merged; each operator-gated by a real smoke), plus a
filed remediation backlog. **Do not declare anything "done" the operator hasn't smoked.**

## Three draft PRs (all off `main`, all DRAFT)
- **#114** `bd-tuxlink-39b/gps-privacy-settings` — GPS privacy settings (config_set_privacy + inline SettingsPanel + consolidated Tools→Settings→"GPS & Privacy…") + Tools-menu coherence (Preferences removed, dead stubs greyed+v0.1, uniform 30px row height). Operator-validated core (chip green-when-locked, panel, menu). 375 vitest + tsc green.
- **#116** `bd-tuxlink-dj6/wizard-shippable` — first-run wizard: real `wizard.css` (it shipped with NONE) + completion hand-off (`App.tsx onComplete` → mounts shell; was a dead-end dev placeholder). Operator-validated. 129 wizard/App tests green.
- **#117** `bd-tuxlink-nki/cms-raw-session-log` — **CURRENT worktree.** Tees raw B2F wire dialogue into the session log as `LogSource::Wire` (WireTap in telnet.rs → WireSink mirrors ProgressSink → bootstrap → existing Raw/Human toggle, NO frontend change). Binary payloads summarized as `<N bytes binary>` (not mojibake). **Live-verified vs cms-z** + 213 lib tests green. Operator smoked Raw output — saw real trace; flagged the binary-mojibake which is now fixed (f35a8d2), **re-smoke pending**.

## Integration build (throwaway)
`worktrees/bd-tuxlink-9yx-integration-smoke` = `main` + #114 + #116 (merged via no-ff; has menu-height fix). **Does NOT include #117.** bd-tuxlink-9yx. Dispose per ADR 0009 when done. Smoke configs live in `~/.cache/tuxlink-smoke-{combined,telnet,probe-config}/` (isolated XDG_CONFIG_HOME so the 7fr `packet` field doesn't trip `deny_unknown_fields`).

## THE OPEN QUESTION (was mid-answer): why did the test→gmail bounce "unauthenticated"?
Operator received a real Service Advice bounce: a prior "Tuxlink Test" → cameronzucker@gmail.com was rejected `550-5.7.26 ... sender is unauthenticated`. **Finding so far:** `compose.rs` sets `From: N7CPZ` (bare upper-cased callsign — *correct* Winlink wire format; the gateway rewrites to N7CPZ@winlink.org). So this is **almost certainly NOT a tuxlink bug** — it's the **Winlink→internet-email relay** being rejected by **Gmail's SPF/DKIM/DMARC** anti-spoofing (gmail rejects unauthenticated forwarded mail; winlink.org→gmail is a known deliverability pain). **Next agent: confirm + give the operator the full answer** (the client delivered to Winlink fine; the bounce came BACK through Winlink, proving the receive path works).

## Remediation backlog (filed in bd) — the audit's output
- **CMS connect default is broken by construction.** Shipped default `CmsSsl`→8773→cms-z, but **cms-z has NO 8773 TLS listener** (code comment + live-probed). Only **Telnet/8772→cms-z works** (live-verified: native client completes a real B2F session as N7CPZ). Production (server.winlink.org, the only TLS host) **rejects the unregistered `tuxlink` SID** (`TODO(register)`). Fix = telnet-honest-default OR register the SID — operator's security call (don't silently default to plaintext). Must ride behind a **real e2e connect test**, not shell mocks (operator's explicit requirement).
- **Cc needlessly disabled** in Compose citing "Pat 1.0.0 drops cc" — but native backend fully supports Cc (compose.rs writes Cc headers). Stale-Pat cripple; ~1-line UI re-enable.
- **Pat half-removed**: ~1.1k LOC dead (pat_client/config/process + PatBackend + spawn_pat) + stale comments + the misleading `Pat 1.0.0` status-bar label (StatusBar.tsx:15). Runtime is native-only ("no Pat").
- **AX.25 never run end-to-end** (`bd-tuxlink-7fr`, PR #115 OPEN+conflicting): well-written, 65 tests, but never moved a real frame; its own HEAD handoff says "NOT done". Serial/BT abort is a no-op; no live status feed.
- **VARA HF/FM don't exist** (decorative sidebar labels).
- **Menu**: 10 non-Tools items still render as live no-op buttons (Message/Session/View/Help) — same dead-stub pattern, not yet fixed.
- **tuxlink-p3u**: received messages show `1970-01-01` (Date-header parse → epoch fallback).
- **tuxlink-e13**: integration tests don't compile after 686's `position_source` (gate didn't cover them).
- **Mechanical gates** (the real process fix — operator rejected "agent self-discipline"): deny cargo warnings (dead-code is already flagged: cms_locator/resolve_locator), className-without-CSS check, menu-action-without-handler check, e2e-real-connect-or-exercise (not mock), closed-issue-needs-smoke-artifact.

## Process truth (operator-established this session)
"Done" = tests-green, never product-walked → compiled mocks. **The operator is the only reliable acceptance gate**; agents (incl. parallel clones — same model/principles) keep shipping broken-but-"done" work. Fixes that survive: **mechanical gates** (hooks/CI, not agent judgment) + operator smoke. **Never claim "validated/works" without a verification the operator can see.** Surface the exact runnable command; the operator runs the GUI smoke (`pnpm tauri dev`) + on-air steps.

## Disk (operator corrected the memory mid-session)
Per-worktree builds are FINE; just clean up `target/` when done. Disk is NOT tight (~430G free). Use shared `CARGO_TARGET_DIR=/home/administrator/.cache/tuxlink-cargo-target` as an OPTIONAL warm cache. The old "prefix every cargo / --lib-only tiptoeing" was an overcorrection — dropped.

## Git mechanics that bit this session
- In a worktree session, **pin `git -C <worktree>`** for commits/pushes — bare `cd && git commit` trips block-main-checkout-race (another live session active). `git -C` is recognized as worktree-scoped.
- One `pnpm tauri dev` at a time (port 1420, machine-wide). Operator stops/starts; agent never launches it.

## Worktrees in flight
nki (#117, this), 39b (#114), dj6 (#116), 9yx (integration throwaway). Plus pre-existing stale ones (0ic/7fr/etc.). `~/.cache/tuxlink-smoke-*` configs are throwaway.
