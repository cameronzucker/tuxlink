# Research notes — docs-content-fill verification pass

> **Source pass:** 2026-06-04 · **Author:** gorge-ridge-bog · **bd:** tuxlink-obxz
> **PR:** https://github.com/cameronzucker/tuxlink/pull/351

This note captures the research that grounded the verification batches on the
docs knowledge base content fill (bd-tuxlink-obxz). It is the §5.2-style
research-note artifact for the work, written retrospectively after the
verification corrections landed. The spec envisioned per-section notes
written before content authoring; this note instead consolidates the
research that informed corrections across all sections.

## Methodology

Per the AI-amateur-radio-reliability memory: training-data claims about
amateur radio specifics (VARA internals, current hardware prevalence,
Part 97 nuance, Winlink protocol details) are structurally unreliable.
Ground claims in one of three tiers:

1. **Tuxlink's own implementation** — authoritative for any claim about
   what tuxlink does (paths, wire formats, default values).
2. **Upstream canonical source** — Hamlib's GitHub for rig model IDs,
   the project's design docs for grounded protocol decisions.
3. **Hamexandria** — operator-recorded YouTube content, for amateur-
   radio operating practice and community-tested conventions.

The web at large is the fallback when the above don't cover a fact, but
its content quality on amateur radio is mixed.

## Queries run + findings

### Hamlib model IDs and serial settings (topics 12, 13)

The previous draft had model numbers from training data — 5 of 6 were
wrong. Ground truth was the Hamlib supported-radios wiki + per-rig
backend source.

| Radio | Old (training) | Correct (Hamlib master) | Source |
|---|---|---|---|
| Xiegu G90 | 3081 | 3088 | rigs/icom/xiegu.c |
| Icom IC-7300 | 3061 | 3073 | rigs/icom/ic7300.c |
| Icom IC-705 | 3085 | 3085 (correct) | rigs/icom/ic7300.c (ic705_caps lives there) |
| Yaesu FT-991A | 1027 | 1035 (via FT-991 backend) | rigs/yaesu/ft991.c |
| Yaesu FT-818 | 1042 | 1041 | wiki |
| Kenwood TS-590S | 2014 | 2031 | rigs/kenwood/ts590.c |
| Kenwood TS-590SG | 2032 | 2037 | rigs/kenwood/ts590.c |

Serial baud min/max + data bits / parity / stop bits were also pulled
from the per-rig source files. The G90 backend pins both min and max at
19200 (fixed); the others accept a 4800–115200 range and rely on the
operator setting the radio's menu to a matching baud.

Caveat now in topic 13: model IDs can shift between Hamlib versions,
so `rigctl --list | grep <model>` is the verify-at-install step the
operator should always run.

### tuxlink internals — paths, AX.25, VARA, ARDOP (topics 02, 07, 14, 15, 16, 22, 32)

Tuxlink's own implementation files were the authoritative source for
several claims:

