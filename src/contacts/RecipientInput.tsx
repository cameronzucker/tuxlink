// RecipientInput — chips + inline autocomplete for Compose To/Cc (Task A5).
//
// A CONTROLLED component over a semicolon-separated value string (same idiom as
// `splitAddrs`), so it round-trips through draft autosave unchanged. Internally
// it parses the value into display chips and renders an inline (NON-native)
// dropdown filtered by the in-progress input text.
//
// STYLE mirrors `src/search/ChipStrip.tsx` (chips) and
// `src/search/SearchDropdown.tsx` (row layout), but DIVERGES on keyboard
// handling per the adversarial fixes below.
//
// Adversarial-hardened behavior:
//   - H10 — keyboard is INPUT-SCOPED (`onKeyDown` on the <input>, NOT a global
//           `window` listener) so two instances (To + Cc) never fight over
//           arrows/Enter. ArrowDown/ArrowUp move a CLAMPED focus (no wrap),
//           starting at none-focused. Enter with a focused row adds that row;
//           Enter with NO focused row (or no matches) commits the trimmed input
//           text as a RAW chip (passthrough) — the divergence from
//           SearchDropdown, whose `focusIdx<0` early-return would swallow it.
//           Esc closes the dropdown; Backspace on an empty input removes the
//           last chip.
//   - H5  — a group chip serializes as the `group:<id>` sentinel; an
//           unresolvable group token renders a visibly-distinct "unknown-group"
//           chip rather than vanishing.
//   - M6  — a group chip displays `name · <resolvedCount>` where resolvedCount
//           equals the eventual send-time expansion length.
//   - Codex#12 — the dropdown offers a contact's primary callsign AND its email
//           and tactical (when present) as separately-pickable rows.
//   - NEVER a native `<select>` / `<datalist>` (those render DISABLED on
//           WebKitGTK).

import { useMemo, useRef, useState } from 'react';
import './RecipientInput.css';
import type { Contact, Group } from './types';
import {
  formatChips,
  matchRecipients,
  parseChips,
  resolveGroupMemberCount,
  type Chip,
  type MatchRow,
} from './recipients';

export interface RecipientInputProps {
  /// DOM id for the input (so a <label htmlFor> can target it).
  id: string;
  /// Semicolon-separated recipient tokens (group chips use the `group:<id>`
  /// sentinel). The single source of truth — parsed to chips for display.
  value: string;
  /// Emits the new semicolon-separated value on any chip add/remove.
  onChange: (value: string) => void;
  /// Address book + groups (the caller passes these from `useContacts`).
  contacts: Contact[];
  groups: Group[];
  placeholder?: string;
  'aria-label'?: string;
}

