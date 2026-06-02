// Sort selector rendered above the virtualized message list.
//
// bd issue: tuxlink-2x0l (MessageList sort UI — Phase 2 of mailbox-sort).
// Lives above MessageList rather than inside them because rows are 3-line
// grids, not tabular — column headers don't fit. Native <select> for
// accessibility + platform-conventional behavior on Tauri/WebKitGTK.

import React from 'react';
import { type SortMode, SORT_OPTIONS } from './messageSort';

export interface MessageListSortControlProps {
  value: SortMode;
  onChange: (mode: SortMode) => void;
}

export function MessageListSortControl({ value, onChange }: MessageListSortControlProps) {
  const selectId = React.useId();
  return (
    <div className="message-list-sort" data-testid="message-list-sort">
      <label htmlFor={selectId} className="message-list-sort-label">
        Sort
      </label>
      <select
        id={selectId}
        className="message-list-sort-select"
        data-testid="message-list-sort-select"
        value={value}
        onChange={(e) => onChange(e.target.value as SortMode)}
      >
        {SORT_OPTIONS.map((opt) => (
          <option key={opt.id} value={opt.id}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
}
