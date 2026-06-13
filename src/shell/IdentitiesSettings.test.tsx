import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';

import type { IdentityListDto } from './identityTypes';

// Mock the hook boundary so the component test exercises rendering + wiring
// without a real QueryClient or the Tauri `invoke` shim.
const addFullMutate = vi.fn();
const addTacticalMutate = vi.fn();
const removeMutate = vi.fn();
let listState: {
  data: IdentityListDto | undefined;
  isLoading: boolean;
  isError: boolean;
};

vi.mock('./useIdentities', () => ({
  useIdentityList: () => listState,
  useAddFullIdentity: () => ({ mutateAsync: addFullMutate, isPending: false }),
  useAddTactical: () => ({ mutateAsync: addTacticalMutate, isPending: false }),
  useRemoveIdentity: () => ({ mutateAsync: removeMutate, isPending: false }),
}));

import { IdentitiesSettings } from './IdentitiesSettings';

function listWith(over: Partial<IdentityListDto> = {}): IdentityListDto {
  return {
    full: [
      {
        callsign: 'W1ABC',
        label: 'Personal',
        has_cms_account: true,
        cms_registered: true,
        needs_auth: false,
      },
    ],
    tactical: [{ label: 'EOC-3', parent: 'W1ABC', cms_badge: 'registered' }],
    last_selected: null,
    ...over,
  };
}

beforeEach(() => {
  addFullMutate.mockReset().mockResolvedValue(undefined);
  addTacticalMutate.mockReset().mockResolvedValue(undefined);
  removeMutate.mockReset().mockResolvedValue(undefined);
  listState = { data: listWith(), isLoading: false, isError: false };
});

describe('IdentitiesSettings — listing', () => {
  it('renders each FULL with its nested tacticals', () => {
    render(<IdentitiesSettings />);
    const fullRow = screen.getByTestId('identity-row-full-W1ABC');
    const tacticalRow = screen.getByTestId('identity-row-tactical-EOC-3');
    expect(fullRow).toBeInTheDocument();
    expect(tacticalRow).toBeInTheDocument();
    // Scope text lookups to the row to avoid matching the <option> in the
    // tactical-parent <select> (which also renders the callsign text).
    expect(within(fullRow).getByText('W1ABC')).toBeInTheDocument();
    expect(within(tacticalRow).getByText('EOC-3')).toBeInTheDocument();
  });

  it('nests a tactical only under its parent FULL', () => {
    listState = {
      data: listWith({
        full: [
          { callsign: 'W1ABC', label: null, has_cms_account: true, cms_registered: true, needs_auth: false },
          { callsign: 'W7XYZ', label: null, has_cms_account: false, cms_registered: false, needs_auth: true },
        ],
        tactical: [{ label: 'EOC-3', parent: 'W1ABC', cms_badge: 'unknown' }],
      }),
      isLoading: false,
      isError: false,
    };
    render(<IdentitiesSettings />);
    const w1abc = screen.getByTestId('identity-row-full-W1ABC');
    const w7xyz = screen.getByTestId('identity-row-full-W7XYZ');
    expect(w1abc).toContainElement(screen.getByTestId('identity-row-tactical-EOC-3'));
    expect(w7xyz).not.toContainElement(screen.queryByTestId('identity-row-tactical-EOC-3'));
  });

  it('shows the empty state + add-FULL form when there are no identities', () => {
    listState = { data: listWith({ full: [], tactical: [] }), isLoading: false, isError: false };
    render(<IdentitiesSettings />);
    expect(screen.getByText(/no identities configured/i)).toBeInTheDocument();
    expect(screen.getByTestId('identity-add-full-form')).toBeInTheDocument();
  });
});

