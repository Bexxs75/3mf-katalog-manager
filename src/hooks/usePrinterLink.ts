import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import * as api from '../lib/api/printerLink';
import type { JobDecision, PrinterConnection, PrinterJob } from '../types';

/**
 * Zustand der Druckeranbindung. Lädt beim Start und nach jedem Abgleich
 * (Ereignis vom Backend) neu.
 */
export function usePrinterLink() {
  const [enabled, setEnabledState] = useState(false);
  const [connections, setConnections] = useState<PrinterConnection[]>([]);
  const [jobs, setJobs] = useState<PrinterJob[]>([]);
  const [error, setError] = useState<string | null>(null);
  // Verhindert setState nach dem Unmount; in Effects gesetzt, damit StrictMode passt.
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
      setError(null);
    } catch (e) {
      if (mounted.current) setError(String(e));
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
    enabled,
    connections,
    jobs,
    error,
    refresh,
    setEnabled: (on: boolean) => after(api.setPrinterLinkEnabled(on)),
    testConnection: (printerId: string, address: string) => after(api.testPrinterConnection(printerId, address)),
    removeConnection: (printerId: string) => after(api.removePrinterConnection(printerId)),
    syncNow: () => api.syncPrintersNow(),
    ignoreJob: (jobId: string) => after(api.ignorePrinterJob(jobId)),
    confirmJobs: (decisions: JobDecision[]) => after(api.confirmPrinterJobs(decisions)),
    previewJob: (jobId: string, spoolId: string) => api.previewPrinterJob(jobId, spoolId),
  };
}

export type PrinterLinkState = ReturnType<typeof usePrinterLink>;
