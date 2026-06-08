# GPS setup UX — design doc

> **Status:** APPROVED 2026-06-05 by operator. Mockup at [docs/design/mockups/2026-06-04-gps-setup-mocks.html](mockups/2026-06-04-gps-setup-mocks.html).
>
> **Authors:** magpie-isthmus-gorge (Claude Opus 4.7) + Cameron Zucker.
> **Mode:** Builder (open-source ham radio software, "give a shit" UX).
> **Supersedes:** none.

---

## Problem statement

Linux GPS setup is famously brutal. Four documented pain points wreck the first-run experience regardless of architecture choice:

1. **ModemManager hijacking USB serial.** NetworkManager's ModemManager runs AT commands at every plugged-in USB serial device, including GPS pucks. The device responds with garbage, ModemManager may wedge it. This is the #1 reason "I plugged in my GPS and nothing worked" on Debian/Ubuntu/Raspbian.
2. **`dialout` group membership requires re-login.** Reading `/dev/ttyACM0` requires being in the `dialout` group. New install → `sudo usermod -aG dialout $USER` → must log out and back in for group membership to take effect. Users add themselves, GPS still doesn't work, blame the software.
3. **`/etc/default/gpsd` config requires sudo + ncurses.** If we lean on gpsd, the user has to edit a system file to point it at their device. Standard Debian path is `sudo dpkg-reconfigure gpsd`, an interactive ncurses dialog Tauri can't drive.
4. **Permission elevation.** Anything touching `/etc/` or `systemctl` needs sudo. Tuxlink runs unprivileged. Surfacing copy-paste commands is one path; `pkexec` (PolicyKit) is the other.

Tuxlink's current state: connects to `127.0.0.1:2947` via TCP, parses gpsd JSON TPV reports, feeds the `PositionArbiter`. Works zero-touch on systems with gpsd pre-configured. On vanilla installs, logs "gpsd unavailable" once and silently falls back to the operator-configured manual Maidenhead grid. No setup wizard step for GPS. No Settings panel for GPS. No detection beyond gpsd.

The Winlink Programs Group user-pain corpus ([dev/research/2026-06-04-winlink-group-pain-points.md](../research/2026-06-04-winlink-group-pain-points.md)) doesn't speak directly to GPS pain, but the shape of the corpus is signal — users post when failures are opaque. The "give a shit" thesis: every tuxlink failure surface explains itself so completely the user never has to post.

## Why this matters now

Per the operator's framing: "Linux is going to be the real make-or-break" for the GPS feature. The userbase is amateur radio operators on Linux — a mix Cameron knows well from his own operating experience and the Winlink Programs Group research. The four canonical personas hitting GPS setup, in rough order of stake:

1. **Sue (W7SUE), EmComm field operator** — ARES deployment, parking-lot QTH, USB GPS puck on a table, can't be editing `/etc/default/gpsd` with the EOC tent flap whipping. **Highest stakes** — the others have time to recover from a stumble; Sue may not.
2. **Dave (N3DAV), Windows-Winlink convert** — 25 years of Winlink Express, first Linux ham app he's ever tried. If the failure surface says "Check your configuration", he uninstalls tuxlink and posts in the Winlink group about Linux being broken. **Highest visibility** — his experience becomes the public narrative.
3. **Bob (K6ABC), seasoned Debian ham** — gpsd already configured because xastir wants it. Easy customer, loudest complainer if we make him click through 5 confirmation screens.
4. **Mike (W4MIKE), fixed-station retiree** — knows his grid, doesn't want GPS, doesn't want to wait 30 seconds for a gpsd timeout to skip. **First-class manual-entry path is mandatory**, not buried under "Advanced."

## Constraints

- **No sudo held by tuxlink.** The pkexec path goes through a tightly-scoped helper binary with a fixed action enum. Tuxlink never caches elevation, never runs arbitrary commands as root, never asks for unprompted privilege escalation.
- **The Linux re-login requirement after `dialout` change is non-negotiable.** Group membership loads at session start, not in a running shell. The "Fix it for me" UX must surface this clearly and offer to log the user out via session APIs (no sudo).
- **Reversibility for every "Fix it for me" action.** ModemManager mask must be unmaskable from the same UI. dialout addition must be safely re-applicable. We don't burn user trust by hiding what we did.
- **No new dashboard ribbon work.** The dashboard GPS pill already exists in tuxlink (per operator). This design emits the Tauri events the existing ribbon consumes; no ribbon rebuild.
- **Settings → Location panel scope.** Per operator clarification, the Settings panel for this feature contains only the GPS / position surface. Radio modem configuration lives in the modem pane, not Settings. The Settings → Location sidebar should show Profile (Callsign / Winlink account / **Location & GPS**) + App (Theme / Privacy) only.

