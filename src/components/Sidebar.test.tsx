import { ImportLockContext } from '../hooks/ImportLockContext';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
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
  onDragFolderStart: vi.fn(), query: '', onQueryChange: vi.fn(), queue: [], onQueueReorder: vi.fn(),
  onQueueRemove: vi.fn(), onQueueSelect: vi.fn(), totalModelCount: 1,
  folders: [
    { id: 'a', name: 'Parent', parentId: null, path: '/a', count: 1 },
    { id: 'b', name: 'Child', parentId: 'a', path: '/a/b', count: 1 },
  ],
  activeFolderId: 'a', onFolderSelect: vi.fn(), onCreateFolder: vi.fn(),
  tags: [], activeTag: null, onTagSelect: vi.fn(), collections: [],
  activeCollection: null, collectionsGalleryOpen: false, onSelectCollection: vi.fn(),
  onOpenCollectionsGallery: vi.fn(), onCreateCollection: vi.fn(), onRenameCollection: vi.fn(), onDeleteCollection: vi.fn(), toolView: null,
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

it('locks new folder, cleanup, and folder drag during an import', () => {
  baseProps.onDragFolderStart.mockClear();
  render(<LanguageProvider><ImportLockContext.Provider value={true}><Harness /></ImportLockContext.Provider></LanguageProvider>);
  const create = screen.getByRole('button', {name: /Neuer Ordner/});
  expect(create).toBeDisabled(); expect(create).toHaveAttribute('title','Während eines Imports gesperrt');
  const tools = screen.getByRole('button',{name:/Werkzeuge/});
  if (tools.getAttribute('aria-expanded') === 'false') fireEvent.click(tools);
  const cleanup = screen.getByRole('button', {name: /Aufräum/}); expect(cleanup).toBeDisabled(); expect(cleanup).toHaveAttribute('title','Während eines Imports gesperrt');
  fireEvent.mouseDown(screen.getByText('Parent'),{clientX:10,clientY:10}); fireEvent.mouseMove(document,{clientX:40,clientY:40});
  expect(baseProps.onDragFolderStart).not.toHaveBeenCalled();
});

it('filters expanded tags, includes rare matches and handles Enter, clear and Escape', () => {
  const onTagSelect = vi.fn();
  const tags = [{label: 'BÄREN', count: 1, colorHue: 12}, {label: 'Zubehör', count: 2, colorHue: 30}];
  render(<LanguageProvider><Sidebar {...baseProps} tags={tags} onTagSelect={onTagSelect}
    expansion={{expanded: new Set<string>(), toggle: vi.fn(), isExpanded: () => false, setAll: vi.fn(), expand: vi.fn()}}
    width={242} setWidth={vi.fn()} resetWidth={vi.fn()} allFoldersCollapsed onToggleAllFolders={vi.fn()} /></LanguageProvider>);
  expect(screen.queryByPlaceholderText('Tags filtern …')).toBeNull();
  fireEvent.click(screen.getByText('Tags'));
  const input = screen.getByPlaceholderText('Tags filtern …');
  expect(screen.queryByText('#BÄREN')).toBeNull();
  fireEvent.change(input, {target: {value: 'bär'}});
  expect(screen.getByText('#BÄREN')).toBeVisible();
  expect(screen.queryByText('#Zubehör')).toBeNull();
  fireEvent.keyDown(input, {key: 'Enter'});
  expect(onTagSelect).toHaveBeenCalledWith('BÄREN');
  fireEvent.click(screen.getByLabelText('Tags filtern … Löschen'));
  expect(input).toHaveValue('');
  fireEvent.change(input, {target: {value: 'xyz'}});
  expect(screen.getByText('Keine Tags gefunden')).toBeVisible();
  fireEvent.click(screen.getByText('Tags'));
  fireEvent.click(screen.getByText('Tags'));
  expect(screen.getByPlaceholderText('Tags filtern …')).toHaveValue('xyz');
  fireEvent.keyDown(screen.getByPlaceholderText('Tags filtern …'), {key: 'Escape'});
  expect(screen.getByPlaceholderText('Tags filtern …')).toHaveValue('');
});

