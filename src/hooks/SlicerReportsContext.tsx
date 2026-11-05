import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { listen } from '../lib/events';
import * as api from '../lib/api/slicerReports';
import type { SlicerReport } from '../lib/slicerReports';

/** How often the list is read again while the window is open (reports also arrive on focus and by event). */
const REFRESH_MS = 30_000;

interface SlicerReportsValue {
  /** Open reports first, then the last handled ones. */
  reports: SlicerReport[];
  open: SlicerReport[];
  /** Open reports the notice has not shown yet (a closed notice returns only for newer reports). */
  unseen: SlicerReport[];
  refresh: () => Promise<void>;
  /** Marks all open reports as seen by the notice. */
  dismissNotice: () => void;
  resolve: (id: string, action: 'link' | 'dismiss', modelFileId?: string) => Promise<void>;
  inboxOpen: boolean;
  openInbox: () => void;
  closeInbox: () => void;
}

const Fallback: SlicerReportsValue = {
  reports: [], open: [], unseen: [], refresh: async () => {}, dismissNotice: () => {}, resolve: async () => {},
  inboxOpen: false, openInbox: () => {}, closeInbox: () => {},
};
const SlicerReportsContext = createContext<SlicerReportsValue>(Fallback);

/** Desktop only: the browser version has no slicer reports and asks for nothing. */
export function SlicerReportsProvider({ children }: { children: ReactNode }) {
  const web = import.meta.env.VITE_TARGET === 'web';
  const [reports, setReports] = useState<SlicerReport[]>([]);
  const [seen, setSeen] = useState<ReadonlySet<string>>(new Set());
  const [inboxOpen, setInboxOpen] = useState(false);
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    if (web) return;
    try {
      const next = await api.listSlicerReports();
      if (alive.current) setReports(next);
    } catch (e) {
      console.warn('[slicer-reports] could not read the list:', e);
    }
  }, [web]);

  useEffect(() => {
    if (web) return;
    alive.current = true;
    let unlisten: (() => void) | undefined;
    // Listening first, so a report between the first read and the registration is not lost.
    listen(api.SLICER_REPORTS_EVENT, () => { void refresh(); }).then((fn) => {
      if (!alive.current) fn();
      else unlisten = fn;
    }).catch((e) => {
      // Without the event the focus and timer refreshes below still pick up new reports.
      console.warn('[slicer-reports] could not listen for new reports:', e);
    });
    void refresh();
    const onFocus = () => { void refresh(); };
    window.addEventListener('focus', onFocus);
    const timer = window.setInterval(() => { if (!document.hidden) void refresh(); }, REFRESH_MS);
    return () => {
      alive.current = false;
      unlisten?.();
      window.removeEventListener('focus', onFocus);
      window.clearInterval(timer);
    };
  }, [web, refresh]);

  const open = useMemo(() => reports.filter((r) => r.state === 'open'), [reports]);
  const unseen = useMemo(() => open.filter((r) => !seen.has(r.id)), [open, seen]);
  const dismissNotice = useCallback(() => setSeen((s) => new Set([...s, ...open.map((r) => r.id)])), [open]);
  const resolve = useCallback(async (id: string, action: 'link' | 'dismiss', modelFileId?: string) => {
    await api.resolveSlicerReport(id, action, modelFileId);
    await refresh();
  }, [refresh]);
  const openInbox = useCallback(() => { setInboxOpen(true); void refresh(); }, [refresh]);
  const closeInbox = useCallback(() => { setInboxOpen(false); setSeen((s) => new Set([...s, ...open.map((r) => r.id)])); }, [open]);

  const value = useMemo(
    () => ({ reports, open, unseen, refresh, dismissNotice, resolve, inboxOpen, openInbox, closeInbox }),
    [reports, open, unseen, refresh, dismissNotice, resolve, inboxOpen, openInbox, closeInbox],
  );
  return <SlicerReportsContext.Provider value={value}>{children}</SlicerReportsContext.Provider>;
}

export function useSlicerReports() {
  return useContext(SlicerReportsContext);
}