## Premises

1. **The wizard step and the Settings → Location panel render the same React component.** Different chrome (wizard has Back/Continue; Settings has Save/Cancel + tabs) but identical detection + triage + "Fix it for me" surface. This is the architectural insight that makes field-deployment + distro-upgrade flows first-class without doubling the code surface.

2. **Failure mode is the product.** The interesting UX work is what happens when GPS *doesn't* work — the triage cards that detect dialout / ModemManager / missing-device and offer one-click fixes are the differentiator. Bob's "everything just works" path is table stakes; Dave's "the error told me exactly what to type" path is the screenshot users share.

3. **Detection runs entirely unprivileged.** Probing gpsd (TCP connect), listing `/dev/ttyACM*` (readdir), running `udevadm info` (read-only), checking `id -Gn` against `getent group dialout` (read-only), checking `systemctl is-active ModemManager` (read-only on session bus) — none require sudo. 90% of the UX is achievable today with zero elevation.

4. **Sudo enters only via `pkexec`, only for one-shot fixes the user explicitly authorizes.** Tightly-scoped helper binary `/usr/libexec/tuxlink-gps-fix <action>` with a fixed enum (`add-dialout`, `mask-modemmanager`, `unmask-modemmanager`). PolicyKit policy in `/usr/share/polkit-1/actions/com.tuxlink.app.policy` authorizes only the helper. User types password in a system-rendered dialog and consents to one specific named action.

5. **Stations are stood up in one place and deployed somewhere else.** Field deployment, distro upgrade, device swap are first-class flows. The Settings → Location panel's three tabs (Troubleshoot / Source picker / Manual grid + Privacy) cover the field-deployment loop. Background detection task expands to notice changes without the user opening Settings.

## Cross-model perspective

Skipped this brainstorm. The architectural shape was driven by source-level reading of tao-0.35.2 / wry-0.55.1 / tauri-plugin-window-state + the operator's own product taste (Laserfiche Support Tools as the proof that shell friction is real for technical users). No Codex consult run for this brainstorm; if the bd-1 implementation needs cross-provider review, it'll happen at the build-robust-features phase.

## Recommended approach

A four-bd chain. bd-1 is the foundation that ships Bob + Mike + manual paths immediately and the diagnostic half of Dave's path. bd-2 unlocks Dave's "magic button" experience. bd-3 unlocks Sue's zero-config USB GPS. bd-4 wires the existing dashboard ribbon to live monitoring so the field-deployment loop becomes truly observable.

### Scope-correction note (2026-06-05, post-Codex review)

Codex's adversarial review of the bd-1 plan caught a scope creep: the original "State B" mockup (native NMEA "Use this" green source for Sue's path) is **bd-3 territory** because it depends on the `ProviderArbiter` refactor and the native NMEA reader. **bd-1's State B is now scoped to a diagnostic info card** that names the detected USB GPS, surfaces two complete paths forward (install gpsd + manual grid), and tells the operator the green-source one-click experience arrives in the next release. The bd-3 mockup is preserved as the design target for that ship.

This split honors the alpha-quality bar: every shipped surface is honest about what works today vs what's coming, with no placeholder stubs or incomplete user-facing refs (per `[[no-incomplete-or-internal-refs-in-shipped-features]]`).

### bd-1: GPS source-picker React component + Settings → Location wiring

**Scope.** A single React component (`GpsSourcePicker` or similar) that owns:

- **Detection probes** (Tauri commands, all unprivileged):
  - `gps_probe_gpsd()` — TCP connect to 127.0.0.1:2947 (existing code factored into a probe).
  - `gps_probe_serial_devices()` — readdir `/dev/ttyACM*` + `/dev/ttyUSB*`; for each, `udevadm info -q property` to extract `ID_VENDOR_FROM_DATABASE`, `ID_MODEL_FROM_DATABASE`, `ID_VENDOR_ID`, `ID_MODEL_ID`.
  - `gps_probe_dialout_membership()` — read `id -Gn $USER` or use `nix::unistd::Group::from_name("dialout")` + `getgroups()`.
  - `gps_probe_modemmanager_status()` — D-Bus query to the session bus for `org.freedesktop.ModemManager1` presence + `systemctl is-active ModemManager` via `zbus`.
  - `gps_probe_bluetooth_nmea()` — BlueZ device enumeration (optional in bd-1; can defer).
