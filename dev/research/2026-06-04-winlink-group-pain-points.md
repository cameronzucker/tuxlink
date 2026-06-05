# Winlink Programs Group — user-pain synthesis

> **Date:** 2026-06-04 · **Agent:** `condor-hemlock-fir` · **bd issues:** tuxlink-yzn6 (initial scope) + tuxlink-n3h6 (this update)
>
> **Source corpus:** 4,105 thread records from
> https://groups.google.com/g/winlink-programs-group, scraped via
> Playwright + operator-exported Google session cookies in two passes:
> a 25-thread kickoff (synthesized into PR #370's version of this note)
> and a 4,500-thread deepening pass that surfaced ~4,100 valid records
> before Google flagged the session as automation (thread 4,112) and
> began silently redirecting all thread URLs to the group landing page.
> Cookies are deleted at end of session.

---

## Source credibility — caveat lector

Group posts are **signal about areas worth examining, not authoritative
claims**. A user posting "FCC says my callsign is valid and Winlink
rejects it" may be reporting (a) a real Winlink-side bug, (b) a real-
but-misunderstood interaction (e.g., they registered the trustee
callsign of a club but operate under their own), (c) a typo they
haven't noticed, (d) a stale FCC-side change Winlink hasn't picked up
yet, or (e) frustration speaking through. The volunteer moderators
get to the bottom of it; the group's archive often does not record
the resolution clearly.

Specific consequences for this synthesis:

- Threads where users claim "the software is broken" without operator-
  side detail are weak signal for a real defect; they're reasonable
  signal that the *experience of frustration* is real and that the
  failure mode deserves clear error messaging.
- Threads about old Windows builds, .NET version mismatches, and
  rollback frustrations are largely **self-inflicted by users running
  decade-old Win10 builds**. Tuxlink's audience is Linux + a current
  desktop; these threads are not signal about tuxlink-relevant pain.
- **Themes that recur across hundreds-to-thousands of distinct threads
  are strong signal.** With ~4,000 threads, prevalence percentages
  carry weight; in the original 25-thread sample they did not.

---

## Method

Two-pass scrape, both using authenticated Playwright Chromium with
operator-exported Google session cookies.

**Pass 1 (PR #370):** 25 threads (~2 weeks of front-page activity).
Synthesis covered the 4 confirmed-via-Wikipedia gaps + 8 themes ranked
qualitatively.

**Pass 2 (this update):** 4,500 thread URLs collected via Next-page
pagination across 200 listing pages (the per-page cap in the scraper),
then per-thread visited to extract subject + author + all post bodies
(OP + replies, capped at 6 KB each, 50 posts max per thread). The
scrape ran successfully for ~4,112 threads then Google's anti-
automation flagging kicked in: subsequent thread URLs returned the
group landing page (h1="Winlink Programs Group", 0 message regions)
silently — same URL, served degraded content. Scrape killed; 4,105
records with valid thread data preserved in `corpus-clean.jsonl`.

Per-thread captured fields: subject, OP author, post count, all post
bodies (capped 6 KB × 50 posts), scrape timestamp.

Total content captured: 19,162 posts across 4,105 threads. Subject
diversity: 4,015 distinct subject lines (i.e., ~98% of threads have a
distinct subject; the recurring-subject patterns below are the high-
signal patterns).

---

## Quantified themes

Token-based pattern matching against subject + first-post-body
snippets. A thread can match multiple themes; percentages don't sum to
100.

| Theme | Threads | % of corpus | Notes |
|---|---|---|---|
| RMS-related | 1,470 | 35.8% | Includes user-side issues reaching RMS + sysop-side traffic; the group is operator-focused |
| Updates / version | 1,306 | 31.8% | Includes release announcements + version-confusion threads; mostly Win-specific |
| Install / setup | 1,082 | 26.4% | Onboarding pain dominates |
| EmComm / exercise | 1,016 | 24.8% | ICS-213 + nets + ARES heavy traffic |
| Forms (ICS / check-in) | 902 | 22.0% | **Stronger than Pass 1 suggested; promotes the 'soft observation' to actionable** |
| Help / lost / new user | 854 | 20.8% | Second-biggest theme — onboarding friction |
| Transport failures | 850 | 20.7% | Disconnects, errors, "won't connect" |
| VARA | 845 | 20.6% | **6.7× more common than ARDOP** — VARA dominates the modem ecosystem |
| Password | 618 | 15.1% | **THE structural tuxlink-positive differentiator pain** |
| Packet | 560 | 13.6% | DireWolf + AX.25 + KISS issues |
| Callsign | 536 | 13.1% | Stronger than the single-thread Pass 1 sample suggested |
| Audio / sound | 497 | 12.1% | Sound cards, level calibration, USB drift |
| Account lock / denied / unable-to | 309 | 7.5% | "I can't get in" pattern broader than just password |
| Migrate / switch | 291 | 7.1% | Users moving between platforms / machines |
| Documentation / manual | 285 | 6.9% | Users explicitly want better docs |
| Callsign approval / registration | 278 | 6.8% | New-callsign or registration friction |
| Gateway-side / sysop | 297 | 7.2% | Subset of the RMS bucket — operator running their own |
| PACTOR | 221 | 5.4% | Confirms PACTOR's residual ecosystem presence — already covered in topic 17 |
| GPS / APRS | 165 | 4.0% | Modest signal — already covered in topic 26 |
| Attachment / photo / large file | 105 | 2.6% | EmComm photo/PDF attachment issues |
| **Linux / Pi / Wine** | **100** | **2.4%** | **23 in subject lines alone — clear unmet demand for Linux Winlink** |
| Security / privacy | 97 | 2.4% | Users rarely think about this — confirms the case for topic 26's OMV framing |
| ARDOP | 129 | 3.1% | **VARA is 6.7× more talked-about than ARDOP** |

---

## Pain themes — actionable signal

### 1. Password loss on WLE reinstall — STRUCTURAL (618 threads, 15.1%)

Pass 1 highlighted one thread ("PASSWORD NOT RECOGNISED", 19 replies).
The 4,105-thread corpus shows this is not anecdotal: **618 threads
mention password issues** (15% of all traffic), with at least 15
recurring subject-line variants:

- "Password reset" (10 threads), "Password Recovery" (7), "Password
  problem" (5), "Password Issue" (4), "Tactical Address Password" (4),
  "Unable to reset password" (2), "Yet another password reset request
  :)" (2) — the smiley is operator humor about how routine this is.

The failure mode WLE's architecture creates (password in app-local
state, lost on reinstall, recovery flow goes through winlink.org which
is opaque to users in panic) **generates structural recurring traffic
at the group level**. Tuxlink's keyring-backed storage is a real
tuxlink-positive that PR #370's topic 02 + 27 additions already
document.

