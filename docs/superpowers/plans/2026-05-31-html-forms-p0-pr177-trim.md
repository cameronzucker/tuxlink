# HTML Forms — Phase 0: Trim PR #177 to ship valid v0.1

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trim PR #177 so the form picker only exposes forms whose compose UX is correct (ICS-213, Bulletin). Remove the three broken-as-fill-in forms (GPS Position, ICS-309, Damage Assessment) from the picker while preserving their wire-format machinery and their receive-side `View` components — received messages of those types must still render correctly.

**Architecture:** Make `Form` field optional on `FormRegistryEntry`; add `composableForms()` filter for the picker. Each broken form's `index.ts` registers only its `View`. Delete the broken `Form` component files + their tests (they will be rebuilt from scratch in P2 with correct auto-fill / auto-aggregation UX per the design doc, so retaining them adds no value and tempts future agents to use them as reference).

**Tech stack:** TypeScript / React / Vitest / Rust / Tauri (all existing).

**Branch:** `bd-tuxlink-v1p/html-forms-execution` — append commits to the existing PR #177. **All work happens in the v1p worktree at** `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution`.

**Design reference:** [`docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md`](../specs/2026-05-31-html-forms-full-parity-design.md) §6 "Phase 0".

---

## Task 1: Make `Form` optional on `FormRegistryEntry` + add `composableForms()` helper

**Files:**
- Modify: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/forms.ts`
- Modify: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/forms.test.ts`

