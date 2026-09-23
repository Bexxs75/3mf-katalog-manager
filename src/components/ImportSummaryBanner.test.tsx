import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ImportSummaryBanner } from './ImportSummaryBanner';
import type { ArchiveOutcome } from '../types';

function outcome(overrides: Partial<ArchiveOutcome>): ArchiveOutcome {
  return {
    path: '/dl/a.zip',
    extractedTo: '/k/a',
    existingSkipped: 0,
    unsafeSkipped: 0,
    blockedSkipped: 0,
    archiveDeleted: false,
    deleteError: null,
    error: null,
    ...overrides,
  };
}

function renderBanner(archives?: ArchiveOutcome[]) {
  render(
    <LanguageProvider>
      <ImportSummaryBanner imported={3} duplicates={1} archives={archives} onClose={vi.fn()} />
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
    expect(screen.getByText('1 unsichere Einträge übersprungen')).toBeInTheDocument();
    expect(screen.getByText('2 Programme/Verknüpfungen aus Sicherheitsgründen nicht entpackt')).toBeInTheDocument();
    expect(screen.getByText('2 Archive gelöscht')).toBeInTheDocument();
    expect(screen.getByText('kaputt.zip fehlgeschlagen: beschaedigt')).toBeInTheDocument();
    expect(screen.getByText('c.rar nicht gelöscht: veraendert')).toBeInTheDocument();
  });
});
