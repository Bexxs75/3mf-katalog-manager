import { createContext, useContext } from 'react';

/**
 * Opens the plate overview of a model. Provided once by the app; null where no dialog is
 * available (a bare card in a test, the trash), so the entry points simply do not appear there.
 */
export const PlateOverviewContext = createContext<((modelId: string) => void) | null>(null);

export function useOpenPlateOverview(): ((modelId: string) => void) | null {
  return useContext(PlateOverviewContext);
}