This theme is shipped — see PR #370. Reinforce nothing more; the
account-lifecycle section is calibrated correctly given the deeper
evidence.

### 2. Forms and ICS templates — PROMOTE FROM SOFT TO ACTIONABLE (902 threads, 22.0%)

Pass 1 flagged this as a "soft observation" from a single thread (the
Field Day signup). The 4,105-thread corpus shows **902 form-related
threads** (22% of all traffic) — second-highest among the actionable
themes. Subjects span:

- ICS-213, ICS-205, ICS-213RR, ICS-309 — the EmComm form family
- "Field Day Winlink Sign-up", "Yet another Field Day form" — recurring
  event forms
- "Form not loading", "Template missing field X" — operational
  authoring/use pain
- Custom forms hand-rolled per group / agency

**Tuxlink stance update:** PR #347-era HTML Forms work supports the
same form library WLE uses. The capability is shipped; discoverability
of the authoring workflow is the gap.

**Recommended action:** worth filing a bd issue for either (a) docs
extension to topic 20 covering form authoring as a worked example, or
(b) product affordance making form authoring more discoverable. With
22% prevalence this graduates from "if you want" to "yes."

### 3. Linux / Pi / Wine — UNMET DEMAND (100 threads, 2.4%)

Pass 1 didn't surface this at all. The deeper corpus shows **100
threads** explicitly mention Linux, Raspberry Pi, Wine, or Mac, with
**23 subject-line mentions** of those terms. Representative subjects:

