import { useState, useCallback } from 'react';

export function useConsent() {
  const [token, setToken] = useState<string | null>(null);
  const grant = useCallback((t: string) => setToken(t), []);
  const clear = useCallback(() => setToken(null), []);
  return { token, grant, clear };
}
