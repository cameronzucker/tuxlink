import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

export interface RecentGatewayPin {
  gateway: string;
  grid?: string;
  last_attempt_at: string;
  outcome: 'reached' | 'failed';
}

export const recentGatewaysKey = (withinHours: number) =>
  ['winlink', 'recent_gateways', withinHours] as const;

export function useRecentGateways(withinHours: number): {
  gateways: RecentGatewayPin[];
  isLoading: boolean;
} {
  const query = useQuery({
    queryKey: recentGatewaysKey(withinHours),
    queryFn: () => invoke<RecentGatewayPin[]>('contacts_recent_gateways', { withinHours }),
    refetchInterval: 60_000, // recency window ages; refresh gently
  });
  return { gateways: query.data ?? [], isLoading: query.isLoading };
}
