# Handoff — tanager-ridge-larch — doc sweeps shipped + multi-network EmComm thesis (research live)

Date: 2026-06-14 · Agent: tanager-ridge-larch · Branch (main checkout): `bd-tuxlink-xygm/recover-handoffs`

## One-line
Two doc PRs opened early-session (staleness sweep + Winlink password recovery-address), then a long
research/design arc: Winlink CMS deep-research, a clean-sheet network-architecture exploration, a
Winlink-Forms efficiency takedown, and a **multi-network EmComm product thesis**. No app code
changed in the design arc. An **ATAK-plugin feasibility research pass is running in the background**
as the live next step.

## Shipped this session (code/docs)
- **tuxlink-ivc1 — doc staleness sweep**, PR **#680**: executed the pine-arroyo-delta verified
  rewrites across ~16 `docs/user-guide` files (menu paths → radio panels / unified Settings; identity
  → Settings → Identities; outbound attachments shipped; Request Center; VARA P2P pending; native form
  composers; position precision 4/6-char; ux-anti-patterns Pat-daemon; 12-cat-and-rigctld rewritten as
  external operator-run infra). `lint:docs` green. Worktree `worktrees/bd-tuxlink-ivc1-doc-staleness-sweep`.
- **tuxlink-mtio — recovery-address docs**, PR **#682**: added the winlink.org password-recovery-address
  precondition (preventive + reactive) + tactical-address forum-only caveat to `02-first-launch-wizard.md`.
  Deliberately SEPARATE from #680 per operator. Worktree `worktrees/bd-tuxlink-mtio-recovery-address-docs`.
- ⚠ **VERIFY MERGE STATE:** `gh pr list --state open` (2026-06-14) shows only #690 (release-please draft).
  #680 and #682 are no longer in the open list → presumably merged since. Confirm with `gh pr view 680`
  / `gh pr view 682` before assuming. tuxlink-ivc1 / tuxlink-mtio still show `in_progress` in bd.

## The design arc (research + thesis — captured in dev/scratch, **gitignored / local-only on pandora**)
Five durable artifacts under `dev/scratch/` (NOT pushed — dev/scratch is .gitignore'd; they live on
pandora only). Read them for full detail; summary below.

1. **2026-06-13-arrl-radiogram-winlink-form.md** — a real ChatGPT-generated ARRL radiogram Winlink form
   (K2EFG) + the on-air model: forms transmit a rendered text body **+** an `RMS_Express_Form_*.xml`
   attachment (data sent twice). Winlink team added an "omit XML" setting because the bloat is real.
2. **2026-06-13-winlink-cms-and-clean-sheet-architecture.md** — CMS deep-research (workflow
   `wf_76e1ca3b-d29`): ARSFI all-volunteer 501(c)(3), conventional internet store-and-forward + SMTP
   egress, **B2F is open & reimplementable** (wl2k-go/paclink-unix/ARSFI's own), ~20,749 registered /
   7,642 active (2017 = dated lower bound), Open Message Viewer is the volume oracle. Build-your-own:
   **nearly free ($5–50/mo to a 10k-user ceiling) for an independent closed-namespace internet core**;
   the only real costs (email deliverability, EmComm-grade uptime) are opt-in.
3. **2026-06-13-winlink-forms-efficiency-rebuttal.md** — deployable rebuttal to "Tuxlink needs more
   forms!" The forms wire architecture is ~10× its payload (~9–19 plain messages of overhead per form);
   the radiogram/iamsafe "efficiency" defense is a category error (concise-for-voice ≠ efficient-for-
   bytes); read the auth-walled group thread (`w0cocPbnq3E`, "ARRL Radiogram as a Template") — Andrew
   Watson is right (handshake > payload; direct SMTP avoids relay), the rebuttal rests on three false
   premises (contention=sysop config; the case is always hypothetical/never a real incident; modern
   data-HTs like UV-Pro dissolve the laptop-less premise).
4. **2026-06-13-clean-sheet-network-shape-reticulum-gap.md** — the optimal-shape exploration. 2×2
   (centralized/decentralized × async-S&F/real-time-mesh); Babel/batman-adv = connectivity only (assume
   contemporaneous path); **Reticulum + LXMF ≈ 80% of the picture** but encrypted-by-default → Part
   97-illegal on ham RF (no cleartext mode, builders won't add it). Unoccupied ground = **Reticulum's
   shape with the secrecy axis FLIPPED: signed-and-public instead of encrypted** (Part 97-legal by
   construction; public record falls out free) + convergent-state data model + real UX. Part 97 latitude:
   §97.113(a)(4) bans "obscuring meaning," NOT all crypto — keep signatures/Merkle/published-compression,
   lose only secret-key confidentiality. HF budget: security/evidence overhead FITS (~1–2 s VARA airtime;
   dropping encryption even saves handshake round-trips); a naive CRDT does NOT (KB metadata on a 10-byte
   change). **HF constrains the DATA MODEL, not the security model** → cross HF as compact signed op-log
   deltas.
5. **2026-06-13-tuxlink-multi-network-emcomm-thesis.md** — THE forward thesis. Tuxlink as a multi-network
   EmComm platform (APRS + Meshtastic/MeshCore + Reticulum). Three strategies: (1) **separate native
   channels in tac chat** [trivial first slice; builds the per-network transport I/O]; (2) **semantic
   bridging** [the trap — identity/consent/payload-LCD hell; SKIP]; (3) **networks as dumb transport for
   a Tuxlink overlay ("speak Tuxlink", send bytes)** = the destination; it IS the clean-sheet bearer-
   agnostic substrate instantiated over existing RF; **ATAK-precedented** (Meshtastic ships an ATAK mode;
   APRS/AX.25 ATAK transport plugins exist). Roadmap: **1 → open ATAK plugin → 3; skip 2.**

## Scope posture (important)
Codex reviewed tuxlink and flagged **scope creep** given it's barely in alpha — fair, and the operator
agrees. Resolution used here: **thesis ≠ feature.** Multi-network is the deliberate v0.x→product bet;
Winlink-on-Linux is the wedge. Do NOT build the platform now. The ATAK plugin is **scope-safe** because
it's a *separate Android/ATAK-CIV SDK codebase*, not a tuxlink feature — it doesn't touch the alpha.

## LIVE next step (running now)
A **background research agent** is verifying ATAK-plugin feasibility (see new bd issue, P3, title
"ATAK-over-APRS/UV-Pro open-source transport plugin feasibility …"). Four questions: (a) ATAK-CIV plugin
SDK status/license; (b) existing open vs closed APRS/Meshtastic ATAK transport plugins; (c) the specific
predatory ~$200 product(s) + what they do; (d) UV-Pro data path (tuxlink already does native UV-Pro
control). Deliverable: go/no-go + parts list. **If picking up fresh: check for the agent's result, or
re-run the four-question pass.** Domain web-claims are stale-prone — verify, don't assume.

## Other state
- bd: tuxlink-ivc1 + tuxlink-mtio still `in_progress` (close once #680/#682 merges confirmed). New P3
  backlog issue filed for the ATAK/multi-network thesis.
- Memory saved this session: `feedback_authorized_cookie_scrape_workflow` (operator pastes Google
  cookies in chat for authenticated Playwright scrapes — the corpus method; execute transiently via
  stdin, don't relitigate; clipboard-paste crashes WayVNC).
- Main checkout working tree is dirty with many *pre-existing* untracked handoffs/PNGs from prior
  sessions + a `.beads/issues.jsonl` change — NOT mine; left untouched.
