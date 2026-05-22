# 14. Design the v0.5+ modem clean-sheet; do not examine VARA's internals (preserve the independent-creation defense)

Date: 2026-05-22
Status: Accepted
Deciders: cameronzucker, hemlock-arroyo-mink

## Context

Tuxlink's v0.5+ HF modem is a from-scratch replacement for VARA — a full replacement, not a bridge, with no waveform-bit-compatibility or interop goals (the design-space posture is recorded in the `project-v05-modem-design-posture` memory entry, established 2026-05-18). VARA is closed-source proprietary software (allegedly written in VB6, which is unusually amenable to decompilation).

A recurring and intuitive temptation surfaces whenever this modem is discussed: to make sure tuxlink's modem does not infringe VARA, *understand VARA precisely first* — up to and including decompiling the binary — on the theory that "we can't deliberately avoid what makes the prior art unique if we don't understand how it works." This decision exists because that intuition, while reasonable on its face, is partly inverted by how copyright law actually allocates defenses, and the project's explicit success criterion for the modem is that it "survive scrutiny and legal challenges, or avoid the latter altogether."

The relevant legal landscape, as understood by the engineering team (this ADR is not legal advice):

- **Copyright protects expression, not ideas, algorithms, or functional behavior.** The DSP/modem *method* that lets VARA work on poor HF is largely idea/function and is freely reimplementable; VARA's *code* (and its structure, sequence, and organization) is protected expression and must never be copied.
- **Independent creation is a complete defense to copyright infringement.** If you never accessed the protected work, resemblance is not infringement — full stop. This is the strongest defense available.
- **Clean-room ("Chinese wall") reverse engineering** is the structured fallback that lets you build a defensible reimplementation *after* examining the original: a "dirty" team examines the original and produces a sanitized functional specification (behavior, never expression); a separate "clean" team implements solely from that spec, having never seen the original; meticulous provenance records back the wall.
- **The trade is one-directional.** The moment you examine VARA's internals you forfeit independent creation and must rely on the clean-room defense instead — which is real but harder to win, execution-dependent, documentation-dependent, and structurally weak for a solo or very small developer who cannot maintain a genuine wall (the same person who studies the decompilation then writes the implementation *is* the contamination).
- **Patents are a separate, strict-liability axis.** Independent creation is *not* a defense to patent infringement. Copyright posture and patent posture must be handled separately.

On 2026-05-22 Cameron weighed decompiling VARA against staying clean-sheet, with the tradeoff above made explicit, and chose to keep the independent-creation defense intact.

## Decision

1. **The v0.5+ modem is designed clean-sheet** from open, general engineering knowledge: modem theory (OFDM/QAM/PSK, FEC, ARQ), published academic and general amateur digital-mode literature, and first principles.

2. **No examination of VARA's internals, from any source.** This bright line covers: decompilation or disassembly of VARA binaries; obtained or leaked VARA source; third-party reverse-engineering write-ups of VARA's internal protocol or algorithms; and **black-box on-air characterization of VARA itself** undertaken to inform the modem design. If a contributor — human or AI agent — feels the urge to "just check how VARA does it," **STOP**: that single act can forfeit the independent-creation defense for the entire effort.

3. **Advertised, common-knowledge facts are background, not examination-derived inputs.** Publicly advertised, operator-observable specifications that Cameron already knows from licensed operation of VARA — e.g., that VARA HF Standard occupies ≈2300 Hz of bandwidth, or that it is OFDM-based — are general background and do not require avoidance. The line is "publicly advertised / common knowledge" versus "internal detail extracted by examining the product."

4. **Characterizing your own equipment and the channel is explicitly in-scope.** The planned RF measurement rig (`project-rf-measurement-rig-design`) characterizing the Xiegu G90 and the HF channel measures *physics and your own radios*, not VARA. That work is unaffected by this ADR. The forbidden activity is pointing measurement at *VARA's emissions* to reverse its design.

5. **This ADR is the "document non-access" record.** An independent-creation defense rests on a contemporaneous record that the design was produced without accessing the prior art. This ADR, dated and committed to git history, is that record. Design provenance should cite open sources; contributors must not introduce VARA-internal material into the design record.

6. **Scope and limits.** This ADR addresses copyright posture only. Before any production release, obtain an IP-attorney patent clearance (independent creation does not defend against patents). The legal doctrine summarized here is the engineering team's working understanding, not legal advice; if the stakes warrant, have counsel bless the posture.

## Consequences

**Positive:**

- The strongest copyright defense — independent creation — is preserved rather than traded away.
- The process is the *simplest* one: no quarantine, no dirty/clean wall, no decompilation-provenance log, no EULA reverse-engineering exposure against licensed proprietary software.
- Aligns with the existing modem posture (`project-v05-modem-design-posture`): optimize purely for technical merit, do not adopt VARA's specific protocol choices, and do not inherit its failure modes. Not-looking and not-adopting are mutually reinforcing.
- Gives future agents an unambiguous, index-visible bright line at exactly the moment the temptation arises.

**Negative:**

- The project forgoes any shortcut that examining VARA's internal solutions might have offered; functional understanding must come from general literature, first principles, and measurement of the team's own equipment and the channel — slower than copying.
- There is a real possibility of independently re-deriving something VARA also does. This is acceptable for copyright (independent creation covers it) and is handled separately for patents (clearance before production).
- It requires ongoing discipline. The temptation is genuine and recurring; the bright line and the index annotation exist precisely because prose alone is weak against an eager contributor.

**Reversal cost (one-way door):** You cannot "un-see" VARA once examined. Reversing this decision — adopting clean-room examination — is a deliberate *downgrade* of the legal posture and must be its own superseding ADR, with a real wall, provenance discipline, and ideally counsel in place *before* any examination occurs. Do not reverse casually or implicitly.

## Alternatives considered

- **Clean-room reverse engineering** (dirty room documents VARA's behavior → sanitized functional spec → walled clean implementation). Stronger than naive copying, but weaker than independent creation, and it demands a genuine wall, meticulous provenance, and ideally legal review — disproportionate for a solo/small effort whose entire advantage is that it can simply decline to look. Remains available as a future superseding decision if a concrete, justified need arises.

- **Decompile VARA to "understand it in order to avoid it."** Rejected. This is the configuration that forfeits independent creation while handing back mostly the *protected expression* layer (code) rather than the functional layer the team actually needs; it also likely bumps VARA's EULA anti-reverse-engineering clause. The intuition "avoid what you understand" is inverted by the law: declining to look is the stronger form of avoidance.

- **Black-box on-air characterization of VARA via SDR** (observe behavior, not code). More defensible than decompilation and it dovetails with the planned RF rig, but it is still examination of the prior art and still forfeits the pristine independent-creation posture. Rejected as a *design input*. (Measuring the team's own radios and the HF channel with that same rig remains fully in scope — see Decision §4.)
