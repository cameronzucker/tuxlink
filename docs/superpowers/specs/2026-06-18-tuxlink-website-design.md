# tuxlink.org landing site — design

- **Issue:** tuxlink-tp1m (build); future elevation pass tracked as tuxlink-nyyp
- **Date:** 2026-06-18
- **Agent:** marten-owl-poplar
- **Status:** approved (scope, visual direction, and full-page design confirmed by the operator on 2026-06-18 via the visual companion)

## Goal

A polished public landing page at apex `tuxlink.org` whose single job is to turn
a Linux amateur-radio operator into someone who downloads Tuxlink, tries it, and
reports back. Alpha-stage, honest about maturity, trust-forward for an EmComm
audience. Docs remain on GitHub for this first version.

This is a complete, well-made first site — not a throwaway. A later pass
(tuxlink-nyyp) may add richer motion / scroll-driven storytelling; the structure
below is built so that elevation is additive, not a rewrite.

## Decisions

| Decision | Choice |
|---|---|
| Scope | Landing + downloads. One cohesive page. Hosted docs, blog/news, and community are out of scope for this version. |
| Visual direction | **B — Modern product**: refined dark product-landing, a soft amber glow behind the hero screenshot, generous spacing, large type, single amber accent. (Chosen over a field-manual/tactical and a terminal/operator direction.) |
| Source | A dedicated `tuxlink-website` git repo (separate from the GPL app repo). |
| Stack | Astro — component-based static-site generator that ships ~zero JS by default (fast, content-first, scales to hosted docs later). |
| Host | Cloudflare Pages, git-connected (auto-build on push, edge CDN, per-PR preview URLs, free privacy-respecting analytics). |
| Domain | Apex `tuxlink.org`; DNS already on Cloudflare → one-click Pages custom-domain. |
| Voice | Declarative, present-indicative, no first person. No defensive self-assertion. Alpha stated plainly ("looking for testers"), positioned as confidence, not apology. |

## Brand tokens

Derived from the existing app icon (dark base, amber radio-tower-over-envelope,
white linework) and the app UI palette.

- **Background:** `#0c1118` (page), `#0a0f16` (alternating sections), `#0d141d` (cards).
- **Text:** `#f3f7fb` (headings), `#cdd9e5` (body), `#9fb0c0` (muted), `#7f8c99` (meta).
- **Accent (primary):** amber `#f0c24a` — the signature, used for the logo motif, the primary CTA, eyebrows, and the hero glow.
- **Accent (secondary):** green `#39d98a` — the tactical/"live" signal (status dot, the tactical mission column).
- **Borders:** `#18222f`–`#243446`.
- **Type:** system UI sans stack for all copy (no web-font dependency — fast, robust). Headings 800 weight, tight tracking.
- **Hero treatment:** a radial amber glow (`rgba(240,194,74,.16)`) behind the screenshot; the screenshot in a soft 1px-ringed card with a deep shadow.

## Information architecture (page sections)

1. **Sticky nav** — logo + "Tuxlink"; links Features / Screenshots / Download / GitHub; a Download button. Translucent, blurred on scroll.
2. **Hero** — eyebrow pill ("NATIVE LINUX WINLINK"); H1 "One Linux window for strategic & tactical EmComm"; one-sentence subhead (Winlink over HF, APRS over VHF/UHF, single Rust+Tauri app, no sidecar/Wine/daemon); dual CTA (Download for Linux / View on GitHub); a plain status line (Alpha · looking for testers · deb·rpm·AppImage · x86-64 & arm64 · GPL-3.0); the workspace screenshot (ICS-213 Winlink message beside the live APRS Tac Chat net) under the amber glow.
3. **Two missions, one workspace** — the differentiator. Two cards: **Strategic · HF** (amber) — native B2F over ARDOP/VARA, CMS telnet/TLS + radio-only + P2P, forms/attachments/Request Center; **Tactical · VHF/UHF** (green) — APRS messaging with authentic symbols on an offline map, native Benshi UV-Pro control, AX.25 packet to a local RMS.
4. **Built for the field, not the lab** — six-feature grid: native Winlink (no sidecar), strategic+tactical in one, multi-transport, UV-Pro control, offline maps, keyring credentials. Each: glyph + title + one sentence. (Glyphs are proper radio/map/key icons in the build — the mock used placeholder dots.)
5. **See it running** — four real screenshots (mailbox, ARDOP HF, Request Center, AX.25 packet), alternating sides, each with a one-line caption. Privacy-safe sample data.
6. **Migration callout** — "Coming from Winlink Express or Pat?" → links the existing migration guide (`docs/user-guide/32-from-express-or-pat.md` on GitHub for now).
7. **Download** — three format cards (.deb / .rpm / .AppImage) each with the target distros and a one-line install command; links resolve to the GitHub `releases/latest` assets. A plain alpha/testers note beneath.
8. **Footer** — GitHub, Releases, Docs, `security@tuxlink.org`, GPL-3.0, "Made for Linux amateur radio operators."

Responsive: the two-column hero, missions, feature grid, and showcase collapse
to single-column under ~860px.

## Architecture

- **Repo:** new `tuxlink-website` (created during implementation; operator
  confirmation required before creating the GitHub repo). Astro project:
  `src/pages/index.astro` composes section components from `src/components/`
  (`Nav`, `Hero`, `Missions`, `Features`, `Showcase`, `Migration`, `Download`,
  `Footer`); brand tokens centralized in one CSS file / Astro layout; assets in
  `src/assets/` or `public/`.
- **Content:** copy lives in the section components (or a small `content/` data
  module), sourced from and kept consistent with the app `README.md`. Screenshots
  copied from the app repo's `docs/readme/images/` into the site repo, optimized
  (resized + compressed) at build.
- **Download links:** point at `https://github.com/cameronzucker/tuxlink/releases/latest`
  and the per-format asset URLs; no release automation owned by the site.
- **Deploy:** Cloudflare Pages project connected to the repo; build command
  `astro build`, output `dist/`; custom domain `tuxlink.org` (apex) + `www`
  redirect. A PR preview URL per change.
- **Homepage metadata (folded in here, executed against the APP repo):** add
  `homepage = "https://tuxlink.org"` to `src-tauri/Cargo.toml`, a `homepage`
  field to `package.json`, and a tuxlink.org link in `README.md`. These land in
  the app repo (a small separate commit/PR there), not the website repo.

## Testing / verification

- A static landing site has no unit-test surface worth inventing. Verification is:
  Astro `build` succeeds; the page renders correctly in a real browser (grim on
  the converged display, or the Cloudflare PR preview); links resolve; the page is
  responsive at mobile/tablet/desktop widths; Lighthouse/structure sanity
  (performance, no console errors, alt text on images).
- A simple link-check (internal anchors + outbound to GitHub/releases/docs) is the
  one automatable gate.

## Out of scope (this version)

- Hosted user-guide docs on the site (stays on GitHub).
- Blog / changelog / news, community/contributing pages.
- The elevation pass — advanced frontend frameworks, motion, scroll-driven
  storytelling (tracked as tuxlink-nyyp; depends on this build).
- Any release/build automation (the site only links to existing GitHub releases).

## Definition of done

1. `tuxlink.org` serves the landing page over HTTPS from Cloudflare Pages.
2. All eight sections render with real copy and real screenshots, responsive
   across mobile/tablet/desktop.
3. The Download buttons resolve to the current GitHub release assets.
4. The migration callout and footer links resolve.
5. The app repo carries `homepage = https://tuxlink.org` in Cargo.toml +
   package.json + a README link.
