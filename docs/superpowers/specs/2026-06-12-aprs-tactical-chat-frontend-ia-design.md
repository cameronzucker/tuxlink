# Design: APRS Tactical Chat — Frontend IA & Surface

**Date:** 2026-06-12
**Agent:** slate-arroyo-marsh
**Status:** Approved — placement, entry points, and visual register locked via brainstorm (2026-06-12).
**Scope:** The frontend information architecture and visual surface of the APRS tactical chat **only**. The protocol/feature design lives in [`docs/design/2026-06-12-aprs-tactical-chat-design.md`](../../design/2026-06-12-aprs-tactical-chat-design.md); this document details where the chat lives in the tuxlink shell and how it looks.
**Supersedes:** the placeholder `'aprs'` pseudo-folder mount shipped in the Phase 1a build (PR #642), which filed a live radio surface under address-book navigation.

---

## ⚠️ CORRECTION (settled 2026-06-13, extensively litigated) — OPEN CHANNEL, not threaded

The §1-3 placement decisions (shared right dock; connection driven from the status bar / status strip, not the panel) stand. The §4 **surface** prose originally described a *threaded* chat — per-callsign threads, a thread selector, inbound-left/outbound-right bubbles, and a "callsign / To" field in the composer. **That is superseded and must not be reintroduced.** The settled, shipped model is an **OPEN CHANNEL**:

- **One flat, time-ordered feed** of every message heard on the channel plus our own sends. NO per-callsign threads, NO thread selector, NO conversation roster, NO side rail. Rows are light log lines (`from → to`, or `→ all` for a broadcast), not left/right chat bubbles.
- **Broadcast by default.** Addressing (the "To") is expressed **inline within the single compose field** per APRS prior art — there is **NO separate To / recipient input box**. No addressee ⇒ broadcast to all. (This was vetoed repeatedly; do not add a To field.)
- **Connection is not a panel concern.** Transport/radio/connect live in the status bar + status-strip APRS control (§3 ①). The chat panel never hosts transport/radio/connect form fields.

Where §4 / §7 below conflict with this banner, **the banner wins.** It is aligned to the shipped open-channel `src/aprs/AprsChatPanel.tsx`, minus that file's separate recipient input, which is the defect to remove (tuxlink-ckmb).

---

## Decision summary

| Decision | Locked choice |
|---|---|
| **Placement** | Option C — the APRS chat lives in the **right dock**, which becomes a **shared, switchable surface** (APRS chat ⇄ Modem console). |
| **Primary entry point** | ① A **status-strip APRS control** beside the Connect button — glanceable (listening state + unread count), click brings the chat into the dock. |
| **In-dock switcher** | ② **Dock tabs** `[ APRS chat | Modem ]` — the dock's content switch and the way back to the modem console. |
| **Backstop entry** | ③ `View → APRS chat` (+ keyboard shortcut) — discoverability only, not the primary path. |
| **Visual register** | Clean, professional, Office-adjacent — the existing tuxlink Winlink-workspace language. **Not** a "tactical / ops-console" aesthetic. |

---

## 1. Placement — the right dock as a shared switchable surface

The right dock today hosts the modem console (the `MODEM · VARA HF` panel). The brainstorm established that the modem connection is **configure-once-and-saved**, and a session is started from the **status-bar Connect button** — so the dock does not need to display the modem console persistently. The dock is therefore a **shared surface** that hosts one of:

- **APRS chat** (the default tenant once the modem is configured) — the live, frequently-touched surface.
- **Modem console** — summoned when the operator reconfigures the modem.

This resolves the only objection to placing chat in the dock: there is **no capability degradation**, because the modem console remains one tab away and connection is driven from the status bar. The chat does not steal the mailbox's reading pane (it shares the dock the modem already occupies), preserving single-pane coherence.

The dock width for chat is the existing dock width extended to a comfortable reading column (target ≈ 380–400px). It is bounded — it does **not** stretch full-width (per the project's no-stretched-full-width rule).

## 2. Connection lifecycle is decoupled from dock content

- **Modem configuration** persists across sessions (already the case).
- **Connect / disconnect** is driven from the **status-bar Connect button** (already present), independent of which surface the dock shows.
- The dock's content (APRS chat vs Modem console) is a **view switch**, not a connection action. Switching the dock to APRS chat never disturbs an active Winlink/modem session, and vice versa.

## 3. Entry points

### ① Status-strip APRS control (primary)
A control in the top status strip, beside the Connect button, of the form:

> `📡 APRS · UV-Pro · ● listening · [1]`

- Glanceable: shows the listening state (armed/down, honest) and an **unread count** badge.
- Clicking it brings the chat into the dock (selects the **APRS chat** dock tab).
- Mirrors the operator's existing mental model: connection state and the chat entry both live in the status strip.

### ② Dock tab switcher (in-dock)
The shared dock carries a tab row at its top: `[ APRS chat | Modem ]`.

- Switches the dock between the chat and the modem console.
- The **APRS chat** tab shows an unread badge when the control is not focused.
- This is the conventional side-panel pattern (familiar from tabbed side panels) and the always-available way back to the modem console.

### ③ View menu / toolbar (backstop)
`View → APRS chat`, with a keyboard shortcut. Provides Office-conventional discoverability; it is not the primary path and carries no glanceable state.

## 4. The chat surface (visual design)

Clean Office register. Keep the functional substance; drop the ops-console styling. The surface, top to bottom:

- **Header row** — the `APRS channel` title, a **listening indicator** (honest: "Listening" / "Radio disconnected"), and the connection control (status-strip APRS control, §3 ①). NO active-thread callsign, NO thread selector — this is one open channel, not a set of conversations.
- **Open-channel honesty** — a quiet, persistent cue that APRS is heard by all stations in range and digipeated (not a private DM). Understated, not a loud badge.
- **Feed (one flat channel log)** — a single time-ordered list of every message heard on the channel plus our own sends. Light log rows showing `FROM → TO` (or `→ all` for a broadcast), our own sends subtly accented — NOT left/right chat bubbles. Each row carries a **timestamp** (client-stamped local time — when tuxlink heard/sent it; honest, no backend dependency).
- **Delivery states (outbound only) — RF-honest, no fabricated "delivered":**
  - `Sent` — queued/transmitted, no ACK yet (neutral).
  - `Acked HH:MM` — round-trip confirmed; shows **when** the ACK arrived (success color).
  - `Timed out` — no ACK after the bounded retransmit schedule (warning color); may note the try count.
  - `Rejected` — explicit REJ received (error color).
- **Composer** — a single message field + Send, plus a live **`n / 67` character counter** (the APRS per-message airtime budget, felt as you type). Addressing is inline in that one field (empty ⇒ broadcast to all); there is **NO separate To / recipient input**.
- **Inline error notice** — a rejected send queues nothing and shows an inline notice, never a phantom bubble (existing behavior, retained).

Typography and color use the existing tuxlink tokens (`--surface` / `--border` / `--text` / `--modem-accent` / semantic `--success` / `--accent-2` / `--error`) so the surface reads as part of the workspace across all themes. Monospace is used for RF-identity data (callsigns, times, the character count) and sans for human message text and UI chrome — a restrained accent, not an all-mono console.

**Honesty adjustments (do not fabricate values the app does not have):**
- No radio frequency is shown unless the app actually knows it. Phase 1a is KISS data-only and does not read the UV-Pro VFO, so the frequency is **omitted** here; it appears only once the native control surface (Section 5) can read it.
- Hop count / "heard N ago" render only if the inbound event actually carries the data; otherwise omitted.

## 5. The UV-Pro device-control surface (placement decided, wiring dependent)

Native UV-Pro control (channel / frequency / mode / battery / RSSI) is being built by a **parallel backend effort** (the Layer 2 Benshi/Vero profile, a separate bd issue with a dependency edge to `tuxlink-2f2n`). Its **placement is decided here**; its **wiring depends on that backend's published command/event contract**.

- The control lives **in the dock**, co-located with the chat it serves: a compact **control strip** within the chat surface (e.g. `UV-Pro · 144.390 · ch APRS · set channel ›`) for at-a-glance status + quick actions, with fuller control available through the dock's device view.
- The control area is also where the **single-Bluetooth-host arbitration** (the UV-Pro is one host at a time: KISS APRS *or* native control over the link) is surfaced honestly.
- Until the native backend lands, the chat surface omits device-control affordances rather than showing inert or fabricated ones.

## 6. Constraints honored

- **Inline only, no window clutter** — the chat is a dock surface, never a pop-up window.
- **Single pane** — the dock is shared and switchable; nothing opens a second window or a separate mode that strands the mailbox.
- **No stretched full-width** — the dock column is bounded.
- **RF-honesty** — delivery states reflect only backend-reported truth; no fabricated "delivered"; no fabricated frequency/values (Section 4).
- **Single-BT-host** — surfaced in the control area (Section 5), not hidden.
- **Theme cohesion** — existing tokens; Office register; no novelty fonts or off-brand aesthetic.

## 7. What changes from the Phase 1a build (PR #642)

| Built (Phase 1a) | This design |
|---|---|
| `AprsChatPanel` mounted via an `'aprs'` pseudo-folder in the sidebar (near Contacts), taking over the reading pane. | Re-homed into the **shared right dock**; removed from sidebar/Contacts nav. |
| No entry control beyond the sidebar item. | **① status-strip control** + **② dock tabs** + **③ View-menu** backstop. |
| Flat-feed rows: text + delivery chip; **no timestamps**. | Add **timestamps** (client-stamped) and **`Acked HH:MM`** (ACK time). Still one open-channel feed — no threads/bubbles. |
| Composer: single message field + Send (addressing inline; **no To field**). | Add the **`n / 67` character counter**. |
| Delivery chips styled as pills. | Restrained, Office-register delivery states (same four: Sent / Acked / Timed out / Rejected). |
| No open-channel cue; no device-control placement. | Quiet **open-channel** cue; **UV-Pro control strip** placement (wiring per Section 5). |

The existing `useAprsChat` hook, the four-state `DeliveryState` model, the no-bubble-on-rejected-send behavior, and the RF-honest delivery semantics are **retained**; this is a re-home + re-skin + additive-affordance pass, not a logic rewrite.

## 8. Phasing & dependencies

- **This design (chat surface + IA + entry points)** is implementable **frontend-only**: client-stamped timestamps, config-derived identity/path, the shared-dock re-home, and the status-strip + tab entry points need no backend change.
- **The UV-Pro device-control surface (Section 5)** depends on the parallel native-backend's published Tauri command/event contract. Its placement is fixed here; its wiring is a follow-on once that contract exists.
- The redesign lands on the `tuxlink-2f2n` branch (PR #642) **before** the feature is marked ready, so the chat ships in its intended form rather than the placeholder mount (per the project's "alpha = vettedness, not partial slices" standard).

## 9. Mockups

Static, high-fidelity renders produced during the brainstorm (real tuxlink shell, real tokens, Office register) — local dev scratch, not committed:

- Placement comparison (A / B / C): `dev/scratch/aprs-placement-{A,B,C}.png`
- Entry-point study (① / ② / ③ on the locked Option C): `dev/scratch/aprs-entrypoint.png`
- Source HTML: `dev/scratch/aprs-placement-mocks.html`, `dev/scratch/aprs-entrypoint-mock.html`

(Mock source + renders are dev scratch, `.gitignore`d; this spec is the durable record of the decisions they informed.)
