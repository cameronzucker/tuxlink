# GPS setup UX design — adversarial review addendum (rounds 2-4)

> **Status:** Synthesis of three Claude adversarial review rounds. Round 1 (Codex architectural) hit OpenAI quota; Round 5 (Codex synthesis) deferred ~90 min until quota resets — will run against the WRITTEN PLAN rather than the design doc, which is higher leverage.
>
> **Parent doc:** [2026-06-05-gps-setup-ux-design.md](2026-06-05-gps-setup-ux-design.md)
> **Raw transcripts:** local-only in agent context (not on disk; the Codex transcript that DID land is the quota-error stub).
> **Author:** magpie-isthmus-gorge synthesis of three subagent reviews.

---

## TL;DR — what changes in the implementation plan

Three rounds (Claude security / Claude UX-persona / Claude implementation-feasibility) surfaced **55 findings.** No findings invalidate the architectural direction (pkexec + tightly-scoped helper + shared component for wizard / Settings is still the right shape). But several findings materially re-scope the work:

1. **bd-1 is closer to 2-3 weeks than 1 week.** The wizard chrome doesn't have a "Step 4 of 6" concept currently, the Settings panel has no tabs framework, and "same component, different chrome" is technically a 3-component split (presentational + wizard-container + settings-container) because of wizardContext coupling.

2. **bd-4's premise that the dashboard ribbon already consumes `position://` events is empirically false.** `grep -rn "position://"` returns zero hits in the codebase; the existing ribbon polls `position_status` via TanStack Query, doesn't subscribe to events. bd-4 must build the event consumer (subscription module + debounced toast + modal-interrupt logic), not just emit the events.

3. **PositionArbiter needs decoupling.** The design conflates "provider routing" (which loop is feeding fixes — gpsd vs native NMEA vs Bluetooth) with "user-intent source" (Manual vs GPS, which the privacy gate already uses). Introduce a `ProviderArbiter` layer above the existing `PositionArbiter`; this is a real refactor, not a variant addition.

4. **The PolicyKit policy must split into three action IDs**, not one. `com.tuxlink.app.add-dialout` / `com.tuxlink.app.mask-modemmanager` / `com.tuxlink.app.unmask-modemmanager` — each with a distinct, named-action message so consent is specific not vague.

5. **UX-5 is critical and unaddressed:** after a `dialout` fix the design says "log out and back in" but doesn't define wizard-resume-on-relogin behavior. Persisting wizard progress + a `pending-dialout-verification` flag is a real product surface, not a nice-to-have.

6. **"Log out for me" is not portable across desktop environments.** Cutting that affordance and replacing with a copy-paste prompt is the honest call. GNOME / KDE / XFCE / sway / hyprland all have different session-quit APIs and pandora (labwc) has none of them.

7. **Three Claude rounds + one failed Codex round + one deferred Codex round.** Plan-writing proceeds on the synthesis below; the deferred Codex run targets the written plan as the highest-leverage adversarial check.

---

## What the adversarial review confirmed is genuinely solid

Before listing what changes — what stands:

- **The shared-component architectural call** is the right shape (even though IMPL-2 corrects the framing to "shared presentational + distinct containers").
- **The "failure mode is the product"** thesis. All three reviewers independently endorsed the triage-panel approach.
- **The pkexec via tightly-scoped helper binary** is the right security architecture — better than sudo NOPASSWD, better than a privileged daemon, better than copy-paste-only.
- **Mike's first-class manual path** without GPS timeout — explicitly endorsed.
- **Privacy precision-reduction defaults** (4-char Maidenhead per the APRS convention memory) — confirmed correct.
- **Reversibility commitment** — the architectural intent is right; UX-19 only flags discovery, not the commitment.
- **Per-action PolicyKit prompts** (auth_admin not auth_admin_keep, password every time) — correct.

---

## Findings — categorized + collapsed to the action they force

Findings are renumbered for the plan. Each one shows: severity, what changes, which round flagged it.

### A. ARCHITECTURE & SCOPE

**A1 — Wizard step framework is undefined.** The current wizard has 5 step IDs (`account | credentials | offline_identity | cms_verify | complete`) and no "n of m" numeric chrome. Inserting a GPS step requires either inventing a global step-numbering contract (which touches Step1-3 chrome too) or dropping the "Step 4 of 6" framing in the mockup. *Severity: HIGH. From IMPL-1.*

