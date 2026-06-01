# HTML Forms — Phase 2: Native auto-fill / auto-gen forms

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the three native forms where tuxlink has a unique data layer to contribute, with the right auto-fill / auto-gen UX. After P2, the GPS Position Report, the ICS-309 Comms Log, and the new Winlink Check-In all live in `src/compose/` as React forms backed by Rust data sources (`PositionArbiter`, `messages_meta`), routed via the `CatalogBrowser` from P1.

**Architecture:** Each form gets a top-level React component in `src/compose/`, an entry in the form registry (`forms.ts::registerForm`), and a thin Tauri command pair for whichever Rust data source it pulls from. Existing PR #177 wire-format machinery (`forms::serialize::build_xml_attachment`, `forms::types::FormPayload`, `send_form` IPC) is reused as-is — P2 adds **no new wire format**.

**Tech stack:** TypeScript / React 19 / Vitest (CSS-blind) / Rust / Tauri. Map widget + PDF library deps decided in Task 0 once operator answers spec §13 open Qs.

**Branch / worktree:** Execute in a fresh worktree owned by bd `tuxlink-hnkn`. Create before Task 1:

```bash
python3 .claude/scripts/new_tuxlink_worktree.py \
  --slug p2-native-autofill \
  --issue tuxlink-hnkn \
  --base main \
  --moniker <your-session-moniker>
```

Branch will be `bd-tuxlink-hnkn/p2-native-autofill`. All file paths in this plan are relative to that worktree.

**Design reference:** [`docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md`](../specs/2026-05-31-html-forms-full-parity-design.md) §6 Phase 2 (deliverables), §7 (components), §8.1 (native path data flow), §13 (open questions).

**Depends on P1 (`tuxlink-ytya`) landing first**, because:
- `CatalogBrowser` is the entry point that picks native-vs-webview routing
- `FormMode` discriminated union with `webview-form` variant is used as the fallback
- Receive-side native View → KeyValueView fallback path covers cases where native and webview both fail

If P1 has not landed by the time this plan executes, **STOP** and re-coordinate with `tuxlink-ytya` first. Per ADR 0010 (no-squash-merge), P2 PR can stack on the P1 PR if P1 isn't merged yet.

**Operator-decision dependencies (spec §13):**

| Open Q | Affects | If unresolved at plan-time |
|---|---|---|
| Map widget for Position Report | Task 1 step "map override UI" | Ship Task 1 with a text-input-only fallback (operator pastes Maidenhead grid manually); add a TBD-banner; file follow-up bd issue for the map widget once chosen |
| PDF library for ICS-309 | Task 2 step "export-PDF action" | Ship Task 2 with XML attachment + CSV-export action only; PDF deferred to a P2 follow-up or P3 |
| Form draft library scope | Task 4 | Plan ships Check-In-only `FormDraftLibrary`; generalize-to-all-forms moves to P3 (default, matching design §6 P3) |

**Adversarial review:** Per `feedback_codex_post_subagent_review`, each form module ships with a Codex round before its commit lands on the PR. The cross-cutting concern (`FormDraftLibrary` persistence schema + atomicity) gets a dedicated round.

**Browser-smoke gate:** Per `feedback_browser_smoke_before_ship`, the PR stays open for operator smoke before merge. Walk-through enumerated in Task 6.

---

## Task 0: Lock the open operator decisions

**Files / artifacts:**
- Read: spec §13 open questions verbatim
- Read: operator-supplied answers in the bd `tuxlink-hnkn` issue notes (if posted)
- Write: `dev/scratch/2026-06-XX-p2-decision-lock.md` (gitignored decision record)
- Read: `dev/handoffs/<latest>.md` if the operator posted preferences there

The three open Qs at spec §13 that directly affect this plan:

1. **PDF generation library for ICS-309** — operator preference among:
   - `wkhtmltopdf` (external dep, well-supported but a ~50MB system package)
   - `typst` (pure-Rust, modern; lighter dep but learning curve)
   - `printpdf` (pure-Rust crate; primitive layout)
   - "punt to browser print from Viewer mode" — minimal-effort path
2. **Map widget for Position Report** — operator preference among:
   - Leaflet w/ offline tile cache (~200KB JS + tens-of-MB tile pack)
   - MapLibre GL (similar size; vector tiles)
   - Skip the map; keep a text input for the grid only
3. **Form draft library scope** — Winlink Check-In only (P2), or generalize across all 5 native forms at P2 launch

- [ ] **Step 1: Read `bd show tuxlink-hnkn` notes for operator answers.**

```bash
bd show tuxlink-hnkn | tail -30
```

If three answers are present, proceed with those choices. If absent, default to:
- PDF: **defer to P3** (ship XML + CSV at P2)
- Map: **skip widget; text input for grid** (ship the data-source pull; add the map widget as a P3 follow-up bd issue)
- Draft library: **Check-In-only** (P3 generalizes to all forms)

- [ ] **Step 2: Write the decision record.**

