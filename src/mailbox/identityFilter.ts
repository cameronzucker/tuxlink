// Mailbox identity filter (Task 11, tuxlink-noa0 Phase 7).
//
// The mailbox list can be narrowed to a single identity: one FULL callsign or
// one tactical label. The filter is a pure predicate over a message's optional
// `identity` tag plus a derivation of the toolbar `<select>` options from the
// identity list.
//
// PHASE-4 RECONCILIATION: the per-FULL mailbox work (PR #627, bd-tuxlink-2ns7)
// stores mail's owning identity as a `<mid>.identity` sidecar (the per-FULL
// namespace for received mail; the sent/queued-as identity for the shared Sent/
// Outbox) but did NOT surface it onto the list-row DTO. Phase 7 closes that gap:
// `Mailbox::list_*` now reads the sidecar onto `MessageMeta.identity`, which
// flows through `MessageMetaDto.identity` (Rust) → `MessageMeta.identity` (TS,
// `src/mailbox/types.ts`). So this filter is functional end-to-end. Untagged
// messages (legacy / pre-Phase-4) carry no `identity` → they match only "All".
// (Note: the SEARCH subsystem's `identity_tag` — `src-tauri/src/search/*` — is a
// distinct DTO, unrelated to this list-row tag.)

import type { IdentityListDto } from '../shell/identityTypes';

/// True when a message should be shown under the current identity filter.
/// `filter === null` is the "All identities" selection and matches everything.
/// Otherwise the message's `identity` tag must equal the filter exactly; an
/// untagged message (no `identity`) matches ONLY "All".
export function messageMatchesIdentity(msg: { identity?: string }, filter: string | null): boolean {
  return filter === null || msg.identity === filter;
}

/// One entry in the toolbar identity `<select>`. `value: null` is the sentinel
/// "All identities" row; concrete values are FULL callsigns and tactical labels.
export interface IdentityFilterOption {
  value: string | null;
  label: string;
}

/// Build the filter options from the identity list: "All identities", then one
/// option per FULL callsign, then one per tactical label. A null list (not yet
/// loaded) yields just the "All" row.
export function deriveIdentityFilterOptions(list: IdentityListDto | null): IdentityFilterOption[] {
  const options: IdentityFilterOption[] = [{ value: null, label: 'All identities' }];
  if (!list) return options;
  for (const full of list.full) {
    options.push({ value: full.callsign, label: full.callsign });
  }
  for (const tactical of list.tactical) {
    options.push({ value: tactical.label, label: tactical.label });
  }
  return options;
}
