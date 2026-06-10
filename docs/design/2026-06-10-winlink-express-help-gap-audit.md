# Winlink Express CHM help corpus - Tuxlink docs gap audit

> Date: 2026-06-10
> Agent: jay-bluff-fir
> Issue: tuxlink-tdjs
> Source corpus: local Winlink Express / RMS Express CHM help extracted with `archmage`

## Purpose

This audit mines the legacy Winlink Express help file as a documentation
coverage source. It is not a license to copy Winlink Express help text into
Tuxlink. The CHM is valuable because it shows what an experienced Winlink
operator expected the official client to explain: setup, connection flows,
message routing, forms, contact management, radio setup, diagnostics, and
operating edge cases.

The output here is a gap map for Tuxlink docs:

- What does the WLE help cover?
- Where do Tuxlink docs already cover the concept?
- Where are Tuxlink docs partial, missing, or blocked by product gaps?
- Which WLE behaviors should be intentionally omitted or reframed for
  Tuxlink?

## Source handling

Raw and extracted WLE help stays out of git under `dev/winlink-reference/`.
That path is gitignored because the artifacts are third-party Windows client
material, not Tuxlink project assets. Track only paraphrased findings, source
page IDs, hashes, and gap decisions.

Local extraction performed in this worktree:

```bash
mkdir -p dev/winlink-reference/source
cp /home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS\ Express/RMS\ Express/RMS\ Express.chm dev/winlink-reference/source/
cp /home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS\ Express/RMS\ Express/TemplateHelp.txt dev/winlink-reference/source/
cp /home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS\ Express/RMS\ Express/Winlink_Express_Revision_History.txt dev/winlink-reference/source/
archmage -x dev/winlink-reference/source/RMS\ Express.chm dev/winlink-reference/express-chm
sha256sum dev/winlink-reference/source/RMS\ Express.chm \
  dev/winlink-reference/source/TemplateHelp.txt \
  dev/winlink-reference/source/Winlink_Express_Revision_History.txt
```

Source hashes:

| File | SHA-256 |
|---|---|
| `RMS Express.chm` | `caec1bb35babf65275f48b6060d73a71a8df4b8f89961aec1784fc52af75d28a` |
| `TemplateHelp.txt` | `2972e7155fa0802cd615c561ac564e37bff314f99fa8d28ae2ea41728c0699f3` |
| `Winlink_Express_Revision_History.txt` | `99553b3afc94d211ff6ad93932debd69926b7a1ea57744370196c7bf24fed68b` |

Extraction result: 330 files total, 75 CHM TOC entries, 73 entries with local
page targets, 67 unique target pages.

## Usage rules for docs authors

- Use page IDs such as `html/hs80.htm` or `page_5.html` as citations.
- Paraphrase the behavior in Tuxlink's own language.
- Do not paste WLE prose, screenshots, or diagrams into tracked docs.
- Before implementing a product behavior from this audit, read the source page
  and confirm against current Tuxlink code. This audit is a coverage map, not
  a protocol specification.
- Prefer Tuxlink-native explanations over "Express does X" unless the topic is
  explicitly migration-oriented.

## High-value findings

1. WLE help is not just a UI manual. It explains operating concepts that Tuxlink
   docs need: message types, routing behavior, pending-download triage, channel
   data, radio-only/hybrid operation, Post Office sessions, acceptlist/spam
   controls, attachment resizing, logs/statistics, and troubleshooting method.
2. Tuxlink's user guide is broad, but several areas are feature-oriented rather
   than operator-procedure-oriented. The CHM is strongest where it says "to do
   this, open this kind of session and expect this behavior."
3. The existing WLE capability inventory
   (`docs/design/2026-05-29-winlink-express-feature-inventory.md`) audits
   product parity from the decompiled UI. This document is different: it audits
   documentation coverage and operator teaching depth.
4. The richest docs gaps are now tracked:
   - `tuxlink-ng91` - contacts/address book and group-address coverage.
   - `tuxlink-v3aq` - message-management and operator-admin coverage.
   - `tuxlink-1doe` - connection walkthrough depth.
