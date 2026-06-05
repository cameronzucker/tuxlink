/**
 * useLoggingStatus — React Query hook that polls the backend's logging_status
 * Tauri command on a 30-second interval.
 *
 * tuxlink-qjgx alpha-logging plan Task 7.4.
 */
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

export interface LoggingStatus {
  disk_usage_bytes: number;
  disk_cap_bytes: number;
  retained_window_seconds: number;
  event_rate_per_hour: number;
  last_export: {
    path: string;
    size_bytes: number;
    at: string;
    correlation_id: string | null;
  } | null;
  detailed_mode: 'off' | 'on' | 'bounded';
  bounded_remaining_seconds: number | null;
  retention_days: number;
  retention_mb_cap: number;
}

export function useLoggingStatus() {
  return useQuery<LoggingStatus>({
    queryKey: ['logging_status'],
    queryFn: () => invoke<LoggingStatus>('logging_status'),
    refetchInterval: 30_000,
    refetchOnWindowFocus: true,
  });
}
