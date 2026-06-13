// react-query hooks over the identity Tauri commands.
//
// Phase 7 (tuxlink-noa0). Wraps `identity_list` / `identity_active` /
// `identity_authenticate` from `src-tauri/src/identity/commands.rs`.
//
// Switching identity IS authenticating — there is no `identity_switch` command.
// `useIdentitySwitch` calls `identity_authenticate(callsign, credential,
// tactical_label?)`. Tauri renames snake_case command params to camelCase on the
// JS side, so the Rust `tactical_label` arg is passed as `tacticalLabel` (matches
// the codebase convention: `draftId`→`draft_id`, `parentSlug`→`parent_slug`,
// `orderedIds`→`ordered_ids`).

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { ActiveIdentityDto, IdentityListDto } from './identityTypes';

export const IDENTITY_LIST_QUERY_KEY = ['identity_list'] as const;
export const IDENTITY_ACTIVE_QUERY_KEY = ['identity_active'] as const;

/// The full identity list (FULLs + tacticals, flat). The UI derives nesting by
/// matching `tactical.parent === full.callsign`.
export function useIdentityList() {
  return useQuery({
    queryKey: IDENTITY_LIST_QUERY_KEY,
    queryFn: () => invoke<IdentityListDto>('identity_list'),
  });
}

/// The active session, or `null` when no identity is authenticated this launch
/// (re-auth-on-launch — the active slot starts empty every launch).
export function useActiveIdentity() {
  return useQuery({
    queryKey: IDENTITY_ACTIVE_QUERY_KEY,
    queryFn: () => invoke<ActiveIdentityDto | null>('identity_active'),
  });
}

/// Arguments to switch (authenticate) an identity. To switch to a FULL, pass its
/// callsign + credential with `tacticalLabel: null`. To switch to a tactical,
/// pass the PARENT FULL callsign + that FULL's credential + the tactical
/// `tacticalLabel`.
export interface IdentitySwitchArgs {
  callsign: string;
  credential: string;
  tacticalLabel: string | null;
}

/// Authenticate an identity (the switch action). On success, invalidate the list
/// (its `needs_auth` flags change) and the active-session query (the chip/header
/// update). The mutation rejects with the raw `UiError`; callers run it through
/// `parseIdentityError` for display.
export function useIdentitySwitch() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ callsign, credential, tacticalLabel }: IdentitySwitchArgs) =>
      invoke<void>('identity_authenticate', { callsign, credential, tacticalLabel }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: IDENTITY_LIST_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: IDENTITY_ACTIVE_QUERY_KEY });
    },
  });
}

/// Arguments to add a FULL identity. `password` doubles as the activation secret
/// (the credential the operator later re-enters to authenticate). When
/// `hasCmsAccount` is true, the same password is ALSO written to the CMS keyring
/// entry (see the F2 coordination note below). `label` is the optional display
/// label; pass `null` for none.
export interface AddFullIdentityArgs {
  callsign: string;
  label: string | null;
  hasCmsAccount: boolean;
  password: string;
}

/// Add a FULL identity. A CMS-account FULL needs BOTH its CMS keyring password
/// (`credentials_write_password`) AND its store record + activation secret
/// (`identity_add_full`) — these are separate keyring entries (design-review F2).
/// So for a CMS FULL we write the password first, then add the store record with
/// the SAME password as the activation secret. For a non-CMS FULL there is no
/// CMS password to write — we call only `identity_add_full`, still using the
/// password as the activation secret. Order matters: if `identity_add_full`
/// fails after the password write, the password write already happened; that is
/// acceptable (a retry is idempotent — `write_password` overwrites). On success,
/// invalidate the list (the new row needs to appear).
export function useAddFullIdentity() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ callsign, label, hasCmsAccount, password }: AddFullIdentityArgs) => {
      if (hasCmsAccount) {
        await invoke<void>('credentials_write_password', { callsign, password });
      }
      await invoke<void>('identity_add_full', {
        callsign,
        label,
        hasCmsAccount,
        activationSecret: password,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: IDENTITY_LIST_QUERY_KEY });
    },
  });
}

/// Arguments to add a tactical identity under an existing FULL parent.
export interface AddTacticalArgs {
  label: string;
  parent: string;
}

/// Add a tactical identity under an existing FULL parent. Rejects with the raw
/// `UiError` (e.g. `ParentNotFound` → surfaced via `parseIdentityError`) when the
/// parent FULL is absent. On success, invalidate the list.
export function useAddTactical() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ label, parent }: AddTacticalArgs) =>
      invoke<void>('identity_add_tactical', { label, parent }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: IDENTITY_LIST_QUERY_KEY });
    },
  });
}

/// Discriminated argument to remove an identity. The backend `identity_remove`
/// takes an `Address` — a serde externally-tagged enum — so a FULL is removed
/// with `{ Full: callsign }` and a tactical with `{ Tactical: label }`.
export type RemoveIdentityArgs =
  | { kind: 'full'; callsign: string }
  | { kind: 'tactical'; label: string };

/// Remove a FULL or tactical identity. Builds the externally-tagged `Address`
/// wire shape (`{ Full: callsign }` / `{ Tactical: label }`) and passes it as the
/// `address` arg. Rejects with the raw `UiError` (e.g. `RemoveHasTacticals` when a
/// FULL still owns tacticals — surface it). On success, invalidate the list AND
/// the active query (removing the active identity changes the chip/header).
export function useRemoveIdentity() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: RemoveIdentityArgs) => {
      const address =
        args.kind === 'full' ? { Full: args.callsign } : { Tactical: args.label };
      return invoke<void>('identity_remove', { address });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: IDENTITY_LIST_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: IDENTITY_ACTIVE_QUERY_KEY });
    },
  });
}