**Action:** Plan picks one. Recommended: drop "Step 4 of 6" framing globally; use named transitions instead ("Welcome → Callsign → Account → **Location** → Test send → Done") rendered as a vertical progress sidebar without numbers. This preserves the wizard's current step contract.

**A2 — "Same component, different chrome" is technically false; it's a 3-component split.** `useWizard()` throws outside `<WizardProvider>` ([wizardContext.tsx:30](../../../../src/wizard/wizardContext.tsx#L30)). The Settings panel can't reuse the wizard's `useWizard()`-coupled component. *Severity: HIGH. From IMPL-2.*

**Action:** Plan factors three components:
- `GpsSourcePickerPresentational` — pure props in / events out, no context coupling.
- `WizardGpsStep` — wraps the presentational with `useWizard()` consumer + wizard reducer dispatch.
- `SettingsGpsPanel` — wraps the presentational with `config_read` / `config_set_grid` / `position_set_source` consumer.

**A3 — PositionArbiter conflates provider with user-intent source.** Today `PositionSource` is `Manual | Gps` (used for the privacy gate). Adding `NativeNmea` to the same enum breaks the gate's semantics and rippsles through all `match a.source()` sites. *Severity: HIGH. From IMPL-3.*

**Action:** Plan introduces a new `ProviderArbiter` layer:
- `enum PositionProvider { Gpsd, NativeNmea(DeviceMeta), Bluetooth(DeviceMeta) }` held at the provider-loop layer.
- `tokio::sync::watch::Sender<ProviderConfig>` for runtime switching; loops subscribe.
- `JoinHandle + AbortHandle` retained in App state for cancellable provider tasks.
- The existing `PositionArbiter` stays Manual/Gps; the new layer routes which provider feeds it.

**A4 — Dashboard ribbon does NOT consume position:// events.** Grep evidence: zero hits for `position://`, `source-changed`, `fix-stale`, `triage-required`. Existing surface polls `position_status` ([useStatus.ts:347-353](../../../../src/shell/useStatus.ts#L347-L353)). *Severity: HIGH. From IMPL-10.*

**Action:** bd-4 scope expands to include:
- React event-subscription module that listens for `position://*` events.
- Debounced toast renderer (8 s auto-dismiss; 3 s debounce window).
- Modal-interrupt component for mid-Winlink-session GPS loss.
- The dashboard ribbon's polled `gpsReady` state stays as-is; the event consumer is additive.

**A5 — Settings panel has no tabs framework.** Today's `SettingsPanel.tsx` is a flat dialog. The four-tabs design (Troubleshoot / Source picker / Manual grid / Privacy) requires a sub-tab UI built from scratch. *Severity: MEDIUM. From IMPL-20.*

**Action:** Plan picks: **expandable-section design within the existing flat panel**, not a tabs framework. The four "tabs" become four expandable sections under a "Location & GPS" header. Cheaper, preserves existing Settings panel architecture, and the user's mental model ("scroll to find GPS, expand to see Troubleshoot") is fine for a settings page.

**A6 — Helper binary build order requires `converge-build.sh` update.** Tauri bundler doesn't auto-build sibling `[[bin]]` targets. `cargo build --release --bin tuxlink-gps-fix` MUST run before `cargo tauri build` so `linux.deb.files` can stage the artifact. *Severity: HIGH. From IMPL-5.*

**Action:** Plan adds a workspace split OR feature-gated minimal bin (the helper should be ~500 KB not 12 MB). `scripts/converge-build.sh` learns to sequence the helper build. AppImage path: helper doesn't ship in AppImage (graceful degradation is the expected behavior).

### B. SECURITY

**B1 — Split pkexec actions, distinct messages.** A single action ID for all three operations means consenting to one consents to all. *Severity: MEDIUM. From S-6.*

**Action:** Three action IDs in the policy file:
- `com.tuxlink.app.add-dialout` — "Tuxlink would like to add you to the **dialout** group so it can read GPS / serial devices."
- `com.tuxlink.app.mask-modemmanager` — "Tuxlink would like to **disable ModemManager system-wide**. This prevents it from interfering with GPS devices but **also disables USB cellular modem support**. Reversible from Settings → Location → Troubleshoot."
- `com.tuxlink.app.unmask-modemmanager` — "Tuxlink would like to **re-enable ModemManager**. This will re-allow it to probe USB serial devices, which may interfere with GPS."

