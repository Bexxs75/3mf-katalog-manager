import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import { getRuntimeEnvironment, type RuntimeEnvironment } from '../lib/api/runtime';

const RuntimeEnvironmentContext = createContext<RuntimeEnvironment>({ container: false });

export function RuntimeEnvironmentProvider({ children, value }: {
  children: ReactNode; value?: RuntimeEnvironment;
}) {
  const [environment, setEnvironment] = useState<RuntimeEnvironment | null>(value ?? null);
  useEffect(() => {
    if (value) return;
    let active = true;
    void getRuntimeEnvironment().then(result => { if (active) setEnvironment(result); });
    return () => { active = false; };
  }, [value]);
  const resolved = value ?? environment;
  // Mount startup effects only once the environment is known.
  if (!resolved) return null;
  return <RuntimeEnvironmentContext.Provider value={resolved}>{children}</RuntimeEnvironmentContext.Provider>;
}

export const useRuntimeEnvironment = () => useContext(RuntimeEnvironmentContext);
