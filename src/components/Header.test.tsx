import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Header } from './Header';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function renderHeader(mainView: 'catalog' | 'filament' | 'trash') {
  return render(
    <LanguageProvider>
      <Header
        mainView={mainView} view="grid" sort="name" sortDirection="asc" onSortDirectionChange={vi.fn()} count={0}
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
      'ui-label', 'text-[var(--accent)]',
    );
  });
});

it.each(['grid', 'groupedGrid', 'groupedList'] as const)('shows the global toggle only in folder views: %s', (view) => {
  const onToggle = vi.fn();
  const props = { mainView: 'catalog' as const, view, sort: 'name' as const, sortDirection: 'asc' as const, onSortDirectionChange: vi.fn(), count: 0,
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

it('labels Printer Manager and hides all catalog controls', () => {
  render(<LanguageProvider><Header mainView="printers" allFoldersCollapsed={false} onToggleAllFolders={vi.fn()}
    view="grid" onViewChange={vi.fn()} sort="name" sortDirection="asc" onSortDirectionChange={vi.fn()} onSortChange={vi.fn()} count={0}
    onImportFiles={vi.fn()} onImportFolder={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText('PRINTER MANAGER')).toBeInTheDocument();
  expect(screen.getAllByRole('button')).toHaveLength(1);
  expect(screen.getByRole('button', { name: 'Tastenkürzel und Tipps' })).toBeVisible();
});

it.each([undefined, 'Küche'])('explains the target for file and folder actions: %s', target => {
  render(<LanguageProvider><Header mainView="catalog" view="grid" sort="name" sortDirection="asc" onSortDirectionChange={vi.fn()} count={0} onViewChange={vi.fn()} onSortChange={vi.fn()} onImportFiles={vi.fn()} onImportFolder={vi.fn()} allFoldersCollapsed={false} onToggleAllFolders={vi.fn()} importTargetName={target} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', {name: /Importieren/}));
  expect(screen.getByText(/Ziel wird beim Start festgehalten/)).toBeVisible();
  if (target) expect(screen.getByText('Küche')).toBeVisible();
  else expect(screen.getByText('Ausgewählte Dateien werden katalogisiert und bleiben an ihrem Ort.')).toBeVisible();
  const folder = screen.getByRole('menuitem', {name: 'Ordner...'});
  fireEvent.focus(folder); expect(screen.getByText('Dateien bleiben an ihrem Ort, Unterordner werden als Katalogordner übernommen.')).toBeVisible();
  fireEvent.mouseEnter(screen.getByRole('menuitem', {name: 'Dateien...'})); expect(screen.getByText(/Ziel wird beim Start festgehalten/)).toBeVisible();
});

it('pins details with an accessible toggle and pressed state', () => {
  const change = vi.fn();
  const props = { mainView: 'catalog' as const, view: 'grid' as const, sort: 'name' as const, sortDirection: 'asc' as const,
    onSortDirectionChange: vi.fn(), count: 0, onViewChange: vi.fn(), onSortChange: vi.fn(), onImportFiles: vi.fn(),
    onImportFolder: vi.fn(), allFoldersCollapsed: false, onToggleAllFolders: vi.fn(), onDetailPanelChange: change };
  const { rerender } = render(<LanguageProvider><Header {...props} detailPanel="auto" /></LanguageProvider>);
  const toggle = screen.getByRole('button', { name: 'Detailbereich festhalten' });
  expect(toggle).toHaveAttribute('aria-pressed', 'false');
  expect(toggle).toHaveAttribute('title', 'Detailbereich festhalten');
  fireEvent.click(toggle);
  expect(change).toHaveBeenCalledWith('pinned');
  rerender(<LanguageProvider><Header {...props} detailPanel="pinned" /></LanguageProvider>);
  expect(toggle).toHaveAttribute('aria-pressed', 'true');
  fireEvent.click(toggle);
  expect(change).toHaveBeenLastCalledWith('auto');
});