- **State machine** (XState-style or useReducer): probes run in parallel, results render as source-cards (working sources) or triage-cards (blocked sources). Manual grid is always available as a card.
- **UI surfaces** (both chromes):
  - Wizard Step 4 wrapper: `<WizardChrome step="4 of 6" backLabel="Back" continueLabel="Continue">` around the picker.
  - Settings → Location wrapper: `<SettingsTabsChrome tabs={['Troubleshoot', 'Source picker', 'Manual grid', 'Privacy']}>` around the picker.
- **Triage cards** with "Show command" buttons that expand the exact sudo command + a copy button. **"Fix it for me" buttons are present but disabled** with tooltip "Coming in next release" — they activate in bd-2.
- **Manual grid editing** + 4-char / 6-char precision-reduction radio per [[gps-precision-reduction]] memory.
- **Source switching** without restart — the `PositionArbiter` accepts source changes at runtime.

**Out of scope for bd-1:** the pkexec helper, the native NMEA reader (gpsd-only for now), the Bluetooth NMEA reader (optional), live detection monitoring.

**Acceptance criteria.**
- Bob's wizard path: gpsd detected, "Use this" pre-selected, one Enter completes the step.
- Mike's wizard path: "Skip — I'll enter my grid manually" available as a first-class option from any state, no GPS timeout required.
- Dave's wizard path: triage cards render with `dialout` / ModemManager / no-device diagnoses. "Show command" reveals copy-pasteable sudo command. "Fix it for me" disabled with "Coming soon" tooltip.
- Settings → Location panel: same picker, same triage, same source-meta rendering. Three tabs (Troubleshoot / Source picker / Manual grid) + Privacy as a fourth.
- Detection probes run in &lt;500 ms p95 (parallel; gpsd is bounded by TCP backoff which we override to a 200 ms timeout for the wizard probe).
- Manual grid validation reuses [src-tauri/src/position/maidenhead.rs](../../src-tauri/src/position/maidenhead.rs).
- All unit tests for probe functions pass; component-level tests cover each persona's path.

**Effort estimate:** 1 week eng time for a careful pass.

### bd-2: GPS pkexec helper binary + PolicyKit policy + "Fix it for me" wiring

**Scope.** Activates the "Fix it for me" buttons disabled in bd-1.