export function RecipientInput(props: RecipientInputProps) {
  const { id, value, onChange, contacts, groups, placeholder } = props;
  const [text, setText] = useState('');
  // -1 = no row focused (the default — Enter then commits raw text, H10).
  const [focusIdx, setFocusIdx] = useState(-1);
  const inputRef = useRef<HTMLInputElement>(null);

  const chips = useMemo(() => parseChips(value, groups), [value, groups]);
  const rows = useMemo(
    () => matchRecipients(text, contacts, groups),
    [text, contacts, groups],
  );
  const dropdownOpen = text.trim().length > 0 && rows.length > 0;

  const commitChips = (next: Chip[]) => {
    onChange(formatChips(next));
  };

  /// Append a raw token (a typed recipient or a picked alternate address).
  const addRawToken = (token: string) => {
    const t = token.trim();
    if (!t) return;
    commitChips([...chips, { kind: 'raw', token: t }]);
    setText('');
    setFocusIdx(-1);
  };

  /// Append a group chip via its `group:<id>` sentinel.
  const addGroupToken = (token: string, group: Group) => {
    commitChips([...chips, { kind: 'group', token, group }]);
    setText('');
    setFocusIdx(-1);
  };

  /// Commit a chosen dropdown row (group sentinel or raw alternate).
  const selectRow = (row: MatchRow) => {
    if (row.kind === 'group') {
      const g = groups.find((x) => `group:${x.id}` === row.insert);
      if (g) addGroupToken(row.insert, g);
      else addRawToken(row.insert);
    } else {
      addRawToken(row.insert);
    }
  };

  const removeChipAt = (idx: number) => {
    commitChips(chips.filter((_, i) => i !== idx));
    inputRef.current?.focus();
  };

  // H10 — INPUT-SCOPED key handling (NOT a window listener).
  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown') {
      if (!dropdownOpen) return;
      e.preventDefault();
      setFocusIdx((i) => Math.min(i + 1, rows.length - 1)); // clamp, no wrap
    } else if (e.key === 'ArrowUp') {
      if (!dropdownOpen) return;
      e.preventDefault();
      setFocusIdx((i) => Math.max(i - 1, -1)); // clamp to none-focused, no wrap
    } else if (e.key === 'Enter') {
      e.preventDefault();
      // Enter with a focused row → add that row (DIVERGENCE: SearchDropdown
      // returns early for focusIdx<0; here that path commits the raw text).
      if (dropdownOpen && focusIdx >= 0 && focusIdx < rows.length) {
        selectRow(rows[focusIdx]);
      } else if (text.trim().length > 0) {
        addRawToken(text); // H10 passthrough — raw chip
      }
    } else if (e.key === 'Escape') {
      // Close the dropdown without committing (clear the in-progress text).
      e.preventDefault();
      setText('');
      setFocusIdx(-1);
    } else if (e.key === 'Backspace') {
      // Remove the last chip only when the input is empty.
      if (text.length === 0 && chips.length > 0) {
        e.preventDefault();
        removeChipAt(chips.length - 1);
      }
    }
  };

  const onChangeText = (e: React.ChangeEvent<HTMLInputElement>) => {
    setText(e.target.value);
    setFocusIdx(-1); // a new query resets focus to none (H10)
  };

  return (
    <div className="recipient-input" data-testid={`recipient-input-root-${id}`}>
      <div className="recipient-chips">
        {chips.map((chip, idx) => (
          <ChipView
            key={`${chip.token}-${idx}`}
            chip={chip}
            contacts={contacts}
            onRemove={() => removeChipAt(idx)}
          />
        ))}
        <input
          ref={inputRef}
          id={id}
          className="recipient-text"
          type="text"
          autoComplete="off"
          value={text}
          onChange={onChangeText}
          onKeyDown={onKeyDown}
          placeholder={chips.length === 0 ? placeholder : ''}
          aria-label={props['aria-label']}
          data-testid={`recipient-input-${id}`}
        />
      </div>

      {dropdownOpen && (
        <div className="recipient-dropdown" data-testid={`recipient-dropdown-${id}`}>
          {rows.map((row, i) => (
            <div
              key={row.key}
              className={`recipient-row${focusIdx === i ? ' focused' : ''} kind-${row.kind}`}
              data-testid={`recipient-row-${row.insert}`}
              // mouseDown (not click) so the input doesn't lose focus first.
              onMouseDown={(e) => {
                e.preventDefault();
                selectRow(row);
              }}
              onMouseEnter={() => setFocusIdx(i)}
            >
              <span className="recipient-row-label">{row.label}</span>
              <span className="recipient-row-sub">{row.sublabel}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/// A single rendered chip. Group chips show `name · <resolvedCount>` (M6);
/// unknown-group chips show their raw token with a distinct class (H5).
function ChipView({
  chip,
  contacts,
  onRemove,
}: {
  chip: Chip;
  contacts: Contact[];
  onRemove: () => void;
}) {
  let label: string;
  let className = 'recipient-chip';

  if (chip.kind === 'group' && chip.group) {
    const count = resolveGroupMemberCount(chip.group, contacts);
    label = `${chip.group.name} · ${count}`;
    className += ' is-group';
  } else if (chip.kind === 'unknown-group') {
    label = `${chip.token} (unknown group)`;
    className += ' is-unknown-group';
  } else {
    label = chip.token;
    className += ' is-raw';
  }

  return (
    <span className={className} data-testid={`recipient-chip-${chip.token}`}>
      <span className="recipient-chip-label">{label}</span>
      <button
        type="button"
        className="recipient-chip-x"
        aria-label={`Remove ${label}`}
        data-testid={`recipient-chip-x-${chip.token}`}
        // mouseDown so the chip removes before the input blur fires.
        onMouseDown={(e) => {
          e.preventDefault();
          onRemove();
        }}
      >
        ×
      </button>
    </span>
  );
}
