import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UpdatePanel } from './UpdatePanel';
import type { UpdaterView } from '../hooks/useUpdater';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

function makeView(overrides: Partial<UpdaterView> = {}): UpdaterView {
  return {
    info: { currentVersion: '0.15.0', availableVersion: null, releaseUrl: null, canInstall: true, lastUpdate: null },
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

function renderPanel(view: UpdaterView) {
  render(
    <LanguageProvider>
      <UpdatePanel view={view} />
    </LanguageProvider>,
  );
}

describe('UpdatePanel', () => {
  it('shows the current version and "up to date" when nothing is available', () => {
    renderPanel(makeView());
    expect(screen.getByText('Version 0.15.0')).toBeInTheDocument();
    expect(screen.getByText('Du hast die aktuelle Version.')).toBeInTheDocument();
  });

  it('shows the available-update box with the same actions as the toast', () => {
    const view = makeView({
      info: { currentVersion: '0.15.0', availableVersion: '0.15.1', releaseUrl: 'https://example.com', canInstall: true, lastUpdate: null },
    });
    renderPanel(view);
    expect(screen.getByText('Version 0.15.1 ist verfügbar')).toBeInTheDocument();
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

  it('disables the recheck button while checking', () => {
    renderPanel(makeView({ phase: 'checking' }));
    expect(screen.getByText('Suche läuft…')).toBeDisabled();
  });

  it('calls checkNow when the recheck button is clicked', () => {
    const view = makeView();
    renderPanel(view);
    screen.getByText('Erneut nach Updates suchen').click();
    expect(view.checkNow).toHaveBeenCalled();
  });

  it('shows the last-update line with a formatted date when present', () => {
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
    expect(screen.getByText('Aktualisiert am 03.10.2026 · Sicherung: catalog-vor-0.15.1.db')).toBeInTheDocument();
  });

  it('shows no last-update line when there is none', () => {
    renderPanel(makeView());
    expect(screen.queryByText(/Sicherung:/)).not.toBeInTheDocument();
  });
});
