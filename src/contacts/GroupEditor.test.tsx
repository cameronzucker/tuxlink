// GroupEditor tests (Task A8b).
//
// Covers the create/edit-group + manage-members surface:
//   - Name is REQUIRED (Save disabled until non-empty).
//   - Add a member from a contact → GroupMember{type:'contact', contact_id}
//     (the Locked rule: store contact_id when added from a contact).
//   - Add a member by typing a raw callsign → GroupMember{type:'raw', callsign}.
//   - Remove a member.
//   - A deleted-contact member (contact_id no longer resolves) renders distinctly
//     and is removable — never silently absent, never a crash (M6).
//   - Save calls group_upsert with the EXACT assembled members.
//   - Delete (edit mode only) calls group_delete.
//   - Cancel discards.
//
// The member list is a plain list (no react-virtuoso), so no virtuoso mock is
// needed here.

import { describe, it, expect, vi } from 'vitest';
import type { Mock } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { GroupEditor, emptyGroup } from './GroupEditor';
import type { Contact, Group, GroupMember } from './types';

const NOW = '2026-06-07T00:00:00Z';

const ALICE: Contact = {
  id: 'c-alice',
  name: 'Alice Operator',
  callsign: 'W6ABC',
  email: 'w6abc@winlink.org',
  created_at: NOW,
  updated_at: NOW,
};
const BOB: Contact = {
  id: 'c-bob',
  name: 'Bob Relay',
  callsign: 'KE7XYZ',
  created_at: NOW,
  updated_at: NOW,
};

const CONTACTS = [ALICE, BOB];

function renderEditor(opts: { group?: Group; contacts?: Contact[] } = {}) {
  const onSave: Mock = vi.fn();
  const onDelete: Mock = vi.fn();
  const onCancel: Mock = vi.fn();
  render(
    <GroupEditor
      group={opts.group ?? emptyGroup()}
      contacts={opts.contacts ?? CONTACTS}
      onSave={onSave}
      onDelete={onDelete}
      onCancel={onCancel}
    />,
  );
  return { onSave, onDelete, onCancel };
}

/// The group passed to the single onSave (→ group_upsert) call.
function savedGroup(onSave: Mock): Group {
  const calls = onSave.mock.calls;
  expect(calls.length).toBeGreaterThan(0);
  return calls[calls.length - 1][0] as Group;
}

/// The assembled members from the onSave call.
function savedMembers(onSave: Mock): GroupMember[] {
  return savedGroup(onSave).members;
}

describe('<GroupEditor> — name required', () => {
  it('disables Save until a non-empty name is entered', () => {
    renderEditor();
    const save = screen.getByTestId('group-editor-save');
    expect(save).toBeDisabled();

    fireEvent.change(screen.getByTestId('group-editor-name'), {
      target: { value: 'ARES — Multnomah Co.' },
    });
    expect(save).toBeEnabled();

    // Whitespace-only does NOT satisfy the requirement.
    fireEvent.change(screen.getByTestId('group-editor-name'), {
      target: { value: '   ' },
    });
    expect(save).toBeDisabled();
  });
});

describe('<GroupEditor> — add members', () => {
  it('adds a contact member as {type:"contact", contact_id} (not raw)', () => {
    const { onSave } = renderEditor();
    fireEvent.change(screen.getByTestId('group-editor-name'), {
      target: { value: 'Net' },
    });

    // Search a contact, pick it from the dropdown.
    fireEvent.change(screen.getByTestId('group-member-search'), {
      target: { value: 'alice' },
    });
    fireEvent.click(screen.getByTestId('member-option-c-alice'));

    // The member row shows the contact's display form.
    const list = screen.getByTestId('group-member-list');
    expect(within(list).getByText(/Alice Operator/)).toBeInTheDocument();
    expect(within(list).getByText('W6ABC')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('group-editor-save'));
    expect(savedMembers(onSave)).toEqual([
      { type: 'contact', contact_id: 'c-alice' },
    ]);
  });

  it('adds a typed raw callsign as {type:"raw", callsign}', () => {
    const { onSave } = renderEditor();
    fireEvent.change(screen.getByTestId('group-editor-name'), {
      target: { value: 'Net' },
    });

    const search = screen.getByTestId('group-member-search');
    // A callsign with NO matching contact, committed with Enter, becomes a raw member.
    fireEvent.change(search, { target: { value: 'N0CALL-7' } });
    fireEvent.keyDown(search, { key: 'Enter' });

    const list = screen.getByTestId('group-member-list');
    expect(within(list).getByText('N0CALL-7')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('group-editor-save'));
    expect(savedMembers(onSave)).toEqual([
      { type: 'raw', callsign: 'N0CALL-7' },
    ]);
  });

  it('excludes an already-added contact from the picker (no duplicate member)', () => {
    const { onSave } = renderEditor();
    fireEvent.change(screen.getByTestId('group-editor-name'), {
      target: { value: 'Net' },
    });

    fireEvent.change(screen.getByTestId('group-member-search'), { target: { value: 'alice' } });
    fireEvent.click(screen.getByTestId('member-option-c-alice'));

    // Searching the same contact again offers NO option — she is already a member.
    fireEvent.change(screen.getByTestId('group-member-search'), { target: { value: 'alice' } });
    expect(screen.queryByTestId('member-option-c-alice')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('group-editor-save'));
    expect(savedMembers(onSave)).toEqual([
      { type: 'contact', contact_id: 'c-alice' },
    ]);
  });
});

