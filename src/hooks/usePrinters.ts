import { useCallback, useEffect, useState } from 'react';
import * as printersApi from '../lib/api/printers';
import type { Printer, UnitKind } from '../types';

/**
 * Drucker und ihre Mehrfarbeinheiten. Jede Aenderung laedt die Liste neu;
 * Fehler landen in `error` und werden zusaetzlich weitergereicht, damit der
 * Aufrufer z.B. ein Eingabefeld offen lassen kann.
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
    addPrinter: (name: string, holderName: string) => run(printersApi.addPrinter(name, holderName)),
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
 * Eine einzige Instanz gehoert nach App.tsx; wird an Rail (Reiter "Drucker")
 * und FilamentView weitergereicht, damit beide denselben Stand sehen (sonst
 * bleibt z.B. der Rail-Reiter nach einer Aenderung im Filament-Lager veraltet).
 */
export type PrintersState = ReturnType<typeof usePrinters>;
