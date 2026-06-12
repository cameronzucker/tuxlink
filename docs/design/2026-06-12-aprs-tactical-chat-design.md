# Design: APRS Tactical Chat in Tuxlink

**Date:** 2026-06-12 · **bd:** tuxlink-2f2n · **Status:** DRAFT (brainstormed in office-hours; ready for build-robust-features)
**Branch:** bd-tuxlink-2f2n/aprs-tactical-chat (off main) · **Brainstorm agent:** glade-clover-bison
**Builds on:** managed Dire Wolf (tuxlink-yq3l) + the proven UV-Pro Bluetooth KISS transport (`KissLinkConfig::Bluetooth`).

---

## The thesis: power consolidation, not chat sync

Tuxlink is already an email-shaped Winlink workspace. Adding tactical VHF chat next to store-and-forward is the Gmail-plus-Chat move: once you own the messaging surface, the live layer is a natural neighbor.

But the reason it matters for EmComm is not feature breadth, it is **power**. The win is that when an operator sits down at the station, their tactical conversation runs on the device that is **already plugged in** (the Pi/laptop running tuxlink), and the battery-powered phone leaves the power budget. In a field deployment, especially solar, that is a load-shedding decision, not a convenience: fewer batteries to rotate, fewer charge adapters thermal-cutting in the sun, fewer random-voltage conversions stressing the setup. The UV-Pro stays as the RF front end on its swappable battery; the phone-as-chat-client goes away. (Operator direct experience: solar charge adapters heat-shut-off outdoors, and extra connections at random voltages exacerbate it.)

This thesis deliberately **kills the sync problem**. We do NOT carry chat history between the phone and the desktop. The phone app and tuxlink are two alternative clients to the same radio, used at different times, with no shared state. The "continuity" the operator wants is: same frequency, same device (UV-Pro), better-powered client. Nothing more. The single-Bluetooth-host constraint (UV-Pro = one RFCOMM host at a time) is therefore **acceptable** — it is a disconnect-and-reconnect handoff, which the BTECH Pro app already models. Surface a small tooltip; do not engineer around it.

## Prior-art venn — keep / toss / rebuild

| Source | Contributes | Keep | Toss | Rebuild |
|---|---|---|---|---|
| **BTECH Pro app** (the ideal target) | The "iMessage over radio" UX bar — threads, bubbles, delivery states, feels like a phone messenger | the interaction model | Android-only, closed, phone-centric, no Winlink/HF | the same UX as a desktop surface fused with Winlink |
| **HT Commander** (open source) | The Rosetta Stone for the Benshi/Vero radio protocol (UV-Pro, VR-N76, GMRS-Pro) — authoritative prior-art implementation, same status Pat/wl2k-go held for Winlink | the protocol knowledge | its standalone-app framing; its **WLE-SID-spoof Winlink** (tuxlink does Winlink legit with a registered SID); its janky "radio on screen" UI | the device-control UX done *right* |
| **UV-Pro** (hardware) | tuxlink already drives it over Bluetooth KISS, on-air-proven (2026-05-22, classic RFCOMM/SPP, channel-rotation handled) | the existing BT transport | — | nothing — it already works |

The empty center of that venn — the polished, desktop, Winlink-integrated, on-hardware-tuxlink-already-drives intersection — is the product.

## What tuxlink uniquely is here

Not "another APRS messenger." **The fixed-station brain that unifies the tactical layer (UV-Pro VHF chat) and the strategic layer (HF Winlink) in one polished surface, and follows the operator between the field and the desk.** BTECH Pro can't touch HF/Winlink. HT Commander does Winlink by faking it. Winlink Express does neither chat nor APRS. The overlap, on hardware tuxlink already drives, is empty.

---

## Architecture: a transport-agnostic messaging core + per-radio capability profiles

The load-bearing decision. The messaging/chat layer sits **above** whatever carries frames, and each transport advertises a **capability profile**. This mirrors tuxlink's existing `KissLinkConfig` variants and modem abstraction — it is one more profile that happens to also expose control, not a new architectural concept.

```
        ┌─────────────────────────────────────────────┐
        │  Chat UX (conversation threads, delivery     │   ← new, the product
        │  states, RF-honest) — in the Winlink surface │
        └───────────────────────┬─────────────────────┘
        ┌───────────────────────┴─────────────────────┐
        │  APRS / messaging core (transport-agnostic)  │   ← new, bounded
        │  UI-frame codec + ":addressee:msg{seq" + ACK │
        └───────────────────────┬─────────────────────┘
        ┌───────────────────────┴─────────────────────┐
        │  Transport + capability profile              │
        │  ┌─────────────────┐   ┌───────────────────┐ │
        │  │ KISS floor      │   │ UV-Pro native     │ │
        │  │ (data only)     │   │ Benshi profile    │ │
        │  │ UV-Pro BT KISS, │   │ (data + CONTROL:  │ │
        │  │ managed Dire    │   │ channel/freq/     │ │
        │  │ Wolf, any TNC   │   │ settings on one   │ │
        │  │                 │   │ BT link)          │ │
        │  └─────────────────┘   └───────────────────┘ │
        └─────────────────────────────────────────────┘
```