5. Existing product/docs work already covers some discovered gaps:
   - `tuxlink-px36` / `tuxlink-zmzx` - WLE mailbox migration tooling/docs.
   - `tuxlink-ytya` - HTML forms infrastructure.
   - `tuxlink-ddiq` - catalog request builder.
   - `tuxlink-vrpk` - GRIB request flow.
   - `tuxlink-pxf` - attachment shrink-to-fit.
   - `tuxlink-bsiy` - pending incoming message selection.

## Coverage status key

| Status | Meaning |
|---|---|
| Covered | Current user docs cover the concept well enough for alpha. |
| Partial | Current docs mention it, but need more procedure, migration mapping, or edge cases. |
| Gap | User-facing docs do not cover it in a useful way. |
| Product gap | Docs should say "not yet" or wait for implementation. |
| Intentional omit | WLE behavior should not be copied into Tuxlink, or belongs to a different product role. |

## Corpus coverage matrix

### Overview and system concepts

| WLE help topic | Source | Tuxlink docs status | Gap/action |
|---|---|---|---|
| Winlink Express overview, supported modes, callsigns/tactical addresses | `html/hs10.htm` | Partial: `01-what-is-tuxlink.md`, `04-the-winlink-ecosystem.md`, `32-from-express-or-pat.md` | Use as migration grounding for what Express users expect, especially multi-callsign/tactical language. |
| B2F protocol | `page_8.html` | Covered: `06-the-b2f-protocol.md` | Keep as cross-check source when protocol docs change. |
| Amateur Radio Safety Foundation | `html/hs100.htm` | Partial: credits/ecosystem mention Winlink context | No urgent action; avoid overexplaining the organization unless relevant. |
| License | `html/hs360.htm` | Intentional omit | WLE licensing is not Tuxlink licensing. |
| Registration | `html/hs35.htm` | Partial: first-launch and migration docs | Make sure account-registration docs are explicit for alpha users. |
| Updates | `html/hs370.htm` | Intentional omit | WLE self-update is not Tuxlink's Linux package flow. |
| New call/account change | `html/hs10.htm` | Partial | Migration docs should explain changing callsign/account in Tuxlink settings/keyring. |
| Third-party traffic rules | `page_14.html` | Gap/partial: emcomm docs mention compliance but not this operator concern | Add careful, non-legal-advice framing to operating practices. Tracked by `tuxlink-v3aq`. |

### Setup and preferences

