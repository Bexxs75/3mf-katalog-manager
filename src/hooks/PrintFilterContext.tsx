import { createContext, useContext } from 'react';
import type { PrintDataFilter } from '../lib/printDataFilter';

/**
 * The print data filter while it is active, else null. The cards read it to show their extra
 * block: only while a filter is active, so all cards of a view get the same height and the grid
 * (which measures its first card) stays regular.
 */
export const PrintFilterContext = createContext<PrintDataFilter | null>(null);

export function useActivePrintFilter(): PrintDataFilter | null {
  return useContext(PrintFilterContext);
}
