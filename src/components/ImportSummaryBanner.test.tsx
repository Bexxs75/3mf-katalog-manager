import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ImportSummaryBanner } from './ImportSummaryBanner';
import type { ArchiveOutcome, SkippedFile } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

function outcome(overrides: Partial<ArchiveOutcome>): ArchiveOutcome {
  return {
    path: '/dl/a.zip',
    extractedTo: '/k/a',
    strippedRoot: null,
    existingSkipped: 0,
    unsafeSkipped: 0,
    blockedSkipped: 0,
    archiveDeleted: false,
    deleteError: null,
    error: null,
    ...overrides,
  };
}

function renderBanner(archives?: ArchiveOutcome[], onClose: () => void = vi.fn()) {
  render(
    <LanguageProvider>
      <ImportSummaryBanner imported={3} duplicates={1} archives={archives} onClose={onClose} />
    </LanguageProvider>,
  );
}

describe('ImportSummaryBanner', () => {
  it('shows only the base line without archives', () => {
    renderBanner();
    expect(screen.getByText('3 importiert, 1 Duplikate übersprungen')).toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });

  it('adds summed skip counts, deletions and per-archive problems', () => {
    renderBanner([
      outcome({ existingSkipped: 2, unsafeSkipped: 1, blockedSkipped: 2, archiveDeleted: true }),
      outcome({ path: '/dl/b.7z', existingSkipped: 1, archiveDeleted: true }),
      outcome({ path: '/dl/kaputt.zip', extractedTo: null, error: 'beschaedigt' }),
      outcome({ path: '/dl/c.rar', deleteError: 'veraendert' }),
    ]);
    expect(screen.getByText('3 vorhandene Dateien übersprungen')).toBeInTheDocument();
    expect(screen.getByText('1 unsicherer Eintrag übersprungen')).toBeInTheDocument();
    expect(screen.getByText('2 Programme/Verknüpfungen aus Sicherheitsgründen nicht entpackt')).toBeInTheDocument();
    expect(screen.getByText('2 Archive gelöscht')).toBeInTheDocument();
    expect(screen.getByText('kaputt.zip fehlgeschlagen: beschaedigt')).toBeInTheDocument();
    expect(screen.getByText('c.rar nicht gelöscht: veraendert')).toBeInTheDocument();
  });

  it('does not auto-close when an archive has an error', () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    renderBanner([outcome({ path: '/dl/kaputt.zip', extractedTo: null, error: 'beschaedigt' })], onClose);
    vi.advanceTimersByTime(6000);
    expect(onClose).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('auto-closes after 5s when there are no problems', () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    renderBanner([outcome({ existingSkipped: 1 })], onClose);
    vi.advanceTimersByTime(6000);
    expect(onClose).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it('gives each same-basename failed archive its own list item', () => {
    renderBanner([
      outcome({ path: '/a/model.zip', error: 'x' }),
      outcome({ path: '/b/model.zip', error: 'x' }),
    ]);
    expect(screen.getAllByText('model.zip fehlgeschlagen: x')).toHaveLength(2);
  });

  it('offers "Report problem" for an unexpected archive failure', () => {
    renderBanner([outcome({ path: '/dl/kaputt.zip', extractedTo: null, error: 'beschaedigt', unexpected: true })]);
    expect(screen.getByText('kaputt.zip fehlgeschlagen: beschaedigt')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });

  it('does not offer "Report problem" for an expected (or unmarked) archive failure', () => {
    renderBanner([outcome({ path: '/dl/kaputt.zip', extractedTo: null, error: 'beschaedigt', unexpected: false })]);
    expect(screen.getByText('kaputt.zip fehlgeschlagen: beschaedigt')).toBeInTheDocument();
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });

  it('names skipped files per reason, shortens long lists and stays open', () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    const skipped: SkippedFile[] = [
      { path: '/k/leer.stl', reason: 'empty' },
      ...['a', 'b', 'c', 'd', 'e'].map((n) => ({ path: `/k/${n}.3mf`, reason: 'invalid' as const })),
    ];
    render(
      <LanguageProvider>
        <ImportSummaryBanner imported={0} duplicates={0} skipped={skipped} onClose={onClose} />
      </LanguageProvider>,
    );
    expect(screen.getByText('1 leere Datei nicht importiert: leer.stl')).toBeInTheDocument();
    expect(
      screen.getByText('5 Dateien beschädigt oder ohne lesbares Modell: a.3mf, b.3mf, c.3mf +2 weitere'),
    ).toBeInTheDocument();
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
    vi.advanceTimersByTime(6000);
    expect(onClose).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('offers "Report problem" when the catalog could not store a file', () => {
    render(
      <LanguageProvider>
        <ImportSummaryBanner imported={0} duplicates={0} skipped={[{ path: '/k/x.stl', reason: 'failed' }]} onClose={vi.fn()} />
      </LanguageProvider>,
    );
    expect(screen.getByText('1 Datei konnte nicht gespeichert werden: x.stl')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });
});

it('shows the stripped folder only when the result supplies it', () => {
  renderBanner([outcome({ strippedRoot: 'Garten-Paket' }), outcome({ path: '/dl/b.zip' })]);
  expect(screen.getByText('Innerer Ordner „Garten-Paket“ wurde übersprungen')).toBeInTheDocument();
  expect(screen.getAllByRole('listitem')).toHaveLength(1);
});
