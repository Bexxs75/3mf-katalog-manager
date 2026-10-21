import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
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
    otherFilesExtracted: 0,
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

  it('shows the note about non-model files as a plural line and keeps the banner for 15s', () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    renderBanner([outcome({ otherFilesExtracted: 2 }), outcome({ path: '/dl/b.zip', otherFilesExtracted: 1 })], onClose);
    expect(screen.getByText(/3 weitere Dateien wurden entpackt, aber nicht in den Katalog aufgenommen/)).toBeInTheDocument();
    vi.advanceTimersByTime(6000);
    expect(onClose).not.toHaveBeenCalled();
    vi.advanceTimersByTime(10000);
    expect(onClose).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it('uses the singular and omits the note when no other files were extracted', () => {
    const { unmount } = render(
      <LanguageProvider>
        <ImportSummaryBanner imported={1} duplicates={0} archives={[outcome({ otherFilesExtracted: 1 })]} onClose={vi.fn()} />
      </LanguageProvider>,
    );
    expect(screen.getByText(/^1 weitere Datei wurde entpackt/)).toBeInTheDocument();
    unmount();
    renderBanner([outcome({ otherFilesExtracted: 0 })]);
    expect(screen.queryByText(/weitere Datei/)).not.toBeInTheDocument();
  });

  it('keeps the banner open with the note when there are also problems', () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    renderBanner([outcome({ otherFilesExtracted: 1 }), outcome({ path: '/dl/kaputt.zip', extractedTo: null, error: 'beschaedigt' })], onClose);
    vi.advanceTimersByTime(20000);
    expect(onClose).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});

it('shows the stripped folder only when the result supplies it', () => {
  renderBanner([outcome({ strippedRoot: 'Garten-Paket' }), outcome({ path: '/dl/b.zip' })]);
  expect(screen.getByText('Innerer Ordner „Garten-Paket“ wurde übersprungen')).toBeInTheDocument();
  expect(screen.getAllByRole('listitem')).toHaveLength(1);
});


it.each(['error', 'deleteError'] as const)('exposes bidi file paths embedded in archive %s without changing the outcome', field => {
  const message = 'Cannot read /models/part\u202etxt.3mf';
  const archive = Object.freeze(outcome({[field]: message}));
  renderBanner([archive]);
  expect(screen.getByText(/Cannot read \/models\/part\[U\+202E\]txt\.3mf/)).toBeVisible();
  expect(archive[field]).toBe(message);
});

describe('ImportSummaryBanner "view recently added" button', () => {
  function renderWith(imported: number, onShowNew?: () => void, onClose: () => void = vi.fn()) {
    render(
      <LanguageProvider>
        <ImportSummaryBanner imported={imported} duplicates={1} onClose={onClose} onShowNew={onShowNew} />
      </LanguageProvider>,
    );
  }

  it('is a real button that calls back and closes the notice', () => {
    const onShowNew = vi.fn();
    const onClose = vi.fn();
    renderWith(3, onShowNew, onClose);
    const button = screen.getByRole('button', { name: 'Neu hinzugefügt ansehen' });
    button.focus();
    expect(button).toHaveFocus();
    fireEvent.click(button);
    expect(onShowNew).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('is absent when only duplicates were found or no handler is given', () => {
    renderWith(0, vi.fn());
    expect(screen.queryByRole('button', { name: 'Neu hinzugefügt ansehen' })).not.toBeInTheDocument();
    renderWith(3);
    expect(screen.queryByRole('button', { name: 'Neu hinzugefügt ansehen' })).not.toBeInTheDocument();
  });
});
