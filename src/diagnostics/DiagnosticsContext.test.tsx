import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { error as logError, info as logInfo } from '@tauri-apps/plugin-log';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DiagnosticsProvider } from './DiagnosticsContext';
import { ErrorText } from './ErrorText';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(logError).mockClear();
  vi.mocked(logInfo).mockClear();
});

function renderProvider() {
  return render(
    <LanguageProvider>
      <DiagnosticsProvider>
        <div />
      </DiagnosticsProvider>
    </LanguageProvider>,
  );
}

describe('DiagnosticsProvider', () => {
  it('shows a toast with "Report problem" for an uncaught window error and logs it', () => {
    renderProvider();
    act(() => {
      window.dispatchEvent(new ErrorEvent('error', { message: 'kaputt' }));
    });

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('kaputt');
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
    expect(logError).toHaveBeenCalledWith(expect.stringContaining('kaputt'));
  });

  it('closes the toast when its close button is clicked', () => {
    renderProvider();
    act(() => {
      window.dispatchEvent(new ErrorEvent('error', { message: 'kaputt' }));
    });
    expect(screen.getByRole('alert')).toBeInTheDocument();

    fireEvent.click(screen.getByText('Schließen'));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('logs the UI language once on mount', () => {
    renderProvider();
    expect(logInfo).toHaveBeenCalledWith('UI language de');
  });
});

describe('ErrorText', () => {
  function renderError(error: { message: string; unexpected: boolean } | null) {
    return render(
      <LanguageProvider>
        <DiagnosticsProvider>
          <ErrorText error={error} />
        </DiagnosticsProvider>
      </LanguageProvider>,
    );
  }

  it('renders nothing without an error', () => {
    const { container } = renderError(null);
    expect(container.querySelector('span')).not.toBeInTheDocument();
  });

  it('shows the "Report problem" link for an unexpected error', () => {
    renderError({ message: 'boom', unexpected: true });
    expect(screen.getByText('boom')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });

  it('does not show the link for an expected error', () => {
    renderError({ message: 'Bitte Ordner wählen', unexpected: false });
    expect(screen.getByText('Bitte Ordner wählen')).toBeInTheDocument();
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });
});