- **Helper binary `/usr/libexec/tuxlink-gps-fix`** (small Rust binary, ~50 LOC) with a fixed action enum:
  - `add-dialout` → `usermod -aG dialout $REAL_USER` (resolves via `pkexec`'s `$PKEXEC_UID` environment).
  - `mask-modemmanager` → `systemctl mask ModemManager`.
  - `unmask-modemmanager` → `systemctl unmask ModemManager` (reversibility commitment).
- **PolicyKit policy** at `/usr/share/polkit-1/actions/com.tuxlink.app.policy`:
  ```xml
  <action id="com.tuxlink.app.gps-fix">
    <description>Tuxlink wants to adjust GPS-related system configuration</description>
    <message>Tuxlink would like to fix a GPS configuration issue. This will require entering your password. You can reverse the change from Settings → Location → Troubleshoot.</message>
    <icon_name>com.tuxlink.app</icon_name>
    <defaults>
      <allow_active>auth_admin</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/usr/libexec/tuxlink-gps-fix</annotate>
  </action>
  ```
- **Bundle wiring** — `linux.deb.files` ships both the binary and the policy.
- **Tauri-side spawner** — Rust command that invokes `pkexec /usr/libexec/tuxlink-gps-fix <action>`, captures exit code + stderr, emits a Tauri event with the result, re-runs the relevant detection probe.
- **Post-fix flow for dialout:**
  - Action completes → UI shows: "Done. You need to log out and back in for this to take effect — that's a Linux rule we can't bypass. Want me to log you out now?"
  - "Log out now" invokes `loginctl terminate-session` or `gnome-session-quit --logout` (operator-side, no sudo for the user's own session).
  - If user defers, dashboard pill stays amber until next login confirms group membership.
- **Tests:** helper binary stdout contract, PolicyKit policy syntax validation, Rust-side error handling for pkexec exit codes (1 = action failed, 126 = auth dismissed, 127 = pkexec not installed).

**Acceptance criteria.**
- Dave clicks "Fix it for me" on the dialout card → PolicyKit auth dialog renders with the tuxlink icon + the policy message → password entered → command runs → "log out and back in" notice appears.
- ModemManager mask works identically; the reverse (`Unmask ModemManager`) is reachable from the Settings → Location → Troubleshoot tab even after the fix succeeded.
- pkexec absent / disabled → "Fix it for me" buttons hidden with explanation "PolicyKit not available — use Show command".
- The helper binary rejects unknown actions and refuses to execute without `$PKEXEC_UID` set.

**Effort estimate:** 3-4 days eng time.

### bd-3: Native NMEA serial reader (no gpsd required)

**Scope.** Sue's full zero-config path.

- Use existing `serialport` crate (in `Cargo.toml` from `tuxlink-7fr`) to open `/dev/ttyACM*` or `/dev/ttyUSB*` directly when gpsd is unavailable.
- NMEA parser (small dependency or hand-rolled — NMEA 0183 is well-documented): handle `$GPGGA`, `$GPRMC`, `$GPGSA` sentences. Reuse the lat/lon-to-Maidenhead conversion in [src-tauri/src/position/maidenhead.rs](../../src-tauri/src/position/maidenhead.rs).
- Source picker UI: when gpsd is unreachable AND a serial device is detected AND the user is in dialout, the native source becomes the "recommended" green card with "Use this" pre-selected.
- `PositionArbiter` extension: add a `Source::NativeNmea { device_path, vendor, model }` variant alongside the existing gpsd source. Privacy gating (`gps_state`) applies identically.
- Bluetooth NMEA via `/dev/rfcomm*` is a follow-up under this same bd or a sub-bd, depending on complexity.

**Acceptance criteria.**
- Sue's EOC arrival path: no gpsd reachable + USB GPS on `/dev/ttyACM0` → tuxlink reads it directly → grid displayed in Settings + dashboard pill within 5 s of plug-in.
- gpsd present + native source both available → user can switch in Source picker; PositionArbiter respects the selection.
- USB device disconnect → source dies cleanly, dashboard pill goes red, manual fallback activates.

**Effort estimate:** 4-5 days eng time.

### bd-4: Live GPS detection monitoring + dashboard event emission

**Scope.** Wires the existing dashboard pill to react to GPS state changes mid-session.

- Background tokio task expands `run_gpsd_client` to also poll:
  - `/dev/ttyACM*` device enumeration (every 5 s — cheap readdir).
  - ModemManager presence (every 30 s — D-Bus query).
  - dialout group membership (only at startup + after a successful pkexec `add-dialout` — group membership doesn't change without explicit action).
- State change detection: when the source set changes (new device, source disappeared, fix age exceeded threshold), emit a Tauri event:
  - `position://source-changed` — payload includes old + new source IDs, reason (new-device / source-lost / config-changed).
  - `position://fix-stale` — payload includes age in seconds, last good fix.
  - `position://triage-required` — payload includes the open diagnostic codes (dialout-missing, modemmanager-running, no-device).
- **The dashboard ribbon already exists** and consumes these events (operator confirmed). This bd ships the event source, not the consumer.
- **In-app toast for major changes** — small bottom-right notification ("GPS changed: new device detected · Review") that auto-dismisses after 8 s. Click bounces to Settings → Source picker.
- **Modal interrupt only when GPS becomes unavailable during an active Winlink connection** — "GPS is now dark. Using stored manual grid FN31pr. Continue session?" Yes/No/Open Settings. This is the only case where silence is harmful (broadcasting wrong position mid-session).

**Acceptance criteria.**
- Sue's EOC arrival: laptop boots, gpsd unreachable, USB puck plugged in → `position://source-changed` fires → amber pill on dashboard → click jumps to Settings → Source picker with the new device pre-selected as recommended.
- Dave's overnight distro upgrade: morning launch, dialout lost, ModemManager running → `position://triage-required` fires at startup → red pill → click jumps to Settings → Troubleshoot with both cards expanded.
- Mid-Winlink-session GPS loss: modal interrupt only, not for idle-app GPS loss.
- Toast notifications are debounced: rapid state changes (USB device flapping on a loose cable) collapse into a single toast.

**Effort estimate:** 3-4 days eng time.

## Implementation order

bd-1 first — it ships value alone and unblocks the others. bd-2 next (activates Dave's magic button; this is the "screenshot for the club Slack" milestone). bd-3 and bd-4 can ship in either order; bd-3 expands the working source set, bd-4 makes the existing surface live.

If we want to ship the headline "screenshot for the club Slack" moment in a single PR train, bd-1 + bd-2 land together as a single PR or stacked pair. bd-3 and bd-4 follow.

## Distribution plan

No new distribution surface. All four bds land in the existing `.deb` / `.rpm` / AppImage bundle via the packaging pipeline shipped in PR #325. The pkexec helper binary (bd-2) ships via `linux.deb.files` overlay; the PolicyKit policy file ships the same way. AppImage users get `gpsd`/`pkexec` support iff their host system has them — the AppImage cannot ship a PolicyKit policy that gets registered with the system, so the "Fix it for me" buttons gracefully degrade to "Show command" on AppImage installs.

## Open questions

None blocking implementation. Two non-blocking polish questions surfaced during the brainstorm but were deferred:

1. **Bluetooth NMEA scope** — phone-as-GPS and Bluetooth GPS pucks are common, but BlueZ pairing has its own UX. Initial implementation under bd-3 covers `/dev/rfcomm*` if the operator has already done the pairing externally. Full in-app pairing wizard is a follow-up if user feedback says it matters.

2. **`pkexec` absence on minimal installs** — minimal Debian / Alpine-on-the-Pi installs may lack PolicyKit. The graceful degradation ("Fix it for me" hidden, only "Show command" available) is the answer. No proactive policy.

## Success criteria

- A new Linux ham who has never configured a GPS device can complete the wizard step from a stock distro install with no terminal trip if their distro has PolicyKit (default on Debian/Ubuntu/Fedora/openSUSE/Mint).
- An EmComm field operator can deploy from a configured home QTH to a field site with a different GPS device and have the Settings → Location panel surface the right path inside 60 seconds.
- An operator whose distro upgrade broke their GPS gets a red dashboard pill on next launch, clicks it, and is one "Fix it for me" + a re-login away from working.
- A fixed-station retiree can skip GPS entirely in the wizard with one click, no GPS timeout, no friction.
- Every diagnostic the triage panel surfaces is **specific** ("you're not in `dialout`") and **actionable** ("here's the exact command, here's a button to run it for you").

## Next steps

1. ✓ This design doc written + APPROVED.
2. ✓ bd issues filed with dependency edges:
   - **tuxlink-9xy1** (P1) — GPS source-picker component + Settings wiring (foundation).
   - **tuxlink-m9ej** (P1) — pkexec helper + PolicyKit policy + "Fix it for me" wiring. Depends on tuxlink-9xy1.
   - **tuxlink-ley0** (P2) — native NMEA serial reader. Depends on tuxlink-9xy1.
   - **tuxlink-gnws** (P2) — live monitoring + dashboard event emission. Depends on tuxlink-9xy1 + tuxlink-ley0.
3. ⏭ tuxlink-9xy1 implementation begins. Recommended skill route: `superpowers:brainstorming` was this conversation; `superpowers:writing-plans` next; `superpowers:build-robust-features` for the implementation pass.
4. ⏭ Mockup at [docs/design/mockups/2026-06-04-gps-setup-mocks.html](mockups/2026-06-04-gps-setup-mocks.html) stays as the visual anchor for the component implementation. Update if scope shifts during build.

## What I noticed about how Cameron thinks

- **You named Laserfiche Support Tools as the proof that shell friction is real even for technical users.** That's a direct piece of practitioner evidence behind a product decision — the kind of thing most PMs would handwave as "users want fewer clicks." You named the specific product you built, the specific userbase, the specific decision. That's why the pkexec call landed in this design instead of getting deferred to v2.
- **You flagged the field-deployment loop before I did.** "Stations are stood up in one place then deployed somewhere else, which is extremely common." The mockup I wrote assumed the wizard was the only entry. Your pushback added an entire architectural dimension (wizard + Settings share one component) that genuinely improved the design. That's user-empathy-driven product instinct at work.
- **You corrected the Settings sidebar scope without being asked.** "Should not have the radio modem stuff since we handle that in the modem pane itself." You know your own architecture. The mockup was assuming a flat Settings panel; you're modeling it as feature-scoped sub-surfaces. That's the kind of consistency that separates products users love from products users tolerate.
- **You named the existing dashboard ribbon as out-of-scope.** "What we have works great." A weaker product owner would let me redesign it because the mockup looked nice. You're protecting working things from cosmetic churn. That's mature taste.
