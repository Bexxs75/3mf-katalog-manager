import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UpdateToast } from './UpdateToast';
import type { UpdaterView } from '../hooks/useUpdater';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

function makeView(overrides: Partial<UpdaterView> = {}): UpdaterView {
  return {
    info: null,
    phase: 'idle',
    progress: null,
    error: null,
    dismissed: false,
    checkNow: vi.fn(),
    startUpdate: vi.fn(),
    install: vi.fn(),
    retry: vi.fn(),
    later: vi.fn(),
    dismiss: vi.fn(),
    openNotes: vi.fn(),
    ...overrides,
  };
}

function renderToast(view: UpdaterView) {
  render(
    <LanguageProvider>
      <UpdateToast view={view} />
    </LanguageProvider>,
  );
}

describe('UpdateToast', () => {
  it('renders nothing when there is no available update and no active phase', () => {
    renderToast(makeView());
    expect(screen.queryByText(/verfügbar/)).not.toBeInTheDocument();
  });

  it('renders nothing when the available update was dismissed', () => {
    renderToast(makeView({ info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null }, dismissed: true }));
    expect(screen.queryByText('Version 0.15.1 ist verfügbar')).not.toBeInTheDocument();
  });

  it('available state: shows title, body and calls startUpdate/openNotes/dismiss', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: true, lastUpdate: null },
    });
    renderToast(view);
    expect(screen.getByText('Version 0.15.1 ist verfügbar')).toBeInTheDocument();
    expect(screen.getByText('Du hast 0.15.0. Vor der Installation wird dein Katalog gesichert.')).toBeInTheDocument();

    screen.getByText('Jetzt aktualisieren').click();
    expect(view.startUpdate).toHaveBeenCalled();

    screen.getByText('Was ist neu?').click();
    expect(view.openNotes).toHaveBeenCalled();

    screen.getByText('✕').click();
    expect(view.dismiss).toHaveBeenCalled();
  });

  it('available state without canInstall: primary button goes to the download page', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: false, lastUpdate: null },
    });
    renderToast(view);
    expect(screen.queryByText('Jetzt aktualisieren')).not.toBeInTheDocument();
    screen.getByText('Zur Download-Seite').click();
    expect(view.openNotes).toHaveBeenCalled();
  });

  it('downloading state: shows progress with a rounded percentage and MB values', () => {
    const view = makeView({
      phase: 'downloading',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null },
      progress: { downloaded: 48 * 1024 * 1024, total: 106 * 1024 * 1024 },
    });
    renderToast(view);
    expect(screen.getByText('Update 0.15.1 wird geladen …')).toBeInTheDocument();
    expect(screen.getByText('48 von 106 MB · 45 %')).toBeInTheDocument();
    expect(screen.getByText('Du kannst währenddessen weiterarbeiten.')).toBeInTheDocument();
    const bar = screen.getByRole('progressbar');
    expect(bar).toHaveAttribute('aria-valuenow', '45');
  });

  it('downloading state with unknown total: shows only the downloaded MB', () => {
    const view = makeView({
      phase: 'downloading',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null },
      progress: { downloaded: 5 * 1024 * 1024, total: null },
    });
    renderToast(view);
    expect(screen.getByText('5 MB geladen')).toBeInTheDocument();
  });

  it('ready state: restart-and-install calls install, later calls later', () => {
    const view = makeView({
      phase: 'ready',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null },
    });
    renderToast(view);
    expect(screen.getByText('Update 0.15.1 ist bereit')).toBeInTheDocument();

    screen.getByText('Neu starten und installieren').click();
    expect(view.install).toHaveBeenCalled();

    screen.getByText('Später').click();
    expect(view.later).toHaveBeenCalled();
  });

  it('installing state: shows the backup file path', () => {
    const view = makeView({
      phase: 'installing',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null },
    });
    renderToast(view);
    expect(screen.getByText('Katalog wird gesichert …')).toBeInTheDocument();
    expect(screen.getByText('update-backups/catalog-vor-0.15.1.db')).toBeInTheDocument();
  });

  it('error state: shows the error message and retry calls view.retry, not startUpdate', () => {
    const view = makeView({
      phase: 'error',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null },
      error: { message: 'Sicherung fehlgeschlagen', unexpected: false },
    });
    renderToast(view);
    expect(screen.getByText('Update fehlgeschlagen')).toBeInTheDocument();
    expect(screen.getByText('Sicherung fehlgeschlagen')).toBeInTheDocument();
    expect(screen.getByText('Es wurde nichts installiert, deine Version und dein Katalog sind unverändert.')).toBeInTheDocument();

    screen.getByText('Erneut versuchen').click();
    expect(view.retry).toHaveBeenCalled();
    expect(view.startUpdate).not.toHaveBeenCalled();

    screen.getByText('✕').click();
    expect(view.dismiss).toHaveBeenCalled();
  });
});
