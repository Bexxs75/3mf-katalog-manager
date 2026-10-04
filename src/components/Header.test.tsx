import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Header } from './Header';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function renderHeader(mainView: 'catalog' | 'filament' | 'trash') {
  return render(
    <LanguageProvider>
      <Header
        mainView={mainView} view="grid" sort="name" count={0}
        onViewChange={vi.fn()} onSortChange={vi.fn()}
        onImportFiles={vi.fn()} onImportFolder={vi.fn()}
        allFoldersCollapsed={false} onToggleAllFolders={vi.fn()}
      />
    </LanguageProvider>,
  );
}

describe('Header', () => {
  it.each(['catalog', 'trash'] as const)('omits the manager label in %s', (mainView) => {
    renderHeader(mainView);
    expect(screen.getByText('3MF Katalog')).toBeVisible();
    expect(screen.queryByText(/MANAGER/i)).not.toBeInTheDocument();
  });

  it('shows the material manager label in the filament view', () => {
    renderHeader('filament');
    expect(screen.getByText('3MF Katalog')).toBeVisible();
    expect(screen.getByText('MATERIAL MANAGER')).toBeVisible();
    expect(screen.getByText('MATERIAL MANAGER')).toHaveClass(
      'font-mono-ui', 'text-[11px]', 'text-[var(--accent)]', 'tracking-[0.08em]',
    );
  });
});

it.each(['grid', 'groupedGrid', 'groupedList'] as const)('shows the global toggle only in folder views: %s', (view) => {
  const onToggle = vi.fn();
  const props = { mainView: 'catalog' as const, view, sort: 'name' as const, count: 0,
    onViewChange: vi.fn(), onSortChange: vi.fn(), onImportFiles: vi.fn(), onImportFolder: vi.fn(),
    allFoldersCollapsed: false, onToggleAllFolders: onToggle };
  const { rerender } = render(<LanguageProvider><Header {...props} /></LanguageProvider>);
  if (view === 'grid') {
    expect(screen.queryByRole('button', { name: 'Alle Ordner zuklappen' })).toBeNull();
  } else {
    fireEvent.click(screen.getByRole('button', { name: 'Alle Ordner zuklappen' }));
    expect(onToggle).toHaveBeenCalledOnce();
    rerender(<LanguageProvider><Header {...props} allFoldersCollapsed /></LanguageProvider>);
    expect(screen.getByRole('button', { name: 'Alle Ordner aufklappen' })).toBeVisible();
  }
});