**B2 — `$PKEXEC_UID` is a sanity check, not a security boundary.** The helper must document this clearly and add input validation on the resolved username. *Severity: HIGH framing-wise. From S-2.*

**Action:** Helper code comments call out that `$PKEXEC_UID` is set by pkexec (not user-controlled) AND validates the resolved username against `^[a-z_][a-z0-9_-]*$` regex before passing to `usermod`. Defense in depth against a hypothetical pkexec bug.

**B3 — Verify state-change actually happened after helper return.** A fake pkexec or a real one that succeeded-but-the-action-was-a-noop both look identical to "exit code 0." *Severity: LOW. From S-7.*

**Action:** After every helper invocation, re-run the relevant detection probe. Compare pre/post. If no state change, surface "the fix command returned success but state didn't change — your system may have a setup that prevents this fix."

**B4 — Tuxlink-side log of helper invocations.** Pkexec logs to journald but tuxlink doesn't log when it INVOKES pkexec. Cross-reference forensics. *Severity: INFORMATIONAL. From S-9.*

**Action:** Append-only log entry to tuxlink's app log: timestamp + action + originating UI event ID. Two timestamps + correlation against journald gives end-to-end forensics.

**B5 — Explicit `.service` suffix + `--no-ask-password --quiet`** for systemctl invocations. *Severity: LOW. From S-10.*

**Action:** Helper invokes `systemctl --no-ask-password --quiet mask ModemManager.service` (and same for unmask). Trivial polish.

**B6 — One helper per discrete feature, not a kitchen-sink helper.** If future tuxlink features want similar elevation, they get their own helpers with their own policies, not new actions in `tuxlink-gps-fix`. *Severity: INFORMATIONAL. From S-8.*

**Action:** Add ADR-style note in the design doc + helper source: "this helper is GPS-specific; future features get their own helpers." Caps blast radius.

### C. UX & DISCOVERABILITY

**C1 — Wizard resume after dialout-fix logout is undefined. Critical UX gap.** *Severity: CRITICAL. From UX-5.*

**Action:** Plan adds:
- Wizard state persisted to `~/.config/tuxlink/wizard-state.json` (or via existing Tauri state mechanism) before logout dispatch.
- `pending-dialout-verification: true` flag set when the dialout fix is dispatched.
- On next launch, wizard checks the flag; if set, lands user on the Location step with a banner: "Welcome back. Let's verify the dialout fix worked." Auto-rescan, show result.
- If verification fails, surface the specific diagnosis (still not in group, OR group exists but device still fails).

**C2 — "Log out and back in" is incomprehensible to Linux-novice users.** Many users conflate "log out" with "shut down", "close window", or "lock screen." *Severity: HIGH. From UX-12.*

**Action:** Replace literal "log out and back in" instruction with desktop-vocabulary framing:
- Detect DE via `$XDG_CURRENT_DESKTOP`.
- If GNOME / KDE / XFCE / Cinnamon: "Your computer needs to load your new permissions. Click here to sign out (your apps will close; tuxlink will pick up where it left off when you sign back in)."
- If no detectable DE (sway/hyprland/labwc): "Linux loaded your new permissions, but only when you sign back into your computer. Sign out from your top bar, then sign back in."
- Drop the "Log out for me" auto-button entirely — per IMPL-7, no portable cross-DE API exists.

**C3 — gpsd connected ≠ working.** Probe must verify a fresh 3D fix, not just TCP handshake. *Severity: HIGH. From UX-1.*

**Action:** `probe_gpsd()` returns a 3-tier result:
- `Connected + fresh 3D fix < 5 s old` → green source card.
- `Connected + no fix OR stale fix > 30 s` → amber card: "gpsd is running but isn't getting a position. Your device may be indoors, disconnected, or pointed at a different `/dev/` than gpsd is configured for."
- `Connected + reports a different DEVICES path than what's currently plugged in` → amber card with source-switch offer.

