import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, act, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UpdateToast } from './UpdateToast';
import * as api from '../lib/api/updater';
import type { UpdaterView } from '../hooks/useUpdater';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));
vi.mock('../lib/api/updater', () => ({ discardAppUpdate: vi.fn() }));

function makeView(overrides: Partial<UpdaterView> = {}): UpdaterView {
  return {
    currentVersion: '0.15.0',
    info: null,
    phase: 'idle',
    progress: null,
    error: null,
    notesError: null,
    dismissed: false,
    checkNow: vi.fn(),
    startUpdate: vi.fn(),
    install: vi.fn(),
    retry: vi.fn(),
    later: vi.fn(),
    dismiss: vi.fn(),
    openNotes: vi.fn(),
    preview: false,
    ...overrides,
  };
}

function renderToast(view: UpdaterView) {
  return render(
    <LanguageProvider>
      <UpdateToast view={view} />
    </LanguageProvider>,
  );
}

describe('UpdateToast', () => {
  beforeEach(() => {
    vi.mocked(api.discardAppUpdate).mockReset().mockResolvedValue(undefined);
  });

  it('renders nothing when there is no available update and no active phase', () => {
    const { container } = renderToast(makeView());
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when the available update was dismissed', () => {
    const { container } = renderToast(
      makeView({
        info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
        dismissed: true,
      }),
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('available state: shows title, body and calls startUpdate/openNotes/dismiss', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: true, lastUpdate: null, checkFailed: false },
    });
    renderToast(view);
    expect(screen.getByText('Version 0.15.1 ist verfügbar')).toBeInTheDocument();
    expect(screen.getByText('Du hast 0.15.0. Vor der Installation wird dein Katalog gesichert.')).toBeInTheDocument();

    screen.getByText('Jetzt aktualisieren').click();
    expect(view.startUpdate).toHaveBeenCalled();

    screen.getByText('Was ist neu?').click();
    expect(view.openNotes).toHaveBeenCalled();

    const closeButton = screen.getByRole('button', { name: 'Hinweis schließen' });
    closeButton.click();
    expect(view.dismiss).toHaveBeenCalled();
    // Nothing was downloaded yet for a plain "available" offer, so dismissing it
    // must not try to discard a pending update.
    expect(api.discardAppUpdate).not.toHaveBeenCalled();
  });

  it('available state without canInstall: primary button goes to the download page', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: false, lastUpdate: null, checkFailed: false },
    });
    renderToast(view);
    expect(screen.queryByText('Jetzt aktualisieren')).not.toBeInTheDocument();
    screen.getByText('Zur Download-Seite').click();
    expect(view.openNotes).toHaveBeenCalled();
  });

  it('available state without canInstall: a failed openNotes shows the error and "Problem melden"', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: false, lastUpdate: null, checkFailed: false },
      notesError: { message: 'Konnte die Seite nicht öffnen', unexpected: true },
    });
    renderToast(view);
    expect(screen.getByText('Konnte die Seite nicht öffnen')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });

  it('downloading state: shows progress with a rounded percentage and MB values', () => {
    const view = makeView({
      phase: 'downloading',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
      progress: { downloaded: 48 * 1024 * 1024, total: 106 * 1024 * 1024 },
    });
    renderToast(view);
    expect(screen.getByText('Update 0.15.1 wird geladen …')).toBeInTheDocument();
    expect(screen.getByText('48 von 106 MB · 45 %')).toBeInTheDocument();
    expect(screen.getByText('Du kannst währenddessen weiterarbeiten.')).toBeInTheDocument();
    const bar = screen.getByRole('progressbar');
    expect(bar).toHaveAttribute('aria-valuenow', '45');
  });

  it('downloading state with unknown total: shows only the downloaded MB and an indeterminate bar', () => {
    const view = makeView({
      phase: 'downloading',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
      progress: { downloaded: 5 * 1024 * 1024, total: null },
    });
    renderToast(view);
    expect(screen.getByText('5 MB geladen')).toBeInTheDocument();
    const bar = screen.getByRole('progressbar');
    expect(bar).not.toHaveAttribute('aria-valuenow');
  });

  it('ready state: restart-and-install calls install, later calls later', () => {
    const view = makeView({
      phase: 'ready',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
    });
    renderToast(view);
    expect(screen.getByText('Update 0.15.1 ist bereit')).toBeInTheDocument();

    screen.getByText('Neu starten und installieren').click();
    expect(view.install).toHaveBeenCalled();

    screen.getByText('Später').click();
    expect(view.later).toHaveBeenCalled();
  });

  it.each(['de', 'en', 'es', 'fr'])('uses actual versions in each backup notice (%s)', (language) => {
    localStorage.setItem('3mf-katalog-language', language);
    try {
      for (const phase of ['idle', 'ready', 'installing'] as const) {
        const view = makeView({
          phase,
          currentVersion: '0.15.3',
          info: { currentVersion: '0.15.3', availableVersion: '0.16.0-2', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
        });
        const { unmount } = renderToast(view);
        expect(screen.getByText(/Katalog-Sicherung_/)).toHaveTextContent(
          /Katalog-Sicherung_<[^>]+>_vor-Update_0\.15\.3_auf_0\.16\.0-2\.db/,
        );
        unmount();
      }
    } finally { localStorage.removeItem('3mf-katalog-language'); }
  });

  it('installing state: shows the backup name pattern as prose', () => {
    const view = makeView({
      phase: 'installing',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
    });
    renderToast(view);
    expect(screen.getByText('Katalog wird gesichert …')).toBeInTheDocument();
    const notice = screen.getByText(/automatisch im Ordner update-backups/);
    expect(notice).toHaveTextContent('Katalog-Sicherung_<Datum>_vor-Update_0.15.0_auf_0.15.1.db');
    expect(notice).not.toHaveClass('font-code');
  });

  it('error state: shows the error message, "Problem melden" for an unexpected error, and retry calls view.retry (not startUpdate)', () => {
    const view = makeView({
      phase: 'error',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
      error: { message: 'Sicherung fehlgeschlagen', unexpected: true },
    });
    renderToast(view);
    expect(screen.getByText('Update fehlgeschlagen')).toBeInTheDocument();
    expect(screen.getByText('Sicherung fehlgeschlagen')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
    expect(screen.getByText('Es wurde nichts installiert, deine Version und dein Katalog sind unverändert.')).toBeInTheDocument();

    screen.getByText('Erneut versuchen').click();
    expect(view.retry).toHaveBeenCalled();
    expect(view.startUpdate).not.toHaveBeenCalled();
  });

  it('clicking ✕ on the error toast actually hides it, unlike phase alone which would keep it visible', () => {
    // A plain `view` object can't simulate the toast disappearing on its own -
    // wrap it in a tiny stateful harness so `dismiss` really flips `dismissed`,
    // the same way the real hook's setter would.
    function Harness() {
      const [dismissed, setDismissed] = useState(false);
      const view = makeView({
        phase: 'error',
        info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
        error: { message: 'Sicherung fehlgeschlagen', unexpected: true },
        dismissed,
        dismiss: () => setDismissed(true),
      });
      return <UpdateToast view={view} />;
    }
    const { container } = render(
      <LanguageProvider>
        <Harness />
      </LanguageProvider>,
    );
    expect(screen.getByText('Update fehlgeschlagen')).toBeInTheDocument();
    act(() => screen.getByRole('button', { name: 'Hinweis schließen' }).click());
    expect(screen.queryByText('Update fehlgeschlagen')).not.toBeInTheDocument();
    expect(container).toBeEmptyDOMElement();
  });

  it('dismissing the error toast discards the pending update so it does not linger in memory', async () => {
    const view = makeView({
      phase: 'error',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
      error: { message: 'Sicherung fehlgeschlagen', unexpected: true },
    });
    renderToast(view);

    screen.getByRole('button', { name: 'Hinweis schließen' }).click();
    expect(view.dismiss).toHaveBeenCalled();
    await waitFor(() => expect(api.discardAppUpdate).toHaveBeenCalled());
  });

  it('a failed discard on dismiss is only logged, not surfaced', async () => {
    vi.mocked(api.discardAppUpdate).mockRejectedValue(new Error('ipc down'));
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const view = makeView({
      phase: 'error',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null, checkFailed: false },
      error: { message: 'Sicherung fehlgeschlagen', unexpected: true },
    });
    renderToast(view);

    screen.getByRole('button', { name: 'Hinweis schließen' }).click();
    await waitFor(() => expect(warnSpy).toHaveBeenCalled());
    warnSpy.mockRestore();
  });
});