```bash
cat > dev/scratch/2026-06-XX-p2-decision-lock.md <<'EOF'
# P2 Operator-decision lock — captured <date>

## §13 Q1: PDF library
Decision: [chosen / deferred]
Rationale: [operator's answer OR the documented default]

## §13 Q2: Map widget
Decision: [chosen / skipped]
Rationale: [operator's answer OR the documented default]

## §13 Q3: Draft library scope
Decision: [Check-In only / all-forms-from-day-1]
Rationale: [operator's answer OR the documented default]

## Implications for this plan
- Task 1 step <N>: <does/does not> mount a map widget
- Task 2 step <N>: <does/does not> include PDF export
- Task 4: <scope choice>
EOF
```

- [ ] **Step 3: If PDF is chosen, append the dep to `src-tauri/Cargo.toml`; if a map widget is chosen, append `package.json` dep; commit deps separately.**

```bash
# Example for the typst choice:
# In src-tauri/Cargo.toml [dependencies]:
#   typst = { version = "0.13", default-features = false }
#   typst-pdf = "0.13"
# Then: cargo --manifest-path src-tauri/Cargo.toml check
# Commit: "build(deps): add typst + typst-pdf for ICS-309 PDF export"
```

If both decisions are "defer," skip this step.

---

## Task 1: Position Report (native rebuild) — `PositionFormV2`

**Files:**
- New: `src/compose/PositionFormV2.tsx`
- New: `src/compose/PositionFormV2.css`
- New: `src/compose/PositionFormV2.test.tsx`
- Update: `src/forms/position/index.ts` (register the new Form alongside the existing PositionView)
- Update: `src-tauri/src/ui_commands.rs` (new command: `position_current_fix`)

Pulls the current grid + lat/lon from `PositionArbiter::active_grid()` + `PositionArbiter::has_fresh_fix()`. Renders a confirm-or-override card: the operator sees the current fix, optionally edits the grid (text input), optionally adds a free-text remark, and one-clicks Send. Submitter writes the Position Report XML via the existing `send_form` IPC.

Spec §6 P2: "pull from PositionArbiter; map widget for override (TBD: which map lib — leaflet w/ offline tiles?); one-click send."

**Decision dependency:** Map widget. If Task 0 chose to skip the widget, the override is a text input only.

- [ ] **Step 1: Backend command — `position_current_fix`.**

In `src-tauri/src/ui_commands.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PositionFix {
    pub grid: Option<String>,
    pub source: String,  // "gps" | "manual" | "configured"
    pub fresh: bool,     // is the GPS fix < 60s old?
}

#[tauri::command]
pub async fn position_current_fix(
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
) -> Result<PositionFix, String> {
    Ok(PositionFix {
        grid: arbiter.active_grid(),
        source: format!("{:?}", arbiter.source()),
        fresh: arbiter.has_fresh_fix(),
    })
}
```

Register in `invoke_handler`. Commit:

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib ui_commands 2>&1 | tail -5
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit -m "feat(forms): position_current_fix Tauri command for PositionFormV2

Thin shim over PositionArbiter::active_grid() + has_fresh_fix() +
source(). Consumed by the React Position form rebuild for one-click
pre-fill.

Refs: bd tuxlink-hnkn P2 Task 1.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 2: Test scaffold — PositionFormV2 component.**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PositionFormV2 } from './PositionFormV2';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'position_current_fix') {
      return { grid: 'CN87us', source: 'gps', fresh: true };
    }
    if (cmd === 'send_form') return 'MID-MOCK-123';
    return null;
  }),
}));