**C4 — Auto-switch policy undefined.** What happens when gpsd dies AND a USB puck appears? Silent switch or modal? *Severity: HIGH. From UX-4.*

**Action:** Plan specifies the policy explicitly:
- **Never silently auto-switch a working source.**
- **Auto-switch only when:** current source is dead AND exactly one alternative is healthy AND user has set "prefer auto-switch" preference (default: OFF for EmComm-leaning safety).
- **In all other cases:** dashboard pill goes amber, in-app toast offers the switch, user confirms.

**C5 — Mike has no in-app discovery path post-wizard.** New device plugged in after manual-grid setup goes silently undetected by Mike. *Severity: HIGH. From UX-7.*

**Action:** Background detection (bd-4) emits a low-friction toast when:
- A new GPS device is detected that wasn't present at wizard time, AND
- The current source is manual-grid (not deliberately disabled in privacy settings).
- Debounced: once per (device-id × install) lifetime.

**C6 — Screen-reader accessibility of triage cards.** Color-only severity, unlabeled code blocks, copy-button announcement gaps. *Severity: HIGH. From UX-8.*

**Action:** Plan adds explicit a11y requirements:
- Each triage card has a text severity label ("Critical: …", "Warning: …", "Info: …") in addition to color.
- Icons (`✗ ⚠ ○`) have `aria-label` matching the severity.
- Code blocks are `<pre>` with `aria-roledescription="shell command, copyable"`.
- Copy button announces "Copied" via `aria-live="polite"`.
- After "Fix it for me" success, polite-priority live region announces "Fix applied; re-login required."
- QA pass: tuxlink boots on labwc with Orca enabled; magpie-isthmus-gorge or operator walks the wizard.

**C7 — Remote-shell pkexec degrades silently.** SSH X11 / x2go / NoMachine sessions render the auth dialog on the wrong display or not at all. *Severity: HIGH. From UX-10.*

**Action:** Detect remote session at probe time (`$SSH_CLIENT` or `$DISPLAY != :0` or `$XDG_SESSION_TYPE == "tty"`). When detected, surface in the wizard intro AND triage panel: "You're connected remotely. 'Fix it for me' buttons require a local desktop session — use 'Show command' instead." Hide the buttons in this state.

**C8 — ModemManager warning is buried fine print.** The mask fix has system-wide effects (USB cellular modem support gone). *Severity: HIGH. From UX-18.*

**Action:** Plan replaces the mask card UX:
- Primary button changes from "Fix it for me" to "Disable ModemManager (affects USB cellular modems)".
- On click, an explicit confirmation modal: "This disables USB cellular modem support system-wide. Do you use a USB cellular modem? [No, disable ModemManager] [Yes, let me think about this]." Only after "No" does pkexec spawn.
- **Alternative considered (UX-18 + IMPL-18):** drop the udev rule that excludes GPS vendors from MM probing instead of masking MM entirely. Less destructive; equally effective. Plan investigates this as the preferred fix in bd-2; full mask becomes a fallback.

**C9 — AppImage degradation undermines the screenshot thesis.** Friends of Dave who try the AppImage can't reproduce the "magic button" moment. *Severity: HIGH. From UX-20.*

**Action:** Plan ships .deb as the **only blessed install path** for v0.0.1's GPS feature debut. The AppImage works (with degraded "Show command" only) but the wizard's intro screen surfaces: "For the full GPS-setup experience including one-click fixes, install the .deb package — link." Doesn't gate; nudges. v0.0.1 release notes prominently document this as "current limitation, AppImage parity is bd-XXXX."

**C10 — 60s GPS fix-age threshold needs RF-context rationale.** Bob mid-30-minute Pat session shouldn't trip a modal because of momentary GPS clouds. *Severity: MEDIUM. From UX-13.*

**Action:** Plan splits fix-age thresholds:
- **Session-start freshness:** 60 s. Tight, because position MUST be correct when broadcast.
- **Mid-session staleness:** position uncertainty must exceed broadcast precision before tripping. 4-char broadcast tolerates ~70 km uncertainty; that's roughly 30 minutes of no-fix for a stationary station. 6-char broadcast tolerates ~5 km; ~2 minutes.
- Modal interrupt fires only when uncertainty crosses broadcast-precision boundary, not on absolute fix age.

