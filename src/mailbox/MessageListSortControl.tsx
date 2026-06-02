// Sort selector rendered above the virtualized message list.
//
// bd issue: tuxlink-2x0l (MessageList sort UI — Phase 2 of mailbox-sort).
// Operator iteration on PR #244 (2026-06-02): the native <select> inherited
// OS chrome on WebKitGTK and read as "disabled" against the dark theme.
// Replaced with a Radix DropdownMenu popup — fully themable end to end, and
// the two-radio-group split (key + direction) lets the direction labels
// adapt to the active key (Newest/Oldest, A→Z, Smallest/Largest).

import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import {
  type SortKey,
  type SortDirection,
  type SortState,
  SORT_KEY_OPTIONS,
  DIRECTION_LABELS,
} from './messageSort';

export interface MessageListSortControlProps {
  value: SortState;
  onChange: (state: SortState) => void;
}

/// Inline sort-icon glyph (two-headed vertical arrow). Drawn here rather than
/// pulled from a library so it inherits `currentColor` and stays under
/// theme-token control. 14px viewport matches the rows-pane header type size.
function SortIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      aria-hidden="true"
      focusable="false"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M4 3 v8" />
      <path d="M2 5 L4 3 L6 5" />
      <path d="M10 11 v-8" />
      <path d="M8 9 L10 11 L12 9" />
    </svg>
  );
}

export function MessageListSortControl({ value, onChange }: MessageListSortControlProps) {
  const directionLabels = DIRECTION_LABELS[value.key];
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          className="message-list-sort-trigger"
          data-testid="message-list-sort-trigger"
          aria-label="Sort messages"
          title="Sort messages"
        >
          <SortIcon />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          className="message-list-sort-menu"
          data-testid="message-list-sort-menu"
          align="end"
          sideOffset={4}
          collisionPadding={8}
        >
          <DropdownMenu.Label className="message-list-sort-section-label">Sort by</DropdownMenu.Label>
          <DropdownMenu.RadioGroup
            value={value.key}
            onValueChange={(v) => onChange({ key: v as SortKey, direction: value.direction })}
          >
            {SORT_KEY_OPTIONS.map((opt) => (
              <DropdownMenu.RadioItem
                key={opt.id}
                value={opt.id}
                className="message-list-sort-item"
                data-testid={`message-list-sort-key-${opt.id}`}
              >
                <DropdownMenu.ItemIndicator className="message-list-sort-indicator">●</DropdownMenu.ItemIndicator>
                <span className="message-list-sort-item-label">{opt.label}</span>
              </DropdownMenu.RadioItem>
            ))}
          </DropdownMenu.RadioGroup>
          <DropdownMenu.Separator className="message-list-sort-separator" />
          <DropdownMenu.RadioGroup
            value={value.direction}
            onValueChange={(v) => onChange({ key: value.key, direction: v as SortDirection })}
          >
            <DropdownMenu.RadioItem
              value="desc"
              className="message-list-sort-item"
              data-testid="message-list-sort-direction-desc"
            >
              <DropdownMenu.ItemIndicator className="message-list-sort-indicator">●</DropdownMenu.ItemIndicator>
              <span className="message-list-sort-item-label">{directionLabels.desc}</span>
            </DropdownMenu.RadioItem>
            <DropdownMenu.RadioItem
              value="asc"
              className="message-list-sort-item"
              data-testid="message-list-sort-direction-asc"
            >
              <DropdownMenu.ItemIndicator className="message-list-sort-indicator">●</DropdownMenu.ItemIndicator>
              <span className="message-list-sort-item-label">{directionLabels.asc}</span>
            </DropdownMenu.RadioItem>
          </DropdownMenu.RadioGroup>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
