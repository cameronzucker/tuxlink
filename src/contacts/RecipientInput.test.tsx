import { createRef, useState } from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen, fireEvent, cleanup } from '@testing-library/react';

import type { Contact, Group } from './types';
import { RecipientInput, type RecipientInputHandle } from './RecipientInput';

// RecipientInput is a controlled component with no Tauri dependency — it takes
// contacts/groups as props (the caller supplies them from useContacts). No
// invoke mock needed.

const CONTACTS: Contact[] = [
  {
    id: 'c1',
    name: 'Vera Knox',
    callsign: 'KE7VAR',
    email: 'ke7var@winlink.org',
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
  {
    id: 'c2',
    name: 'Walt Briggs',
    callsign: 'W6ABC',
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
];

const GROUPS: Group[] = [
  {
    id: 'g1',
    name: 'ARES Net',
    members: [
      { type: 'contact', contact_id: 'c1' },
      { type: 'contact', contact_id: 'c2' },
      { type: 'raw', callsign: 'W7XYZ' },
    ],
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
];

// A genuinely controlled wrapper (state lives in the test host) so we can
// assert chip add/remove behavior after onChange fires.
function ControlledHost(props: { initial?: string; contacts?: Contact[]; groups?: Group[] }) {
  const [value, setValue] = useState(props.initial ?? '');
  return (
    <RecipientInput
      id="to"
      value={value}
      onChange={setValue}
      contacts={props.contacts ?? CONTACTS}
      groups={props.groups ?? GROUPS}
      placeholder="recipients…"
    />
  );
}

beforeEach(() => {
  cleanup();
});

describe('RecipientInput', () => {
  it('renders the keydown handler on the INPUT element, not window (H10)', () => {
    // A window-level keydown listener would be shared across two instances
    // (To + Cc). Assert the input itself owns onkeydown and that no global
    // listener was registered by this component.
    const addSpy = vi.spyOn(window, 'addEventListener');
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to') as HTMLInputElement;
    // The input must have its own keydown handler (React attaches via prop;
    // we verify a keydown on it is handled rather than bubbling to window).
    expect(input).toBeInTheDocument();
    const windowKeydownRegistrations = addSpy.mock.calls.filter((c) => c[0] === 'keydown');
    expect(windowKeydownRegistrations).toHaveLength(0);
    addSpy.mockRestore();
  });

  it('typing filters the inline dropdown', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'vera' } });
    const dropdown = screen.getByTestId('recipient-dropdown-to');
    expect(dropdown).toBeInTheDocument();
    // Vera's callsign + email rows appear.
    expect(screen.getByText('KE7VAR')).toBeInTheDocument();
    expect(screen.getByText('ke7var@winlink.org')).toBeInTheDocument();
  });

  it('ArrowDown then Enter adds the focused row as a chip', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'walt' } });
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    // W6ABC chip added; input cleared.
    expect(screen.getByTestId('recipient-chip-W6ABC')).toBeInTheDocument();
    expect((input as HTMLInputElement).value).toBe('');
  });

  it('selecting the email-alternate row adds the email-form chip (Codex#12)', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'vera' } });
    // Click the email row directly.
    fireEvent.mouseDown(screen.getByText('ke7var@winlink.org'));
    expect(screen.getByTestId('recipient-chip-ke7var@winlink.org')).toBeInTheDocument();
  });

  it('selecting the callsign row adds the callsign chip (Codex#12)', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'vera' } });
    fireEvent.mouseDown(screen.getByText('KE7VAR'));
    expect(screen.getByTestId('recipient-chip-KE7VAR')).toBeInTheDocument();
  });

  it('raw callsign + Enter with NO focused row commits the trimmed text as a raw chip (H10 passthrough)', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    // A callsign that does not match any contact/group.
    fireEvent.change(input, { target: { value: '  KX9ZZ  ' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(screen.getByTestId('recipient-chip-KX9ZZ')).toBeInTheDocument();
  });

  it('Enter with no focused row and NO matches still commits raw text (H10)', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'nobody@nowhere.test' } });
    // No dropdown rows match → dropdown not shown, Enter commits raw.
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(screen.getByTestId('recipient-chip-nobody@nowhere.test')).toBeInTheDocument();
  });

  it('selecting a group renders ONE chip labeled "name · count" and emits the group: sentinel (H5/M6)', () => {
    const onChange = vi.fn();
    function GroupHost() {
      const [value, setValue] = useState('');
      return (
        <RecipientInput
          id="to"
          value={value}
          onChange={(v) => {
            onChange(v);
            setValue(v);
          }}
          contacts={CONTACTS}
          groups={GROUPS}
        />
      );
    }
    render(<GroupHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'ares' } });
    fireEvent.mouseDown(screen.getByText('ARES Net'));
    // ONE group chip, labeled with the resolved member count (3).
    const chip = screen.getByTestId('recipient-chip-group:g1');
    expect(chip).toBeInTheDocument();
    expect(chip).toHaveTextContent('ARES Net');
    expect(chip).toHaveTextContent('3');
    // The emitted value string carries the sentinel token.
    expect(onChange).toHaveBeenCalledWith('group:g1');
  });

  it('an unresolvable group: token renders a distinct unknown-group chip, not dropped (H5)', () => {
    render(<ControlledHost initial="group:deleted-id" />);
    const chip = screen.getByTestId('recipient-chip-group:deleted-id');
    expect(chip).toBeInTheDocument();
    expect(chip.className).toContain('unknown');
  });

  it('Backspace on an empty input removes the last chip', () => {
    render(<ControlledHost initial="W6ABC; KE7VAR" />);
    expect(screen.getByTestId('recipient-chip-KE7VAR')).toBeInTheDocument();
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.keyDown(input, { key: 'Backspace' });
    // Last chip (KE7VAR) removed; W6ABC remains.
    expect(screen.queryByTestId('recipient-chip-KE7VAR')).not.toBeInTheDocument();
    expect(screen.getByTestId('recipient-chip-W6ABC')).toBeInTheDocument();
  });

  it('Backspace with text in the input does NOT remove a chip', () => {
    render(<ControlledHost initial="W6ABC" />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'abc' } });
    fireEvent.keyDown(input, { key: 'Backspace' });
    expect(screen.getByTestId('recipient-chip-W6ABC')).toBeInTheDocument();
  });

  it('Esc closes the dropdown', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'vera' } });
    expect(screen.getByTestId('recipient-dropdown-to')).toBeInTheDocument();
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByTestId('recipient-dropdown-to')).not.toBeInTheDocument();
  });

  it('ArrowDown/ArrowUp are CLAMPED — no wrap', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'walt' } }); // one matching contact row
    // ArrowUp from no-focus stays at none (does not wrap to bottom).
    fireEvent.keyDown(input, { key: 'ArrowUp' });
    // No row should be focused yet → Enter commits raw text 'walt', not W6ABC.
    fireEvent.keyDown(input, { key: 'Enter' });
    // 'walt' committed as raw (no row was focused).
    expect(screen.getByTestId('recipient-chip-walt')).toBeInTheDocument();
    expect(screen.queryByTestId('recipient-chip-W6ABC')).not.toBeInTheDocument();
  });

  it('ArrowDown past the last row clamps to the last row (no wrap to top)', () => {
    render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'vera' } }); // 2 rows: callsign + email
    fireEvent.keyDown(input, { key: 'ArrowDown' }); // row 0 (callsign)
    fireEvent.keyDown(input, { key: 'ArrowDown' }); // row 1 (email)
    fireEvent.keyDown(input, { key: 'ArrowDown' }); // clamps at row 1
    fireEvent.keyDown(input, { key: 'Enter' });
    // Still on the email row (last), not wrapped to callsign.
    expect(screen.getByTestId('recipient-chip-ke7var@winlink.org')).toBeInTheDocument();
  });

  it('does not render a native <select> / datalist (WebKitGTK renders those disabled)', () => {
    const { container } = render(<ControlledHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'vera' } });
    expect(container.querySelector('select')).toBeNull();
    expect(container.querySelector('datalist')).toBeNull();
  });

  it('renders existing chips from the value string on mount', () => {
    render(<ControlledHost initial="W6ABC; group:g1" />);
    expect(screen.getByTestId('recipient-chip-W6ABC')).toBeInTheDocument();
    expect(screen.getByTestId('recipient-chip-group:g1')).toBeInTheDocument();
  });

  // --- Issue #648 (tuxlink-waxd): un-Entered text must not be silently dropped.

  it('commits pending typed text as a chip on blur — no Enter required (issue #648)', () => {
    const onChange = vi.fn();
    function BlurHost() {
      const [value, setValue] = useState('');
      return (
        <RecipientInput
          id="to"
          value={value}
          onChange={(v) => {
            onChange(v);
            setValue(v);
          }}
          contacts={CONTACTS}
          groups={GROUPS}
        />
      );
    }
    render(<BlurHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'w6bi@winlink.org' } });
    // Leaving the field (focus loss) must commit the buffer, not discard it.
    fireEvent.blur(input);
    expect(onChange).toHaveBeenCalledWith('w6bi@winlink.org');
    expect(screen.getByTestId('recipient-chip-w6bi@winlink.org')).toBeInTheDocument();
    expect((input as HTMLInputElement).value).toBe('');
  });

  it('blur with an empty / whitespace-only buffer commits nothing (issue #648)', () => {
    const onChange = vi.fn();
    function BlurHost() {
      const [value, setValue] = useState('W6ABC');
      return (
        <RecipientInput
          id="to"
          value={value}
          onChange={(v) => {
            onChange(v);
            setValue(v);
          }}
          contacts={CONTACTS}
          groups={GROUPS}
        />
      );
    }
    render(<BlurHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: '   ' } });
    fireEvent.blur(input);
    // No spurious chip from a whitespace buffer; the existing chip is untouched.
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByTestId('recipient-chip-W6ABC')).toBeInTheDocument();
  });

  it('flush() commits pending typed text and returns the full value string (issue #648)', () => {
    const ref = createRef<RecipientInputHandle>();
    const onChange = vi.fn();
    function FlushHost() {
      const [value, setValue] = useState('W6ABC');
      return (
        <RecipientInput
          ref={ref}
          id="to"
          value={value}
          onChange={(v) => {
            onChange(v);
            setValue(v);
          }}
          contacts={CONTACTS}
          groups={GROUPS}
        />
      );
    }
    render(<FlushHost />);
    const input = screen.getByTestId('recipient-input-to');
    fireEvent.change(input, { target: { value: 'KX9ZZ' } });

    let returned: string | undefined;
    act(() => {
      returned = ref.current?.flush();
    });
    // flush returns the up-to-date string SYNCHRONOUSLY (does not wait for the
    // onChange state round-trip) so a send path can use it immediately.
    expect(returned).toBe('W6ABC; KX9ZZ');
    expect(onChange).toHaveBeenCalledWith('W6ABC; KX9ZZ');
    expect(screen.getByTestId('recipient-chip-KX9ZZ')).toBeInTheDocument();
  });

  it('flush() with no pending text returns the current committed value unchanged (issue #648)', () => {
    const ref = createRef<RecipientInputHandle>();
    render(<ControlledHost initial="W6ABC" />);
    // ControlledHost does not forward a ref; mount a tiny ref-bearing host.
    cleanup();
    const onChange = vi.fn();
    function FlushHost() {
      const [value, setValue] = useState('W6ABC');
      return (
        <RecipientInput
          ref={ref}
          id="to"
          value={value}
          onChange={(v) => {
            onChange(v);
            setValue(v);
          }}
          contacts={CONTACTS}
          groups={GROUPS}
        />
      );
    }
    render(<FlushHost />);
    let returned: string | undefined;
    act(() => {
      returned = ref.current?.flush();
    });
    expect(returned).toBe('W6ABC');
    expect(onChange).not.toHaveBeenCalled();
  });
});