it('resets tag search only when the catalog changes and retains the active rare tag', () => {
  const tags = [{label: 'Rare', count: 1, colorHue: 20}, {label: 'Alpha', count: 3, colorHue: 30}];
  const page = (catalogKey: string) => <LanguageProvider><Sidebar {...baseProps} catalogKey={catalogKey}
    tags={tags} activeTag="Rare" expansion={{expanded: new Set<string>(), toggle: vi.fn(), isExpanded: () => false, setAll: vi.fn(), expand: vi.fn()}}
    width={242} setWidth={vi.fn()} resetWidth={vi.fn()} allFoldersCollapsed onToggleAllFolders={vi.fn()} /></LanguageProvider>;
  const {rerender} = render(page('one'));
  fireEvent.click(screen.getByText('Tags'));
  expect(screen.getByText('#Rare')).toHaveClass('text-[var(--accent)]');
  fireEvent.change(screen.getByPlaceholderText('Tags filtern …'), {target: {value: 'Alpha'}});
  rerender(page('one'));
  expect(screen.getByPlaceholderText('Tags filtern …')).toHaveValue('Alpha');
  rerender(page('two'));
  expect(screen.getByPlaceholderText('Tags filtern …')).toHaveValue('');
});

function collectionSidebar(locked = false, onDelete = vi.fn(), onRename = vi.fn()) {
  return render(<LanguageProvider><ImportLockContext.Provider value={locked}><Sidebar {...baseProps}
    collections={[{id: 'c1', name: 'Kitchen', modelCount: 2}]} onDeleteCollection={onDelete} onRenameCollection={onRename}
    expansion={{expanded: new Set<string>(), toggle: vi.fn(), isExpanded: () => false, setAll: vi.fn(), expand: vi.fn()}}
    width={242} setWidth={vi.fn()} resetWidth={vi.fn()} allFoldersCollapsed onToggleAllFolders={vi.fn()} />
  </ImportLockContext.Provider></LanguageProvider>);
}

it('opens the collection menu on right-click and deletes only after confirmation, returning focus to the heading', async () => {
  const onDelete = vi.fn(); collectionSidebar(false, onDelete);
  const row = screen.getByRole('button', {name: 'Kitchen'});
  fireEvent.contextMenu(row, {clientX: 80, clientY: 90});
  expect(screen.getByRole('button', {name: 'Löschen'}).parentElement).toHaveStyle({left: '80px', top: '90px'});
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  expect(screen.getByRole('dialog')).toBeVisible();
  fireEvent.click(screen.getByRole('button', {name: 'Abbrechen'}));
  expect(onDelete).not.toHaveBeenCalled(); expect(row).toHaveFocus();
  fireEvent.contextMenu(row);
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  await act(async () => fireEvent.click(screen.getByRole('button', {name: 'Löschen'})));
  expect(onDelete).toHaveBeenCalledExactlyOnceWith('c1');
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.getByText('Sammlungen')).toHaveFocus();
});

it('renames with Enter, ignores empty and unchanged names, and discards with Escape', async () => {
  const onRename = vi.fn(); collectionSidebar(false, vi.fn(), onRename);
  const row = screen.getByRole('button', {name: 'Kitchen'});
  const open = () => { fireEvent.contextMenu(row); fireEvent.click(screen.getByRole('button', {name: 'Umbenennen'})); return screen.getByLabelText('Sammlung umbenennen'); };
  let input = open(); expect(input).toHaveFocus();
  fireEvent.change(input, {target: {value: '  New  '}});
  await act(async () => fireEvent.keyDown(input, {key: 'Enter'}));
  expect(onRename).toHaveBeenCalledExactlyOnceWith('c1', 'New'); expect(row).toHaveFocus();
  onRename.mockClear();
  input = open(); fireEvent.change(input, {target: {value: 'Discard'}}); fireEvent.keyDown(input, {key: 'Escape'});
  expect(onRename).not.toHaveBeenCalled(); expect(row).toHaveFocus();
  for (const value of ['', ' Kitchen ']) {
    input = open(); fireEvent.change(input, {target: {value}});
    await act(async () => fireEvent.keyDown(input, {key: 'Enter'}));
  }
  expect(onRename).not.toHaveBeenCalled();
});

it('opens with the menu key and Shift+F10, closes with Escape and outside click, and clamps to the window', () => {
  collectionSidebar(); const row = screen.getByRole('button', {name: 'Kitchen'});
  row.focus(); fireEvent.keyDown(row, {key: 'ContextMenu'});
  expect(screen.getByRole('button', {name: 'Umbenennen'})).toHaveFocus();
  fireEvent.keyDown(document, {key: 'Escape'}); expect(row).toHaveFocus();
  fireEvent.keyDown(row, {key: 'F10', shiftKey: true});
  expect(screen.getByRole('button', {name: 'Umbenennen'})).toBeVisible();
  fireEvent.mouseDown(document.body); expect(row).toHaveFocus();
  const geometry = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    x: 0, y: 0, left: 0, top: 0, right: 172, bottom: 70, width: 172, height: 70, toJSON: () => ({}),
  });
  fireEvent.contextMenu(row, {clientX: 2000, clientY: 2000});
  expect(screen.getByRole('button', {name: 'Umbenennen'}).parentElement).toHaveStyle({left: `${window.innerWidth - 172}px`, top: `${window.innerHeight - 70}px`});
  geometry.mockRestore();
});