- **Wizard config path:** `~/.config/tuxlink/config.json` per
  `src-tauri/src/config.rs::resolve_config_path()`. NOT
  `~/.local/share/com.tuxlink.app/config.json` as the prior draft of
  topic 02 (and the retained file from PR #347) said.
- **Mailbox storage path:** `~/.local/share/com.tuxlink.app/native-mbox/`
  per `bootstrap.rs` + `ui_commands.rs` (which `app_data_dir().join("native-mbox")`).
  NOT `mailbox/` as the prior topic 07 said.
- **User folder registry:** `<mailbox-root>/.folders.json` per
  `user_folders.rs::REGISTRY_FILENAME`. Dot-prefixed sidecar inside the
  mailbox root.
- **AX.25 frame size:** tuxlink defaults to `paclen = 128`, not 256 as
  the prior draft claimed. From `ax25/params.rs::Ax25Params::default()`.
- **AX.25 window size:** `maxframe = 4`. Mod-8 numbering caps at 7.
- **ARDOP bandwidth options:** 200/500/1000/2000 Hz confirmed via
  `ardop/session.rs` (the `arq_bandwidth_hz` field's doc comment lists
  exactly those four).
- **VARA HF bandwidth modes:** Narrow (500 Hz, `BW500`), Standard
  (2300 Hz, `BW2300`), Tactical/Wide (2750 Hz, `BW2750`) per
  `vara/command.rs::Bandwidth` enum. The prior draft framed the tiers
  vaguely; the wire tokens are concrete.
- **B2F FS-answer codes:** Letter forms (Y/N/L), symbol forms (+/-/=),
  and an accept-at-offset variant (A/a/! + bytes) per
  `winlink/proposal.rs::Answer`. The prior draft listed only one shape
  per answer.

### Catalog request protocol (topic 23)

Largest correction. The prior draft hallucinated subject syntax
(`request RMS_LIST`) and recipient (`query@winlink.org`). Real shape
per `src-tauri/src/catalog/composer.rs`:

- Recipient: `INQUIRY@winlink.org`
- Subject: `REQUEST`
- Body: newline-joined catalog item filenames (e.g., `PUB_PACKET`,
  `PUB_VARA`, `WL2K_HELP`)

The composer was empirically verified against a real Winlink Express
outbox fixture per the project's `docs/design/2026-06-02-cms-request-
protocol-grounding.md`. The grounding doc also clarified that GRIB /
weather is NOT a Winlink catalog inquiry — it's a Saildocs request
(query@saildocs.com), a separate third-party service. The prior topic
draft conflated the two.

The RMS gateway list has two valid mechanisms in modern Winlink — HTTPS
REST at `api.winlink.org/gateway/status.json` (Pat-style, online) and
in-band catalog inquiry for the `PUB_PACKET` / `PUB_VARA` filenames
(RF-friendly, slower).

### Net check-in form names (topic 25)

The prior draft claimed ARES check-ins use ICS-213. Hamexandria's
OH8STN walkthrough — "Email over Ham Radio | Portable Winlink Net
Check-in", https://www.youtube.com/watch?v=DxPWgkTATYg — established
the canonical form is **Winlink Check-in** under Standard Forms →
General Forms. ICS-213 is the general-message form, not the
standardised check-in. Some ARES nets use ICS-213 as a check-in by
local convention, but the form catalog's standard check-in is the
distinct Winlink Check-in form. Topic 25 was rewritten accordingly.

SHARES form specifics — the prior draft claimed a "specific SHARES
catalog form" with strict validation. Hamexandria did not surface
authoritative SHARES form details. The topic was softened to:
registration required, nets publish their own protocols, same
Compose-Use form-Submit-Connect workflow.

### Maidenhead grid precision (topic 26, glossary)

The prior precision table claimed square resolutions
(`~100km × ~100km` for 4-character, etc.). Maidenhead grids are NOT
square — they encode degrees of latitude and longitude with different
factors. At mid-latitudes, a 4-character grid is ~110 km N-S and
~150-200 km E-W. Table reworked with separate N-S / E-W columns and
operator-oriented mental models (county, town, blocks).

### DigiRig USB vendor / product ID (topic 10)

UDEV rule example uses `ATTRS{idVendor}=="10c4", ATTRS{idProduct}=="ea60"`.
Verified via usb-ids.gowdy.us and devicehunt.com: those are the
Silicon Labs CP2102 USB-UART chip's IDs, which DigiRig uses. The rule
example is correct as written.

## What still wants verification (deferred to operator)

These claims are operator-veriable but not authoritatively confirmable
from documentation alone:

1. **G90 + DigiRig cable kit part number.** The DigiRig store sells a
   G90-specific cable kit; the topic refers to it generically rather
   than naming a part number. Adding the specific part number is a
   future polish pass.
2. **IC-7300 USB SEND-pin PTT menu name.** The IC-7300's data-mode menu
   has a setting that routes the SEND pin as PTT; the menu's exact label
   in the radio's firmware shifts between firmware revisions. Topic 13
   describes the feature without naming the menu path.
3. **SignaLink hardware-PTT jumper location.** The SignaLink's
   hardware-PTT mod uses a jumper inside the case. The exact jumper
   header label (JP3 / similar) is in the SignaLink documentation; the
   topic refers operators to the SignaLink documentation rather than
   asserting a label that may shift between SignaLink hardware
   revisions.
4. **TS-590 PKT mode menu navigation.** Same shape as IC-7300: the
   feature is real, the menu path moves between firmware revisions.

## Hamexandria queries that surfaced operator-context

The following Hamexandria searches surfaced useful framing without
generating direct corrections — recorded so the operator can review
the content informing future edits:

- "ARES Winlink check-in net format ICS-213" → OH8STN's portable
  net-check-in walkthrough; established the canonical Winlink Check-in
  form is the standard.
- "VARA HF Standard Tactical bandwidth tier license" → various;
  surfaced VARA-C alternative mode discussion but not authoritative
  tier-bandwidth specifics (tuxlink's code was authoritative instead).
- "B2F protocol Winlink frame" → operator-level Winlink discussions
  (Tech Minds PACTOR demo, ModernHam Packet setup); useful background,
  not protocol-detail-level.
- "winlink CMS RMS gateway" → operator framings; informed topic 04 +
  05 emphasis on RMS-as-volunteer infrastructure.

## Decisions logged

- **Where Hamexandria conflicts with code, code wins.** Documentation
  of behavior should match implementation; if implementation is
  wrong, that's a code bug, not a docs bug.
- **Where Hamexandria conflicts with WLE Express observed behavior,
  Express wins.** Tuxlink is implementing the same network protocol
  as Express; Express is the protocol's reference.
- **For ham-radio operating-practice claims with no clear authority,
  the topic should be soft / framing-only.** Topic 25's SHARES section
  is the canonical example: "registration required, follow the net's
  protocol" is correct without naming forms the project hasn't verified.

## Open questions

- VARA HF Tactical and Narrow modes are not separately confirmed by
  the project (only Standard is, per `project_g90_vara_standard_works_firsthand`).
  Topic 16 flags this; future operator smoke can close the gap.
- ARDOP throughput numbers are vague qualitative bands. A measurement
  campaign with calibrated SNR would produce real throughput tables.
  Out of scope for this PR.
- The Winlink Catalog Requests menu in WLE has a tree of named
  inquiries beyond `PUB_PACKET` and `PUB_VARA`. The full tree is in
  WLE's binary; topic 23 names the common ones and refers operators
  to the catalog index for the full list.