describe('<GroupEditor> — remove members', () => {
  it('removes a member when its X is clicked', () => {
    const group: Group = {
      id: 'g-1',
      name: 'Net',
      members: [
        { type: 'contact', contact_id: 'c-alice' },
        { type: 'raw', callsign: 'N0CALL' },
      ],
      created_at: NOW,
      updated_at: NOW,
    };
    const { onSave } = renderEditor({ group });

    // Remove the Alice member.
    fireEvent.click(screen.getByTestId('member-remove-contact:c-alice'));

    fireEvent.click(screen.getByTestId('group-editor-save'));
    expect(savedMembers(onSave)).toEqual([{ type: 'raw', callsign: 'N0CALL' }]);
  });
});

describe('<GroupEditor> — deleted-contact member', () => {
  it('renders a deleted-contact member distinctly and allows removing it (no crash)', () => {
    const group: Group = {
      id: 'g-1',
      name: 'Net',
      // c-ghost is NOT in CONTACTS — it was deleted.
      members: [
        { type: 'contact', contact_id: 'c-ghost' },
        { type: 'contact', contact_id: 'c-alice' },
      ],
      created_at: NOW,
      updated_at: NOW,
    };
    const { onSave } = renderEditor({ group });

    const list = screen.getByTestId('group-member-list');
    // The deleted member is shown distinctly (not silently absent).
    const ghostRow = within(list).getByTestId('member-row-contact:c-ghost');
    expect(ghostRow).toBeInTheDocument();
    expect(ghostRow).toHaveClass('group-member-row--unknown');
    expect(within(ghostRow).getByText(/unknown.*removed contact/i)).toBeInTheDocument();

    // It is removable.
    fireEvent.click(screen.getByTestId('member-remove-contact:c-ghost'));
    expect(within(list).queryByTestId('member-row-contact:c-ghost')).not.toBeInTheDocument();

    // Surviving member persists; the dropped ghost is gone.
    fireEvent.click(screen.getByTestId('group-editor-save'));
    expect(savedMembers(onSave)).toEqual([
      { type: 'contact', contact_id: 'c-alice' },
    ]);
  });
});

describe('<GroupEditor> — save / delete / cancel', () => {
  it('Save calls onSave with the group carrying name + the exact members', () => {
    const existing: Group = {
      id: 'g-1',
      name: 'Old name',
      members: [{ type: 'raw', callsign: 'W1AW' }],
      created_at: NOW,
      updated_at: NOW,
    };
    const { onSave } = renderEditor({ group: existing });

    fireEvent.change(screen.getByTestId('group-editor-name'), {
      target: { value: 'New name' },
    });
    fireEvent.change(screen.getByTestId('group-member-search'), { target: { value: 'bob' } });
    fireEvent.click(screen.getByTestId('member-option-c-bob'));
    fireEvent.click(screen.getByTestId('group-editor-save'));

    const saved = savedGroup(onSave);
    expect(saved.id).toBe('g-1');
    expect(saved.name).toBe('New name');
    expect(saved.members).toEqual([
      { type: 'raw', callsign: 'W1AW' },
      { type: 'contact', contact_id: 'c-bob' },
    ]);
  });

  it('Delete (edit mode) calls onDelete with the group id', () => {
    const existing: Group = {
      id: 'g-1',
      name: 'Net',
      members: [],
      created_at: NOW,
      updated_at: NOW,
    };
    const { onDelete } = renderEditor({ group: existing });

    fireEvent.click(screen.getByTestId('group-editor-delete'));
    expect(onDelete).toHaveBeenCalledWith('g-1');
  });

  it('does not show Delete for a brand-new group', () => {
    renderEditor();
    expect(screen.queryByTestId('group-editor-delete')).not.toBeInTheDocument();
  });

  it('Cancel discards via onCancel', () => {
    const { onCancel } = renderEditor();
    fireEvent.click(screen.getByTestId('group-editor-cancel'));
    expect(onCancel).toHaveBeenCalled();
  });
});