describe('<PositionFormV2>', () => {
  it('renders the current GPS grid with a fresh-fix indicator', async () => {
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByDisplayValue('CN87us')).toBeInTheDocument();
    expect(screen.getByText(/fresh.*GPS/i)).toBeInTheDocument();
  });

  it('allows manual grid override', async () => {
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const input = await screen.findByLabelText(/grid/i);
    fireEvent.change(input, { target: { value: 'EM26' } });
    expect((input as HTMLInputElement).value).toBe('EM26');
  });

  it('Send button calls onSubmit with the rendered FormPayload shape', async () => {
    const onSubmit = vi.fn();
    render(<PositionFormV2 onSubmit={onSubmit} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('CN87us');
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    const arg = onSubmit.mock.calls[0][0];
    expect(arg.grid).toBe('CN87us');
    expect(arg.formId).toBe('Position_Report');
  });

  it('shows a stale-fix warning when fresh=false', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      grid: 'CN87us',
      source: 'gps',
      fresh: false,
    });
    render(<PositionFormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText(/stale/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Implement `PositionFormV2.tsx`.**

```tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './PositionFormV2.css';

interface PositionFix {
  grid: string | null;
  source: string;
  fresh: boolean;
}

interface Props {
  onSubmit: (payload: { formId: string; grid: string; remark: string }) => void;
  onCancel: () => void;
}

export function PositionFormV2({ onSubmit, onCancel }: Props) {
  const [fix, setFix] = useState<PositionFix | null>(null);
  const [grid, setGrid] = useState('');
  const [remark, setRemark] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<PositionFix>('position_current_fix')
      .then((f) => {
        setFix(f);
        if (f.grid) setGrid(f.grid);
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) {
    return (
      <div className="position-form-v2" role="alert">
        Position fix unavailable: {error}
      </div>
    );
  }

  return (
    <div className="position-form-v2" data-testid="position-form-v2">
      <div className="position-form-v2__header">
        <h2>Position Report</h2>
        {fix && (
          <div className={`position-form-v2__fix-badge ${fix.fresh ? 'fresh' : 'stale'}`}>
            {fix.fresh ? 'Fresh' : 'Stale'} {fix.source.toUpperCase()} fix
          </div>
        )}
      </div>

      <label htmlFor="position-grid">Maidenhead grid</label>
      <input
        id="position-grid"
        type="text"
        value={grid}
        onChange={(e) => setGrid(e.target.value.toUpperCase())}
        placeholder="CN87us"
        aria-label="Maidenhead grid"
      />

      {/* Map widget mount-point — populated in Task 0 if chosen, else hidden. */}
      <div className="position-form-v2__map" data-testid="position-map-mount">
        {/* If map widget is enabled, the widget mounts here; else this stays empty. */}
      </div>

      <label htmlFor="position-remark">Remark (optional)</label>
      <textarea
        id="position-remark"
        value={remark}
        onChange={(e) => setRemark(e.target.value)}
        rows={3}
      />

      <div className="position-form-v2__actions">
        <button onClick={onCancel}>Cancel</button>
        <button
          className="primary"
          onClick={() => onSubmit({ formId: 'Position_Report', grid, remark })}
          disabled={!grid}
        >
          Send
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Register in form registry.**

In `src/forms/position/index.ts`:

```diff
-import { Ics213View } from './Ics213View'; // wrong import - using actual file
+import { PositionView } from './PositionView';
+import { PositionFormV2 } from '../../compose/PositionFormV2';
 import { registerForm } from '../forms';

 registerForm({
   id: 'Position_Report',
   name: 'Position Report',
-  View: PositionView, // P0 trim removed the Form; restore here as PositionFormV2
+  Form: PositionFormV2,
+  View: PositionView,
 });
```

(Adjust the imports based on what P0 actually left in the file. If P0 deleted `position/index.ts` entirely, recreate per the pattern from `ics213/index.ts`.)

- [ ] **Step 5: Verify + commit.**

```bash
pnpm exec vitest run src/compose/PositionFormV2 src/forms/position 2>&1 | tail -10
git add src/compose/PositionFormV2.tsx src/compose/PositionFormV2.css \
        src/compose/PositionFormV2.test.tsx src/forms/position/index.ts
git commit -m "feat(forms): PositionFormV2 — native Position Report with PositionArbiter pull

Replaces the P0-removed compose form with the right UX: pre-fills grid
from PositionArbiter::active_grid(), shows fresh/stale fix indicator,
allows manual grid override, free-text remark. Sends via existing
send_form IPC + PR #177 wire-format machinery (no new serialize code).

Map widget mount point present but unwired — Task 0 decision pending /
deferred to follow-up bd if 'skip' was chosen.

Refs: bd tuxlink-hnkn P2 Task 1; spec §6 P2.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 6: Codex adrev.**

```bash
cat > /tmp/codex-prompt-position-v2.txt <<'EOF'
Adversarial review of PositionFormV2 against origin/main.
Run `git diff origin/main -- src/compose/PositionFormV2.tsx src/forms/position/index.ts src-tauri/src/ui_commands.rs` for the diff.

Attack angles:
1. Stale-fix UX: an operator hits Send on a stale GPS fix and broadcasts
   an old position. Is the stale-fix indicator prominent enough? Is there
   a confirm-or-cancel for stale sends?
2. Manual override: the operator types a grid, GPS subsequently delivers
   a fresh fix → does the auto-fill clobber the operator's manual edit?
3. Grid validation: is the grid input validated against the Maidenhead
   format (2/4/6 char patterns) before send? What does the wire format
   do with an invalid grid?
4. RADIO-1 compliance: the form is just preparing a payload (no on-air
   action) so RADIO-1 doesn't apply here directly — but is there any
   path where a button click could initiate a TX without operator
   confirmation? (Spec §10 says no; verify.)
5. Backend command: position_current_fix returns Option<String> for the
   grid; what does the form render when grid is None (no fix yet)?
EOF
cat /tmp/codex-prompt-position-v2.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-XX-p2-position-form-codex.md
```

Apply P0/P1 findings; file P2/P3.

---

## Task 2: ICS-309 Comms Log (native rebuild) — `Ics309FormV2`

**Files:**
- New: `src/compose/Ics309FormV2.tsx`
- New: `src/compose/Ics309FormV2.css`
- New: `src/compose/Ics309FormV2.test.tsx`
- Update: `src/forms/ics309/index.ts` (register the new Form alongside the existing Ics309View)
- Update: `src-tauri/src/ui_commands.rs` (new command: `messages_meta_query_for_log`)

Spec §6 P2: "time-range picker; `messages_meta` query; preview pane showing aggregated rows; submit emits the standard XML attachment, optionally also attaching a CSV/PDF."

Operator-flow:
1. Operator picks a time range (presets: "last hour" / "today" / "this op-period" / custom date-range).
2. tuxlink queries `messages_meta` for sent + received messages in that range, filtered by required ICS-309 fields (from, to, datetime, subject).
3. Preview pane renders the aggregated table.
4. Submit emits ICS-309 XML attachment + CSV (always) + PDF (if Task 0 chose a PDF library).

- [ ] **Step 1: Backend command — `messages_meta_query_for_log`.**

```rust
#[derive(Debug, Clone, Serialize)]
pub struct LogRow {
    pub datetime: String,   // RFC 3339 UTC
    pub from: String,       // sender callsign
    pub to: String,         // primary recipient (first To: address)
    pub subject: String,
    pub direction: String,  // "in" | "out"
}

#[tauri::command]
pub async fn messages_meta_query_for_log(
    start_rfc3339: String,
    end_rfc3339: String,
    mailbox: tauri::State<'_, std::sync::Arc<crate::native_mailbox::Mailbox>>,
) -> Result<Vec<LogRow>, String> {
    mailbox.query_log_rows(&start_rfc3339, &end_rfc3339)
        .map_err(|e| e.to_string())
}
```

In `src-tauri/src/native_mailbox.rs`, add the SQL helper:

```rust
impl Mailbox {
    pub fn query_log_rows(&self, start_rfc3339: &str, end_rfc3339: &str)
        -> Result<Vec<LogRow>, MailboxError>
    {
        // messages_meta columns: mid TEXT, folder TEXT, date_utc TEXT, ...
        // Sent folder + Inbox: filter by date_utc BETWEEN start AND end.
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT date_utc, from_addr, to_addr, subject, folder
             FROM messages_meta
             WHERE date_utc BETWEEN ?1 AND ?2
               AND folder IN ('inbox', 'sent')
             ORDER BY date_utc ASC",
        )?;
        let rows = stmt.query_map([start_rfc3339, end_rfc3339], |r| {
            let folder: String = r.get(4)?;
            Ok(LogRow {
                datetime: r.get(0)?,
                from: r.get(1)?,
                to: r.get(2)?,
                subject: r.get(3)?,
                direction: if folder == "sent" { "out".to_string() } else { "in".to_string() },
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(MailboxError::from)
    }
}
```

(Adjust column names against the actual `messages_meta` schema — confirm via `grep CREATE TABLE messages_meta src-tauri/src/`.)

- [ ] **Step 2: Test scaffold — Ics309FormV2.**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { Ics309FormV2 } from './Ics309FormV2';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args: any) => {
    if (cmd === 'messages_meta_query_for_log') {
      return [
        { datetime: '2026-06-01T14:30:00Z', from: 'W7CPZ', to: 'NET', subject: 'Check-in', direction: 'out' },
        { datetime: '2026-06-01T14:32:00Z', from: 'NET',    to: 'W7CPZ', subject: 'Roger', direction: 'in' },
      ];
    }
    if (cmd === 'send_form') return 'MID-MOCK';
    return null;
  }),
}));

