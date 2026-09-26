import { useCallback, useEffect, useState } from 'react';
import * as printersApi from '../lib/api/printers';
import type { Printer, PrinterKind, UnitKind } from '../types';

/**
 * Printers and their multi-color units. Every change reloads the list;
 * errors end up in `error` and are also passed on so the caller can,
 * for example, keep an input open.
 */
export function usePrinters() {
  const [printers, setPrinters] = useState<Printer[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(
    () =>
      printersApi
        .listPrinters()
        .then((result) => {
          setPrinters(result);
          setError(null);
        })
        .catch((e) => setError(String(e))),
    [],
  );

  useEffect(() => {
    refresh();
  }, [refresh]);

  const run = useCallback(
    <T,>(operation: Promise<T>): Promise<T> =>
      operation
        .then((result) => {
          refresh();
          return result;
        })
        .catch((e) => {
          setError(String(e));
          throw e;
        }),
    [refresh],
  );

  return {
    printers,
    error,
    refresh,
    addPrinter: (name: string, holderName: string, kind: PrinterKind) =>
      run(printersApi.addPrinter(name, holderName, kind)),
    renamePrinter: (printerId: string, name: string) => run(printersApi.renamePrinter(printerId, name)),
    deletePrinter: (printerId: string) => run(printersApi.deletePrinter(printerId)),
    addUnit: (printerId: string, kind: UnitKind, name: string, slotCount: number | null) =>
      run(printersApi.addUnit(printerId, kind, name, slotCount)),
    updateUnit: (unitId: string, name: string, slotCount: number | null) =>
      run(printersApi.updateUnit(unitId, name, slotCount)),
    deleteUnit: (unitId: string) => run(printersApi.deleteUnit(unitId)),
    reorderUnits: (printerId: string, unitIds: string[]) => run(printersApi.reorderUnits(printerId, unitIds)),
  };
}

/**
 * A single instance in App.tsx, passed to Rail and FilamentView so both
 * see the same state.
 */
export type PrintersState = ReturnType<typeof usePrinters>;
