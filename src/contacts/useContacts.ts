// useContacts — the Contacts data layer (TanStack Query) + H9 cross-window
// listener. Task A4.
//
// Mirrors `src/search/useSavedSearches.ts` (query + invoke + invalidate) for the
// CRUD surface and `src/mailbox/useMailbox.ts::useMailboxChangeEvents` for the
// cross-window event subscription.
//
// Contract:
//   - `contacts_read` returns the whole `ContactsFile`; the hook splits it into
//     `.contacts` / `.groups` (each defaulting to []).
//   - Mutations await the invoke, then invalidate `['contacts']` so the query
//     refetches. Invoke arg-key names match the Rust `#[tauri::command]` params
//     EXACTLY: `contact_upsert(contact)`, `contact_delete(id)`,
//     `group_upsert(group)`, `group_delete(id)`.
//   - Mutation errors are NON-BLOCKING (`.catch(() => {})`): they surface in the
//     backend session log, not in React state — there is deliberately no error
//     field in the return type (Cross-cutting §1).
//   - H9: a `useEffect` subscribes to the app-level `contacts:changed` event and
//     invalidates `['contacts']` on fire, so a contact added/edited in the main
//     window propagates to a separate Compose window's `useContacts` instance.

import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Contact, ContactsFile, Group } from './types';

/// Query key for the whole contacts file. A single key (not split per
/// contacts/groups) because `contacts_read` returns both together.
export const CONTACTS_QUERY_KEY = ['contacts'] as const;

/// App-level Tauri event the Rust command layer emits after every contacts
/// mutation (H9). Mirrors `CONTACTS_CHANGED_EVENT` in
/// `src-tauri/src/contacts/commands.rs`.
export const CONTACTS_CHANGED_EVENT = 'contacts:changed';

export interface UseContacts {
  contacts: Contact[];
  groups: Group[];
  isLoading: boolean;
  upsertContact: (contact: Contact) => Promise<void>;
  deleteContact: (id: string) => Promise<void>;
  upsertGroup: (group: Group) => Promise<void>;
  deleteGroup: (id: string) => Promise<void>;
}

export function useContacts(): UseContacts {
  const qc = useQueryClient();

  const query = useQuery({
    queryKey: CONTACTS_QUERY_KEY,
    queryFn: () => invoke<ContactsFile>('contacts_read'),
  });

  const invalidate = () => qc.invalidateQueries({ queryKey: CONTACTS_QUERY_KEY });

  // H9 — cross-window propagation. Subscribe once; invalidate on fire. Mirrors
  // useMailboxChangeEvents' unmount-before-resolve race handling (the `cancelled`
  // flag), and tolerates a missing Tauri runtime (test/dev harness) via .catch.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    listen<void>(CONTACTS_CHANGED_EVENT, () => {
      void qc.invalidateQueries({ queryKey: CONTACTS_QUERY_KEY });
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch(() => {
        // No Tauri runtime here — the query's own refetch remains the fallback.
      });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [qc]);

  return {
    contacts: query.data?.contacts ?? [],
    groups: query.data?.groups ?? [],
    isLoading: query.isLoading,

    upsertContact: async (contact: Contact) => {
      await invoke('contact_upsert', { contact }).catch(() => {});
      await invalidate();
    },
    deleteContact: async (id: string) => {
      await invoke('contact_delete', { id }).catch(() => {});
      await invalidate();
    },
    upsertGroup: async (group: Group) => {
      await invoke('group_upsert', { group }).catch(() => {});
      await invalidate();
    },
    deleteGroup: async (id: string) => {
      await invoke('group_delete', { id }).catch(() => {});
      await invalidate();
    },
  };
}
