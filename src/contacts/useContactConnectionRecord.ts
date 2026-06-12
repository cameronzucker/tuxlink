// useContactConnectionRecord — the per-contact connection record query
// (tuxlink-je5d).
//
// A contact's connection record is the aggregate of every connection attempt
// against a favorite whose `gateway == contact.callsign`, plus the gated
// time-of-day hint over that combined set. The backend owns the aggregation
// (`contacts_connection_record`) so the frontend keys ONE query by callsign and
// hands the result straight to <ConnectionRecord attempts hint />.
//
// Wire shape (snake_case; the codebase has no `rename_all`):
//   contacts_connection_record(callsign: String)
//     -> { attempts: Vec<ConnectionAttempt>, hint: Option<TodHint> }
//
// Tauri auto-camelCases Rust snake_case command ARGS on the JS wire, so the
// single `callsign: String` param is passed as `{ callsign }` (single word — no
// camel transform). The RESPONSE fields (`attempts` / `hint`) are serde-shaped
// snake_case and read verbatim.

import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { ConnectionAttempt, TodHint } from '../favorites/types';

/// The backend DTO returned by `contacts_connection_record`. `attempts` is the
/// aggregate across every favorite matching the callsign; `hint` is null when
/// the gating threshold is not met (or there are no successes).
export interface ContactConnectionRecord {
  attempts: ConnectionAttempt[];
  hint: TodHint | null;
}

/// Query key for a contact's connection record, namespaced by callsign so each
/// contact caches independently and a callsign change refetches.
export const contactConnectionRecordKey = (callsign: string) =>
  ['contacts', 'connection_record', callsign] as const;

export interface UseContactConnectionRecord {
  attempts: ConnectionAttempt[];
  hint: TodHint | null;
  isLoading: boolean;
}

/// Fetch the aggregated connection record for `callsign`. Returns empty
/// attempts + null hint until the query resolves (and on error — the empty
/// state is the honest "no connection attempts yet" surface, not an error
/// banner; mirrors the contacts layer's non-blocking posture).
export function useContactConnectionRecord(callsign: string): UseContactConnectionRecord {
  const query = useQuery({
    queryKey: contactConnectionRecordKey(callsign),
    queryFn: () =>
      invoke<ContactConnectionRecord>('contacts_connection_record', { callsign }),
    enabled: callsign.trim().length > 0,
  });

  return {
    attempts: query.data?.attempts ?? [],
    hint: query.data?.hint ?? null,
    isLoading: query.isLoading,
  };
}
