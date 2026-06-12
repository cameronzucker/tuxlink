# 19. Sonde rebrand and repo extraction

Date: 2026-06-12
Status: Accepted
Deciders: Cameron Zucker (operator), pine-arroyo-delta (agent)

## Context

The clean-sheet HF modem (ADR 0014, ADR 0015) was developed under the working
name `tuxmodem`, as a six-crate Cargo workspace at `tuxmodem/`: the modem crates
`tuxmodem-phy`, `tuxmodem-fec`, `tuxmodem-tx`, `tuxmodem-rx`, and the rig-control
crates `tux-rig-cm108`, `tux-rig-rts`. The workspace is self-contained — its own
`[workspace]` manifest, LICENSE, and README — and is **not** wired into the
desktop application: it is absent from the root Cargo workspace, and `src-tauri/`
references it only in three passing doc comments. The modem is not yet part of
any release.

Two pre-alpha decisions were outstanding:

1. The working name `tuxmodem` is descriptively flat and brand-coupled to
   tuxlink, which is wrong for a modem intended to stand on its own.
2. The workspace is parked inside the tuxlink repository despite having no build
   or runtime dependency on it.

## Decision

**Rebrand to Sonde.** The product is named **Sonde**, after the *radiosonde* — an
instrument lofted into the sky that transmits data back over RF, which is the
modem's function. The name back-forms an optional expansion, **S**oftware-**O**ptimized
**N**arrowband **D**ata **E**xchange. Crate names take the `sonde-` prefix
(`sonde-phy`, `sonde-fec`, `sonde-tx`, `sonde-rx`) and `sonde-rig-` for the
rig-control crates (`sonde-rig-cm108`, `sonde-rig-rts`).

The name was vetted before adoption: no existing amateur-radio mode or protocol
is named Sonde; the bare `sonde` crate name is taken on crates.io, but
`sonde-phy`, `sonde-fec`, `sonde-rx`, `sonde-tx`, `sonde-rig-cm108`, and
`sonde-rig-rts` are all available. Within the SDR hobby, "sonde" connotes
weather-balloon telemetry tracking; that connotation is RF-data-over-distance,
which reinforces rather than competes with the modem's identity.

**Boundary — the whole workspace is Sonde (Option A).** The rig-control crates
move and rename with the modem rather than staying behind in tuxlink. The
dependency trace is decisive: `tux-rig` is consumed only by the modem's transmit
path (`tuxmodem-tx` depends on `tux-rig-rts` and spawns `tux-rig-watchdog`), and
`src-tauri/` has zero references to it. The application's own PTT for managed
Dire Wolf and VARA does not route through these crates, so tuxlink loses nothing
by their departure. ADR 0015's "unified tux-rig crate" intent carries forward as
a unified `sonde-rig` crate.

**Two phases, never combined.** Combining a rename with a history-preserving
repository split produces an un-bisectable diff.

- **Phase A — rename in place.** Rename within the tuxlink repository on a single
  branch (this ADR's change set): directory and binary `git mv`s with history
  preserved, workspace-wide identifier and string substitution, the three
  `src-tauri` comments, the loopback smoke script, the `hf-channel-sim`
  attribution, the root README, and the live design docs. Decision records
  (this ADR's predecessor, ADR 0015), dated session handoffs, and the migration
  plan document retain their literal `tuxmodem`/`tux-rig` strings — rewriting a
  historical record falsifies it.
- **Phase B — extract.** Seed a standalone repository from the renamed `sonde/`
  prefix via `git subtree split` (history-preserving; `git filter-repo` and
  `git filter-branch` are banned by the destructive-git hook), give that
  repository its own CI, resolve the `../hf-channel-sim` path dependency, and
  remove `sonde/` from tuxlink.

## Phase A verification posture

The tuxlink CI does not build this workspace — it is not a root-workspace member
and no workflow references it — so Phase A cannot be compile-verified in CI, and
cold local cargo builds do not complete on the contended development host. Phase
A's gate is therefore **structural**: zero residual `tuxmodem`/`tux-rig`
references in the renamed surfaces, every workspace member and `[[bin]]` target
resolving to an existing file, and `cargo metadata --no-deps --offline` resolving
the full workspace graph (including the `../hf-channel-sim` path dependency). The
first full build and test of the renamed workspace is a deliberate Phase B
gate, run by the new repository's CI.

## Consequences

- The modem and its rig-control layer present a single, brand-independent name
  ahead of the alpha.
- `sonde-rig-*` becomes the canonical PTT layer. If tuxlink later needs it, it
  consumes `sonde-rig-*` as an external dependency — the normal direction for a
  shared library.
- ADR 0015's naming is superseded by this ADR; its integration and rig-control
  architecture decisions still stand.
- Phase B is tracked separately (bd `tuxlink-twx0`) and gated on operator
  confirmation before the GitHub repository is created.

This ADR supersedes the naming portion of [ADR 0015](0015-modem-integration-and-rig-control-foundation.md)
and continues the clean-sheet program of [ADR 0014](0014-clean-sheet-modem-no-prior-art-examination.md).