**C11 — "Active Winlink connection" boundary undefined.** What counts? B2F-in-progress? Auto-poll cycle? Catalog request? *Severity: HIGH. From UX-14.*

**Action:** Plan cites the existing session state machine ([location TBD; investigate src-tauri/src/winlink/session/]) and defines:
- **Active states triggering modal:** `Connecting | Authenticating | TransferringMessage | Closing`.
- **Idle states using toast only:** `Idle | Polling`.
- The 0.1s-poll between auto-poll cycles does NOT count as active.

**C12 — "Re-run wizard" path is undefined.** No way to get back into the wizard's hand-holding flow post-setup. *Severity: MEDIUM. From UX-16.*

**Action:** Plan adds Help menu item: "Re-run setup wizard". Resets relevant config state with confirmation, re-launches Step 1.

**C13 — 4-char vs 6-char precision picker missing from wizard happy paths.** Auto-detected paths skip the precision choice; user lands in 4-char default forever without seeing the option. *Severity: MEDIUM. From UX-17.*

**Action:** In wizard states A and B (success paths), surface a one-line confirmation row above Continue: "Broadcasting at 4-character precision (`EM35` from your fix `EM35vx`). [Change…]". One-click change to 6-char with rationale.

**C14 — Undo affordance for "Fix it for me" is hidden under Troubleshoot.** Users who change their minds within seconds don't have a contextual Undo. *Severity: MEDIUM. From UX-19.*

**Action:** Plan adds 8-second toast after every "Fix it for me" success with action + Undo button: "Done. ModemManager is now disabled. [Undo]" — Undo invokes the inverse helper action.

**C15 — Stale gpsd protocol version handling.** Distro upgrades change gpsd JSON envelopes; tuxlink may bail on unknown fields with no triage surface. *Severity: MEDIUM. From UX-2.*

**Action:** Probe logs the `VERSION` envelope. If parsing fails on a structurally-valid JSON envelope, surface a triage card: "tuxlink received data from gpsd but couldn't parse it (gpsd version mismatch). Click here to file a bug." Pre-fill report with parsed-vs-received payload.

**C16 — UBX-mode-only u-blox devices.** Cheap u-blox modules ship UBX-only; tuxlink's NMEA probe shows "no GPS data" without diagnosis. *Severity: MEDIUM. From UX-21.*

**Action:** bd-3 acceptance criteria add: when device VID matches a known u-blox VID:PID range AND no NMEA arrives within 5 s of probe, surface a specialized triage card: "Your u-blox device may be in UBX-only mode. Click here for the one-time NMEA-enable command (ubxtool)." Pre-fill the ubxtool command sequence. Don't auto-run ubxtool — too device-specific.

**C17 — gpsd installed but socket-activated/disabled.** Different from "gpsd not installed". *Severity: MEDIUM. From UX-22.*

**Action:** Probe distinguishes three states: not-installed / installed-but-disabled / installed-running. The "installed-but-disabled" case gets a specific fix card ("Enable gpsd's socket activation"): `systemctl enable --now gpsd.socket`. Goes through the pkexec helper as a fourth action: `enable-gpsd`.

**C18 — Discoverability gap on red-pill-but-nothing-to-fix.** User disabled GPS in privacy → pill red → Troubleshoot → nothing broken. *Severity: MEDIUM. From UX-15.*

**Action:** Pill states expand to four: green (live fix), amber (source change pending review), red (no usable fix + manual fallback active), gray (GPS deliberately off). Click on gray → Settings → Privacy section, not Troubleshoot.

**C19 — Stale-source toast spam when unplugging non-active sources.** Sue unplugs a peripheral she wasn't using for GPS; toast fires. *Severity: HIGH. From UX-3.*