| WLE help topic | Source | Tuxlink docs status | Gap/action |
|---|---|---|---|
| Installation | `html/hs20.htm` | Covered for Tuxlink package install | Keep separate from WLE. |
| Basic configuration | `html/hs30.htm` | Partial: first-launch, settings, migration | Cross-check callsign/password/grid behavior; already cited in older plan. |
| User preference settings | `html/hs40.htm` | Partial: `27-settings.md` | Settings docs should become an exhaustive reference, not just a tour. |
| New message notification and forwarding | `html/hs42.htm` | Product gap/docs gap | Decide whether forwarding rules are in scope; otherwise mention not yet in migration docs. |
| Form settings | `html/hs45.htm` | Product gap: forms work in progress | Feed into `tuxlink-ytya` docs when forms UI is stable. |
| Managing multiple call signs | `html/hs50.htm` | Product gap/partial migration docs | Track as future multi-identity work; do not imply alpha supports it. |
| Callsigns with qualifiers | `html/hs60.htm` | Partial | Include in identity/account docs if Tuxlink supports qualifier-like identifiers. |
| Winlink Hybrid / radio-only network | `html/hs70.htm` | Partial: `33-operating-modes.md` | Needs a clearer operator story for client vs hub roles. Tracked by `tuxlink-1doe`. |
| Secure login | `html/hs110.htm` | Partial: keyring/security docs | Tuxlink differs here; explain OS keyring and Winlink password flow. |
| Telnet setup | `html/hs200.htm` | Partial: first-send and transport docs | Add procedure depth and troubleshooting. Tracked by `tuxlink-1doe`. |
| Packet setup | `html/hs280.htm` | Partial: `14-packet-on-ax25.md` | Add "from working modem to session" procedure once packet UX settles. |
| Pactor setup | `html/hs270.htm` | Intentional/product omit | Tuxlink currently omits PACTOR; migration docs should be explicit. |
| Robust Packet setup | `html/hs290.htm` | Intentional/product omit | Specialized SCS path; document as not supported if mentioned. |
| VARA HF and FM | `page_13.html` | Partial: `16-vara-hf-deep-dive.md` | Good conceptual coverage; needs Tuxlink-specific setup walkthrough. |
| Radios with built-in sound cards / ARDOP | `html/hs250.htm` | Partial: radio notes, ARDOP docs | Use as checklist for radio-specific docs and sound-card routing. |
| Sound card selection | `html/hs460.htm` | Partial: DigiRig/SignaLink docs | Add "which device is audio vs PTT vs CAT" troubleshooting language. |
| Optimum receiver settings | `html/hs520.htm` | Gap/partial | Tuxlink docs need practical receive-level/tuning guidance. Tracked by `tuxlink-1doe`. |
| Radio control | `html/hs320.htm` | Covered/partial: `12-cat-and-rigctld.md` | Cross-check CAT setup terms against WLE expectations. |
| Flex Radios | `html/hs570.htm` | Gap | Niche; consider radio-specific notes only if users ask. |
| Icom IC-F8101 | `html/hs260.htm` | Gap | Niche; not alpha-blocking. |
| CODAN radios | `html/hs265.htm` | Gap | Niche; not alpha-blocking. |
| Icom CI-V default addresses | `html/hs560.htm` | Gap/partial | Useful appendix for radio-specific notes if Icom support grows. |
| Option message | `html/hs500.htm` | Gap | Needs source read before action; likely operator/admin message behavior. |
| Personal message folders | `html/hs580.htm` | Partial: `22-user-folders.md` | Cross-check WLE Personal/Global semantics vs Tuxlink local folders. |
| Updating channel data | `html/hs430.htm` | Partial/product gap | Tie to gateway/catalog refresh docs and gateway picker behavior. |

### Operation and message handling

