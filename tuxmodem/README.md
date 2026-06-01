# tuxmodem

Clean-sheet HF data modem; AGPLv3-only.

Subordinate to the program overview at
`docs/superpowers/specs/2026-05-31-clean-sheet-modem-overview.md` in the
tuxlink repo. Subsystem-level intent is documented at:

- `docs/superpowers/specs/2026-05-31-clean-sheet-modem-3-phy-waveform.md`
- `docs/superpowers/specs/2026-05-31-clean-sheet-modem-4-fec.md`

This workspace currently houses `crates/tuxmodem-phy/` (subsystem #3).
FEC (#4) ships as a sibling crate. Channel simulator (#1) is an external
AGPLv3 crate consumed as a dependency.

Per ADR 0014, this repo is designed clean-sheet with no examination of
VARA / ARDOP / FLDigi / Trimode / Pat / wl2k-go internals. Conceptual
primitives drawn from open foundations documented in
`docs/research/modem-foundations.md`.