describe('<Ics309FormV2>', () => {
  it('renders the time-range presets', () => {
    render(<Ics309FormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole('button', { name: /last hour/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /today/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /op-period/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /custom/i })).toBeInTheDocument();
  });

  it('picking "today" runs the query and shows preview rows', async () => {
    render(<Ics309FormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: /today/i }));
    expect(await screen.findByText(/Check-in/)).toBeInTheDocument();
    expect(screen.getByText('W7CPZ → NET')).toBeInTheDocument();
    expect(screen.getByText('NET → W7CPZ')).toBeInTheDocument();
  });

  it('preview is empty before the operator picks a range', () => {
    render(<Ics309FormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.queryByTestId('ics309-preview-row')).toBeNull();
  });

  it('Send is disabled until preview has rows', async () => {
    render(<Ics309FormV2 onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole('button', { name: /send/i })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: /today/i }));
    await screen.findByText(/Check-in/);
    expect(screen.getByRole('button', { name: /send/i })).toBeEnabled();
  });
});
```

- [ ] **Step 3: Implement Ics309FormV2.**

```tsx
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './Ics309FormV2.css';

interface LogRow {
  datetime: string;
  from: string;
  to: string;
  subject: string;
  direction: 'in' | 'out';
}

type Preset = 'last-hour' | 'today' | 'op-period' | 'custom';

interface Props {
  onSubmit: (payload: { formId: string; rows: LogRow[]; rangeStart: string; rangeEnd: string }) => void;
  onCancel: () => void;
}

function isoRange(preset: Preset, custom?: { start: string; end: string }):
  { start: string; end: string }
{
  const now = new Date();
  switch (preset) {
    case 'last-hour':
      return { start: new Date(now.getTime() - 60 * 60 * 1000).toISOString(), end: now.toISOString() };
    case 'today':
      return {
        start: new Date(now.getFullYear(), now.getMonth(), now.getDate()).toISOString(),
        end: now.toISOString(),
      };
    case 'op-period':
      // Conventional ARES op-period: 0000–2359 local — for now, alias to today.
      return {
        start: new Date(now.getFullYear(), now.getMonth(), now.getDate()).toISOString(),
        end: now.toISOString(),
      };
    case 'custom':
      return custom ?? { start: now.toISOString(), end: now.toISOString() };
  }
}

