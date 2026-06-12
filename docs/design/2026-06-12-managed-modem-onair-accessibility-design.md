# Design: tuxlink owns the on-air chain — managed modems, integrated PTT, optional CAT

**Date:** 2026-06-12 · **Author:** opossum-taiga-hawk (agent) · **Operator:** Cameron Zucker
**Status:** DRAFT — office-hours design doc (no implementation in this session)
**Branch context:** bd-tuxlink-xygm/recover-handoffs · reads against origin/main @ 43e248b7
**Supersedes nothing. Finishes:** [ADR 0015](../adr/0015-modem-integration-and-rig-control-foundation.md) (decisions #1, #4) + [tuxlink-5jb](bd). Reconciles `docs/user-guide/12-cat-and-rigctld.md` (doc written ahead of code).

---

## Problem statement

A non-technical operator must be able to get tuxlink on the air **without authoring a config file, looking up a radio model ID, writing a systemd unit, or hand-tuning a VFO to a gateway frequency.** "I can configure and run Dire Wolf" is, in amateur-radio terms, an operator gate that separates the wheat from the chaff. Tuxlink's reason to exist is to **delete** that gate, not relocate it. Software that spends all its effort on engineering the user can't see, and none on the user being able to access it — Winlink Express, Pat's CLI, raw Dire Wolf — hates its audience. Tuxlink will not be that.

Three concrete frictions surfaced while prepping a real on-air test (DigiRig + Masters Communications DRA-100 → a virgin reference Pi → send a Winlink message as an end user, with a Motorola CDM-1550LS+ VHF radio):

1. **The DRA-100 cannot key the CDM-1550LS+ through tuxlink today without hand-configured Dire Wolf.** The DRA-100 keys via a CM108 HID line; for FM packet that PTT is configured inside `direwolf.conf` — which the operator runs himself. VOX is not viable for this radio. So keying depends on the exact config gate we want gone.
2. **Dire Wolf reliance is unmanaged.** ardopcf is spawned and configured by tuxlink; Dire Wolf is not. The operator hand-writes `direwolf.conf` and runs the process. This is an accessibility gate and an unfinished slice of ADR 0015.
3. **No rig control.** Tuxlink does zero CAT; the operator hand-tunes HF gateway frequencies. A shipped user-guide topic (`12-cat-and-rigctld.md`) describes a rigctld integration that does not exist.

## Background — what was already decided, and where the code diverged

[ADR 0015](../adr/0015-modem-integration-and-rig-control-foundation.md) (2026-05-27, accepted) already settled the architecture:

- **#1 — tuxlink launches and owns the modem lifecycle (managed-spawn)** for *all* soundcard modems, as the single arbiter of the one-sound-card conflict. It explicitly **rejected** "operator runs the modem; tuxlink only opens TCP" as *"loses single-pane arbitration of the sound-card conflict, which is the core UX win"* (ADR 0015, verbatim). The companion findings doc (`ardop-deployment-findings.md`, not the ADR) states the intent plainly: *"tuxlink — not Pat — manages BOTH Dire Wolf (VHF packet) and ardopcf (HF)… uncomplicates a notoriously frustrating topic for new operators."*
- **#2 — one generic `ModemTransport` abstraction** over ardopcf / Dire Wolf / VARA / future tuxmodem.
- **#4 — rig control is its own crate** (`tux-rig`: `Ptt/SetFreq/SetMode/ReadStatus` + Hamlib first backend), tracked as [tuxlink-5jb](bd).

**What actually shipped:** ardopcf got managed-spawn (`with_managed_modem`, `build_ardop_extra_args`, audio devices + `-p <serial>` PTT + WebGUI). **Dire Wolf did not** — there is no `direwolf` spawn in `src-tauri/src/`; the packet path only TCP/serial/Bluetooth-connects to an operator-run TNC (`winlink/ax25/link.rs` `KissLinkConfig::{Tcp,Serial,Bluetooth}`; the KISS codec/param frames live in `kiss.rs`). The rig-control crate is seeded — `tuxmodem/crates/tux-rig-cm108/` (CM108 HID PTT: `hidraw.rs`, `ptt.rs`, `writer.rs`) exists — but the Hamlib/CAT plane does not, and none of it is wired into the app's modem paths. The CAT user-guide doc was written ahead of the code.

So the work here is **execution against an accepted decision record**, plus reconciling docs that ran ahead — not a new architecture.

## Goals

- A former-novice operator gets on the air by picking **sound card + PTT line + callsign** in tuxlink. Tuxlink generates every config, spawns and supervises every external modem, and surfaces health/feedback. No `.conf`, no systemd unit, no model-ID lookup is ever handed to the operator.
- The DRA-100 keys the CDM-1550LS+ for FM packet through tuxlink, with no hand-edited Dire Wolf.
- (Completeness) Optional CAT: when an operator opts in, tuxlink reads the frequency for display and offers a "tune this radio to the selected gateway" action — never required, off by default (manual tuning stays first-class).
- The shipped docs match the shipped code.

## Non-goals / explicitly rejected

- **A native packet soundmodem (Bell-202 AFSK/GMSK in tuxlink).** Rejected by the operator: it would be a third clean-sheet modem beside TANDEM HF and TANDEM FM, three on-air-validation problems, and packet AFSK has no upside to reinventing — Dire Wolf's decades of field-hardened DSP (DCD, clock recovery, CSMA timing, FX.25/IL2P) is exactly what a naive reimplementation would fail to re-earn, and only on-air. See [[feedback_ai_amateur_radio_reliability]].
- **Mandatory CAT.** Manual tuning is the official Winlink default (Winlink Express ships Model=Manual / Control=None / PTT=External) and stays fully supported. CAT is opt-in convenience.
- **PACTOR, AGW/Linbpq drivers, unattended/auto-fetch operation** — out of scope (already documented gaps).
- **Tuxmodem (TANDEM) substitution for packet.** Tuxmodem is a clean-sheet HF protocol; it cannot talk to existing AX.25 packet RMS gateways, so it is not a Dire Wolf replacement here.

## The three slices

### Slice A — CM108 PTT for the managed-ardopcf (HF) path

ardopcf's managed spawn exposes only `-p <serial>` (RTS PTT); a CM108-HID interface (DRA-100 class) on an HF/ARDOP setup has no PTT path through tuxlink today (VOX excluded). The `tux-rig-cm108` crate already implements CM108 HID keying. Wire it in so an ARDOP session can key a CM108 interface.

- **Open implementation question (for the plan, not this doc):** **prefer ardopcf's own CM108 PTT if its CLI exposes it.** The fallback — tuxlink keys CM108 via `tux-rig-cm108` "around the ardopcf TX window" — carries a real hazard: tuxlink would have to observe ardopcf's TX-start/TX-end transitions and assert/release PTT in lockstep, and ardopcf does not trivially expose a synchronous "about to transmit" signal to an external keyer. Mistime it and you get key-up-after-audio or, worse, **stuck PTT after TX — a RADIO-1 hazard.** So native-ardopcf-CM108 is preferred specifically to avoid the external-keyer timing race; the external-keyer path is acceptable only with a reliable ardopcf TX-state signal. RADIO-1 correctness applies regardless: keying must honor abort/disarm (the existing abort-before-write discipline; see [tuxlink-0ja]).
- **Scope:** small-medium (a crate that exists + a PTT-source option in the ARDOP config/UI).
- **Not the operator's weekend item** — the CDM-1550LS+ is VHF/UHF (FM packet), not HF/ARDOP. This is completeness for HF operators on CM108 interfaces.

### Slice B — Managed Dire Wolf (the accessibility centerpiece, the weekend-relevant piece)

Finish ADR 0015 #1: tuxlink generates the Dire Wolf config, spawns and supervises the process, and hides Dire Wolf entirely. The operator never sees `direwolf.conf`. This is where "do a really good job" lives.

What "really good" means, concretely:
- **Friendly device selection by STABLE identifier, not `plughw:` index.** Enumerate ALSA cards, present human names ("DigiRig", "DRA-100 / C-Media CM119"), but resolve and persist by **stable USB VID:PID / `/dev/snd/by-id` path**, not the boot-order-dependent card index — DigiRig + DRA-100 both attached means *two* USB cards, and "ignore HDMI, use the USB card" is ambiguous. Ignore the Pi onboard HDMI `Error -524` class. Reuses the same audio-device picking tuxlink already does for ardopcf.
- **PTT auto-detected, not hand-specified.** Detect the CM108/CM119 HID line on the same USB parent as the chosen sound card and offer it; offer serial RTS (DigiRig CP2102) as the alternative. When one adapter exposes both (HID + serial), prefer the HID-on-same-USB-parent and show the resolved choice for operator override. Generate the matching Dire Wolf directive (`PTT CM108 <hidraw>` or `PTT /dev/ttyUSBx RTS`).
- **Generated minimal conf — tuxlink owns ~6 fixed lines**, not Dire Wolf's full surface:
  ```
  ADEVICE  <resolved card>
  CHANNEL  0
  MYCALL   <callsign>      # tuxlink already has identity
  MODEM    1200            # only supported rate; the VHF Winlink-packet RMS baseline
  PTT      <resolved>
  KISSPORT <port>
  ```
  **Timing is intentionally absent.** TXDELAY / persistence / slot-time are NOT put in the conf — tuxlink pushes them as KISS param frames on connect (`push_kiss_params`, `winlink/ax25/datalink.rs`), so duplicating them in the conf would silently fight the wire values. FX.25/IL2P are omitted (plain AX.25 is the interoperable Winlink-packet baseline; tuxlink's stack has no FX.25/IL2P). 9600 is out of scope for v1 (VHF Winlink packet is overwhelmingly 1200). Dire Wolf's hundreds of other knobs (iGate, digipeater, APRS beaconing, multi-channel) are never touched. The variable inputs are the **same three** tuxlink already collects for ardopcf.
- **Transport: KISS-over-TCP on localhost.** Managed Dire Wolf runs `KISSPORT` on loopback; tuxlink's packet path reuses the existing `KissLinkConfig::Tcp{host:127.0.0.1, port}` pointed at the managed instance. AGW is out of scope (KISS is the path; any health signal must not imply a second AGW socket).
- **Validate the generated conf.** The plan must gate on a `direwolf -t 0 -c <generated.conf>` config-parse check before first spawn — never ship a conf template that hasn't been machine-verified to parse.
- **Spawn / supervise / clean-stop / arbitrate, with a pre-spawn device probe.** Reuse the ardopcf lifecycle machinery: start with the packet session, SIGINT clean-stop, confirm the sound device is released before a VHF↔HF modem swap (the single-sound-card arbitration ADR 0015 centers on). **Before spawning, probe device availability** — a stray ardopcf, a crashed prior Dire Wolf, or PipeWire/PulseAudio holding the card must surface a named, actionable error ("DigiRig is in use by another program"), never a black-box failure.
- **Health + feedback, not a black box.** Surface "Dire Wolf running / decoding / audio level" so the operator gets signal that it works — the opposite of raw Dire Wolf's invisible-engineering problem. Consider exposing its KISS/AGW status or a decode indicator.
- **Distribution — `Recommends: direwolf`, NOT `Depends:`.** `Depends:` hard-fails the tuxlink install on any virgin Pi whose configured apt repos lack `direwolf` (it sits in `universe`/varies across Pi OS / Debian / Ubuntu derivatives) — which would brick first-run on the exact reference hardware, defeating the headline goal. Use `Recommends: direwolf (>= <min>)` so it's pulled by default when available, plus a **runtime presence-probe** at packet-setup time and the existing bring-your-own-KISS endpoint as the named fallback ("Dire Wolf not found — install it, or point tuxlink at an existing KISS TNC"). Pin a **minimum version with a stated reason**: `PTT CM108` directive maturity and KISS-over-TCP behavior differ between Dire Wolf 1.4 and 1.6+; an older packaged build can lack robust CM108 PTT. (Architecture call, settled here — not deferred to packaging.)
- **Keep the manual escape hatch.** Advanced operators (or hardware-TNC / remote-Dire-Wolf users) can still point the packet panel at an external KISS TCP/serial/Bluetooth endpoint — the existing `KissLinkConfig` paths stay. Managed is the default; bring-your-own remains for the wheat.
- **Scope:** medium. This is the real build and the one that must be excellent.

### Slice C — `tux-rig` CAT plane + reconcile the phantom doc (completeness)

Execute [tuxlink-5jb](bd) (operator-locked, P3): the `tux-rig` Hamlib backend + a control-plane that binds station + frequency + mode so the client and radio cannot mismatch (the wrong-freq-TX interlock the operator cares about), plus the "tune this radio to the selected gateway" action and a live frequency display. Off by default; manual tuning stays first-class.

- **Reconcile docs regardless of build timing:** `12-cat-and-rigctld.md` currently describes a Settings → Radio → rigctld panel, a frequency ribbon, and a "set radio to this gateway's frequency" button that do not exist. Until the feature ships, the doc must be corrected to reality (rigctld is for *other* clients sharing the rig; tuxlink has no CAT integration yet). This doc-truth fix is independent of the feature and should land first.
- **Scope:** larger; this is the "full single-pane" milestone and includes wrong-freq-TX safety interlocks. Hamlib backend form (libhamlib FFI vs managed `rigctld` subprocess vs minimal own-CAT) is still the open `5jb` research question.
- **Settle the interlock design early, even though CAT ships last.** The wrong-freq-TX interlock is the genuinely hard-to-undo, safety-critical part of this slice. If it rides the largest/last slice unscoped, it risks never getting the `build-robust-features` rigor it needs. Its *design* (what binds station↔freq↔mode, what confirmation gates a freq change before the per-session Connect consent transmits) should be settled in the `5jb` research output up front, independent of when the CAT UI ships.

## Approaches considered (the Dire Wolf dependency)

- **APPROACH A — Managed Dire Wolf (CHOSEN).** Keep Dire Wolf's battle-tested DSP; tuxlink generates the config and spawns/supervises it, hiding it completely. Effort: M. Risk: Low (no new on-air DSP). Reuses: ardopcf lifecycle + audio/PTT picking. Finishes ADR 0015 #1. **Why chosen:** removes the accessibility gate at the source with no RF-correctness risk; the operator's objection was *having to configure it*, not its presence; symmetric with the already-accepted managed ardopcf.
- **APPROACH B — Native AFSK/GMSK soundmodem in tuxlink.** Tuxlink grows its own Bell-202 modem (audio I/O + AFSK DSP + existing AX.25 stack + `tux-rig-cm108`). Effort: XL. Risk: High (on-air validation, field-only failure modes). **Rejected by operator:** a third clean-sheet modem, too much overhead, low confidence it would be gotten right, no upside over Dire Wolf's maturity.
- **APPROACH C — Hardware KISS TNC only.** Offload the modem to hardware (TNC-Pi / Mobilinkd / radio built-in TNC); tuxlink speaks KISS to hardware. Effort: S (already supported via `KissLinkConfig`). **Rejected as the primary answer:** requires specific hardware the operator doesn't have (DigiRig/DRA-100 are sound cards, not TNCs); kept as the existing bring-your-own escape hatch.

## Sequencing (honest about the weekend)

The weekend on-air test and the alpha-completeness build are **different efforts**:

1. **Weekend de-risk (no new build required):** prove the chain on the current path. Highest-leverage home test is the **greenfield install → wizard → CMS/Telnet connect** smoke on the reference Pi (the P0 from PR #619 has never been smoked on a virgin unit). Then a one-time hand-configured Dire Wolf packet test (the operator is competent today) proves the DRA-100 → CDM-1550LS+ RF chain + propagation. The weekend answers "does the radio work," not "is managed Dire Wolf done."
2. **Alpha-completeness build (the real work), in order:**
   - **C-doc first:** correct `12-cat-and-rigctld.md` to reality (cheap, ships truth now).
   - **Slice B — managed Dire Wolf**, done excellently. The centerpiece. Not rushed for the weekend (rushing the accessibility feature would betray the point).
   - **Slice A — CM108 PTT for ARDOP** (HF completeness for CM108 interfaces).
   - **Slice C — `tux-rig` CAT plane** (5jb; the largest, the full single-pane milestone).

## Discipline per slice (per [[feedback_discipline_triage_rule]])

- **C-doc fix:** straight edit. **Slice A:** contained wiring of an existing crate → TDD with the bd issue as the spec, but the PTT/abort RADIO-1 correctness bar applies. **Slice B + Slice C:** hard-to-undo and interop/safety-adjacent (process lifecycle, generated config across hardware permutations; wrong-freq-TX interlocks) → full `build-robust-features` with the cross-provider Codex adversarial round ([[feedback_no_carveout_on_cross_provider_adrev]]).

## RADIO-1 / safety considerations

- Any tuxlink-driven PTT (Slice A) must honor abort/disarm before TX, consistent with the existing abort-before-write discipline ([tuxlink-0ja]). Agent authorship is fine (ADR 0018); on-air verification is the operator's.
- Slice C's frequency-set path does not transmit, but the wrong-freq-TX interlock (set freq, then the operator's per-session Connect consent transmits) is the safety point `5jb` exists to design.
- The managed-modem lifecycle must never leave a modem keying after a session ends (clean SIGINT + device-release confirmation — already an ADR 0015 lifecycle requirement).

## Open questions

1. ardopcf CM108 PTT: native CLI flag vs tuxlink-keys-via-`tux-rig-cm108` as external PTT command. (Slice A plan.)
2. `.deb` `Recommends: direwolf (>= <min>)` (architecture call made in Slice B) — remaining: confirm exact package name across Debian/Ubuntu/Pi OS and the precise minimum version with mature `PTT CM108` + KISS-over-TCP. (Slice B packaging.)
3. Dire Wolf health signal — what to surface (process up / KISS reachable / decode count / audio level) and via what interface (KISS, AGW, log scrape). (Slice B.)
4. Hamlib backend form for `tux-rig` (libhamlib FFI vs managed `rigctld` vs minimal own-CAT) — the still-open `5jb` research. (Slice C.)
5. CM108 device identity: DigiRig and DRA-100 both present as C-Media HID — how to disambiguate when both are attached. (Slice A/B.)

## Success criteria

- A virgin Pi + tuxlink + DRA-100 + CDM-1550LS+: operator picks sound card + PTT + callsign, clicks Connect, and sends a packet Winlink message — having authored zero config files and never seeing the word "Dire Wolf."
- ardopcf + a CM108 interface keys without VOX.
- (Opt-in) CAT enabled: the radio tunes to the selected gateway on one action; disabled: manual tuning, unchanged.
- Shipped docs describe only shipped behavior.

## Distribution plan

Managed Dire Wolf changes packaging: the `.deb` gains `Recommends: direwolf (>= <min>)` (not `Depends:` — see Slice B) plus a runtime presence-probe and a bring-your-own-KISS fallback, so install provisions the modem when available without bricking first-run when it isn't. No new artifact/channel otherwise (existing release-please + `.deb` pipeline). Slice C may add a `rigctld`/Hamlib runtime dependency only if the managed-subprocess backend is chosen.

## Next steps

1. (Now) Operator approves this design doc.
2. Correct `12-cat-and-rigctld.md` to reality (small docs PR).
3. Weekend: greenfield-install smoke (PR #619) + one-time hand-config packet test to prove the RF chain.
4. Post-weekend: plan + `build-robust-features` Slice B (managed Dire Wolf), then Slice A, then Slice C (5jb).
5. File/refresh bd issues: managed-Dire-Wolf (new), Slice-A CM108-PTT-for-ardop (new), reconcile 5jb scope, doc-fix issue.

## What I noticed

- "I can configure and run Dire Wolf is a whole class of operator gate… I certainly used to be the chaff." The design's binding constraint is an empathy memory, not a feature list. That's the right north star for this product.
- "Shipping Dire Wolf with the alpha is the right shape, but we have to do a really good job of it." You separated *the dependency* (fine) from *the experience* (must be excellent) — which is exactly why managed-vs-native was the real fork, not Dire-Wolf-vs-not.