describe('IdentitiesSettings — add FULL', () => {
  it('submits callsign + password + label + hasCmsAccount (checkbox default checked)', async () => {
    render(<IdentitiesSettings />);
    fireEvent.change(screen.getByTestId('identity-add-full-callsign'), {
      target: { value: 'W7XYZ' },
    });
    fireEvent.change(screen.getByTestId('identity-add-full-password'), {
      target: { value: 'pw' },
    });
    fireEvent.click(screen.getByTestId('identity-add-full-submit'));

    await waitFor(() =>
      expect(addFullMutate).toHaveBeenCalledWith({
        callsign: 'W7XYZ',
        label: null,
        hasCmsAccount: true,
        password: 'pw',
      }),
    );
  });

  it('rejects an empty callsign with an inline error and does not call the mutation', async () => {
    render(<IdentitiesSettings />);
    fireEvent.change(screen.getByTestId('identity-add-full-password'), {
      target: { value: 'pw' },
    });
    fireEvent.click(screen.getByTestId('identity-add-full-submit'));

    expect(await screen.findByTestId('identities-error')).toBeInTheDocument();
    expect(addFullMutate).not.toHaveBeenCalled();
  });

  it('surfaces a mutation rejection via parseIdentityError in the alert', async () => {
    addFullMutate.mockRejectedValueOnce({ kind: 'Rejected', detail: 'duplicate callsign' });
    render(<IdentitiesSettings />);
    fireEvent.change(screen.getByTestId('identity-add-full-callsign'), {
      target: { value: 'W7XYZ' },
    });
    fireEvent.change(screen.getByTestId('identity-add-full-password'), {
      target: { value: 'pw' },
    });
    fireEvent.click(screen.getByTestId('identity-add-full-submit'));

    const alert = await screen.findByTestId('identities-error');
    expect(alert).toHaveTextContent('duplicate callsign');
  });

  it('passes hasCmsAccount=false when the checkbox is unchecked', async () => {
    render(<IdentitiesSettings />);
    fireEvent.change(screen.getByTestId('identity-add-full-callsign'), {
      target: { value: 'W7XYZ' },
    });
    fireEvent.change(screen.getByTestId('identity-add-full-password'), {
      target: { value: 'pw' },
    });
    fireEvent.click(screen.getByLabelText(/has CMS account/i));
    fireEvent.click(screen.getByTestId('identity-add-full-submit'));

    await waitFor(() =>
      expect(addFullMutate).toHaveBeenCalledWith(
        expect.objectContaining({ hasCmsAccount: false }),
      ),
    );
  });
});

describe('IdentitiesSettings — add tactical', () => {
  it('submits label + selected parent', async () => {
    render(<IdentitiesSettings />);
    fireEvent.change(screen.getByTestId('identity-add-tactical-label'), {
      target: { value: 'EOC-9' },
    });
    fireEvent.change(screen.getByTestId('identity-add-tactical-parent'), {
      target: { value: 'W1ABC' },
    });
    fireEvent.click(screen.getByTestId('identity-add-tactical-submit'));

    await waitFor(() =>
      expect(addTacticalMutate).toHaveBeenCalledWith({ label: 'EOC-9', parent: 'W1ABC' }),
    );
  });

  it('does not render the add-tactical form when there is no FULL', () => {
    listState = { data: listWith({ full: [], tactical: [] }), isLoading: false, isError: false };
    render(<IdentitiesSettings />);
    expect(screen.queryByTestId('identity-add-tactical-form')).not.toBeInTheDocument();
  });
});

describe('IdentitiesSettings — remove', () => {
  it('removes a FULL (kind full) after inline confirm', async () => {
    listState = {
      data: listWith({ tactical: [] }),
      isLoading: false,
      isError: false,
    };
    render(<IdentitiesSettings />);
    fireEvent.click(screen.getByTestId('identity-remove-W1ABC'));
    // Inline confirm step, then a confirm button.
    fireEvent.click(await screen.findByTestId('identity-remove-W1ABC-confirm'));

    await waitFor(() =>
      expect(removeMutate).toHaveBeenCalledWith({ kind: 'full', callsign: 'W1ABC' }),
    );
  });

  it('removes a tactical (kind tactical) after inline confirm', async () => {
    render(<IdentitiesSettings />);
    fireEvent.click(screen.getByTestId('identity-remove-EOC-3'));
    fireEvent.click(await screen.findByTestId('identity-remove-EOC-3-confirm'));

    await waitFor(() =>
      expect(removeMutate).toHaveBeenCalledWith({ kind: 'tactical', label: 'EOC-3' }),
    );
  });

  it('surfaces RemoveHasTacticals in the alert', async () => {
    removeMutate.mockRejectedValueOnce({
      kind: 'Internal',
      detail: { detail: 'remove its tactical labels first' },
    });
    listState = { data: listWith({ tactical: [] }), isLoading: false, isError: false };
    render(<IdentitiesSettings />);
    fireEvent.click(screen.getByTestId('identity-remove-W1ABC'));
    fireEvent.click(await screen.findByTestId('identity-remove-W1ABC-confirm'));

    const alert = await screen.findByTestId('identities-error');
    expect(alert).toHaveTextContent(/remove its tactical labels first/i);
  });
});
