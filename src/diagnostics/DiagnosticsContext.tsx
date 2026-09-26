import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { error as logError, info as logInfo } from '@tauri-apps/plugin-log';
import { useLanguage } from '../i18n/LanguageContext';
import { BugReportDialog } from './BugReportDialog';
import { UnexpectedErrorToast } from './UnexpectedErrorToast';

interface DiagnosticsValue {
  openBugReport: () => void;
  reportUnexpected: (message: string) => void;
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
    const onError = (e: ErrorEvent) => reportUnexpected(`${e.message}${e.error?.stack ? `\n${e.error.stack}` : ''}`);
    const onRejection = (e: PromiseRejectionEvent) => {
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
    () => ({ openBugReport: () => { setToast(null); setDialogOpen(true); }, reportUnexpected }),
    [reportUnexpected],
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