The current registry forces every entry to carry a `Form` component. We need the picker to filter out forms that only register a `View` (P0's stripped-compose forms; future webview-only forms in P1+).

- [ ] **Step 1: Add a failing test for `composableForms()` to `src/forms/forms.test.ts`**

Append to `src/forms/forms.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import './ics213';
import './bulletin';
import './position';
import './ics309';
import './damage_assessment';
import { lookupForm, allForms, composableForms } from './forms';

// ... existing tests ...

describe('composableForms', () => {
  it('returns only forms with a Form component (picker scope)', () => {
    const composable = composableForms();
    const composableIds = composable.map((f) => f.id);
    expect(composableIds).toContain('ICS213_Initial');
    expect(composableIds).toContain('Bulletin_Initial');
    expect(composableIds).not.toContain('Position_Report');
    expect(composableIds).not.toContain('Form-309_Initial');
    expect(composableIds).not.toContain('Damage_Assessment_Initial');
  });

  it('still allows lookupForm to find view-only entries', () => {
    expect(lookupForm('Position_Report')).toBeDefined();
    expect(lookupForm('Form-309_Initial')).toBeDefined();
    expect(lookupForm('Damage_Assessment_Initial')).toBeDefined();
  });
});
```

Note: the existing first describe block already imports `'./ics213'`. The new describe relies on the broken-form imports too (added at the top of the file). Re-imports of the same module are no-ops at runtime — safe.

- [ ] **Step 2: Run test — verify it fails**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution
pnpm vitest run src/forms/forms.test.ts
```

Expected: FAIL with `composableForms is not exported from './forms'`.

- [ ] **Step 3: Make `Form` optional + add `composableForms()` to `src/forms/forms.ts`**

Edit `src/forms/forms.ts` — change the `FormRegistryEntry` interface to make `Form` optional, and add the new helper.

Replace this block:

```typescript
/** Registry entry for a single bundled form. */
export interface FormRegistryEntry {
  id: string;
  name: string;
  Form: ComponentType<FormComposeProps>;
  View: ComponentType<FormViewProps>;
}
```

With:

```typescript
/** Registry entry for a single bundled form.
 *
 * `Form` is optional: forms that ship only a receive-side renderer (no
 * compose-side authoring UX yet, e.g., forms pending the Phase 1 webview
 * path or the Phase 2 native auto-fill rebuild) register with `Form`
 * undefined. The picker filters via `composableForms()`. */
export interface FormRegistryEntry {
  id: string;
  name: string;
  Form?: ComponentType<FormComposeProps>;
  View: ComponentType<FormViewProps>;
}
```

Then append after the existing `allForms` function:

```typescript
/** Forms with a compose-side Form component — the picker scope. */
export function allForms(): FormRegistryEntry[] {
  return Array.from(REGISTRY.values());
}

export function composableForms(): Array<Required<Pick<FormRegistryEntry, 'id' | 'name' | 'Form'>> & FormRegistryEntry> {
  return Array.from(REGISTRY.values()).filter(
    (e): e is typeof e & { Form: NonNullable<typeof e.Form> } => e.Form !== undefined,
  );
}
```

Note: replace the existing `allForms` declaration with the version shown — the actual change is appending `composableForms`. Keep `allForms` as it is.

- [ ] **Step 4: Run test — verify it passes**

```bash
pnpm vitest run src/forms/forms.test.ts
```

Expected: all tests PASS (existing 2 + new 2).

- [ ] **Step 5: Run tsc to verify no other consumer broke**

```bash
pnpm exec tsc --noEmit
```

Expected: exit 0, no output.

- [ ] **Step 6: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution
git add src/forms/forms.ts src/forms/forms.test.ts
git commit -m "$(cat <<'EOF'
refactor(forms): make Form optional + add composableForms() helper (tuxlink-v1p)

Picker needs to filter to forms that have a compose-side Form
component. Forms that ship only the receive-side View (P0: Position,
ICS-309, Damage Assessment whose fill-in UX is being pulled per the
full-parity design; P1+: webview-only forms; P2+: forms pending native
auto-fill rebuild) now register with Form undefined, and the picker
queries composableForms() instead of allForms().

allForms() is preserved for any caller that wants the full registry
(e.g., lookupForm-by-id remains correct for receive-side dispatch).

Design ref: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §6 P0.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Replace `<YOUR-MONIKER>` with the executing agent's session moniker.

---

## Task 2: Wire `Compose.tsx`'s FormPicker call to `composableForms()`

**Files:**
- Modify: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/compose/Compose.tsx` (lines ~40, ~599-603)

- [ ] **Step 1: Update Compose.tsx imports**

Find this import on line ~40:

```typescript
import { FormPicker, lookupForm, allForms } from '../forms';
```

Replace with:

```typescript
import { FormPicker, lookupForm, composableForms } from '../forms';
```

- [ ] **Step 2: Update the FormPicker call**

Find this block around line 599:

```tsx
{formMode.kind === 'pick' && (
  <FormPicker
    forms={allForms().map((f) => ({ id: f.id, name: f.name }))}
    onPick={(id) => setFormMode({ kind: 'form', formId: id, values: {} })}
    onCancel={() => setFormMode({ kind: 'plain' })}
  />
)}
```

Change `allForms()` to `composableForms()`:

```tsx
{formMode.kind === 'pick' && (
  <FormPicker
    forms={composableForms().map((f) => ({ id: f.id, name: f.name }))}
    onPick={(id) => setFormMode({ kind: 'form', formId: id, values: {} })}
    onCancel={() => setFormMode({ kind: 'plain' })}
  />
)}
```

- [ ] **Step 3: Run vitest for compose + forms to verify no regression**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution
pnpm vitest run src/compose/ src/forms/
```

Expected: all tests PASS.

- [ ] **Step 4: Run tsc**

```bash
pnpm exec tsc --noEmit
```

Expected: exit 0, no output.

- [ ] **Step 5: Commit**

```bash
git add src/compose/Compose.tsx
git commit -m "$(cat <<'EOF'
refactor(compose): FormPicker now reads composableForms() (tuxlink-v1p)

The picker should only surface forms whose compose UX is shippable.
After Task 1's registry refactor, that's everything with a Form
component — which post-trim is ICS-213 + Bulletin only. Position,
ICS-309, Damage Assessment will stop appearing in the picker after
Task 3-5 strip their Form registration; their View components remain
for receive-side rendering.

Design ref: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §6 P0.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Strip `Form` registration from `position/index.ts`

**Files:**
- Modify: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/position/index.ts`

- [ ] **Step 1: Replace the file contents**

Current file:

```typescript
import { PositionForm } from './PositionForm';
import { PositionView } from './PositionView';
import { registerForm } from '../forms';

registerForm({
  id: 'Position_Report',
  name: 'GPS Position Report',
  Form: PositionForm,
  View: PositionView,
});
```

Replace with:

```typescript
import { PositionView } from './PositionView';
import { registerForm } from '../forms';

// Form field intentionally omitted — the v1 native React PositionForm was
// pulled because it had no GPS auto-pull from PositionArbiter (operator
// critique 2026-05-31). The native rebuild ships in Phase 2 of the
// full-parity design (docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md).
// Receive-side rendering via PositionView is preserved.
registerForm({
  id: 'Position_Report',
  name: 'GPS Position Report',
  View: PositionView,
});
```

- [ ] **Step 2: Run vitest for position**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution
pnpm vitest run src/forms/position/
```

Expected: `PositionView` tests PASS. The `PositionForm` tests will fail because the file still exists and imports may now be stale; Task 6 deletes them.

- [ ] **Step 3: Commit**

```bash
git add src/forms/position/index.ts
git commit -m "$(cat <<'EOF'
refactor(forms): strip Form registration from position/index.ts (tuxlink-v1p)

GPS Position Report should not appear in the picker until Phase 2 ships
its native rebuild with PositionArbiter auto-pull + map widget. View
stays for receive-side rendering of inbound position-report messages.

PositionForm.tsx + its test get deleted in Task 6 (they're orphaned
after this commit).

Design ref: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §6 P0, §4.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Strip `Form` registration from `ics309/index.ts`

**Files:**
- Modify: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/ics309/index.ts`

- [ ] **Step 1: Replace the file contents**

Current file:

```typescript
import { Ics309Form } from './Ics309Form';
import { Ics309View } from './Ics309View';
import { registerForm } from '../forms';

registerForm({
  id: 'Form-309_Initial',
  name: 'ICS-309 Communications Log',
  Form: Ics309Form,
  View: Ics309View,
});
```

Replace with:

```typescript
import { Ics309View } from './Ics309View';
import { registerForm } from '../forms';

// Form field intentionally omitted — the v1 native React Ics309Form was
// pulled because manually typing 30 log entries one-by-one is an emcomm
// error magnet; the form should aggregate from messages_meta over an
// operator-picked time range (operator critique 2026-05-31). Native
// rebuild ships in Phase 2 of the full-parity design
// (docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md).
// Receive-side rendering via Ics309View is preserved.
registerForm({
  id: 'Form-309_Initial',
  name: 'ICS-309 Communications Log',
  View: Ics309View,
});
```

- [ ] **Step 2: Run vitest for ics309**

```bash
pnpm vitest run src/forms/ics309/
```

Expected: `Ics309View` tests PASS.

- [ ] **Step 3: Commit**

```bash
git add src/forms/ics309/index.ts
git commit -m "$(cat <<'EOF'
refactor(forms): strip Form registration from ics309/index.ts (tuxlink-v1p)

ICS-309 should not appear in the picker until Phase 2 ships its native
rebuild with time-range picker + messages_meta aggregation + preview.
View stays for receive-side rendering.

Ics309Form.tsx + its test get deleted in Task 6.

Design ref: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §6 P0, §4.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Strip `Form` registration from `damage_assessment/index.ts`

**Files:**
- Modify: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/damage_assessment/index.ts`

- [ ] **Step 1: Replace the file contents**

Current file:

```typescript
import { DamageAssessmentForm } from './DamageAssessmentForm';
import { DamageAssessmentView } from './DamageAssessmentView';
import { registerForm } from '../forms';

registerForm({
  id: 'Damage_Assessment_Initial',
  name: 'Damage Assessment',
  Form: DamageAssessmentForm,
  View: DamageAssessmentView,
});
```

Replace with:

```typescript
import { DamageAssessmentView } from './DamageAssessmentView';
import { registerForm } from '../forms';

// Form field intentionally omitted — Damage Assessment moves to the
// Phase 1 webview-default rendering path (operator critique 2026-05-31).
// If we later add "active incident" state with metadata to pre-fill, the
// form may be elevated back to native in a future phase. Receive-side
// rendering via DamageAssessmentView is preserved.
registerForm({
  id: 'Damage_Assessment_Initial',
  name: 'Damage Assessment',
  View: DamageAssessmentView,
});
```

- [ ] **Step 2: Run vitest for damage_assessment**

```bash
pnpm vitest run src/forms/damage_assessment/
```

Expected: `DamageAssessmentView` tests PASS.

- [ ] **Step 3: Commit**

```bash
git add src/forms/damage_assessment/index.ts
git commit -m "$(cat <<'EOF'
refactor(forms): strip Form registration from damage_assessment/index.ts (tuxlink-v1p)

Damage Assessment moves to the Phase 1 webview-default rendering path
(no obvious tuxlink-side data layer to auto-fill from). View stays for
receive-side rendering.

DamageAssessmentForm.tsx + its test get deleted in Task 6.

Design ref: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §6 P0, §4.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Delete the broken Form components + their tests

**Files (delete):**
- `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/position/PositionForm.tsx`
- `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/position/PositionForm.test.tsx`
- `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/ics309/Ics309Form.tsx`
- `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/ics309/Ics309Form.test.tsx`
- `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/damage_assessment/DamageAssessmentForm.tsx`
- `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution/src/forms/damage_assessment/DamageAssessmentForm.test.tsx`

The Phase 2 rebuilds (Position, ICS-309) will be designed from scratch using gpsd / `messages_meta` integrations. The current React components have the wrong UX shape and would mislead a future agent into "iterating" them rather than rebuilding. Deletion now keeps the file tree honest. Git history preserves them if anyone needs reference.

- [ ] **Step 1: Verify no other file imports these components**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution
grep -rln "PositionForm\|Ics309Form\|DamageAssessmentForm" src/ --include="*.tsx" --include="*.ts" | grep -v "\.test\." | grep -vE "(PositionForm|Ics309Form|DamageAssessmentForm)\.tsx$"
```

Expected: no output (after Tasks 3-5 stripped the only imports from each `index.ts`).

If output appears, STOP — there's an unexpected consumer; investigate before deleting.

- [ ] **Step 2: Delete the 6 files**

```bash
rm src/forms/position/PositionForm.tsx
rm src/forms/position/PositionForm.test.tsx
rm src/forms/ics309/Ics309Form.tsx
rm src/forms/ics309/Ics309Form.test.tsx
rm src/forms/damage_assessment/DamageAssessmentForm.tsx
rm src/forms/damage_assessment/DamageAssessmentForm.test.tsx
```

- [ ] **Step 3: Run full vitest**

```bash
pnpm vitest run
```

Expected: all tests PASS. The test count drops by however many tests the 3 deleted `*Form.test.tsx` files had (each tested its respective form).

- [ ] **Step 4: Run tsc**

```bash
pnpm exec tsc --noEmit
```

Expected: exit 0, no output. (If imports of the deleted modules linger anywhere, tsc catches them here.)

- [ ] **Step 5: Commit the deletions**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore(forms): delete v1 native Form components for Position/ICS-309/Damage (tuxlink-v1p)

These were unregistered in Tasks 3-5 and have no remaining consumers.
The Phase 2 rebuilds (Position, ICS-309) are clean-room designs against
tuxlink-side data layers (PositionArbiter, messages_meta) that the v1
components didn't use; keeping them as "reference" would tempt future
agents to iterate them rather than rebuild correctly. Damage Assessment
moves to the Phase 1 webview-default path and doesn't get a native
rebuild at all. Git history preserves the v1 components.

Files deleted:
- src/forms/position/PositionForm.tsx + test
- src/forms/ics309/Ics309Form.tsx + test
- src/forms/damage_assessment/DamageAssessmentForm.tsx + test

Design ref: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §6 P0.

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Final verification — full suite green

No file changes — just verify the trim hasn't broken anything across the workspace.

- [ ] **Step 1: Full Rust workspace check**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: exit 0, "Finished `dev` profile…".

- [ ] **Step 2: Full Rust clippy (with `-D warnings`)**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings
```

Expected: exit 0, no warnings.

- [ ] **Step 3: Full Rust test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

Expected: all tests PASS (PR #177's 572 tests minus zero — backend wasn't touched).

- [ ] **Step 4: Full vitest**

```bash
pnpm vitest run
```

Expected: all tests PASS. Test count is PR #177's count minus however many tests were in the 3 deleted `*Form.test.tsx` files.

- [ ] **Step 5: Full tsc**

```bash
pnpm exec tsc --noEmit
```

Expected: exit 0, no output.

- [ ] **Step 6: Push (no separate commit; previous task commits include everything)**

```bash
git push
```

Expected: branch up to date with origin after the push.

---

## Task 8: Operator browser smoke (CANNOT be automated — `feedback_browser_smoke_before_ship`)

This task is **operator-driven**. The agent cannot certify visual correctness of the trimmed picker via jsdom/vitest (jsdom doesn't render CSS or interactivity; tested by manual operator click-through).

- [ ] **Step 1: Operator restarts `pnpm tauri dev`**

```bash
# Stop any currently-running tauri dev process bound to :1420
ss -tlnp 2>/dev/null | grep ':1420' | awk -F'pid=' '{print $2}' | awk -F',' '{print $1}' | xargs -r kill

# Launch fresh from the v1p worktree
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-execution
pnpm tauri dev
```

- [ ] **Step 2: Operator opens Compose → clicks "Compose form…"**

Expected: form picker modal appears centered, showing **only 2 forms**:
- ICS-213 General Message
- Bulletin

The previously-present **GPS Position Report**, **ICS-309 Communications Log**, and **Damage Assessment** should NOT appear.

- [ ] **Step 3: Operator verifies ICS-213 compose flow still works**

Pick ICS-213 → fill required fields (Subject, To, From, Date, Time, Message, Approved By, Position) → click Send. Expected: message posts to Outbox; no compose-time errors.

- [ ] **Step 4: Operator verifies Bulletin compose flow still works**

Same flow with Bulletin. Expected: posts to Outbox successfully.

- [ ] **Step 5: Operator spot-checks receive-side rendering of a removed-form type**

Use the existing DEV-3 fixture (`src/mailbox/devFixture.ts`) which includes an inbound Position message:
- Open Inbox → click the position fixture message
- Expected: the **PositionView** renders correctly (label-value pairs, map link or coordinates display)

Repeat for an ICS-309 fixture if available, or just confirm via code-reading that `lookupForm('Form-309_Initial')` still resolves to `{ View: Ics309View }`.

- [ ] **Step 6: Confirm + report back**

Operator reports either:
- "All 4 verifications passed → P0 ready for #177 to merge" → move to Task 9.
- "Verification N failed because X" → file a follow-up bd issue + halt P0 sign-off.

---

## Task 9: Update PR #177 description (operator-driven)

PR #177 currently describes the v0.1 design before the trim. After Tasks 1-8 land, update the PR body to reflect what actually ships.

- [ ] **Step 1: Operator edits PR #177 description**

```bash
gh pr edit 177 --body "$(cat <<'EOF'
## Summary

HTML Forms v0.1 (trimmed for design pivot — see PR #186 for the full-parity design).

**Ships in this PR:**
- Wire-format machinery: `forms::parse`, `serialize`, `types`, `catalog`, `validation`
- `OutboundMessage::attachments` + `compose_message_with_files` (native B2F send with attachments — ADR 0016 carry-over)
- Native React compose UX for **ICS-213 General Message** and **Bulletin** only
- Receive-side `View` components for all 5 form types (ICS-213, Bulletin, GPS Position Report, ICS-309, Damage Assessment — received messages render correctly even when the compose path isn't bundled)
- FormPicker modal with keyboard navigation + ARIA + tuxlink-themed CSS
- Per-form CSS styling (`forms.css`)
- Compose window: ResizeHandles wired up, Min/Max titlebar buttons, default size bumped to 1100x820, lazy-server-relevant capabilities granted
- 3 follow-up commits + 6 commits' worth of design-pivot trim work (P0 of the full-parity roadmap)

**Pulled from this PR (per operator critique 2026-05-31):**
- GPS Position Report compose form (no GPS auto-pull → wrong UX; native rebuild in P2)
- ICS-309 Comms Log compose form (manual log entry → emcomm error magnet → native auto-aggregator in P2)
- Damage Assessment compose form (moves to webview-default in P1)

## Design context

PR #186 captures the full-parity design and 4-phase roadmap that this PR is now the P0 of. After this merges, P1 implementation plan + work begins.

## Test plan

- [ ] Operator browser smoke per `docs/superpowers/plans/2026-05-31-html-forms-p0-pr177-trim.md` Task 8
- [ ] Full Rust + TS test suite green (verified in Task 7)
- [ ] Picker shows only ICS-213 + Bulletin
- [ ] Inbound receive-side rendering works for all 5 form types

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Optionally rename PR #177**

If the PR title is "HTML Forms v0.1" or similar, consider renaming to "HTML Forms v0.1 (trimmed)" or "HTML Forms wire-format + ICS-213 + Bulletin (P0 of full-parity)" so the scope shift is visible at-a-glance in the PR list.

```bash
gh pr edit 177 --title "HTML Forms wire-format + ICS-213 + Bulletin (P0 of full-parity, tuxlink-v1p)"
```

- [ ] **Step 3: Merge PR #177 (operator decision)**

Once Task 8 verifications pass and PR #177's review is satisfied:

```bash
gh pr merge 177 --merge --delete-branch
```

This unblocks Phase 1 work (which will rebase off `main` to pick up the trimmed v1p baseline).

---

## Out of scope for P0 (explicitly)

To avoid scope creep — these are P1+ work even though they may be tempting to fold in:

- **Catalog browser UI** (replacing flat FormPicker with hierarchical tree) → P1
- **Embedded webview infrastructure** → P1
- **Bundled WLE Standard Forms snapshot** → P1
- **Native auto-fill rebuilds** of Position / ICS-309 / Check-In → P2
- **Custom-forms directory** → P1
- **winlink.org auto-update** → P3
- **Tauri capability changes** beyond what already shipped in this session's commits → P1 introduces `forms-webview.json`

If anything in this list seems essential for P0, raise it for re-scoping — don't quietly add it.

---

## Self-review (run before handing off to execution)

**Spec coverage** (P0-only — full design spec is bigger; this plan covers only §6 Phase 0):

| Design §6 P0 requirement | Plan task |
|---|---|
| Keep wire-format machinery | No-op (preserved by not touching `forms::parse/serialize/types/catalog/validation`) |
| Keep ICS-213 + Bulletin native | No-op (preserved by not stripping their `index.ts`) |
| Keep all View components | No-op (Views remain in registry via Tasks 3-5; deleted files in Task 6 are only Forms) |
| Pull Position / ICS-309 / Damage from picker | Tasks 1, 2, 3, 4, 5 |
| Compose window controls + capabilities + CSS | Already shipped this session (commits 4451d27, 415b7c2, a33a2fd, 71c03f4, 7ad80dd) |

**Placeholder scan**: `<YOUR-MONIKER>` is intentional (executing agent fills in); not a planning placeholder. No TBDs, no "implement appropriate X" prose, no "similar to Task N" hand-waves. All code blocks complete.

**Type consistency**: `composableForms()` returns `FormRegistryEntry[]` with `Form: NonNullable<...>` guaranteed by the type guard. `FormPicker`'s `forms` prop accepts `{ id, name }[]` (unchanged). No drift.

**Scope check**: This plan is a single phase (P0); P1/P2/P3 each get their own plan written later. Self-contained.

---

## Plan complete

Saved to `docs/superpowers/plans/2026-05-31-html-forms-p0-pr177-trim.md`.

**Execution choice (operator decides):**

1. **Subagent-driven (recommended)**: Dispatch a fresh subagent per task using `superpowers:subagent-driven-development`. Two-stage review between tasks. Fast iteration; protects main session context.

2. **Inline execution**: Execute tasks in the current session using `superpowers:executing-plans`. Batch with checkpoints. Best fit for plans this small (9 tasks, ~30 min total execution).

P0 is small enough that either works; subagent-driven is the safer call only if running this from a context-loaded session where the main-session memory matters.
