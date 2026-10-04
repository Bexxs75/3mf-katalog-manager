import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Sidebar } from './Sidebar';
import { useFolderExpansion } from '../hooks/useFolderExpansion';
import { useCollapsedFolders } from '../hooks/useCollapsedFolders';
import { useSidebarWidth } from '../hooks/useSidebarWidth';

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });

function Harness({ draggedFileId = null }: { draggedFileId?: string | null }) {
  const expansion = useFolderExpansion();
  const collapsed = useCollapsedFolders();
  const sidebarWidth = useSidebarWidth();
  const allCollapsed = expansion.expanded.size === 0 && ['a', 'b'].every(collapsed.isCollapsed);
  return <>
    <output data-testid="group-state">{String(collapsed.isCollapsed('a'))}</output>
    <Sidebar expansion={expansion} width={sidebarWidth.width} setWidth={sidebarWidth.setWidth} resetWidth={sidebarWidth.reset}
      allFoldersCollapsed={allCollapsed} onToggleAllFolders={() => {
        if (allCollapsed) { expansion.setAll(['a'], true); collapsed.expandAll(); }
        else { expansion.setAll([], false); collapsed.collapseAll(['a', 'b']); }
      }}
      {...baseProps} draggedFileId={draggedFileId} />
  </>;
}
const baseProps = {
  query: '', onQueryChange: vi.fn(), queue: [], onQueueReorder: vi.fn(),
  onQueueRemove: vi.fn(), onQueueSelect: vi.fn(), totalModelCount: 1,
  folders: [
    { id: 'a', name: 'Parent', parentId: null, path: '/a', count: 1 },
    { id: 'b', name: 'Child', parentId: 'a', path: '/a/b', count: 1 },
  ],
  activeFolderId: 'a', onFolderSelect: vi.fn(), onCreateFolder: vi.fn(),
  tags: [], activeTag: null, onTagSelect: vi.fn(), collections: [],
  activeCollection: null, collectionsGalleryOpen: false, onSelectCollection: vi.fn(),
  onOpenCollectionsGallery: vi.fn(), onCreateCollection: vi.fn(), toolView: null,
  onToolViewChange: vi.fn(), toolCounts: { recent: 0, new: 0, favorites: 0, duplicateGroups: 0 },
  onOpenCleanup: vi.fn(), cleanupScanning: false, cleanupError: null,
};

it('shows search focus on the wrapper, including programmatic focus', () => {
  render(<LanguageProvider><Harness /></LanguageProvider>);
  const search = screen.getByRole('textbox');
  search.focus();
  expect(search).toHaveFocus();
  expect(search.parentElement).toHaveClass('focus-within:border-[var(--accent)]');
});

it('toggles both folder locations and preserves the selected folder', () => {
  render(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Alle Ordner zuklappen' }));
  expect(screen.getByTestId('group-state')).toHaveTextContent('true');
  expect(screen.queryByText('Child')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Alle Ordner aufklappen' }));
  expect(screen.getByText('Child')).toBeVisible();
  expect(screen.getByTestId('group-state')).toHaveTextContent('false');
  expect(screen.getByText('Parent').parentElement).toHaveClass('font-semibold');
  expect(baseProps.onFolderSelect).not.toHaveBeenCalled();
});

it('resizes by keyboard and double-click, exposing the current width', () => {
  render(<LanguageProvider><Harness /></LanguageProvider>);
  const handle = screen.getByRole('separator', { name: 'Breite der Seitenleiste' });
  expect(handle).toHaveAttribute('tabindex', '0');
  expect(handle).toHaveAttribute('aria-orientation', 'vertical');
  expect(handle).toHaveAttribute('aria-valuenow', '242');
  for (const [key, value] of [['ArrowRight', 258], ['ArrowLeft', 242], ['Home', 180], ['End', 420]]) {
    fireEvent.keyDown(handle, { key });
    expect(handle).toHaveAttribute('aria-valuenow', String(value));
  }
  fireEvent.doubleClick(handle);
  expect(handle).toHaveAttribute('aria-valuenow', '242');
});

it('drags using window events and restores selection on release and unmount', () => {
  document.body.style.userSelect = 'text';
  const { unmount } = render(<LanguageProvider><Harness /></LanguageProvider>);
  const handle = screen.getByRole('separator');
  fireEvent.mouseDown(handle, { button: 0, clientX: 302 });
  expect(document.body.style.userSelect).toBe('none');
  fireEvent.mouseMove(window, { clientX: 352 });
  expect(handle).toHaveAttribute('aria-valuenow', '292');
  fireEvent.mouseUp(window);
  expect(document.body.style.userSelect).toBe('text');
  fireEvent.mouseMove(window, { clientX: 400 });
  expect(handle).toHaveAttribute('aria-valuenow', '292');
  fireEvent.mouseDown(handle, { button: 0, clientX: 352 });
  unmount();
  expect(document.body.style.userSelect).toBe('text');
  document.body.style.userSelect = '';
});


afterEach(() => vi.useRealTimers());
it('expands after 600 ms of dragging, and cancels on leave or release', () => {
  vi.useFakeTimers();
  const { rerender } = render(<LanguageProvider><Harness draggedFileId="m1" /></LanguageProvider>);
  const parent = () => screen.getByText('Parent').parentElement!;
  fireEvent.mouseEnter(parent());
  act(() => vi.advanceTimersByTime(599));
  expect(screen.queryByText('Child')).toBeNull();
  fireEvent.mouseLeave(parent());
  act(() => vi.advanceTimersByTime(1));
  expect(screen.queryByText('Child')).toBeNull();
  fireEvent.mouseEnter(parent());
  fireEvent.mouseUp(window);
  act(() => vi.advanceTimersByTime(600));
  expect(screen.queryByText('Child')).toBeNull();
  fireEvent.mouseLeave(parent());
  fireEvent.mouseEnter(parent());
  act(() => vi.advanceTimersByTime(600));
  expect(screen.getByText('Child')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Alle Ordner zuklappen' }));
  rerender(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.mouseEnter(parent());
  act(() => vi.advanceTimersByTime(600));
  expect(screen.queryByText('Child')).toBeNull();
});
