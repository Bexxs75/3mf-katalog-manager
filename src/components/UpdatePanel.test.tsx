import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UpdatePanel } from './UpdatePanel';
import type { UpdaterView } from '../hooks/useUpdater';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

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

function renderPanel(view: UpdaterView) {
  return render(
    <LanguageProvider>
      <UpdatePanel view={view} />
    </LanguageProvider>,
  );
}

describe('UpdatePanel', () => {
  it('shows the running version immediately, before the check has answered', () => {
    renderPanel(makeView({ info: null }));
    expect(screen.getByText('Version 0.15.0')).toBeInTheDocument();
    // No claim either way yet - the check hasn't come back.
    expect(screen.queryByText('Du hast die aktuelle Version.')).not.toBeInTheDocument();
  });

  it('shows "up to date" only once the check has answered and found nothing newer', () => {
    renderPanel(makeView({ info: { currentVersion: '0.15.0', availableVersion: null, releaseUrl: null, canInstall: true, lastUpdate: null } }));
    expect(screen.getByText('Du hast die aktuelle Version.')).toBeInTheDocument();
  });

  it('available box shows only the title and actions, no body text (matches the mockup)', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: true, lastUpdate: null },
    });
    renderPanel(view);
    expect(screen.getByText('Version 0.15.1 ist verfügbar')).toBeInTheDocument();
    expect(screen.queryByText('Du hast 0.15.0. Vor der Installation wird dein Katalog gesichert.')).not.toBeInTheDocument();
    screen.getByText('Jetzt aktualisieren').click();
    expect(view.startUpdate).toHaveBeenCalled();
  });

  it('canInstall = false shows "Zur Download-Seite" instead of the install button', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: false, lastUpdate: null },
    });
    renderPanel(view);
    expect(screen.queryByText('Jetzt aktualisieren')).not.toBeInTheDocument();
    expect(screen.getByText('Zur Download-Seite')).toBeInTheDocument();
  });

  it('shows download progress inline while downloading', () => {
    const view = makeView({
      phase: 'downloading',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null },
      progress: { downloaded: 48 * 1024 * 1024, total: 106 * 1024 * 1024 },
    });
    renderPanel(view);
    expect(screen.getByText('Update 0.15.1 wird geladen …')).toBeInTheDocument();
    expect(screen.getByText('48 von 106 MB · 45 %')).toBeInTheDocument();
  });

  it('shows "Problem melden" for an unexpected error, same as the toast', () => {
    const view = makeView({
      phase: 'error',
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: null, canInstall: true, lastUpdate: null },
      error: { message: 'Sicherung fehlgeschlagen', unexpected: true },
    });
    renderPanel(view);
    expect(screen.getByText('Sicherung fehlgeschlagen')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
    screen.getByText('Erneut versuchen').click();
    expect(view.retry).toHaveBeenCalled();
  });

  it('disables the recheck button while checking', () => {
    renderPanel(makeView({ phase: 'checking' }));
    expect(screen.getByText('Suche läuft…')).toBeDisabled();
  });

  it('disables the recheck button while downloading, ready or installing', () => {
    for (const phase of ['downloading', 'ready', 'installing'] as const) {
      const { unmount } = renderPanel(makeView({ phase }));
      expect(screen.getByText('Erneut nach Updates suchen')).toBeDisabled();
      unmount();
    }
  });

  it('calls checkNow when the recheck button is clicked', () => {
    const view = makeView();
    renderPanel(view);
    screen.getByText('Erneut nach Updates suchen').click();
    expect(view.checkNow).toHaveBeenCalled();
  });

  it('shows the last-update line directly under "up to date", formatted without a timezone shift', () => {
    // The backend writes a local YYYY-MM-DD with no time; parsing that as UTC
    // midnight (`new Date(iso)`) would show 02.10. west of UTC. Building the
    // date from the y/m/d parts avoids that regardless of the machine's zone.
    renderPanel(
      makeView({
        info: {
          currentVersion: '0.15.1',
          availableVersion: null,
          releaseUrl: null,
          canInstall: true,
          lastUpdate: { version: '0.15.1', date: '2026-10-03', backupFile: 'catalog-vor-0.15.1.db' },
        },
      }),
    );
    const upToDate = screen.getByText('Du hast die aktuelle Version.');
    const lastInfo = screen.getByText('Aktualisiert am 03.10.2026 · Sicherung: catalog-vor-0.15.1.db');
    expect(upToDate.compareDocumentPosition(lastInfo) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('shows no last-update line when there is none', () => {
    renderPanel(makeView({ info: { currentVersion: '0.15.0', availableVersion: null, releaseUrl: null, canInstall: true, lastUpdate: null } }));
    expect(screen.queryByText(/Sicherung:/)).not.toBeInTheDocument();
  });

  it('shows the preview note as the first line of the update box on a preview build', () => {
    const view = makeView({
      preview: true,
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: true, lastUpdate: null },
    });
    renderPanel(view);
    expect(screen.getByText('Test-Version – eigener Katalog, getrennt von deiner normalen App.')).toBeInTheDocument();
  });

  it('shows no preview note on a normal build', () => {
    const view = makeView({
      preview: false,
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: true, lastUpdate: null },
    });
    renderPanel(view);
    expect(
      screen.queryByText('Test-Version – eigener Katalog, getrennt von deiner normalen App.'),
    ).not.toBeInTheDocument();
  });
});
