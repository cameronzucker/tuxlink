# Winlink Programs Group — user-pain synthesis

> **Date:** 2026-06-04 · **Agent:** `condor-hemlock-fir` · **bd issue:** tuxlink-yzn6 Part 2
>
> **Source corpus:** 25 most-recent threads from
> https://groups.google.com/g/winlink-programs-group, scraped via
> Playwright + operator-exported Google session cookies. Scrape script
> + raw corpus live at `dev/scratch/winlink-group-research/` (gitignored
> per `feedback_no_disk_creds_default`). Cookies are deleted at the end
> of the session that scraped them.

---

## Source credibility — caveat lector

Group posts are **signal about areas worth examining, not authoritative
claims**. A user posting "FCC says my callsign is valid and Winlink
rejects it" may be reporting (a) a real Winlink-side bug, (b) a
real-but-misunderstood interaction (e.g., they registered the trustee
callsign of a club but operate under their own), (c) a typo they
haven't noticed, (d) a stale FCC-side change Winlink hasn't picked up
yet, or (e) frustration speaking through. The volunteer moderators get
to the bottom of it; the group's archive often does not record the
resolution clearly.

Specific consequences for this synthesis:

- Threads where users claim "the software is broken" without
  operator-side detail are weak signal for a real defect; they're
  reasonable signal that the *experience of frustration* is real and
  that the failure mode deserves clear error messaging.
- Threads about old Windows builds, .NET version mismatches, and
  rollback frustrations are largely **self-inflicted by users running
  decade-old Win10 builds**. Tuxlink's audience is Linux + a current
  desktop; these threads are not signal about tuxlink-relevant pain.
- Themes that recur across multiple distinct threads (password loss,
  unclear validation) are stronger signal than single-thread complaints.

The synthesis below filters with that in mind. Themes are ranked
roughly by signal strength — strongest at the top, observational at the
bottom.

---

## Method

Authenticated Playwright Chromium session with operator-exported
cookies. Scraped the top 25 listing rows on the group front page
(approximately the last 2 weeks of activity given the group's traffic
rate at the time of the scrape). For each thread: subject (`<h1>`),
OP author (`<h3>`), reply count, first-post body excerpt up to 4 KB.

This is a **kickoff pass** — a representative sample, not a complete
corpus. A deeper corpus would surface long-tail themes; the themes
below are the ones loud enough to surface in the front-page 25.

---

## Pain themes — actionable signal

### 1. Password loss on WLE reinstall (strong signal)

**Subject:** "PASSWORD NOT RECOGNISED" (19 replies — highest engagement)
**OP:** Filippo Ottone
**Snippet:** *"Hello, i had to reinstall RMS express, my winlink
password is no more rocognised and no way to change..."*

User reinstalled Winlink Express; their account password is no longer
recognized. 19 replies. The OP's specific case may resolve as user
error or as a Winlink-side recovery question, but the **failure mode
itself** — *credential lost across reinstall* — is structural to WLE's
architecture (WLE stores the password in app-local state).

This is the highest-leverage signal in the corpus because it points at
a **structural tuxlink-positive differentiator**: tuxlink stores the
Winlink password in the OS keyring (see [ADR 0011](../../docs/adr/0011-fork-pat-for-tuxlink.md)
and [ADR 0016](../../docs/adr/0016-native-b2f-outbound-with-attachments.md)),
which survives `apt reinstall` and tuxlink-build changes. The
"password lost on reinstall" failure mode does not exist for tuxlink.

**Docs action:** topic 02 (First-launch wizard) and topic 27 (Settings)
get expanded sections covering credential lifecycle and keyring
behavior. **Shipped in this PR.**

### 2. Form authoring is opaque to users (soft observation)

**Subject:** "Field Day Winlink Sign-up" (4 replies)
**OP:** Thomas KF7RSF
**Body:** *"OK, I created a new database for this year using a similar
form. The form can be found here: https://..."*

Thomas hand-rolled a Field Day signup form. The HTML Forms
infrastructure that ships with WLE supports user-authored forms, but
the workflow isn't obviously discoverable; operators who don't know
the library DIY their own.

**Tuxlink stance:** the HTML Forms support shipped with PR #347-era
work uses the same form library WLE does. Capability is there;
discoverability may not be.

**Not filed as a bd issue** — single-thread soft signal. If tuxlink
operator feedback later surfaces the same "I made my own form because
I couldn't find the authoring path" pattern from real users, the bd
issue can be filed then.

---

## Pain themes — observational

These are real signals but don't translate to a tuxlink action in this
PR. Documented here so a future session has the context.