### Layer 1 (PHASE 1 — the floor, ship first): generic APRS over KISS
- APRS UI frames (AX.25 control `0x03` UI, PID `0xF0` no-layer-3) over the KISS pipe tuxlink already has. Works over the **UV-Pro Bluetooth KISS**, the **managed Dire Wolf** just shipped, a **DigiRig + any radio** — transport-agnostic, broad hardware support on day one.
- APRS messaging: the `:ADDRESSEE :text{seq` format with the APRS app-layer ACK (`:ADDRESSEE :ackNN`). Bounded retransmit (see RADIO-1).
- The chat UX (below).
- **Cannot do device control** — KISS is a dumb data pipe. The operator still turns the knob. Accepted for Phase 1.

### Layer 2 (PHASE 2 — the premium tier): native Benshi/Vero profile for the UV-Pro
- The native radio protocol carries **control + data over the same Bluetooth link**: channel, frequency, settings, from tuxlink's screen — no reaching for the HT on a tactical channel.
- Ties the power thesis off completely: for the UV-Pro, **one wireless tether to the plugged-in Pi = control + chat + radio.** One link, one battery, one plugged-in brain. A generic radio can't match this (it needs a separate CAT path + separate TNC path; the native UV-Pro link collapses both).
- Reverse-engineered. **HT Commander's source is the ground truth** (per the winlink-RE-authoritative-sources principle: prior-art implementations are truth, prose docs are not). UV-Pro / Benshi-Vero family only.
- The bar: HT Commander's on-screen-radio UI is janky. **The native path's reason to exist in tuxlink is on-screen control done right.** If we can't beat janky, don't ship Layer 2.

### Out of scope / parked
- **Position / beaconing** (RX + TX) — natural Phase 3, ties to the existing GPS + Maidenhead precision-reduction work. Not in the first build.
- **Winlink-over-APRS gateway** — a wholly separate idea (the WLNK-1 / APRSLink path: an APRS message routed into Winlink email). tuxlink owning both backends could do this natively/better than a third-party gateway, but it is a different shape (a bridge, not a chat experience). Its own bd issue someday. NOT here.
- **Full APRS client** (digipeating logic, APRS-IS internet linking, Mic-E, weather, telemetry) — explicitly anti-scope. That is YAAC/aprs.fi territory and it dilutes the EmComm focus.

---

## The chat experience (the actual product)

The protocol is a few hundred lines; the experience is the whole ballgame. Requirements:

