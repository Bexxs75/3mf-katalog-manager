import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '../lib/events';
import * as api from '../lib/api/printerLink';
import { toAppError, type AppError } from '../lib/errors';
import type { JobDecision, PrinterConnection, PrinterJob } from '../types';

/**
 * State of the printer connection. Reloads at startup and after every sync
 * (event from the backend).
 */
export function usePrinterLink() {
  const [refreshKey, setRefreshKey] = useState(0);
  const [enabled, setEnabledState] = useState(false);
  const [connections, setConnections] = useState<PrinterConnection[]>([]);
  const [jobs, setJobs] = useState<PrinterJob[]>([]);
  const [error, setError] = useState<AppError | null>(null);
  // A web server without a printer allowlist refuses every printer command with this code. That is a
  // deployment choice, not a failure, so the UI explains it instead of showing an error.
  const [unavailable, setUnavailable] = useState(false);
  // Prevents setState after unmount; set in effects so StrictMode works.
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [on, conns, open] = await Promise.all([
        api.getPrinterLinkEnabled(),
        api.listPrinterConnections(),
        api.listOpenPrinterJobs(),
      ]);
      if (!mounted.current) return;
      setEnabledState(on);
      setConnections(conns);
      setJobs(open);
      setRefreshKey(key => key + 1);
      setError(null);
      setUnavailable(false);
    } catch (e) {
      if (!mounted.current) return;
      const failure = toAppError(e);
      if (import.meta.env.VITE_TARGET === 'web' && failure.message === 'printerAddressNotAllowed') {
        setUnavailable(true);
        setError(null);
      } else setError(failure);
    }
  }, []);

  useEffect(() => {
    refresh();
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen(api.PRINTER_JOBS_CHANGED_EVENT, () => refresh()).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refresh]);

  const after = useCallback(
    async <T,>(p: Promise<T>): Promise<T> => {
      try {
        return await p;
      } finally {
        await refresh();
      }
    },
    [refresh],
  );

  return {
    refreshKey,
    enabled,
    connections,
    jobs,
    error,
    unavailable,
    refresh,
    setEnabled: (on: boolean) => after(api.setPrinterLinkEnabled(on)),
    testConnection: (printerId: string, address: string, apiKey: string | null = null) =>
      after(api.testPrinterConnection(printerId, address, apiKey)),
    clearApiKey: (printerId: string) => after(api.clearPrinterApiKey(printerId)),
    removeConnection: (printerId: string) => after(api.removePrinterConnection(printerId)),
    syncNow: () => api.syncPrintersNow(),
    includeOlderHistory: (printerId: string) => after(api.includeOlderPrinterHistory(printerId)),
    ignoreJob: (jobId: string) => after(api.ignorePrinterJob(jobId)),
    confirmJobs: (decisions: JobDecision[]) => after(api.confirmPrinterJobs(decisions)),
    previewJob: (jobId: string, spoolId: string) => api.previewPrinterJob(jobId, spoolId),
  };
}

export type PrinterLinkState = ReturnType<typeof usePrinterLink>;