it('disables both collection actions during import', () => {
  collectionSidebar(true); fireEvent.contextMenu(screen.getByRole('button', {name: 'Kitchen'}));
  for (const name of ['Umbenennen', 'Löschen']) {
    expect(screen.getByRole('button', {name})).toBeDisabled();
    expect(screen.getByRole('button', {name})).toHaveAttribute('title', 'Während eines Imports gesperrt');
  }
});

it('keeps a failed deletion open and permits a retry', async () => {
  const onDelete = vi.fn().mockRejectedValueOnce(new Error('Failure')).mockResolvedValue(undefined);
  collectionSidebar(false, onDelete); fireEvent.contextMenu(screen.getByRole('button', {name: 'Kitchen'}));
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  await act(async () => fireEvent.click(screen.getByRole('button', {name: 'Löschen'})));
  expect(screen.getByRole('alert')).toHaveTextContent('Failure'); expect(screen.getByRole('dialog')).toBeVisible();
  await act(async () => fireEvent.click(screen.getByRole('button', {name: 'Löschen'})));
  expect(screen.queryByRole('dialog')).toBeNull();
});

it('prevents duplicate deletion and dismissal while the handler is pending', async () => {
  let complete!: () => void;
  const onDelete = vi.fn(() => new Promise<void>(resolve => { complete = resolve; }));
  collectionSidebar(false, onDelete);
  fireEvent.contextMenu(screen.getByRole('button', {name: 'Kitchen'}));
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  expect(screen.getByRole('button', {name: 'Löschen'})).toBeDisabled();
  expect(screen.getByRole('button', {name: 'Abbrechen'})).toBeDisabled();
  fireEvent.keyDown(document, {key: 'Escape'});
  expect(screen.getByRole('dialog')).toBeVisible();
  await act(async () => complete());
  expect(onDelete).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole('dialog')).toBeNull();
});

it('shows rename errors and retains the draft for correction', async () => {
  const onRename = vi.fn().mockRejectedValue(new Error('Invalid name'));
  collectionSidebar(false, vi.fn(), onRename);
  fireEvent.contextMenu(screen.getByRole('button', {name: 'Kitchen'}));
  fireEvent.click(screen.getByRole('button', {name: 'Umbenennen'}));
  const input = screen.getByLabelText('Sammlung umbenennen');
  fireEvent.change(input, {target: {value: 'New'}});
  await act(async () => fireEvent.keyDown(input, {key: 'Enter'}));
  expect(screen.getByRole('alert')).toHaveTextContent('Invalid name');
  expect(input).toHaveValue('New'); expect(input).toBeEnabled();
});


it('reveals and hides single-model tags in alphabetical order with the correct count', () => {
  const tags = [{label: 'Zulu', count: 1, colorHue: 20}, {label: 'Beta', count: 3, colorHue: 30}, {label: 'Alpha', count: 1, colorHue: 50}];
  render(<LanguageProvider><Sidebar {...baseProps} tags={tags}
    expansion={{expanded: new Set<string>(), toggle: vi.fn(), isExpanded: () => false, setAll: vi.fn(), expand: vi.fn()}}
    width={242} setWidth={vi.fn()} resetWidth={vi.fn()} allFoldersCollapsed onToggleAllFolders={vi.fn()} /></LanguageProvider>);
  fireEvent.click(screen.getByText('Tags'));
  const toggle = screen.getByRole('button', {name: '2 weitere Tags mit nur einem Modell anzeigen'});
  expect(toggle).toHaveAttribute('aria-expanded', 'false');
  expect(screen.queryByText('#Alpha')).toBeNull();
  fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  expect(toggle).toHaveTextContent('Nur Tags mit mindestens 2 Modellen anzeigen');
  expect(Array.from(screen.getByText('#Alpha').parentElement!.querySelectorAll('button')).map(node => node.textContent?.replace(/[0-9]/g, '').trim())).toEqual(['#Alpha', '#Beta', '#Zulu']);
  fireEvent.change(screen.getByPlaceholderText('Tags filtern …'), {target: {value: 'Alpha'}});
  expect(screen.queryByRole('button', {name: 'Nur Tags mit mindestens 2 Modellen anzeigen'})).toBeNull();
  expect(screen.getByText('#Alpha')).toBeVisible();
  fireEvent.change(screen.getByPlaceholderText('Tags filtern …'), {target: {value: ''}});
  fireEvent.click(screen.getByRole('button', {name: 'Nur Tags mit mindestens 2 Modellen anzeigen'}));
  expect(screen.queryByText('#Alpha')).toBeNull();
});

