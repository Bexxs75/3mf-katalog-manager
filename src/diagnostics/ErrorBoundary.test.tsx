import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { error as logError } from '@tauri-apps/plugin-log';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DiagnosticsProvider } from './DiagnosticsContext';
import { ErrorBoundary, CrashFallback } from './ErrorBoundary';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

function Boom(): never {
  throw new Error('kaboom');
}

describe('ErrorBoundary + CrashFallback', () => {
  let consoleError: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    localStorage.setItem('3mf-katalog-language', 'de');
    vi.mocked(logError).mockClear();
    // React logs the caught render error to console.error; expected here.
    consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    consoleError.mockRestore();
  });

  it('shows the themed fallback and logs the error, even without a data-app attribute on <html>', () => {
    expect(document.documentElement.hasAttribute('data-app')).toBe(false);

    render(
      <LanguageProvider>
        <DiagnosticsProvider>
          <ErrorBoundary fallback={(reload) => <CrashFallback onReload={reload} />}>
            <Boom />
          </ErrorBoundary>
        </DiagnosticsProvider>
      </LanguageProvider>,
    );

    expect(screen.getByText('Etwas ist schiefgelaufen')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
    expect(screen.getByText('Neu laden')).toBeInTheDocument();
    expect(logError).toHaveBeenCalledWith(expect.stringContaining('kaboom'));

    const themedRoot = document.querySelector('[data-app]');
    expect(themedRoot).not.toBeNull();
    expect(['light', 'dark']).toContain(themedRoot?.getAttribute('data-app'));
  });
});