export function Ics309FormV2({ onSubmit, onCancel }: Props) {
  const [rows, setRows] = useState<LogRow[]>([]);
  const [range, setRange] = useState<{ start: string; end: string } | null>(null);
  const [loading, setLoading] = useState(false);

  const runQuery = async (preset: Preset, custom?: { start: string; end: string }) => {
    const { start, end } = isoRange(preset, custom);
    setRange({ start, end });
    setLoading(true);
    try {
      const result = await invoke<LogRow[]>('messages_meta_query_for_log', {
        startRfc3339: start, endRfc3339: end,
      });
      setRows(result);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="ics309-form-v2" data-testid="ics309-form-v2">
      <div className="ics309-form-v2__range">
        <button onClick={() => runQuery('last-hour')}>Last hour</button>
        <button onClick={() => runQuery('today')}>Today</button>
        <button onClick={() => runQuery('op-period')}>Op-period</button>
        <button>Custom range</button>
      </div>

      {loading && <div>Loading…</div>}

      <table className="ics309-form-v2__preview" data-testid="ics309-preview-table">
        <thead>
          <tr><th>Datetime</th><th>From / To</th><th>Subject</th></tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i} data-testid="ics309-preview-row">
              <td>{r.datetime}</td>
              <td>{r.from} → {r.to}</td>
              <td>{r.subject}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <div className="ics309-form-v2__actions">
        <button onClick={onCancel}>Cancel</button>
        <button
          className="primary"
          disabled={rows.length === 0 || !range}
          onClick={() => range && onSubmit({
            formId: 'Form-309_Initial',
            rows,
            rangeStart: range.start,
            rangeEnd: range.end,
          })}
        >
          Send
        </button>
        {/* PDF/CSV export buttons gated on Task 0 decision; render placeholders
            with `disabled title="Coming soon"` when deferred. */}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: CSV export (always present).**

Add a `Download CSV` button that converts `rows` to CSV in-process (no
filesystem; let the browser handle the blob). Verbatim implementation:

```tsx
function exportCsv(rows: LogRow[]): Blob {
  const header = 'Datetime,From,To,Subject,Direction\n';
  const body = rows.map(r =>
    [r.datetime, r.from, r.to, r.subject.replace(/"/g, '""'), r.direction]
      .map(c => `"${c}"`).join(',')
  ).join('\n');
  return new Blob([header + body], { type: 'text/csv' });
}
```

UI:

```tsx
<button onClick={() => {
  const url = URL.createObjectURL(exportCsv(rows));
  const a = document.createElement('a');
  a.href = url;
  a.download = `ics309-${range?.start.slice(0, 10) ?? 'log'}.csv`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}}>Download CSV</button>
```

- [ ] **Step 5: PDF export (gated on Task 0).**

If Task 0 chose a PDF library, add a `Download PDF` button. Implementation
depends on the chosen library; verbatim code lands at plan-execute time.
If deferred, render the button as `disabled title="PDF export deferred — see bd <issue>"`.

- [ ] **Step 6: Register in form registry.**

In `src/forms/ics309/index.ts`:

```diff
+import { Ics309FormV2 } from '../../compose/Ics309FormV2';
 import { Ics309View } from './Ics309View';
 import { registerForm } from '../forms';

 registerForm({
   id: 'Form-309_Initial',
   name: 'ICS-309 Comms Log',
+  Form: Ics309FormV2,
   View: Ics309View,
 });
```

- [ ] **Step 7: Verify + commit + Codex adrev.**

```bash
pnpm exec vitest run src/compose/Ics309FormV2 src/forms/ics309 2>&1 | tail -10
cargo --manifest-path src-tauri/Cargo.toml test --lib native_mailbox::query_log_rows 2>&1 | tail -5
git add src/compose/Ics309FormV2.tsx src/compose/Ics309FormV2.css \
        src/compose/Ics309FormV2.test.tsx src/forms/ics309/index.ts \
        src-tauri/src/ui_commands.rs src-tauri/src/native_mailbox.rs
git commit -m "feat(forms): Ics309FormV2 — native ICS-309 with messages_meta aggregation

Replaces the P0-removed compose form. Time-range presets (last-hour /
today / op-period / custom) drive a messages_meta query that aggregates
sent + received messages into the preview table. CSV export always
present; PDF export gated on operator-decision lock (Task 0).

Refs: bd tuxlink-hnkn P2 Task 2; spec §6 P2.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

Codex adrev attack angles:
- Time-range edge cases (range that spans midnight UTC; range that crosses op-period boundaries)
- messages_meta column schema mismatch (verify the SELECT matches CREATE TABLE)
- Empty-row handling (no rows in the chosen range → preview empty + Send disabled)
- Sort-order: rows must be in time-ascending order regardless of folder
- CSV escaping: subject with `,` or `"` — verify CSV escaping is correct

---

## Task 3: Winlink Check-In (new native form) — `CheckInForm`

**Files:**
- New: `src/compose/CheckInForm.tsx`
- New: `src/compose/CheckInForm.css`
- New: `src/compose/CheckInForm.test.tsx`
- New: `src/forms/checkin/index.ts` (register the form)
- New: `src-tauri/src/forms/templates/checkin.rs` (per-form Rust template; mirror the existing pattern)

Spec §6 P2 + spec §4 hamexandria research note: "the most-frequently-demonstrated form in EmComm tutorial corpus; missed entirely by the v0.1 spec."

Fields (verify against actual WLE Winlink_Check-In template; sketch here):
- Tactical call (= operator's callsign, pre-filled from config)
- Operator name (free text, save in profile)
- Group/Net (free text, pre-fillable from FormDraftLibrary slot)
- Status (Ready / Standby / Out — discriminated radio buttons)
- Comments (free text)
- Position (auto-filled from PositionArbiter, like PositionFormV2)
- Submitter operator initials (one-line)

- [ ] **Step 1: Add the WLE Check-In template to `forms::templates`.**

(Mirror the pattern in `src-tauri/src/forms/templates/ics213.rs`. Pull
field schema + XML body template from the WLE Winlink_Check-In source.)

- [ ] **Step 2: Add the form to the bundled catalog enumerator.**

In `src-tauri/src/forms/catalog.rs`, register `Winlink_Check-In` so
`find_form()` resolves it.

- [ ] **Step 3: Test scaffold for CheckInForm.**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { CheckInForm } from './CheckInForm';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'position_current_fix') return { grid: 'CN87', source: 'gps', fresh: true };
    if (cmd === 'config_read') return { callsign: 'W7CPZ' };
    if (cmd === 'send_form') return 'MID';
    if (cmd === 'form_draft_library_list') return [
      { id: 'slot-monday-night-net', label: 'Monday Night Net', payload: { Group: 'ARES Net' } },
    ];
    return null;
  }),
}));

