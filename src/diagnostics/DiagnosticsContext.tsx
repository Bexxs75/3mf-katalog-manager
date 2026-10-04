import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { error as logError, info as logInfo } from '@tauri-apps/plugin-log';
import { useLanguage } from '../i18n/LanguageContext';
import { toAppError } from '../lib/errors';
import { BugReportDialog } from './BugReportDialog';
import { UnexpectedErrorToast } from './UnexpectedErrorToast';

interface DiagnosticsValue {
  openBugReport: () => void;
}

const DiagnosticsContext = createContext<DiagnosticsValue | null>(null);

export function DiagnosticsProvider({ children }: { children: ReactNode }) {
  const { language } = useLanguage();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const reportUnexpected = useCallback((message: string) => {
    logError(message).catch(() => {});
    setToast(message);
  }, []);

  useEffect(() => {
    logInfo(`UI language ${language}`).catch(() => {});
  }, [language]);

  // Errors nobody catches would otherwise vanish silently.
  useEffect(() => {
    // The browser reports this when a resize callback changes layout again within the
    // same frame. It is harmless by specification and says nothing about a fault.
    const isResizeLoopNotice = (message: string) =>
      /^ResizeObserver loop (completed with undelivered notifications|limit exceeded)/.test(message);
    const onError = (e: ErrorEvent) => isResizeLoopNotice(e.message) || reportUnexpected(`${e.message}${e.error?.stack ? `\n${e.error.stack}` : ''}`);
    const onRejection = (e: PromiseRejectionEvent) => {
      // A rejection with an *expected* CmdError (e.g. a command's own validation
      // error, awaited but not caught somewhere) is normal feedback, not a fault -
      // it must not show the unexpected-error toast or be logged as one.
      if (!toAppError(e.reason).unexpected) return;
      const r = e.reason as { message?: unknown; stack?: unknown } | undefined;
      const text = typeof r?.message === 'string' ? r.message : String(e.reason);
      reportUnexpected(typeof r?.stack === 'string' ? `${text}\n${r.stack}` : text);
    };
    window.addEventListener('error', onError);
    window.addEventListener('unhandledrejection', onRejection);
    return () => {
      window.removeEventListener('error', onError);
      window.removeEventListener('unhandledrejection', onRejection);
    };
  }, [reportUnexpected]);

  const value = useMemo(
    () => ({ openBugReport: () => { setToast(null); setDialogOpen(true); } }),
    [],
  );

  return (
    <DiagnosticsContext.Provider value={value}>
      {children}
      {toast && <UnexpectedErrorToast message={toast.split('\n')[0]} onClose={() => setToast(null)} />}
      {dialogOpen && <BugReportDialog onClose={() => setDialogOpen(false)} />}
    </DiagnosticsContext.Provider>
  );
}

export function useDiagnostics(): DiagnosticsValue {
  const ctx = useContext(DiagnosticsContext);
  if (!ctx) throw new Error('useDiagnostics outside DiagnosticsProvider');
  return ctx;
}
