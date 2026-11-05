import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import * as api from '../lib/api/printTechnology';
import type { PrintTechnology } from '../lib/printTechnology';

interface PrintTechnologyValue {
  technology: PrintTechnology;
  /** True once the question was answered or dismissed; the start-up question waits for `false`. */
  asked: boolean;
  /** Stores the choice. Rejects when it cannot be saved; the shown choice then stays as it was. */
  save: (value: PrintTechnology) => Promise<void>;
}

// Without a provider (single components in tests) everything is visible and nothing asks.
const Fallback: PrintTechnologyValue = { technology: 'both', asked: true, save: async () => {} };
const PrintTechnologyContext = createContext<PrintTechnologyValue>(Fallback);

export function PrintTechnologyProvider({ children }: { children: ReactNode }) {
  // Until the answer arrives nothing is hidden and nothing is asked.
  const [state, setState] = useState<{ technology: PrintTechnology; asked: boolean }>({ technology: 'both', asked: true });
  useEffect(() => {
    let cancelled = false;
    api.getPrintTechnology()
      .then((s) => { if (!cancelled) setState({ technology: s.value, asked: s.asked }); })
      .catch((e) => console.error('[print-technology] could not read the choice:', e));
    return () => { cancelled = true; };
  }, []);
  const save = useCallback(async (value: PrintTechnology) => {
    const s = await api.setPrintTechnology(value);
    setState({ technology: s.value, asked: s.asked });
  }, []);
  const value = useMemo(() => ({ ...state, save }), [state, save]);
  return <PrintTechnologyContext.Provider value={value}>{children}</PrintTechnologyContext.Provider>;
}

export function usePrintTechnology() {
  return useContext(PrintTechnologyContext);
}