describe('<CheckInForm>', () => {
  it('pre-fills tactical call from config', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByDisplayValue('W7CPZ')).toBeInTheDocument();
  });

  it('renders the saved-slot dropdown', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText(/Monday Night Net/)).toBeInTheDocument();
  });

  it('clicking a saved slot applies its payload to the form', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByText(/Monday Night Net/));
    expect((screen.getByLabelText(/group/i) as HTMLInputElement).value).toBe('ARES Net');
  });

  it('Status defaults to Ready', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect((await screen.findByLabelText(/ready/i) as HTMLInputElement).checked).toBe(true);
  });
});
```

- [ ] **Step 4: Implement CheckInForm.**

Skeleton; full implementation ~200 lines following the PositionFormV2 + Ics213Form patterns. Key invariants:
- Tactical call defaults to `config.callsign`, but editable (operators sometimes check in for someone else)
- Group/Net populated from the chosen FormDraftLibrary slot (or blank)
- Position auto-filled from `position_current_fix`
- "Save as slot" button next to the Group field — opens a name prompt, persists via `form_draft_library_upsert`

- [ ] **Step 5: Register in form registry.**

```typescript
// src/forms/checkin/index.ts
import { CheckInForm } from '../../compose/CheckInForm';
import { registerForm } from '../forms';

registerForm({
  id: 'Winlink_Check-In',  // verify against actual WLE form id
  name: 'Winlink Check-In',
  Form: CheckInForm,
  // View: not custom; falls through to KeyValueView or the receive-side
  // Viewer-mode webview (P1 Task 11)
});
```

- [ ] **Step 6: Update `src/forms/index.ts` to import it.**

```diff
 import './ics213';
 import './bulletin';
 import './position';
 import './ics309';
 import './damage_assessment';
