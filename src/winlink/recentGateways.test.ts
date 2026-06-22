import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { vi, it, expect } from 'vitest';

// Mock MUST precede the module-under-test import (vi.mock is hoisted).
const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { useRecentGateways } from './recentGateways';

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

it('queries contacts_recent_gateways with withinHours and returns rows', async () => {
  invokeMock.mockResolvedValue([
    { gateway: 'W6DRZ', grid: 'CM97', last_attempt_at: '2026-06-22T11:30:00-07:00', outcome: 'reached' },
  ]);
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const { result } = renderHook(() => useRecentGateways(6), { wrapper: wrapperWith(qc) });
  await waitFor(() => expect(result.current.gateways.length).toBe(1));
  expect(invokeMock).toHaveBeenCalledWith('contacts_recent_gateways', { withinHours: 6 });
  expect(result.current.gateways[0].gateway).toBe('W6DRZ');
});
