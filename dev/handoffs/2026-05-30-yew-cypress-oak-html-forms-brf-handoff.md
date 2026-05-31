# Handoff — 2026-05-30 — yew-cypress-oak — HTML Forms v0.1 BRF pipeline complete; ready for subagent-driven execution

> Date: 2026-05-30 · Agent: yew-cypress-oak · Machine: pandora · Operator-confirmed scope: Full Phase 0-11, subagent-driven execution

## 0. TL;DR

Operator authorized autonomous work today while driving. This session ran the **full `build-robust-features` (BRF) pipeline** for the HTML Forms v0.1 capability (the operator-prioritized §13.1 must-have from the merged inventory rev-2 / PR #150):

1. **Brainstorming** → design spec (PR #151)
2. **5-round adversarial review** (4 Claude rounds R1-R4 + 1 Codex round R5) → 55 findings, 8+ P0
3. **Spec rev-2** addressing all P0/P1 findings
4. **Writing-plans** → implementation plan
5. **4-round plan review** (R1+R2+R3 design + R4 verification) → caught 6 P0 plan defects, including rev-2's own fabrications
6. **Plan rev-3** addressing all P0s
7. **Operator decision**: full Phase 0-11, subagent-driven

**Execution did not start in this session — context budget insufficient for ~120 subagent dispatches (40 implementer tasks × 3 subagents each: implementer + spec reviewer + code quality reviewer).** The next session picks up at the `superpowers:subagent-driven-development` skill's "Read plan, dispatch first implementer subagent" step.

main HEAD now: `2e7309d` (PR #149 merge — inventory rev-1; rev-2 / PR #150 also merged at `cf1cc72`+ later commits not pulled into this worktree).
PR #151 branch HEAD: `afa2752` (plan rev-3).

---

## 1. PRs landed / open

| PR | State | Title | Notes |
|---|---|---|---|
| #150 | MERGED 2026-05-30 15:58Z | inventory rev-2 (capability-focused) | Operator's redirect from PR #149's menu-enumeration shape |
| #152 | OPEN (correction) | inventory rev-2.0.1 — Reply/Forward shipped-state correction | Tiny doc fix; awaits operator merge |
| **#151** | **OPEN — primary deliverable** | **HTML Forms v0.1 — design spec rev-2 + impl plan rev-3** | **Operator approved scope (full Phase 0-11, subagent-driven); execution NOT yet started** |

---

## 2. PR #151 contents

Branch: `bd-tuxlink-v1p/html-forms-design` (off main; 3 commits on top — design, design-rev2, plan-rev3).

| File | Purpose | Lines |
|---|---|---|
| `docs/superpowers/specs/2026-05-30-html-forms-design.md` | Design spec rev-2 (post-5-round adrev). 17 sections incl. wire format, architecture, security hardening, testing strategy, open operator questions. | 460+ |
| `docs/superpowers/plans/2026-05-30-html-forms-v0.1-plan.md` | Implementation plan rev-3 (post-4-round review). 40+ tasks across 11 phases. Each task TDD-disciplined per BRF requirements. | 2800+ |

**Spec key decisions** (all operator-reviewable on PR #151):

- **Approach**: native React forms over canonical schema, generate WLE+Pat-compatible wire (XML attachment + plain text body). Defer WLE-HTML-webview compat to v0.5+.
- **Bundled forms**: ICS-213, ICS-309, GPS Position Report, Bulletin, Damage Assessment.
- **Wire format**: all `<variables>` lowercase, full `<form_parameters>` (7 elements in WLE order), attachment named `RMS_Express_Form_<DisplayFormBasename>.xml`, UTF-8 BOM, body charset ISO-8859-1.
- **Backend precursor**: extend `OutboundMessage` with `attachments: Vec<OutboundAttachment>` (breaking change, acknowledged at code-comment level).
- **Reply-to-form default**: plain-text reply with placeholder (operator-confirmable; alt is auto-open same form).
- **Security hardening (non-optional)**: `quick-xml 0.39.x` with `Event::DocType` rejection; nesting depth cap 8; event count cap 10k; `MAX_FORM_XML_BYTES = 256 KiB`; `form_id` regex `^[A-Za-z0-9_-]{1,64}$`; `dangerouslySetInnerHTML` Vitest-enforced ban.

**Plan structure** (40+ tasks across 11 phases):

- Phase 0 — Backend precursor: `OutboundAttachment` + `OutboundMessage.attachments` + `pat_client::send_with_attachments` + caller updates (T0.1-0.3)
- Phase 1 — Forms backend: types, validation, parse (hardened), serialize, catalog with ICS-213 (T1.1-1.8)
- Phase 2 — Detection bug fix + DTO extension (T2.1-2.3)
- Phase 3 — `send_form` Tauri command (T3.1)
- Phase 4 — React forms infrastructure (T4.1-4.4)
- Phase 5 — ICS-213 React Form + View (T5.1-5.3)
- Phase 6 — Compose integration (T6.1-6.4)
- Phase 7 — MessageView integration (T7.1)
- Phase 8 — replyActions update (T8.1)
- Phase 9 — Additional bundled forms — SERIAL, not parallel (T9.1 ICS-309, T9.2 Position, T9.3 Bulletin, T9.4 DamageAssessment)
- Phase 10 — Hardening cross-cuts (dangerouslySetInnerHTML ban, attachment-filename sanitize)
- Phase 11 — Codex round + 4 operator-driven live smokes (tuxlink↔WLE+Pat round-trip)

Effort estimate: **12-18 days** for the full plan (originally 4-6 — BRF revisions tripled the realistic budget).

---

## 3. Critical pitfalls the next session MUST avoid

These are non-obvious traps caught by the BRF reviews; ignoring them re-introduces shipped-broken behavior.

### 3.1 Wire-format gotchas (caught by R1+R2+R5)

- **`<display_form>` is REQUIRED** by Pat's parser — without it, Pat returns HTTP 400 ("missing display_form tag"). Spec §3 shows the full 7-element `<form_parameters>` block in WLE order.
- **All XML element names lowercase** — WLE's serializer lowercases via `Template.cs:775 .ToLower()`. Mixed-case in our output breaks Pat (case-sensitive Go `xml.Unmarshal`) and WLE's form-data aggregator.
- **Body must be ISO-8859-1**, NOT UTF-8 — WLE `Message.cs:295-298` down-codes the body to Latin-1 before transmission. UTF-8 in body → mojibake for any Cyrillic / CJK content. XML attachment CAN be UTF-8.
- **Attachment filename uses display-form basename** — `RMS_Express_Form_ICS213_Initial.xml`, NOT `RMS_Express_Form_ICS213.xml`. Matches WLE+Pat convention.

### 3.2 Backend precursor gotchas (caught by R1+R2+R3+R4)

- **TWO structs named `OutboundMessage` exist** in the codebase: `winlink_backend::OutboundMessage` (the one we modify) AND `winlink::session::OutboundMessage` (different struct, B2F session layer). Mechanical `grep "OutboundMessage {"` returns 6 hits across both. Plan T0.2 has the correct grep filter that returns exactly the 3 in-scope callsites.
- **`pat_client::send` signature is positional** — `send(&self, to: &[&str], subject, body, date)` — NOT a method that takes `&OutboundMessage`. Multipart/form-data is already the default. Plan T0.3 adds a NEW method `send_with_attachments` (parallel to send) rather than re-shaping the existing signature.
- **`PatClientError` variants are `Http(reqwest::Error)`, `Status(u16)`, `TooLarge { cap: usize }`** — do NOT invent a `PatClientError::Send(String)` variant (early plan revisions did; rev-3 corrected). If error mapping needs a new shape, extend `PatClientError` as a separate commit.
- **`BackendError::TransportFailed { reason }`** — NOT `BackendError::Transport`. (Easy typo to mis-read; the existing call-site uses the correct variant; copy it.)

### 3.3 UX gotchas (caught by R3)

- **`DraftData` autosave schema has NO formId/formFields today** — extending it is part of T6.3. Without it, an app crash mid-form-fill loses N minutes of typing.
- **Pre-form-switch dialog** — switching from plain-text compose to a form mid-edit MUST trigger Save/Discard/Cancel dialog (T6.2). Reuse the existing `isDirty()` pattern from Compose.tsx.
- **Reply-to-form default decided**: plain-text reply with placeholder. The alternative "Reply with form" is a separate button (T8.1).

### 3.4 Security gotchas (caught by R4 + R5 carry-payload)

- **`ParsedMessageDto` must include BOTH `form_id: Option<String>` AND `form_payload: Option<FormPayload>`** — Codex R5 caught that only adding `form_id` would leave the frontend with no field values to render. Parse eagerly while attachment bytes in hand.
- **`form_id` regex validate at extraction** — `^[A-Za-z0-9_-]{1,64}$`. Without this, `RMS_Express_Form_../../etc/passwd.xml` becomes a usable form_id string (latent attack surface for v0.5+ catalog cache + attachment save features).
- **XML parser hardening** — `quick-xml 0.39.x`, reject `Event::DocType`, nesting cap 8, event cap 10k, payload size cap 256 KiB. Test corpus includes billion-laughs + deep-nesting + field-count bombs.

### 3.5 Plan-execution gotchas (caught by R1-R4)

- **Phase 9 is SERIAL, not parallel** — T9.1-9.4 each edit 3 shared registration files (`forms/catalog.rs::BUNDLED_FORMS`, `forms/templates/mod.rs`, `src/forms/index.ts`). Two parallel subagents = merge conflict.
- **`dev/scratch/` is gitignored AND physically absent from worktrees** — the WLE reference files live at `/home/administrator/Code/tuxlink/dev/scratch/...` (absolute paths in main checkout). Plan T1.7 + T9.x use absolute paths for reads.
- **First compile failure during execution = STOP and escalate to main session**. Do NOT fabricate workarounds. The BRF process is asymptotic; rev-3 may still have P2 polish issues — the first execution-time failure is the real-world gate.

---

## 4. Next session's starting instructions

The plan is a complete specification. The execution model is `superpowers:subagent-driven-development`:

1. Read the plan ONCE — `docs/superpowers/plans/2026-05-30-html-forms-v0.1-plan.md`.
2. Read the spec — `docs/superpowers/specs/2026-05-30-html-forms-design.md` — for normative answers when plan tasks point to "per spec §X".
3. Create a TodoWrite list with all 40+ tasks.
4. Per the skill's prescription: dispatch one implementer subagent per task → spec compliance reviewer → code quality reviewer → mark task complete → next task.
5. Two-stage review per task is non-optional per BRF; don't shortcut.
6. **Continuous execution** per the skill's "do not pause between tasks." Only stop for: BLOCKED status I can't resolve, ambiguity that genuinely prevents progress, or all tasks complete.

The next session works in the EXISTING `worktrees/bd-tuxlink-v1p-html-forms-design/` worktree (do NOT create a new one). All commits land on `bd-tuxlink-v1p/html-forms-design` branch (which is PR #151's branch — commits will append to that PR).

When all 40+ tasks land + Phase 11 live smokes pass: dispatch final code-reviewer per skill prescription, then use `superpowers:finishing-a-development-branch` to land PR #151.

---

## 5. Worktree state at session end

This session created these worktrees:

| Worktree | bd issue | PR | State |
|---|---|---|---|
| `bd-tuxlink-95z-winlink-express-inventory` | tuxlink-95z | #149 (merged), #150 (merged), #152 (open) | Inventory work complete; worktree DISPOSED earlier in session per ADR 0009 |
| `bd-tuxlink-pz7-readme-badges` | tuxlink-pz7 | #148 (merged) | README badges; worktree DISPOSED earlier |
| `bd-tuxlink-1k7-script-default-main` | tuxlink-1k7 | #142 (merged) | Script default fix; worktree DISPOSED earlier |
| `bd-tuxlink-r3a-oxi-extract` | tuxlink-r3a | #141 (merged) | Oxi extract; worktree DISPOSED earlier |
| `bd-tuxlink-v1p-html-forms-design` | tuxlink-v1p | #151 (open — primary) | **LIVE — next session executes here** |

The bd-tuxlink-v1p worktree's untracked content at session end: only the 5 adversarial transcripts under `dev/adversarial/` (gitignored). No `target/` build artifacts (no code compiled this session).

---

## 6. Adversarial transcripts (gitignored, local-only)

Per CLAUDE.md, raw transcripts stay local. Summarized findings already incorporated into spec rev-2 + plan rev-3. Files at:

- `dev/adversarial/2026-05-30-html-forms-design-claude-r1-wire-format.md` (12 findings, 2 P0)
- `dev/adversarial/2026-05-30-html-forms-design-claude-r2-interop.md` (12 findings, 4 P0)
- `dev/adversarial/2026-05-30-html-forms-design-claude-r3-ux.md` (12 findings, 3 P0)
- `dev/adversarial/2026-05-30-html-forms-design-claude-r4-security.md` (12 findings, 3 P0)
- `dev/adversarial/2026-05-30-html-forms-design-codex-r5.md` (7 findings, 1 net-new P1)
- `dev/adversarial/2026-05-30-html-forms-plan-claude-r1-ambiguity.md`
- `dev/adversarial/2026-05-30-html-forms-plan-claude-r2-completeness.md`
- `dev/adversarial/2026-05-30-html-forms-plan-claude-r3-pitfalls.md`
- `dev/adversarial/2026-05-30-html-forms-plan-claude-r4-verify.md`

If next session needs to re-check a finding, transcripts are on this machine's disk only.

---

## 7. Other open work mentioned this session (not blocking forms)

- `tuxlink-wqv` (P3) — CLAUDE.md codex review syntax stale (`--commit`/`--uncommitted` no longer accept `[PROMPT]`). Docs-only fix.
- BT page-timeout (`tuxlink-9ky`, P1) — still gates on-Pi radio work, unchanged from prior session.
- ARDOP MVP on-air bring-up — unchanged from `marten-finch-gorge` handoff; depends on tuxlink-9ky.

Agent: yew-cypress-oak