+import './checkin';
```

- [ ] **Step 7: Verify + commit + Codex adrev.**

(Same pattern as Tasks 1 + 2.)

---

## Task 4: `FormDraftLibrary` — save / reuse named slots (Check-In scope)

**Files:**
- New: `src-tauri/src/forms/draft_library.rs`
- New: `src/compose/FormDraftLibrary.ts` (React-side wrapper)
- Update: `src-tauri/src/ui_commands.rs` (commands: `form_draft_library_list` / `_upsert` / `_delete`)

A per-form-id slot library backed by a SQLite table. P2 scope (per Task 0 default): wired only for Check-In; generalize to all native forms in P3.

**Schema (per-form-id slots):**

```sql
CREATE TABLE IF NOT EXISTS form_draft_slots (
    slot_id TEXT PRIMARY KEY,           -- UUID v4
    form_id TEXT NOT NULL,              -- e.g. 'Winlink_Check-In'
    label TEXT NOT NULL,                -- operator-named (e.g. 'Monday Night Net')
    payload_json TEXT NOT NULL,         -- serde_json of the form's field-values map
    created_at TEXT NOT NULL,           -- RFC 3339 UTC
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS form_draft_slots_by_form ON form_draft_slots(form_id);
```

- [ ] **Step 1: Schema migration.**

Add to `src-tauri/src/native_mailbox.rs`'s schema-init block (SCHEMA_VERSION
gets bumped). Verify the existing pattern; mirror it.

- [ ] **Step 2: Rust commands.**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormDraftSlot {
    pub slot_id: String,
    pub form_id: String,
    pub label: String,
    pub payload: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
pub async fn form_draft_library_list(
    form_id: String,
    mailbox: tauri::State<'_, std::sync::Arc<crate::native_mailbox::Mailbox>>,
) -> Result<Vec<FormDraftSlot>, String> { /* SELECT … WHERE form_id = ?1 */ }

#[tauri::command]
pub async fn form_draft_library_upsert(
    slot_id: Option<String>,
    form_id: String,
    label: String,
    payload: serde_json::Value,
    mailbox: tauri::State<'_, std::sync::Arc<crate::native_mailbox::Mailbox>>,
) -> Result<FormDraftSlot, String> { /* INSERT or UPDATE on conflict */ }

#[tauri::command]
pub async fn form_draft_library_delete(
    slot_id: String,
    mailbox: tauri::State<'_, std::sync::Arc<crate::native_mailbox::Mailbox>>,
) -> Result<(), String> { /* DELETE WHERE slot_id = ?1 */ }
```

- [ ] **Step 3: Rust tests.**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn upsert_creates_new_slot() { /* … */ }

    #[test]
    fn upsert_with_existing_slot_id_updates() { /* … */ }

    #[test]
    fn list_filters_by_form_id() { /* … */ }

    #[test]
    fn delete_removes_slot() { /* … */ }

    #[test]
    fn payload_json_round_trips_unicode() { /* … */ }
}
```

- [ ] **Step 4: Commit + Codex adrev.**

Codex attack angles specific to draft library:
- JSON injection: malformed `payload` from the React side — does the schema reject or store-and-fail-on-read?
- Concurrent upserts: two compose windows simultaneously save to the same slot_id — last-write-wins, or rejected?
- Schema migration: existing tuxlink installations don't have the new table; does the schema-init run idempotently?

```bash
git add src-tauri/src/forms/draft_library.rs src-tauri/src/ui_commands.rs \
        src-tauri/src/native_mailbox.rs src/compose/FormDraftLibrary.ts
git commit -m "feat(forms): FormDraftLibrary — slot persistence for Check-In (P2 scope)

Per-form-id slot library backed by SQLite. P2 scope: wired to Check-In
only via the chosen-slot dropdown. P3 generalizes to all native forms.

Refs: bd tuxlink-hnkn P2 Task 4; spec §6 P2 + §13 Q3.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Update CatalogBrowser entries

**Files:**
- Update: `src/compose/CatalogBrowser.tsx` (if any P2-specific routing changes are needed)

Per P1 design, `CatalogBrowser.onPick` already routes via `lookupForm(id)?.Form` presence — so the new native forms (PositionFormV2, Ics309FormV2, CheckInForm) get picked up automatically once they call `registerForm({ ..., Form: ... })`. **No CatalogBrowser code change should be required** unless P2's testing reveals an edge case (e.g., the Check-In form needs a special category badge in the tree).

- [ ] **Step 1: Verify auto-routing works.**

```bash
pnpm exec vitest run src/compose/CatalogBrowser 2>&1 | tail -5
```

Expected: existing P1 tests still pass. If a test asserts on form-set membership, update its expected list to include the new natives.

- [ ] **Step 2: If a sticky-category change is needed, document it and commit.**