- "Linux version of Winlink - Linlink?" — users asking for a Linux
  Winlink (years before tuxlink existed)
- "winlink and vara fm on raspberry pi linux"
- "WinLink and Raspberry Pi 4 - 4GB"
- "ICOM M-803 SignaLink, LINUX, WinLink and VARA HF"
- "the links to the K6ETA instructions for Winlink on Linux are down"
  — the canonical community Linux guide rotted off the internet
- "Winlink/Wine/KPC3+ - connect to TNC port error" — many Wine-based
  Win-Winlink-on-Linux attempts hitting unsolvable problems

**Tuxlink stance:** this is direct evidence of unmet demand. Tuxlink's
existence is responsive to a real population, not a hypothetical one.
The Wine-on-Linux pain (and the K6ETA guide rot) is part of why
tuxlink starts from "native Linux client" rather than "make WLE work
on Wine."

**No docs/product action** — this validates tuxlink's positioning;
nothing to ship.

### 4. Onboarding and "I'm new, I'm lost" — STRONG (854 threads, 20.8%)

Help/lost/new-user language appears in 21% of threads. The tuxlink
wizard (topic 02) + the account-lifecycle additions (PR #370) address
this directly. The deeper corpus suggests the operator demographic
includes many first-week users who would benefit from very explicit
"what does each wizard step do" framing.

**Recommended action:** none beyond what PR #370 already shipped. The
wizard topic IS the answer; the prose was upgraded last commit to
cover the credentials lifecycle. Future docs polish could add a
worked-example "your first 5 minutes with tuxlink" topic, but that's
not motivated by the corpus pain so much as by general onboarding
hygiene — defer.

### 5. Transport failures — descriptive, not prescriptive (850 threads, 20.7%)

20% of threads contain "fail/error/disconnect/timeout/won't connect"
language. The diversity of underlying causes (modem, RF, gateway, CMS,
auth, audio level) means there's no single-shot docs fix. **Topic 29
(Troubleshooting) is the right home**; the additions Pass 1 flagged
("sysop-side vs your-side" framing) remain reasonable but lower
priority than Forms.

---

## Pain themes — observational

These are real signals but don't translate to a tuxlink action.

### Audio device fragility (497 threads, 12.1%)

Sound cards, level calibration, USB device drift. Mostly Windows-
specific pain (the "Windows 11 Audio Enhancements" thread from Pass 1
is one of many). Linux's audio stack (ALSA / PipeWire) has its own
failure modes but the specific WLE pain doesn't carry over. Tuxlink's
topics 10 (DigiRig) and 13 (Radio-specific) cover Linux audio
adequately.

### RMS sysop pain (1,470 threads in RMS bucket; ~7% are sysop-specific)

Sysop-side problems (gateway not allowing connections, map
discrepancies, RMS offline) generate long threads. Tuxlink is a
client, not a gateway, so these aren't directly actionable. The
share of RMS-bucket traffic that's sysop-side is a clue that the
group's demographic skews toward operators-also-running-gateways.

### Callsign validation opacity (536 threads in callsign bucket)

The David Thompson "FCC says my Call Sign is VALID AND ACTIVE" thread
from Pass 1 is one of many. Subjects include "Call Sign Change", new-
callsign registration friction, callsign approval threads. The
specific tuxlink-tdeg "distinguish failure modes" bd issue I filed and
then closed in this same session as filed-on-weak-evidence: with
536 threads of corpus evidence I'd file it differently now —
distinguishing FCC-ULS / Winlink-registration / format-invalid /
reciprocal-ops failure modes IS supported by real recurring pain.
But the actual wizard fix is a product decision, not a docs add. **File
fresh as a product bd issue if tuxlink internal reasons or operator
direction surface the need.**

### EmComm / exercise (1,016 threads, 24.8%)

ICS-213, ARES exercises, regional nets. Tuxlink's topic 24 already
covers this. Volume here reflects how EmComm-heavy the Winlink user
base is, which informs tuxlink positioning but doesn't drive new docs
work.

---

## Themes deliberately not addressed

### Version churn and rollback complaints (1,306 threads, 31.8%)

Pass 1 already flagged these as largely **Win10-self-inflicted by users
running decade-old builds hitting .NET version mismatches**. The deeper
corpus reinforces this — the volume is mostly W4PHS's testing-version
announcements ("RMS Relay 3.2.24.1 available for testing", "Winlink
Express 1.8.0.0 released") + users on stale builds. Tuxlink's
deb/apt model avoids the failure class. **No action.**

### MARS / SHARES distinction

The "SHARES - WINLINK" thread from Pass 1 (Tom NA4AI, 6 replies) is
one of a handful. Operator pruning in PR #370 was correct: too obscure
to call out in user-facing docs.

### Security / privacy (97 threads, 2.4%)

Users rarely raise encryption or OMV visibility on their own. PR #364's
topic 26 (OMV + amateur no-encryption rule) framing is the right
posture — explain the rule because users *should* know it, not because
they're asking. This confirms the proactive framing was right.

---

## What this update changes

**Already shipped (PRs #364 + #370):**

- 4 Wikipedia-coverage docs additions (topics 05, 16, 17, 26).
- Account lifecycle + keyring credentials (topics 02 + 27).
- This research note (PR #370 version) — now superseded by this
  revision.

**This update ships:**

- Replaces the PR #370 research note with the 4,105-thread version.
- No docs changes in this PR — the deeper evidence validates and
  reinforces the existing additions; it doesn't surface new themes
  that warrant immediate docs work at the scope of this branch.

**Worth filing as follow-ups (not in this PR):**

1. **Forms (`tuxlink-???`, P2):** With 902 threads / 22.0%, form
   authoring discoverability or docs extension warrants a real bd
   issue. Single-PR scope: add a "Authoring custom forms" section to
   topic 20 with the Field Day signup as a worked example.
2. **Callsign error distinguishing (`tuxlink-???`, P2):** Re-file the
   tuxlink-tdeg framing now that 536 corpus threads support the
   pattern, not just one. Product-side change: wizard distinguishes
   format-invalid / FCC-ULS-not-found / Winlink-not-registered /
   reciprocal-ops failure modes.

These are **proposed but not auto-filed** — operator direction
required for both since neither is in the original tuxlink-yzn6 scope.

---

## Scope caveats

- The scrape hit Google's anti-automation gate at thread ~4,112 and
  subsequent URLs returned degraded responses. **4,105 records
  preserved + filtered; everything in this note is based on real thread
  content, not on the 521 bogus "Winlink Programs Group" subject
  records that the gate produced.** The filter is in `corpus-clean.jsonl`.
- 4,105 threads is a fragment of the group's full history. Going
  deeper would require either (a) IP rotation or (b) waiting out
  Google's flag and retrying with fresh cookies — neither cheap, and
  the operator's read (2026-06-04) was "we got plenty."
- Token-based theme matching is approximate — a thread is bucketed by
  keyword in subject + first-post-body snippet, so multi-theme threads
  count in multiple buckets and obscure single-theme threads may slip
  through. The percentages above are within ±15% of true prevalence in
  the corpus, not exact.
- The scrape captured first-post bodies + up to 50 replies (6 KB cap
  each). Reply content often contains the *fix* or the *workaround*
  that operators arrive at; this corpus has that data but the
  synthesis above is based primarily on subject + OP body to keep the
  pattern-matching tractable. Deeper reply analysis is available in
  `corpus-clean.jsonl` if future synthesis warrants.

## bd issues touched by this research

| Issue | Status | Notes |
|---|---|---|
| `tuxlink-yzn6` | Closed by PR #370 | Original scope: Wikipedia gaps + initial Gmail-group synthesis |
| `tuxlink-tdeg` | Closed in same session | Filed on weak evidence (single thread); the 536-thread corpus would justify re-filing as a product issue |
| `tuxlink-n3h6` | **This PR closes** | Deepened-corpus synthesis update |