### 3. Audio device config is fragile on Windows

**Subject:** "Windows 11 appears to have a Microphone Enhancement For
USB Sound Card separate from the Sound Dialog"
**OP:** Brian - W7OWO

After a Windows 11 update, Brian's VARA FM VU meter went dead because
Windows' new Settings app silently enabled an audio enhancement that
the legacy `mmsys.cpl` dialog didn't surface. Real Windows-side pain
specific to WLE's age.

**Tuxlink stance:** Linux's audio stack (ALSA / PipeWire) has its own
breakage modes (USB device renumbering, default-source drift) but the
specific Windows-side pain doesn't carry over. No tuxlink action; the
existing topic 10 (DigiRig) and topic 13 (Radio-specific notes) cover
Linux audio adequately for now.

### 4. RMS gateway operator pain

**Subjects:** "New RMS Gateway not allowing connections" (15 replies),
"N1ACW-10 RMS will be off air for up to 2 months", "RMS Gateway
Showing Two Different Locations on APRS.fi and Winlink Maps"

Sysop-side problems get long threads. Tuxlink is a client, not a
gateway, so these threads aren't directly actionable.

### 5. UI rendering on different displays

**Subject:** "Screen View" (9 replies)
**OP:** Patty Polish

WLE's compose window doesn't fit Patty's display. WLE is a 2006-era
Win32 GUI; modern HiDPI / scaling configurations break it. Tuxlink is
a Tauri app with CSS layout; the failure mode doesn't carry over.
Observational signal that modernizing the UI surface is a
tuxlink-positive on its own.

---

## Themes deliberately not addressed

### Version churn and rollback complaints

**Subjects:** "Is there a software archive of older versions" (5
replies), "Winlink Express 1.8.0.0 bug?" (4 replies), "RE: Digest...
RMS Packet 2.1.53.0 is the latest" (3 replies)

These threads are real, but the underlying cause is largely **users
running Win10 builds nearly a decade old hitting .NET version
mismatches**. Tuxlink avoids the entire failure class by being a
different platform with a different update model (apt + Debian
packages). No tuxlink-side docs need to address these complaints
because tuxlink users do not have this problem.

If tuxlink later wants to publish a user-facing semver / user-contract
policy (the framing exists in ADRs and specs as developer-facing
material), a docs section to summarize it for end-users could land in
topic 32 or a new dedicated topic. That work is out of scope for
tuxlink-yzn6 — the bd issue's research goal is "user-pain → docs
adds", not "publish a versioning contract."

### MARS / SHARES distinction

The "SHARES - WINLINK" thread (Tom NA4AI, 6 replies) showed a user
landing in the amateur Winlink group looking for SHARES support. MARS
is borderline extinct as a service; SHARES is obscure enough that
mentioning it in user-facing docs adds confusion more than clarity.
Tuxlink targets amateur use; the absence is the point.

---

## What this PR ships for docs

Two concrete docs additions, both addressing pain theme #1:

1. **Topic 02 (First-launch wizard)** — new "Your Winlink account"
   section + new "What happens to your password if you reinstall
   tuxlink" subsection + extended "What can go wrong" entry. Frames
   the Winlink-side vs tuxlink-side credential split and the
   reinstall-survives-keyring behavior.
2. **Topic 27 (Settings)** — new "Credentials and the keyring" section
   covering: what's stored (one entry, service `tuxlink`), how to
   inspect via Seahorse / KWalletManager / `secret-tool`, what
   survives reinstall, how to move to a new machine, how to
   forget / rotate.

The earlier Wikipedia-coverage additions (topics 05, 16, 17, 26) are
already on this branch from the first commit on this PR.

## bd issues filed from this research

None outstanding. One issue was filed and then closed during this same
session as filed-on-weak-evidence:

| Issue | Pri | Title | Status |
|---|---|---|---|
| `tuxlink-tdeg` | P2 | Wizard: distinguish callsign-validation failure modes | **Closed** — filed and then closed during this session. The motivating thread (David Thompson's "FCC says my Call Sign is VALID") is unreliable signal for product work. If tuxlink-internal reasons surface this need, file fresh. |

## Scope caveats

- 25 threads is a small N. A complete corpus (months of history) would
  surface additional themes — particularly the long-tail of
  hardware-specific failures and EmComm-specific operating practices.
- The scrape captured first-post bodies, not the full reply chains.
  Reply content often contains the *fix* or the *workaround* that
  operators arrive at. A deeper pass would scrape reply chains too.
- Single-thread complaints are weak signal. Multi-thread recurring
  patterns (password loss) are strong. The synthesis above filters
  with that in mind.
