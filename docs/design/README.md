# docs/design/

Project design docs, in two flavors:

- **Capability audits** — what the reference implementation does, what tuxlink does, where the gaps are.
- **Per-feature design docs** — proposals for specific features under design-before-code.

## Index — WLE-client parity

The WLE-client connection-mode parity work is the spine running through several docs:

- [`2026-05-29-winlink-express-feature-inventory.md`](2026-05-29-winlink-express-feature-inventory.md) — original capability-level audit (372 lines, yew-cypress-oak). Historical artifact: trustworthy at the unshipped-vs-shipped status level; 12.5% of connection-mode rows have framings the Phase 2 deep dives substantively contradicted. Always read alongside the verification + closure plan.
- [`2026-06-02-winlink-express-feature-inventory-verification.md`](2026-06-02-winlink-express-feature-inventory-verification.md) — Phase 1 verification against the decompiled WLE source. 24 in-scope rows verified, 1 ⚠ correction (§10.7 ConfirmConnection is post-connect, not pre-TX).
- [`2026-06-02-wle-client-parity-closure-plan.md`](2026-06-02-wle-client-parity-closure-plan.md) — **closure plan synthesis**. Sequences the 15 connection-mode bd issues into 3 tiers + names operator-decision gates + propagates the 3 audit corrections downstream.

The 15 per-mode deep dives live under `dev/scratch/winlink-re/findings/<mode>.md` (gitignored per `dev/scratch/` convention). Each closure-plan bd issue references its corresponding deep dive path; if a contributor's local clone is missing the cache, regenerate via the documented `ilspycmd` invocation in the closure plan's MANIFEST embed.

## Other design docs

- [`v0.0.1-ux-principles.md`](v0.0.1-ux-principles.md) — UX principles for the v0.0.1 release.
- [`v0.0.1-ux-mockups.md`](v0.0.1-ux-mockups.md) — canonical UX mockups (post-decompile, after the 2026-05-17 design lock).
- [`ax25-packet-protocol-findings.md`](ax25-packet-protocol-findings.md) — AX.25 protocol-level research.
- [`2026-05-22-ax25-packet-v0.1-design.md`](2026-05-22-ax25-packet-v0.1-design.md) — AX.25 v0.1 design.
- [`2026-05-22-session-type-selector-design.md`](2026-05-22-session-type-selector-design.md) — session-type selector design.
- [`2026-05-30-find-messages-design.md`](2026-05-30-find-messages-design.md) — find-messages search design.
- [`2026-06-01-tcp-p2p-telnet-design.md`](2026-06-01-tcp-p2p-telnet-design.md) — TCP P2P Telnet design.
- [`2026-06-02-cms-request-protocol-grounding.md`](2026-06-02-cms-request-protocol-grounding.md) — CMS request protocol grounding.
- [`ardop-deployment-findings.md`](ardop-deployment-findings.md) — ARDOP deployment findings.
- [`rms-node-map-feature.md`](rms-node-map-feature.md) — RMS node map feature design.

## Conventions

- Date-stamped filenames (`YYYY-MM-DD-<slug>.md`) for design docs tied to a specific work cycle.
- Undated filenames for evergreen / cross-cycle reference docs (`v0.0.1-ux-principles.md`, `ardop-deployment-findings.md`).
- Capability audits and closure plans take the `<date>-<topic>-<flavor>.md` shape (`2026-05-29-winlink-express-feature-inventory.md`, `2026-06-02-wle-client-parity-closure-plan.md`).
