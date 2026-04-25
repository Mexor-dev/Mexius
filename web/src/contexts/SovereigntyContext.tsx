/**
 * SovereigntyContext — Global operational mode state.
 *
 * Polls GET /api/state/status every 3 seconds and exposes the current
 * SovereigntyState (idle | active | dreaming | nexus) to the whole app.
 * Any component that needs to know the current mode or toggle it should
 * consume this context instead of making its own API calls.
 */

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from 'react';
import type { SovereigntyStateValue, StateStatusResponse } from '@/types/api';
import { getStateStatus, toggleState } from '@/lib/api';

interface SovereigntyContextType {
  state: SovereigntyStateValue;
  dbReadOnly: boolean;
  userInputEnabled: boolean;
  loading: boolean;
  toggle: (next: SovereigntyStateValue) => Promise<void>;
}

const SovereigntyContext = createContext<SovereigntyContextType>({
  state: 'active',
  dbReadOnly: false,
  userInputEnabled: true,
  loading: false,
  toggle: async () => {},
});

export function SovereigntyProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<StateStatusResponse>({
    state: 'active',
    db_read_only: false,
    user_input_enabled: true,
  });
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const s = await getStateStatus();
      setStatus(s);
    } catch {
      // Gateway unreachable — keep stale state
    }
  }, []);

  // Poll every 3 seconds
  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 3000);
    return () => clearInterval(id);
  }, [refresh]);

  const toggle = useCallback(async (next: SovereigntyStateValue) => {
    setLoading(true);
    try {
      const result = await toggleState(next);
      if (result.ok) {
        setStatus((prev) => ({
          ...prev,
          state: result.state,
          db_read_only: result.state === 'dreaming',
          user_input_enabled: result.state !== 'dreaming',
        }));
      }
    } finally {
      setLoading(false);
    }
  }, []);

  return (
    <SovereigntyContext.Provider
      value={{
        state: status.state,
        dbReadOnly: status.db_read_only,
        userInputEnabled: status.user_input_enabled,
        loading,
        toggle,
      }}
    >
      {children}
    </SovereigntyContext.Provider>
  );
}

export function useSovereignty() {
  return useContext(SovereigntyContext);
}
