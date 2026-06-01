# Radio Panel Width + ARDOP Controls Relocation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Widen the radio-panel chrome from 360 px to 400 px across ALL mode panels (Telnet, Packet, ARDOP, Placeholder, future VARA) AND move every ARDOP control that lives in `SettingsPanel.tsx`'s "GPS and Privacy" fieldset into the ARDOP radio panel's Radio section, then delete the Settings ARDOP fieldset entirely.

**Architecture:** Track B is plumbing-class per `feedback_discipline_triage_rule` — the bd issues serve as the spec. Operator confirmed the design 2026-06-01: *"400 px, but not JUST ARDOP. We ran into this issue before. The Radio control pane is for ALL modes, so any changes need to be propagated across all of them to create a cohesive and professional UI."* + *"The ARDOP controls are hidden in the GPS + privacy sub-menu which is even more things we need on the ARDOP radio control dock. That has to be high priority fixed."*

**Tech Stack:** TypeScript + React + vitest. CSS in `src/radio/RadioPanel.css` + `src/radio/modes/ArdopRadioPanel.css` + `src/shell/AppShell.css`. No backend (Rust) changes needed; the ARDOP config types (`ArdopUiConfig`) already exist in `useArdopConfig`.

**bd issues:** `tuxlink-jmfm` (controls relocate; closes) + `tuxlink-8rng` (panel widening; closes).

**Branch + worktree:** `bd-tuxlink-jmfm/radio-panel-400px-controls-relocate` lives at `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-jmfm-radio-panel-400px-controls-relocate/`. All work happens in that worktree.

---

## File Structure

**T1 — Widen the radio-panel chrome (8rng):**
- Modify: `src/shell/AppShell.css` — grid-template-columns in `.layout-b .panes--with-dock` + `.layout-b .panes--with-dock.panes--with-legacy-dock` from `360px` → `400px`.
- Audit: `src/radio/RadioPanel.css`, `src/radio/modes/ArdopRadioPanel.css`, `src/radio/modes/PacketRadioPanel.css`, `src/radio/modes/TelnetRadioPanel.css`, `src/radio/modes/PlaceholderRadioPanel.css` — any hardcoded `336px` (panel content width = 360 − 24 padding), `360px`, or related width constraints. Update to `376px` content width or `400px` panel width as appropriate.
- Tests (existing): vitest renders confirm no overflow regression.

**T2 — Delete the Settings ARDOP fieldset (jmfm):**
- Modify: `src/shell/SettingsPanel.tsx` — DELETE the `<fieldset className="tux-settings-group"><legend>ARDOP HF</legend>...` block, the `ArdopUiConfig` interface mirror, `ARDOP_DEFAULT`, `ardop` + `setArdop` useState, `persistArdop`, the `BANDWIDTH_OPTIONS` constant if unique to Settings.
- Modify: `src/shell/SettingsPanel.test.tsx` — DELETE every ARDOP-related test in `SettingsPanel.test.tsx`.
- Keep: the `GpsState` controls + `PositionPrecision` controls intact.

