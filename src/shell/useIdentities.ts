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
