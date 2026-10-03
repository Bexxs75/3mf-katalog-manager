import type { ReactNode } from 'react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DiagnosticsProvider } from '../diagnostics/DiagnosticsContext';

/**
 * Drop-in replacement for `LanguageProvider` in tests: components behind an
 * unexpected `AppError` render `ReportProblemLink`, which needs
 * `useDiagnostics()`. Import this as `LanguageProvider` to add the provider
 * without touching every render call in the file. Callers still need their
 * own `vi.mock('@tauri-apps/plugin-log', ...)` - DiagnosticsProvider logs
 * through it and vi.mock does not carry across files.
 */
export function LanguageProviderWithDiagnostics({ children }: { children: ReactNode }) {
  return (
    <LanguageProvider>
      <DiagnosticsProvider>{children}</DiagnosticsProvider>
    </LanguageProvider>
  );
}