| WLE help topic | Source | Tuxlink docs status | Gap/action |
|---|---|---|---|
| Main display | `page_10.html` | Partial: screenshots pending across guide | Use as migration comparison, not as design target. |
| Messages in Winlink Express, message types, connect behavior, why messages might not send | `page_5.html` | Partial | High-value. Tuxlink docs need message-type/routing explanations. Tracked by `tuxlink-v3aq`. |
| Composing a message | `html/hs80.htm` | Covered/partial: `19-composing.md` | Cross-check From/Send-as/To conventions; avoid promising unshipped message types. |
| Adding attachments | `html/hs82.htm` | Partial/product gap | Receive-side works; outbound work pending. Explain current state. |
| Editing/cropping/resizing images | `html/hs85.htm` | Product gap | Already tracked by `tuxlink-pxf`; docs should recommend low-bandwidth practice. |
| Templates and HTML forms | `page_6.html` | Partial/product gap: `20-html-forms.md`, `tuxlink-ytya` | Use to deepen form attachment/rendering docs when form UI ships. |
| ICS-309 communication log generation | `html/hs97.htm` | Product gap/docs gap | Emcomm-important; should be a planned feature/docs item after forms/mailbox stabilize. |
| Message addressing | `html/hs330.htm` | Gap/partial | Add addressing rules for callsigns, tactical addresses, internet email, and local-only modes. Tracked by `tuxlink-v3aq`. |
| Group addresses | `html/hs590.htm` | Partial/product shipped recently | Add docs for Tuxlink groups/distribution lists. Tracked by `tuxlink-ng91`. |
| Contacts and address book | `html/hs592.htm` | Gap/partial | Add a first-class contacts docs path. Tracked by `tuxlink-ng91`. |
| Spam control / acceptlist | `html/hs390.htm` | Gap | Explain Winlink ACCEPTLIST behavior and whether Tuxlink exposes helper flows. Tracked by `tuxlink-v3aq`. |
| Exporting, importing, archiving messages | `html/hs395.htm` | Partial/product gap | Tie to WLE migration (`tuxlink-px36`, `tuxlink-zmzx`) and local archive semantics. |
| GIS mapping forms and catalog items | `page_12.html` | Product gap/partial | Coordinate with map/forms/catalog work; don't overpromise. |
| Importing/exporting contacts | `html/hs400.htm` | Gap | Add migration docs once contacts file format/import support is clear. Tracked by `tuxlink-ng91`. |
| GRIB file requests | `html/hs420.htm` | Partial/product in progress: `23-catalog-requests.md`, `tuxlink-vrpk` | Use for request UX and external viewer expectations. |
| Telnet radio-only RMS Relay connection | `html/hs201.htm` | Partial/product in progress | Feed `33-operating-modes.md` and connection docs. Tracked by `tuxlink-1doe`. |
| Types of Winlink Express connections | `html/hs155.htm` | Partial: `33-operating-modes.md` | High-value migration explanation; map WLE mode names to Tuxlink labels. |
| Starting a session | `html/hs540.htm` | Partial | Add mode-specific "connect and read the result" walkthroughs. Tracked by `tuxlink-1doe`. |
| Telnet CMS connection | `html/hs200.htm` | Partial | See setup row. |
| Telnet P2P connection | `html/hs205.htm` | Partial/product in progress | Link to P2P feature docs once stable. |
| Packet RMS connection | `html/hs190.htm` | Partial | Needs concrete Tuxlink packet session flow. |
| Pactor RMS connection | `html/hs180.htm` | Intentional/product omit | Mention unsupported only in migration/parity docs. |
| Robust Packet RMS connection | `html/hs210.htm` | Intentional/product omit | Mention unsupported only in migration/parity docs. |
| Telnet Post Office connection | `html/hs202.htm` | Partial/product recently shipped | Add concrete operator docs after UX stabilizes. Tracked by `tuxlink-1doe`. |
| Iridium GO connection | `html/hs203.htm` | Intentional/product omit | Niche; document as unsupported if it appears in migration FAQ. |
| Peer-to-peer connection | `html/hs160.htm` | Partial/product in progress | Needs explicit "bypasses CMS" mental model. |
| HF Auto Connect | `page_9.html` | Product gap/partial | Coordinate with AutoConnect issues; document absence honestly. |
| Reviewing pending incoming messages before downloading | `html/hs220.htm` | Product shipped/tracked: `tuxlink-bsiy` | Ensure docs explain why operators may skip/hold large messages. |
| Usage statistics | `html/hs125.htm` | Gap/product gap | Nice diagnostic target; docs should not imply it exists. |
| Logs | `html/hs130.htm` | Partial | Tuxlink has live log panel; persistent logs/docs need detail. Tracked by `tuxlink-v3aq`. |
| Background monitoring tasks | `html/hs225.htm` | Product gap/docs gap | Explain not-yet-shipped auto/background behavior in migration docs. |
| Monitoring | `html/hs490.htm` | Product gap/partial | Decide whether passive-monitor concepts belong in Tuxlink docs. |
| Winlink glossary | `page_7.html` | Covered/partial: `30-glossary.md` | Cross-check missing acronyms after docs pass. |
| Troubleshooting | `html/hs140.htm` | Partial: `29-troubleshooting.md` | WLE's methodical "reduce unknowns" style is worth adapting. Tracked by `tuxlink-v3aq`. |
| Keyboard shortcuts and color scheme | `page_4.html` | Covered/partial: `28-keyboard.md`, `27-settings.md` | Keep current docs as source of truth for Tuxlink. |

## Recommended docs work sequence

1. `tuxlink-1doe`: make connection docs procedural. Operators need
   step-by-step session flows for shipped modes more than another mode taxonomy.
2. `tuxlink-v3aq`: deepen message-management and admin docs. This is where
   WLE's help teaches the most non-obvious operating behavior.
3. `tuxlink-ng91`: add contacts/groups/import-export docs now that Tuxlink has
   contact surfaces.
4. `tuxlink-zmzx`: once `tuxlink-px36` lands, write the WLE mailbox migration
   path with tested steps and "do not lose your old mail" warnings.
5. Re-run this audit after the above docs land and mark each row Covered,
   Partial, Gap, Product gap, or Intentional omit with fresh line references.
