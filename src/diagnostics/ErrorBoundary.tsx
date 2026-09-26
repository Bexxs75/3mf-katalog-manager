import { Component, type ReactNode } from 'react';
import { error as logError } from '@tauri-apps/plugin-log';
import { useT } from '../i18n/LanguageContext';
import { ReportProblemLink } from './ReportProblemLink';

interface Props {
  fallback: (reload: () => void) => ReactNode;
  children: ReactNode;
}

/** Keeps a render error from leaving a blank window and writes it to the log. */
export class ErrorBoundary extends Component<Props, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: { componentStack?: string | null }) {
    logError(`${error.message}\n${error.stack ?? ''}\n${info.componentStack ?? ''}`).catch(() => {});
  }

  render() {
    return this.state.failed ? this.props.fallback(() => window.location.reload()) : this.props.children;
  }
}

// No `data-app` attribute here: useTheme() already sets it on <html> (see
// src/hooks/useTheme.ts), and CSS custom properties inherit down from there,
// so this fallback is themed the same way as the rest of the app.
export function CrashFallback({ onReload }: { onReload: () => void }) {
  const t = useT();
  return (
    <div className="h-screen grid place-items-center bg-[var(--bg)] text-[var(--ink)]">
      <div className="text-center">
        <div className="text-[15px] font-semibold">{t('unexpectedErrorTitle')}</div>
        <div className="mt-1 text-[12.5px] text-[var(--ink-2)]">{t('unexpectedErrorText')}</div>
        <div className="mt-3 flex gap-3 justify-center text-[12.5px]">
          <ReportProblemLink />
          <button
            type="button"
            onClick={onReload}
            className="text-[var(--ink-2)] underline cursor-pointer bg-transparent border-0"
          >
            {t('errorBoundaryReload')}
          </button>
        </div>
      </div>
    </div>
  );
}