**T3 — Add `cmd_port` + `ardopcf binary` controls to the ARDOP panel Radio section (jmfm):**
- Modify: `src/radio/modes/ArdopRadioPanel.tsx` — add two `<label className="radio-panel-input-row">` rows for `cmd_port` (numeric) and `ardopcf binary` (text) inside the existing `<section className="radio-panel-sec" data-testid="ardop-radio-section">` (added by PR #185 commit `4c88618`).
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx` — add tests asserting the new rows render and call `setArdop` with the typed value on blur.

---

## Pre-task setup

- [ ] **Setup Step 0: Verify worktree.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-jmfm-radio-panel-400px-controls-relocate
git status
pnpm install --prefer-offline 2>&1 | tail -3
pnpm vitest run 2>&1 | tail -3
```

Expected: clean working tree on branch `bd-tuxlink-jmfm/radio-panel-400px-controls-relocate`; tests green at baseline.

---

## Task 1: Widen the radio-panel chrome from 360 → 400 px (tuxlink-8rng)

**Files:**
- Modify: `src/shell/AppShell.css`
- Audit + modify as needed: `src/radio/RadioPanel.css`, `src/radio/modes/ArdopRadioPanel.css`

**Context:** The radio-panel chrome is 360 px wide per `.layout-b .panes--with-dock { grid-template-columns: 200px 340px 1fr 360px; }`. Operator 2026-06-01 surfaced clipping in the ARDOP panel that prior CSS clamps (commit `cc82bf4`) only partially fixed. Operator-approved fix: widen to 400 px; the mailbox column absorbs the 40 px reduction (340 px → 300 px) so the 1fr reading-pane keeps its share.

- [x] **Step 1: Write a failing snapshot/dimension test.**

In `src/shell/AppShell.test.tsx` (or wherever the layout grid is tested), ADD:

```typescript
test('radio-panel column is 400px in panes--with-dock (tuxlink-8rng)', () => {
  // Mount AppShell with a mode-selected state so the radio panel is visible.
  // Read the computed grid-template-columns of `.panes--with-dock`.
  // Assert the last column equals "400px".
  const panes = document.querySelector('.panes--with-dock');
  expect(panes).toBeInTheDocument();
  const computed = window.getComputedStyle(panes as Element);
  expect(computed.gridTemplateColumns).toMatch(/400px$/);
});
```

(If the existing AppShell tests don't render with `.panes--with-dock`, mock a state where the radio panel is open or use the existing pattern from prior radio-panel tests in `src/shell/AppShell.radioPanel.test.tsx`.)

- [x] **Step 2: Run the test to verify it fails.**

```bash
pnpm vitest run src/shell/AppShell.test.tsx -t "radio-panel column is 400px" 2>&1 | tail -10
```

Expected: FAIL (column is currently 360 px).

- [x] **Step 3: Update the grid-template-columns.**

In `src/shell/AppShell.css`, find:

```css
.layout-b .panes--with-dock {
  grid-template-columns: 200px 340px 1fr 360px;
}
.layout-b .panes--with-dock.panes--with-legacy-dock {
  grid-template-columns: 200px 340px 1fr 360px 290px;
}
```

REPLACE with:

```css
.layout-b .panes--with-dock {
  grid-template-columns: 200px 300px 1fr 400px;
}
.layout-b .panes--with-dock.panes--with-legacy-dock {
  grid-template-columns: 200px 300px 1fr 400px 290px;
}
```

Update the surrounding comment (around line 53–65 of `AppShell.css`) to reflect 400 px instead of 360 px AND 300 px mailbox instead of 340 px.

In `src/radio/RadioPanel.css`, audit for hardcoded width references:

```bash
grep -nE '336|360' src/radio/RadioPanel.css src/radio/modes/*.css
```

For each match, update from `336px` → `376px` (content width = 400 − 24 padding) OR `360px` → `400px` as appropriate. Update any inline doc comments.

In `src/radio/modes/ArdopRadioPanel.css`, the prior `cc82bf4` clamps (`.ardop-arq-grid`, `.ardop-stats`, `.ardop-meter-v`) should naturally fit the wider panel — but audit for any explicit `max-width: 336px` or similar that needs updating.

- [x] **Step 4: Run the test to verify it passes.**

```bash
pnpm vitest run src/shell/AppShell.test.tsx -t "radio-panel column is 400px" 2>&1 | tail -10
```

Expected: PASS.

- [x] **Step 5: Run the full vitest suite.**

```bash
pnpm vitest run 2>&1 | tail -5
```

Expected: all passing.

- [x] **Step 6: Commit.**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-jmfm-radio-panel-400px-controls-relocate add src/shell/AppShell.css src/shell/AppShell.test.tsx src/radio/RadioPanel.css src/radio/modes/ArdopRadioPanel.css
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-jmfm-radio-panel-400px-controls-relocate commit -m "fix(shell): widen radio-panel chrome 360 → 400 px across all modes (tuxlink-8rng)

Operator 2026-06-01: 'None of the ARDOP elements are sized correctly.
They're clipping into each other now and off the right side. Does this
mean we need a 400 px radio control pane? That's on the table. 360 px
was an arbitrary decision.' + 'The Radio control pane is for ALL modes,
so any changes need to be propagated across all of them.'

The 360 → 400 px change cascades from .panes--with-dock + .panes--with-
dock.panes--with-legacy-dock in AppShell.css. The mailbox column absorbs
the 40 px reduction (340 → 300 px) so the 1fr reading-pane keeps its
share.

Closes tuxlink-8rng.

bd issue: tuxlink-8rng
Plan: docs/superpowers/plans/2026-06-01-radio-panel-width-and-ardop-controls-relocate-plan.md (Task 1)

Agent: bison-condor-grouse
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Delete the Settings ARDOP fieldset (tuxlink-jmfm)

**Files:**
- Modify: `src/shell/SettingsPanel.tsx`
- Modify: `src/shell/SettingsPanel.test.tsx`

**Context:** Operator 2026-06-01: *"The ARDOP controls are hidden in the GPS + privacy sub-menu which is even more things we need on the ARDOP radio control dock. That has to be high priority fixed."* The Settings panel currently contains an ARDOP fieldset with `binary`, `capture_device`, `playback_device`, `ptt_serial_path`, `cmd_port`, `bandwidth_hz` — operator option (a): delete the fieldset entirely. Task 3 ensures every control has an inline-edit equivalent in the ARDOP panel's Radio section.

- [ ] **Step 1: Write the failing assertion.**

In `src/shell/SettingsPanel.test.tsx`, ADD:

```typescript
test('SettingsPanel does NOT render the ARDOP HF fieldset (tuxlink-jmfm)', () => {
  render(<SettingsPanel onClose={vi.fn()} />);
  expect(screen.queryByText(/ARDOP HF/i)).not.toBeInTheDocument();
  expect(screen.queryByLabelText(/ardopcf binary/i)).not.toBeInTheDocument();
  expect(screen.queryByLabelText(/Capture device/i)).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run the test to verify it fails.**

```bash
pnpm vitest run src/shell/SettingsPanel.test.tsx -t "does NOT render the ARDOP HF fieldset" 2>&1 | tail -10
```

Expected: FAIL (the fieldset currently renders).

- [ ] **Step 3: Delete the ARDOP fieldset + supporting code from `SettingsPanel.tsx`.**

In `src/shell/SettingsPanel.tsx`:

1. DELETE the `<fieldset className="tux-settings-group"><legend>ARDOP HF</legend>` block and everything inside it (the binary, capture_device, playback_device, ptt_serial_path, cmd_port, bandwidth_hz controls).
2. DELETE the `ArdopUiConfig` interface mirror (the local interface around lines 57–75 of `SettingsPanel.tsx`).
3. DELETE the `ARDOP_DEFAULT` constant.
4. DELETE the `BANDWIDTH_OPTIONS` constant if it's not used elsewhere.
5. DELETE the `ardop` + `setArdop` useState hook.
6. DELETE the `persistArdop` function.
7. DELETE any related `useEffect` that loads the ARDOP config via `invoke('config_read_ardop')` or similar.
8. REMOVE the unused imports if any (e.g. if `useState` is no longer used, but keep it if other hooks still use it).
9. UPDATE the file's top header comment (lines 1–13) — drop the "GPS privacy controls" framing if it now reads as inaccurate; the panel is purely the GPS-privacy panel after this task.

In `src/shell/SettingsPanel.test.tsx`:

- DELETE every test that asserts ARDOP controls render, ARDOP values persist, ARDOP default loads, ARDOP onBlur fires, etc.
- KEEP every test that asserts GPS-state + precision behavior.

- [ ] **Step 4: Run the test to verify it passes.**

```bash
pnpm vitest run src/shell/SettingsPanel.test.tsx 2>&1 | tail -10
```

Expected: PASS (only GPS-state + precision tests remain; the new "does NOT render the ARDOP HF fieldset" test passes).

- [ ] **Step 5: Run the full vitest suite + tsc.**

```bash
pnpm vitest run 2>&1 | tail -5
pnpm exec tsc --noEmit 2>&1 | tail -5
```

Expected: all passing; tsc clean.

- [ ] **Step 6: Commit.**

```bash
git add src/shell/SettingsPanel.tsx src/shell/SettingsPanel.test.tsx
git commit -m "fix(shell): delete ARDOP fieldset from Settings (tuxlink-jmfm)

Operator 2026-06-01: 'The ARDOP controls are hidden in the GPS +
privacy sub-menu which is even more things we need on the ARDOP radio
control dock. That has to be high priority fixed.'

The ARDOP fieldset (binary, capture_device, playback_device,
ptt_serial_path, cmd_port, bandwidth_hz) is removed from
SettingsPanel.tsx. The SettingsPanel is now purely a GPS-privacy
panel (its original intent per the file header).

PR #185 commit 4c88618 already wired Capture / Playback / PTT / WebGUI
inline-edit in the ARDOP panel's Radio section. Task 3 of this plan
adds the remaining cmd_port + ardopcf-binary inputs there too so every
ARDOP control is reachable inline.

Closes tuxlink-jmfm.

bd issue: tuxlink-jmfm
Plan: docs/superpowers/plans/2026-06-01-radio-panel-width-and-ardop-controls-relocate-plan.md (Task 2)

Agent: bison-condor-grouse
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Add `cmd_port` + `ardopcf binary` inputs to the ARDOP panel Radio section

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.tsx`
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx`

**Context:** PR #185 commit `4c88618` added Capture / Playback / PTT / WebGUI inline-edit rows to the ARDOP panel's Radio section. Two of the ARDOP fields from the (now-deleted, Task 2) Settings fieldset weren't covered: `cmd_port` and the `binary` (ardopcf binary path). Task 3 adds them.

- [ ] **Step 1: Write the failing tests.**

In `src/radio/modes/ArdopRadioPanel.test.tsx`, ADD:

```typescript
test('Radio section has a cmd_port input row (tuxlink-jmfm)', async () => {
  // Mount in stopped state so the Radio section renders.
  render(<ArdopRadioPanel onClose={vi.fn()} />);
  expect(screen.getByTestId('ardop-cmd-port-input')).toBeInTheDocument();
});

test('Radio section has an ardopcf binary input row (tuxlink-jmfm)', () => {
  render(<ArdopRadioPanel onClose={vi.fn()} />);
  expect(screen.getByTestId('ardop-binary-input')).toBeInTheDocument();
});

test('cmd_port input persists on blur', async () => {
  const persistArdop = vi.spyOn(/* mock invoke for config_set_ardop */);
  render(<ArdopRadioPanel onClose={vi.fn()} />);
  const input = screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value: '8520' } });
  fireEvent.blur(input);
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  expect(persistArdop).toHaveBeenCalledWith(expect.objectContaining({ cmd_port: 8520 }));
});
```

(Adjust the persistArdop mocking pattern to match the existing ArdopRadioPanel test patterns for Capture / Playback / PTT / WebGUI inputs.)

- [ ] **Step 2: Run the tests to verify they fail.**

```bash
pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx -t "cmd_port" 2>&1 | tail -10
pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx -t "ardopcf binary" 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Add the two `<label className="radio-panel-input-row">` rows.**

In `src/radio/modes/ArdopRadioPanel.tsx`, locate the `<section className="radio-panel-sec" data-testid="ardop-radio-section">` block (around line 520). After the existing WebGUI input row (around line 575–595), ADD:

```typescript
<label className="radio-panel-input-row">
  <span>Cmd port</span>
  <input
    type="text"
    inputMode="numeric"
    className="radio-panel-input"
    data-testid="ardop-cmd-port-input"
    value={cmdPortInput}
    spellCheck={false}
    autoCapitalize="off"
    autoCorrect="off"
    placeholder="8515 (ardopcf default)"
    onChange={(e) => setCmdPortInput(e.target.value)}
    onBlur={commitCmdPort}
  />
</label>
<label className="radio-panel-input-row">
  <span>Binary</span>
  <input
    type="text"
    className="radio-panel-input"
    data-testid="ardop-binary-input"
    value={binaryInput}
    spellCheck={false}
    autoCapitalize="off"
    autoCorrect="off"
    placeholder="ardopcf"
    onChange={(e) => setBinaryInput(e.target.value)}
    onBlur={commitBinary}
  />
</label>
```

Add the supporting state + commit handlers near the top of the component (matching the pattern of `captureInput` / `commitCapture` from `4c88618`):

```typescript
const [cmdPortInput, setCmdPortInput] = useState<string>(
  ardopConfig ? String(ardopConfig.cmd_port) : '8515',
);
const [binaryInput, setBinaryInput] = useState<string>(
  ardopConfig?.binary ?? 'ardopcf',
);

// Sync local state when ardopConfig loads / refreshes
useEffect(() => {
  if (ardopConfig) {
    setCmdPortInput(String(ardopConfig.cmd_port));
    setBinaryInput(ardopConfig.binary);
  }
}, [ardopConfig]);

const commitCmdPort = useCallback(() => {
  const parsed = parseInt(cmdPortInput.trim(), 10);
  if (Number.isFinite(parsed) && parsed > 0 && ardopConfig && parsed !== ardopConfig.cmd_port) {
    persistArdop({ ...ardopConfig, cmd_port: parsed });
  }
}, [cmdPortInput, ardopConfig, persistArdop]);

const commitBinary = useCallback(() => {
  const trimmed = binaryInput.trim();
  if (trimmed && ardopConfig && trimmed !== ardopConfig.binary) {
    persistArdop({ ...ardopConfig, binary: trimmed });
  }
}, [binaryInput, ardopConfig, persistArdop]);
```

Match the existing pattern from `commitCapture` / `commitPlayback` / `commitPttSerial` / `commitWebguiPort` in the same file.

- [ ] **Step 4: Run the tests to verify they pass.**

```bash
pnpm vitest run src/radio/modes/ArdopRadioPanel.test.tsx 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Run the full vitest suite + tsc.**

```bash
pnpm vitest run 2>&1 | tail -5
pnpm exec tsc --noEmit 2>&1 | tail -5
```

Expected: all passing; tsc clean.

- [ ] **Step 6: Commit.**

```bash
git add src/radio/modes/ArdopRadioPanel.tsx src/radio/modes/ArdopRadioPanel.test.tsx
git commit -m "feat(radio): add cmd_port + binary inline-edit rows to ARDOP Radio section (tuxlink-jmfm)

Closes the last two ARDOP control gaps after Task 2 deletes the Settings
ARDOP fieldset. PR #185 commit 4c88618 already added Capture / Playback
/ PTT / WebGUI. This task adds cmd_port (numeric) + binary (text) rows
matching the same pattern.

Closes tuxlink-jmfm.

bd issue: tuxlink-jmfm
Plan: docs/superpowers/plans/2026-06-01-radio-panel-width-and-ardop-controls-relocate-plan.md (Task 3)

Agent: bison-condor-grouse
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Post-task verification

- [ ] **Verify Step 1: Full vitest green.**

```bash
pnpm vitest run 2>&1 | tail -5
```

- [ ] **Verify Step 2: tsc clean.**

```bash
pnpm exec tsc --noEmit 2>&1 | tail -5
```

- [ ] **Verify Step 3: Push the branch.**

```bash
git push
```

- [ ] **Verify Step 4: Open the PR (ready, not draft per `feedback_no_draft_pr_parking`).**

```bash
gh pr create --base main \
  --head bd-tuxlink-jmfm/radio-panel-400px-controls-relocate \
  --title "[bison-condor-grouse] fix(radio): 400px panel width + ARDOP controls relocate (tuxlink-jmfm + tuxlink-8rng)" \
  --body "$(cat <<'EOF'
## Summary

Two operator-flagged fixes bundled per shared subsystem:

1. **tuxlink-8rng** — widen the radio-panel chrome from 360 → 400 px across ALL mode panels (the prior 360 px was 'arbitrary' per operator). Mailbox column absorbs the 40 px (340 → 300 px).
2. **tuxlink-jmfm** — delete the ARDOP fieldset from \`SettingsPanel.tsx\`'s GPS-privacy panel; add the last two ARDOP control rows (cmd_port + binary) to the ARDOP panel's Radio section. Every ARDOP control is now inline in the dock.

Closes tuxlink-jmfm + tuxlink-8rng.

## Test plan (operator smoke)

- [ ] Smoke 1: Open the ARDOP mode in the dock; confirm no horizontal clipping; confirm Capture / Playback / PTT / WebGUI / Cmd-port / Binary all visible.
- [ ] Smoke 2: Open Settings; confirm no ARDOP fieldset is visible; confirm GPS-state + precision controls intact.
- [ ] Smoke 3: Switch between Telnet / Packet / ARDOP modes in the dock; confirm 400 px width across all modes.
- [ ] Smoke 4: Confirm cmd_port input persists across app restart.
- [ ] Smoke 5: Confirm ardopcf binary path input persists across app restart.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

**Spec coverage:** bd issues `tuxlink-jmfm` (controls relocate) + `tuxlink-8rng` (panel widening) fully addressed.
- T1 = 8rng (400 px propagation).
- T2 = jmfm option (a) (delete fieldset).
- T3 = jmfm gap closure (add the last two controls inline).

**Placeholder scan:** None.

**Type consistency:** `ArdopUiConfig` type comes from `useArdopConfig` hook; T2 deletes the local mirror in `SettingsPanel.tsx`; T3 uses the canonical type from the hook.
