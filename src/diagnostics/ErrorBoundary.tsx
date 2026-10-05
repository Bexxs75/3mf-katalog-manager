import { Component, type ReactNode } from 'react';
import { error as logError } from '@tauri-apps/plugin-log';
import { useT } from '../i18n/LanguageContext';
import { resolveTheme } from '../hooks/useTheme';
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

// `data-app` is normally only set on <html> by useTheme()'s effect (see
// src/hooks/useTheme.ts), which never runs if App throws on its very first
// render. So this sets `data-app` on its own root too, preferring whatever
// is already on <html> (kept in sync with a running app) and otherwise
// resolving the theme the same way useTheme() would.
export function CrashFallback({ onReload }: { onReload: () => void }) {
  const t = useT();
  const theme = document.documentElement.getAttribute('data-app') || resolveTheme();
  return (
    <div data-app={theme} className="h-screen grid place-items-center bg-[var(--bg)] text-[var(--ink)]">
      <div className="text-center">
        <div className="text-title font-semibold">{t('unexpectedErrorTitle')}</div>
        <div className="mt-1 text-small text-[var(--ink-2)]">{t('unexpectedErrorText')}</div>
        <div className="mt-3 flex gap-3 justify-center text-small">
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
