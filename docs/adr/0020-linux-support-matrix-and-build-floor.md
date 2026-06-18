# 20. Linux support matrix and the build-floor policy

Date: 2026-06-18

Status: Accepted

Deciders: Cameron Zucker (operator), kingfisher-owl-vetch (agent)

## Context

A pre-alpha tester on Debian 13 (Trixie) Raspberry Pi could not install
`tuxlink_*.deb` ([GH #786](https://github.com/cameronzucker/tuxlink/issues/786)).
That report was diagnosed as environmental (a broken apt state plus `dpkg -i`,
which does not resolve dependencies), but it prompted a CI install-test on clean
Debian images (tuxlink-w636, PR #794). That test surfaced a real, previously
invisible fact: the published artifacts do **not** run on Debian 12 (Bookworm)
or any older release, even though `docs/install.md` claimed Bookworm support.

Two distinct floors were discovered, set implicitly by the build host (the
GitHub Ubuntu 24.04 runners):

1. **glibc 2.39** — the binary references `GLIBC_2.38/2.39` symbols. Bookworm
   ships glibc 2.36, so the `.deb` installed and then died at `ld.so`.
2. **libheif ≥ 1.17** — `libheif-rs = "=1.0.2"` (→ `libheif-sys 2.2.1+1.17.6`)
   pkg-configs the system libheif (Ubuntu 24.04 ships 1.17.6). Bookworm ships
   1.15; building there fails at the `libheif-sys` build script.

The audience makes this concrete. The stated target user runs **EmComm Tools
Community (ETC)**, the Tech Prepper's appliance, which is deliberately frozen on
**Ubuntu 22.10** (glibc 2.36, libheif ~1.12) and
[will not move](https://community.emcommtools.com/faq/operating-system-and-platform.html)
("This will be an appliance"). So the modern floor excludes the primary
audience's platform.

An audit of the crate's C-library bindings found that **libheif is the only
hard source-level version wall**: `libwebp-sys`, `libsqlite3-sys`, `aws-lc-sys`,
and `zstd-sys` vendor their C code (no system-version floor), and the
WebKitGTK-4.1 / GTK3 / OpenSSL stack is present at satisfactory versions on
Ubuntu 22.04+ (Tauri 2 officially supports 22.04). glibc is a build-host
artifact, not a source contract — compiling on an older base erases it for free.

## Decision

**1. The core product targets a modern floor and is not compromised for older
or EOL distros.** Published mainline artifacts are built on Ubuntu 24.04
(glibc 2.39, libheif 1.17) with all features, including HEIC. The `.deb`
declares `libc6 (>= 2.39)` so package managers **refuse cleanly** on a too-old
release (a clear unmet-dependency error) rather than installing a binary that
cannot launch. Supported: Debian 13 Trixie+, Ubuntu 24.04+, current Raspberry Pi
OS, current Fedora/RHEL. (Implemented in PR #794.)

**2. A separate low-floor build caters to the ETC audience without touching the
core.** It is built on Ubuntu 22.04 LTS (glibc 2.35 — covers ETC's 2.36 and
Debian 12 Bookworm) with the `heif` feature **off**. HEIC/HEIF attachments
degrade to a clear "convert to JPEG/PNG" message; every other capability
(Winlink CMS, all transports, mailbox, forms, APRS, maps, SSTV, GPS, and every
other image format including WebP) is unchanged.

**3. Build-floor policy (the durable precedent).** The build-floor distro is the
project's **system-library budget**. Any dependency that requires a system
library newer than the floor distro provides must be **vendored** (build the C
lib from source) or **feature-gated** (default-on, compiled out of the low-floor
build) — or it silently raises the floor and drops the ETC audience. HEIC is the
first application of this rule: `libheif-rs` is an optional `heif` feature
(default-on); the low-floor build omits it.

HEIC is decode-only, so the low-floor accommodation drops a feature rather than
pulling GPL encoders (x265/aom) — no licensing entanglement either way.

## Consequences

- One codebase, two build configurations — no fork. `#[cfg(feature = "heif")]`
  includes the HEIC decoder in mainline and compiles it out of the ETC build.
- Mainline users are unaffected: full features, modern floor, clean refusal on
  unsupported systems.
- ETC / Bookworm users get an installable build missing only Apple HEIC decode.
- New dependencies must be checked against the floor distro. A dep needing a
  newer system lib forces a choice: vendor it, feature-gate it, or accept that it
  raises the floor (an explicit decision, not an accident of runner choice).
- The ETC build targets an EOL base (22.10, no security updates) — a deliberate
  accommodation of the audience's frozen-appliance choice, not an endorsement.
- Truly lowering the *core* floor (e.g. to ship HEIC on Bookworm/ETC) would mean
  static-linking libheif 1.17 + libde265; out of scope here, revisit only if the
  audience warrants it.

## Alternatives considered

- **Build the core in a Bookworm/22.04 container to lower its floor.** Rejected:
  drags the whole product down to the oldest distro's libraries (libheif 1.15
  breaks the build; HEIC would have to be dropped or static-linked for
  *everyone*), compromising the core for an EOL minority.
- **Static-link libheif 1.17 + libde265 into the core.** Rejected for now:
  real build/packaging complexity and binary-size cost imposed on every user to
  serve an EOL base. Kept open as the path if first-class ETC HEIC is ever wanted.
- **Drop sub-Trixie support entirely (no ETC build).** Rejected: the ETC crowd
  *is* the target audience; a feature-gated low-floor build is nearly free once
  `heif` is optional.
- **Two divergent branches (one per audience).** Rejected: a default-on cargo
  feature gives the same result from one reviewed codebase.

## Propagation

This ADR is canonical. `docs/install.md` states the user-facing support matrix
(pointer, not a parallel rule). No CLAUDE.md restatement.