- **Conversation threads per callsign**, message bubbles, in the Winlink workspace (the Gmail-chat-in-the-inbox home, not a separate window — consistent with tuxlink's inline-UI / no-window-clutter rule).
- **RF-honest delivery states.** This is the hard design problem and the differentiator. APRS is fire-and-forget over a shared, congested, line-of-sight channel; the app-layer ACK is best-effort. The UX must be beautiful AND truthful: states like **sent → heard-locally → ACKed → timed-out**, NO fake "delivered" checkmark. Keep what APRSIS-CE got right (it surfaced ACK status); modernize the legibility. Making it *feel* like internet chat when it is not is the trap that makes it read as broken.
- **Semi-public is honest, too.** APRS messages are addressed but heard by everyone in range and digipeated. Closer to an addressed group channel than a private DM. The UX should not imply privacy it does not have.
- **Position context** (Phase 3) inline when available.

---

## Premises (agree before building)

1. **Power consolidation is the why, not history sync.** No chat-history transfer between phone and desktop. Two independent clients, no shared state. — *load-bearing; it deletes the hardest sub-problem.*
2. **Single-Bluetooth-host is acceptable.** UV-Pro = one RFCOMM host at a time; phone OR Pi, not both. Handoff via disconnect/reconnect + a tooltip. Do not engineer simultaneity.
3. **Transport-agnostic messaging core + capability profiles.** Layer 1 (KISS, data-only) ships first and broadly; Layer 2 (native Benshi, adds control) is the UV-Pro premium tier, later.
4. **RF-honest UX.** Delivery states reflect APRS reality; no fake delivered/encrypted/private affordances.
5. **tuxlink does Winlink legitimately** (registered client SID), in contrast to HT Commander's WLE-SID spoof. (Not new work here — a positioning fact that informs how we relate to HT Commander as prior art: mine protocol, not legitimacy.)
6. **Native Benshi is RE'd from HT Commander source** when we get to Layer 2, and only ships if the on-screen control beats "janky."

## RADIO-1

APRS messaging **transmits** — every sent message keys a transmitter. The standing discipline applies:
- Per-invocation operator consent for transmit (the click on Send), same as the rest of tuxlink. No agent ever transmits; the operator's on-air test is the validation.
- **Bounded airtime by construction.** APRS UI frames are short and discrete (fire-and-forget), so there is no connected-mode runaway risk. The one place to bound: the message-ACK **retransmit** — cap retries (APRS convention is a small bounded retry schedule), never an unbounded resend loop. A Send with a working Cancel/abort that stops further retransmits before the next TX.
- No tuxlink-added safeguards beyond legacy/standard APRS behavior (consistent with the no-added-safeguards rule); the retry cap is standard APRS, not a tuxlink invention.

## Approaches considered

- **A — KISS-only (APRS over the existing KISS transports).** Minimal, broad, ships fast, reuses the AX.25/KISS infra + managed modem + UV-Pro BT KISS. No device control. *This is Phase 1.*
- **B — Native-Benshi-only (UV-Pro premium).** Best UX + on-screen control, but UV-Pro-only and gated on RE work; abandons broad hardware support. *Too narrow as a starting point; it's Phase 2.*
- **C — Both, layered (RECOMMENDED).** Transport-agnostic core; KISS floor first for breadth + to prove the messaging core and chat UX against hardware tuxlink already drives, then layer the native UV-Pro control profile. De-risks: the generic path validates the product before sinking time into reverse-engineered Benshi control.

**RECOMMENDATION: C, built in order — Phase 1 (Layer 1 KISS floor + chat UX) is today's build; Phase 2 (native Benshi control) follows.**

## Reuse map (what Phase 1 leverages — minimal net-new)

- **Transports:** `KissLinkConfig::{Bluetooth (UV-Pro), ManagedDireWolf (just shipped), Tcp, Serial}` — APRS rides any of them unchanged.
- **KISS codec:** `winlink/ax25/kiss.rs` — reusable as-is.
- **Frame layer:** `winlink/ax25/frame.rs` (`Address`) — the addressing is reusable; APRS needs a **UI-frame** encode/decode path (control `0x03` / PID `0xF0`), simpler than the connected-mode SABM/UA/I-frame machinery already there (no state machine).
- **The app shell + the inline-UI patterns** — the chat surface is new but lives in the existing React app alongside the Winlink panels.

Net-new for Phase 1: the APRS UI-frame codec, the APRS message format + bounded-ACK logic, and the chat UX. That is the bounded, today-buildable slice.

## Open questions (resolve early, ground in HT Commander source — do NOT guess)

1. **Does the BTECH "iMessage over radio" ride raw APRS UI frames over KISS, or the radio's native Benshi message path over GAIA/BLE?** HT Commander's source answers it. If native, Layer 2 implements the Benshi message path, not just APRS — changes the Phase 2 build. (Per the AI-amateur-radio-reliability rule: do not assume; read the reference implementation.)
2. APRS message-format edge cases (message-ID/seq handling, ACK matching, the exact `:` field widths/padding) — ground against a real APRS implementation, not prose.
3. UV-Pro KISS-vs-native split: what does the radio expose over plain KISS vs only over the native protocol? (Decides what Layer 1 alone can do on the UV-Pro.)

## Success criteria

- **Phase 1:** an operator picks a transport (UV-Pro BT KISS, or managed Dire Wolf, or any TNC), opens the chat surface, sends an APRS message to a callsign, sees honest delivery states, and receives inbound APRS messages into per-callsign threads — all inside the Winlink workspace. Operator on-air smoke (UV-Pro, real APRS frequency) is the validation; agent never transmits.
- **Phase 2:** the UV-Pro's channel/frequency/settings are controllable from tuxlink's screen over the same Bluetooth link, and the control UX is demonstrably *not* janky.

## Distribution

In-app. No separate distribution channel — it ships in the tuxlink Tauri app like every other panel. APRS messaging needs no extra runtime dependency beyond the chosen transport (which the managed-modem + BT-KISS work already covers).

---

## Build sequencing (this is for the plan, not the spec to decide finally)

Today's target is **Phase 1 (Layer 1 + chat UX)** via build-robust-features → writing-plans → execute, the same pipeline that shipped managed Dire Wolf. Phase 2 (native Benshi control) is a separate build gated on the HT-Commander-source grounding of open question #1. Position/beacon (Phase 3) and Winlink-over-APRS (separate) are later/separate bd issues.