(Most likely a no-op task. Document the verify result in the PR body's Task-5 line.)

---

## Task 6: End-to-end smoke + Codex full-diff adrev + PR open

- [ ] **Step 1: Full vitest + cargo test sweep.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-hnkn-p2-native-autofill
pnpm exec vitest run 2>&1 | tail -5
cargo --manifest-path src-tauri/Cargo.toml test --lib 2>&1 | tail -10
cargo --manifest-path src-tauri/Cargo.toml clippy --all-targets -- -D warnings 2>&1 | tail -5
pnpm exec tsc --noEmit 2>&1 | tail -5
```

All green. The TypeScript check is critical here because P2 adds many React components + new Tauri command surfaces.

- [ ] **Step 2: Codex full-diff adrev.**

```bash
cat > /tmp/codex-prompt-p2-full.txt <<'EOF'
Adversarial review of the full P2 diff against origin/main.
Run `git diff origin/main..HEAD` for the diff.

Cross-module concerns the per-form adrevs can't see:

1. Three new native forms + one new form in the registry. Are there any
   duplicate registrations (id collision) introduced? Does the
   CatalogBrowser auto-routing pick up all 4 correctly?
2. messages_meta SQL column-name assumptions: the Ics309FormV2's
   query_log_rows uses 'date_utc' / 'from_addr' / 'to_addr' / 'subject'
   / 'folder' — verify these match the actual messages_meta schema. If
   names diverge, the query silently returns 0 rows.
3. PositionArbiter call timing: are there cases where active_grid()
   returns Some(grid) but has_fresh_fix() returns false because the GPS
   fix is stale beyond the freshness threshold? Does the form UX
   surface this correctly?
4. FormDraftLibrary + Compose lifecycle: a Check-In form opens, the
   operator applies a slot, then the compose window closes WITHOUT
   sending — does the slot's payload bleed into the next compose-window
   open? (It should not — slot apply is per-open.)
5. send_form IPC + new form ids: ensure each new form's id is in the
   bundled catalog (catalog.rs find_form) so send_form succeeds. A
   missing entry produces `UiError::Internal { detail: "unknown form" }`.
6. PDF/CSV export (if PDF was decided in Task 0): is the in-process
   PDF generation blocking the JS event loop for large logs? Should it
   be wrapped in a worker / Tauri command?

Output P0/P1/P2/P3 severity.
EOF
cat /tmp/codex-prompt-p2-full.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-XX-p2-full-diff-codex.md
wc -l dev/adversarial/2026-06-XX-p2-full-diff-codex.md  # expect 2000+
```

- [ ] **Step 3: Apply P0 + P1 findings.** File P2/P3 as bd issues against `tuxlink-hnkn`.

- [ ] **Step 4: Push + open PR.**

```bash
git push -u origin bd-tuxlink-hnkn/p2-native-autofill
gh pr create --base main --head bd-tuxlink-hnkn/p2-native-autofill \
  --title "[<moniker>] feat(forms): P2 native auto-fill forms (Position / ICS-309 / Check-In) (tuxlink-hnkn)" \
  --body-file dev/scratch/2026-06-XX-p2-pr-body.md
```

PR body MUST include:

- Summary + spec §6 P2 reference
- Per-form bullet list (PositionFormV2, Ics309FormV2, CheckInForm) with
  test counts
- Task 0 decision-record summary (PDF lib chosen / deferred; map widget
  chosen / skipped; draft library scope)
- Codex adrev disposition
- **Browser-smoke walk-through:**

```
1. pnpm tauri dev → File → New Message → form picker.
2. CatalogBrowser → expand ICS Forms → click "Position_Report".
   EXPECTED: PositionFormV2 mounts; grid input pre-filled from GPS; fresh-
   fix indicator visible.
3. Grid override: type a new grid; verify input is editable; verify
   auto-fill does NOT clobber the manual edit when a fresh GPS fix
   arrives.
4. Click Send. EXPECTED: success state ("Posted to Outbox"); CMS-side
   inspection shows correct XML attachment.
5. Back to CatalogBrowser → Form-309_Initial. Pick a time range
   preset; EXPECTED: preview table populates from messages_meta.
6. Click Download CSV → save → open in spreadsheet → verify columns.
7. Click Send. EXPECTED: success state.
8. Back to CatalogBrowser → Winlink_Check-In. EXPECTED: tactical call
   prefilled; position auto-filled; saved slots (if any) listed.
9. Fill Group="My Net", Comments="On QRG"; click "Save as slot",
   name it "Test Slot". Close compose.
10. Re-open Check-In; EXPECTED: "Test Slot" appears in the saved-slots
    dropdown; clicking it pre-fills Group="My Net".
11. (If PDF was decided in Task 0) Repeat ICS-309 step 6 with Download
    PDF; verify file opens in a PDF viewer.
```

- [ ] **Step 5: Update bd `tuxlink-hnkn` with PR ref + Codex disposition + smoke gate.**

---

## Out-of-scope follow-ups (carried to P3)

- **Form draft library generalized to all native forms** (P3): currently
  Check-In-only per Task 0 default
- **PDF library if Task 0 deferred** (P3): pick + ship
- **Map widget if Task 0 deferred** (P3): pick + ship
- **Op-period configuration** (P3): currently aliased to "today"; ARES
  conventional op-period boundaries are configurable per net
- **Form-aware reply** (P3 per spec §6): not in scope here; design §6 P3

---

## Acceptance criteria

- [ ] Three new native forms (`PositionFormV2`, `Ics309FormV2`, `CheckInForm`)
      registered + tested + routed correctly via CatalogBrowser
- [ ] `position_current_fix` + `messages_meta_query_for_log` +
      `form_draft_library_{list,upsert,delete}` Tauri commands land + tested
- [ ] FormDraftLibrary persisted in SQLite per schema; round-trip tests
      green
- [ ] CSV export works end-to-end for ICS-309
- [ ] PDF export (if Task 0 picked a library) works end-to-end for
      ICS-309; deferred-with-bd otherwise
- [ ] Map widget (if Task 0 picked one) wired to PositionFormV2;
      text-input fallback otherwise
- [ ] Codex adrev: per-form rounds + draft-library round + full-diff round
- [ ] `pnpm vitest run` + `cargo test --lib` + `cargo clippy -D warnings`
      + `pnpm tsc --noEmit` all green
- [ ] PR opened with operator browser-smoke checklist
- [ ] bd `tuxlink-hnkn` updated with PR URL + Codex disposition; status
      in_progress until operator merge
