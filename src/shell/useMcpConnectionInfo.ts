import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { McpConnectionInfo } from './connectAgentCommands';

export function useMcpConnectionInfo(enabled: boolean) {
  return useQuery({
    queryKey: ['mcp_connection_info'],
    queryFn: () => invoke<McpConnectionInfo>('mcp_connection_info'),
    enabled,
  });
}