**Action:** Event emission gating:
- `source-changed` event fires ONLY when (a) the active source goes away, OR (b) a new source appears that's better-than-current per the recommendation logic.
- Background source-detection (new device that COULD be used but isn't) becomes a passive Settings → Source picker badge, not a toast.

### D. DEPENDENCY & IMPLEMENTATION

**D1 — Use `udev` Rust crate, not `udevadm` subprocess.** ~50x faster; meets the 500 ms p95 budget. *Severity: LOW. From IMPL-8.*

**Action:** Add `udev = "0.10"` to Cargo.toml. Subprocess fallback only for musl/AppImage minimal hosts.

**D2 — Use `/dev/serial/by-id/*` as the persisted device path.** `/dev/ttyACMN` is unstable across reboots. *Severity: MEDIUM. From IMPL-9.*

**Action:** Probe enumerates BOTH `/dev/ttyACM*` and `/dev/serial/by-id/*`, deduplicates by canonical path. Source picker stores `/dev/serial/by-id/...` symlink as the config value. Vendor/model surfaced to UI for human recognition.

**D3 — Pin the `nmea` crate.** Hand-rolling NMEA is a trap (multi-talker sentences, checksums, proprietary extensions). *Severity: LOW. From IMPL-13.*

**Action:** Add `nmea = "0.7"` (or current). Drop "small dep or hand-rolled" hedge.

**D4 — `nix::unistd::getgroups()` not `id -Gn` subprocess.** Data the process already has. *Severity: LOW. From IMPL-19.*

**Action:** Probe uses `nix::unistd::getgroups()` + `Group::from_name("dialout")`. No subprocess.

**D5 — ModemManager is on the SYSTEM bus.** Design said session bus. *Severity: MEDIUM. From IMPL-4.*

**Action:** Probe connects to system bus for `org.freedesktop.ModemManager1` introspection. Returns `ModemManagerStatus::{NotInstalled, Inactive, Masked, Active}` enum, not boolean.

**D6 — `zbus` is conditional on bd-4 needing a long-lived listener.** For probe-only use, subprocess `busctl` + `systemctl is-active` is fine and saves ~250 KB. *Severity: LOW. From IMPL-15.*

**Action:** Default to subprocess for bd-1's probes. Add `zbus` only if bd-4's continuous monitoring needs a persistent connection.

**D7 — `tokio::process::Command` for pkexec spawning, not `std::process`.** The auth dialog blocks the parent until dismissed; sync command would block the Tauri command worker. *Severity: LOW. From IMPL-11.*

**Action:** Spawner uses `tokio::process::Command::spawn().wait_with_output().await` with a 60 s timeout. No `tauri-plugin-shell` capability grant needed.

**D8 — `probe_gpsd()` is a NEW function, not a refactor of `run_gpsd_client`.** The existing client is fire-and-forget with backoff; the probe is one TCP connect with timeout. *Severity: LOW. From IMPL-16.*

**Action:** Factor `probe_gpsd() -> Result<GpsdProbeResult, ProbeError>` as a separate function. Reuse the parse_tpv helper.

**D9 — pkexec available-detection at startup, cached.** `pkaction --action-id com.tuxlink.app.add-dialout` exit code 0 → buttons enabled; else hidden. *Severity: LOW. From IMPL-17.*

**Action:** Helper-availability probe runs once at app init; result cached in App state; "Fix it for me" buttons read from the cache.

**D10 — Source-switching config persistence reuses `with_inner()` + `config_set_grid` pattern.** *Severity: LOW. From IMPL-21.*

**Action:** Extend `position_set_source_impl` ([ui_commands.rs:2200](../../../../src-tauri/src/ui_commands.rs#L2200)) to accept the new ProviderArbiter selection. Don't introduce parallel sync primitives.

### E. PERSONAS & FAILURE MODES NOT IN THE ORIGINAL DESIGN

**E1 — Sysop / multi-user station (system-account pattern).** Tuxlink running as `tuxlink` system user, multiple humans logging in as themselves. Dialout fix applies to the wrong user. *Severity: HIGH for sysop personas. New persona from UX rounds.*

**Action:** Before invoking the dialout helper, surface the user being modified: "I'll add `pi` to the dialout group. (That's the user tuxlink is running as.) Is that the right user? [Yes] [Change user…]". The "Change user…" branch lets the operator specify a different `$REAL_USER` (which the helper accepts as an arg under the same PolicyKit policy — the policy doesn't restrict the username arg).

**E2 — Container-installed tuxlink (Distrobox / Toolbox / podman).** Increasingly common on immutable distros (Silverblue, MicroOS, Bluefin). Container can't `systemctl mask` the host's MM. *Severity: MEDIUM. New from UX rounds.*

**Action:** Detect container via `/run/.containerenv` or `/.dockerenv` presence. When detected, the triage panel surfaces a specialized message: "tuxlink is running in a container. Some fixes require running them on your host system. Here's the command to run on the host: …". Cuts off the "Fix it for me" buttons in container mode.

**E3 — Pi-as-shack-server with VNC + user-mismatch.** Tuxlink running as `pi` (or a service account), VNC operator logged in as themselves. *Severity: MEDIUM. From UX-11.*

**Action:** Covered by E1 (the surfacing of "who am I modifying"); no additional design surface needed beyond E1.

### F. DEFERRED / OUT-OF-SCOPE (acknowledged, not in v0.0.1)

The following findings are real but deliberately deferred to follow-up bds. The plan flags them in "Out of scope" sections of each bd:

- **i18n / locale support** (UX-9) — strings table is structured for i18n, but v0.0.1 ships English-only. Follow-up bd files when a translator volunteers.
- **Privacy: USB device metadata in logs/crash reports** (UX-27) — local-only commitment is documented; no crash reporting exists yet.
- **Bluetooth NMEA full pairing wizard** (UX-24) — pre-paired `/dev/rfcomm*` enumeration ships in bd-3; in-app pairing is a follow-up bd.
- **Rescan progress indication** (UX-25) — inline "Checking…" lines are a polish; v0.0.1 surfaces a spinner only.
- **AppImage runtime detection of distro pkexec availability** (UX-26) — design says "AppImage hides Fix it for me"; compile-time flag is the simplest implementation, runtime detection is a polish.

---

## Effort recalibration

| bd | Original estimate | Adjusted estimate | What grew |
|---|---|---|---|
| **tuxlink-9xy1 (bd-1)** | 1 week | **2-3 weeks** | Wizard step framework decision, 3-component split (presentational + 2 containers), Settings expandable-section design, 5 new Tauri commands, parallel probe orchestration, wizard-resume-after-logout flow, a11y pass, remote-shell detection, container-mode detection, user-being-modified surfacing |
| **tuxlink-m9ej (bd-2)** | 3-4 days | **5-7 days** | Three split action IDs (not one), workspace-split / feature-gated minimal bin, build-order sequencing in converge-build.sh, helper-availability probe, post-action state verification, tuxlink-side invocation log, ModemManager-mask alternative (udev rule fix) investigation |
| **tuxlink-ley0 (bd-3)** | 4-5 days | **7-10 days** | ProviderArbiter refactor (decoupling from PositionArbiter), `/dev/serial/by-id/*` persistence, u-blox UBX-mode detection + triage card, NMEA crate integration, baud auto-detection sweep |
| **tuxlink-gnws (bd-4)** | 3-4 days | **7-10 days** | Build the event consumer module from scratch (not "ship the event source, not the consumer" — the consumer doesn't exist), debounced toast surface, modal-interrupt component, source-change gating logic (active-source vs background-detection split), Bluetooth NMEA reader extension |

**Total chain:** ~4-6 weeks of focused eng time, plus adversarial review cycles + operator smoke matrix per bd.

**The "screenshot for the club Slack" milestone (bd-1 + bd-2 together)** now lands at ~3-4 weeks. Still tractable. Still the right investment.

---

## What this addendum does NOT change

- The architectural shape (pkexec helper + tightly-scoped action enum + shared component model).
- The four-bd decomposition (foundation → magic button → native NMEA → live monitoring).
- The personas (Bob / Sue / Dave / Mike) — these add E1/E2/E3 but don't replace.
- The "give a shit" UX thesis.
- The privacy-precision-reduction defaults.
- The reversibility commitment.
- The dependency on the existing PositionArbiter for downstream consumers (the new ProviderArbiter sits above, not replaces).

---

## Codex round 5 — pending

Codex quota hit during Round 1. Operator confirmed reset in ~90 min from finding time. Round 5 (Codex synthesis) was originally planned to synthesize rounds 1-4 findings. **Better deployment:** Codex Round 5 reviews the WRITTEN PLAN (the next BRF step) instead. The plan is the higher-value artifact to challenge — incorporates the findings above plus a structured TDD breakdown that Codex can stress-test for missing test surface, ordering errors, and underspecified acceptance criteria.

Scheduling: wakeup at ~T+90 min from operator's notification, after `superpowers:writing-plans` produces the plan doc.