it('has no single-model tag toggle when there are no single-model tags', () => {
  render(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.click(screen.getByText('Tags'));
  expect(screen.queryByRole('button', {name: /weitere Tags/})).toBeNull();
});

it.each([
  ['Projekt: Test', 'Dieses Zeichen ist in Ordnernamen nicht erlaubt: :'],
  ['a?', 'Dieses Zeichen ist in Ordnernamen nicht erlaubt: ?'],
  ['Neu.', 'Ordnernamen dürfen nicht mit einem Punkt oder Leerzeichen enden.'],
  ['CON', 'Dieser Name ist als Ordnername nicht erlaubt.'],
  ['a'.repeat(256), 'Der Name ist zu lang.'],
  ['a\u0001', 'Dieses Zeichen ist in Ordnernamen nicht erlaubt: U+0001'],
])('keeps invalid folder draft %j and describes the problem accessibly', (name, hint) => {
  baseProps.onCreateFolder.mockClear();
  render(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: '+ Neuer Ordner' }));
  const input = screen.getByPlaceholderText('Ordnername');
  fireEvent.change(input, { target: { value: name } });
  const alert = screen.getByRole('alert');
  expect(alert).toHaveTextContent(hint);
  expect(input).toHaveAttribute('aria-invalid', 'true');
  expect(input).toHaveAttribute('aria-describedby', alert.id);
  expect(input).toHaveAccessibleDescription(hint);
  fireEvent.blur(input);
  expect(screen.queryByRole('dialog')).toBeNull();
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(baseProps.onCreateFolder).not.toHaveBeenCalled();
  expect(input).toHaveValue(name);
  expect(alert).toBeVisible();
  const dialog = screen.getByRole('dialog', { name: 'Dieser Ordnername geht nicht' });
  expect(within(dialog).getByRole('alert')).toHaveTextContent(hint);
  fireEvent.click(within(dialog).getByRole('button', { name: 'Verstanden' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(input).toHaveValue(name);
  fireEvent.change(input, { target: { value: 'Projekt 2026' } });
  expect(screen.queryByRole('alert')).toBeNull();
  expect(input).not.toHaveAttribute('aria-describedby');
  expect(input).not.toHaveAttribute('aria-invalid', 'true');
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(baseProps.onCreateFolder).toHaveBeenCalledExactlyOnceWith('a', 'Projekt 2026');
});

it('trims a trailing space instead of treating it as an invalid folder name', () => {
  baseProps.onCreateFolder.mockClear();
  render(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: '+ Neuer Ordner' }));
  const input = screen.getByPlaceholderText('Ordnername');
  fireEvent.change(input, { target: { value: 'Neu ' } });
  expect(screen.queryByRole('alert')).toBeNull();
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(baseProps.onCreateFolder).toHaveBeenCalledExactlyOnceWith('a', 'Neu');
});

it.each(['', '   '])('cancels an empty folder draft %j without a hint', (name) => {
  baseProps.onCreateFolder.mockClear();
  render(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: '+ Neuer Ordner' }));
  const input = screen.getByPlaceholderText('Ordnername');
  fireEvent.change(input, { target: { value: name } });
  expect(screen.queryByRole('alert')).toBeNull();
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(screen.queryByPlaceholderText('Ordnername')).toBeNull();
  expect(baseProps.onCreateFolder).not.toHaveBeenCalled();
});

it('opens the folder-name dialog only on Enter, not when the field loses focus', () => {
  baseProps.onCreateFolder.mockClear();
  render(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: '+ Neuer Ordner' }));
  const input = screen.getByPlaceholderText('Ordnername');
  fireEvent.change(input, { target: { value: 'Projekt: Test' } });
  fireEvent.blur(input);
  expect(screen.queryByRole('dialog')).toBeNull();
  fireEvent.keyDown(input, { key: 'Enter' });
  const dialog = screen.getByRole('dialog', { name: 'Dieser Ordnername geht nicht' });
  expect(within(dialog).getByRole('button', { name: 'Verstanden' })).toHaveFocus();
  fireEvent.keyDown(dialog, { key: 'Escape' });
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(baseProps.onCreateFolder).not.toHaveBeenCalled();
});
