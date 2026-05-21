# 13. v0.0.1 main UI is Mock B (principles-faithful) — supersedes ADR 0012's Mock D

Date: 2026-05-20
Status: Accepted (supersedes [ADR 0012](0012-v001-main-ui-adopts-mock-d.md) in full — 0012's premise, that the operator approved Mock D, was wrong)
Deciders: cameronzucker (operator / design authority), hemlock-raven-wren (implementing agent)

## Context

The approved v0.0.1 main-UI design is **Mock B (principles-faithful)** —
`docs/design/mockups/images/mock-b-principles-faithful.png` and the `MOCK B`
block of `docs/design/mockups/2026-05-17-mocks-v1-four-directions.html`. Mock B
is the design the design-doc §3 decisions describe: a top **dashboard ribbon**
(callsign · grid · GPS/position · UTC+local · connection), a left **folder
sidebar** (Mailbox + Connections), a two-pane list + reading pane, a
**human-shaped session log** at the bottom, a `connection · unread · version ·
Pat` status bar, and **compose as a separate floating window**.

Two consecutive sessions failed to build this:

1. The first built a "synthesis" layout (ribbon + sidebar + session-log + dock)
   that was a poor execution of Mock B; the operator rejected it.
2. The next session (pika-glade-bluff) mis-read that rejection as "switch to
   **Mock D** (Mail.app-minimal)" and recorded it as an operator decision in a
   handoff, the bd issue `tuxlink-yd4`, and **[ADR 0012](0012-v001-main-ui-adopts-mock-d.md)**.
   The following session (hemlock-raven-wren) built faithfully to Mock D — the
   wrong target — because it trusted those agent-authored records instead of
   verifying the approved artifact with the operator. When the operator's
   questions ("why did the folders move to the top? why did the top operator-info
   bar disappear?") pointed directly at Mock B's sidebar + dashboard ribbon, the
   agent rationalized them as Mock D properties rather than recognizing the spec
   was wrong.

On 2026-05-20 the operator stated unambiguously (with the file link) that the
approved design is **Mock B**, not Mock D.

## Decision

1. **The approved v0.0.1 main UI is Mock B.** This ADR supersedes ADR 0012 in
   full. ADR 0012's "v0.0.1 adopts Mock D" is withdrawn — it rested on a
   misidentification of the approved design, not a real operator decision.

2. **The UI is rebuilt to Mock B**, porting its structure/CSS verbatim:
   dashboard ribbon · folder sidebar (Mailbox + Connections) · 2-pane ·
   human session log · status bar · compose-as-separate-window.

3. **Source-of-truth rule (the root-cause fix).** The approved mock
   (PNG + the mock HTML block the operator has signed off on) is the sole spec.
   Agent-authored records (handoffs, ADRs, bd issues) are *derivative* and MUST
   NOT be treated as the spec — when they conflict with the operator's approved
   artifact, the artifact wins and the records are corrected. A session changing
   the approved design verifies the target against the operator, not against a
   prior session's claim of what was decided.

## Consequences

- design-doc §3 carries an "approved design: Mock B" note (replacing the
  incorrect Mock-D banner). `tuxlink-yd4` is corrected/closed as a
  misidentification; a Mock B build issue tracks the rebuild.
- ADR 0012 stays in the tree as the historical record of the error (Status:
  superseded by this ADR), per the no-delete ADR convention.
- The dashboard-ribbon, folder-sidebar, and session-log components (parked
  during the Mock D build) are restored and made faithful to Mock B.
- Validation rule (unchanged from 0012, still correct): fidelity is checked
  against the real compiled Tauri/WebKitGTK app via `grim` vs the approved PNG —
  here `mock-b-principles-faithful.png` — never a Chromium gallery.

## Alternatives considered

- **Keep Mock D, ask the operator to re-approve.** Rejected: the operator has
  stated the approved design is Mock B; there is nothing to re-litigate.
- **Treat both as valid skins behind a toggle.** Rejected for v0.0.1: there is
  one approved design (Mock B); a theme/skin system is out of scope (tuxlink-8za
  tracks selectable schemes separately).
