import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { useFolderDragAndDrop } from '../hooks/useFolderDragAndDrop';
import { useDragOutTransition } from '../hooks/useDragOutTransition';
import { invoke } from '@tauri-apps/api/core';
import { makeFolder } from '../test/factories';
import { beginDragOut } from '../lib/nativeDrag';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('../lib/nativeDrag', async importOriginal => ({ ...await importOriginal<typeof import('../lib/nativeDrag')>(), useNativeDragEnabled: () => true, loadNativeDragEnabled: async () => {}, beginDragOut: vi.fn() }));
beforeEach(() => localStorage.clear());
const props = { models: [makeModelFile()], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(), displayPreference: 'thumbnail' as const };
it.each(['compact', 'comfort', 'list'])('renders no move grip even for movable models (%s)', (view) => {
  localStorage.setItem('3mf-katalog-density', view);
  const Component = view === 'list' ? ModelList : ModelGrid;
  const ui = (extra = {}) => <LanguageProvider><UiDensityProvider><Component {...props} {...extra} /></UiDensityProvider></LanguageProvider>;
  const { container, rerender } = render(ui());
  expect(container.querySelector('[data-drag-grip]')).toBeNull();
  rerender(ui({ onDragFileStart: vi.fn() }));
  expect(container.querySelector('[data-drag-grip]')).toBeNull();
  rerender(ui({ onDragFileStart: vi.fn(), readOnly: true }));
  expect(container.querySelector('[data-drag-grip]')).toBeNull();
  if (view !== 'list') {
    rerender(ui({ onDragFileStart: vi.fn(), reorderable: true }));
    expect(container.querySelector('[data-drag-grip]')).toBeNull();
  }
});

it.each(['compact', 'comfort'])('reserves equal metadata and tag heights for 0, 2 and 12 tags (%s)', density => {
  localStorage.setItem('3mf-katalog-density', density);
  const models = [0, 2, 12].map((count, i) => makeModelFile({ id: String(i), tags: Array.from({ length: count }, (_, j) => `Tag ${j}`) }));
  const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} models={models} /></UiDensityProvider></LanguageProvider>);
  const metadata = [...container.querySelectorAll<HTMLElement>('[data-card-metadata]')];
  expect(metadata).toHaveLength(3);
  expect(new Set(metadata.map(node => node.style.height)).size).toBe(1);
  expect(metadata[0].style.height).not.toBe('');
  expect(container.querySelectorAll('[data-card-tags]')).toHaveLength(3);
});

it.each(['compact', 'comfort'])('counts overflow including space for the counter and remeasures on resize (%s)', density => {
  localStorage.setItem('3mf-katalog-density', density);
  const callbacks = new Set<() => void>();
  vi.stubGlobal('ResizeObserver', class {
    constructor(private callback: () => void) { callbacks.add(callback); }
    observe() {} disconnect() { callbacks.delete(this.callback); }
  });
  let width = 140;
  const widthSpy = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) { return this.hasAttribute('data-card-tags') ? width : 800; });
  const leftSpy = vi.spyOn(HTMLElement.prototype, 'offsetLeft', 'get').mockImplementation(function (this: HTMLElement) { return Number(this.dataset.tagIndex ?? 0) * 54; });
  const chipSpy = vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockImplementation(function (this: HTMLElement) { return this.hasAttribute('data-tag-counter-probe') ? 30 : 50; });
  try {
    const tags = ['Alpha', 'Beta', 'Gamma', 'Delta'];
    const { container, rerender } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} models={[makeModelFile({ tags })]} /></UiDensityProvider></LanguageProvider>);
    const counter = container.querySelector('[data-more-tags]');
    expect(counter).toHaveTextContent('+2');
    tags.forEach(tag => expect(counter?.getAttribute('title')).toContain(tag));
    expect(counter?.getAttribute('title')).toContain('2 weitere Tags');
    act(() => { width = 100; callbacks.forEach(callback => callback()); });
    expect(container.querySelector('[data-more-tags]')).toHaveTextContent('+3');
    act(() => { width = 212; callbacks.forEach(callback => callback()); });
    expect(container.querySelector('[data-more-tags]')).toBeNull();
    rerender(<LanguageProvider><UiDensityProvider><ModelGrid {...props} models={[makeModelFile({ tags: ['Replacement'] })]} /></UiDensityProvider></LanguageProvider>);
    expect(container.querySelector('[data-more-tags]')).toBeNull();
  } finally {
    widthSpy.mockRestore(); leftSpy.mockRestore(); chipSpy.mockRestore(); vi.unstubAllGlobals();
  }
});

it('keeps the grid hidden until a positive local width has been measured', () => {
  let resize!: () => void;
  const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 0 } as DOMRect);
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { resize = callback; }
    observe() {} disconnect() {}
  });
  const props = { models: [makeModelFile({tags: []})], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(),
    onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(),
    displayPreference: 'thumbnail' as const };
  const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} /></UiDensityProvider></LanguageProvider>);
  expect(container.querySelector('.grid.gap-3\\.5')?.parentElement).toHaveStyle({ visibility: 'hidden' });
  rect.mockReturnValue({ width: 800, height: 100, top: 0 } as DOMRect);
  act(() => resize());
  expect(container.querySelector('.grid.gap-3\\.5')).toHaveStyle({gridTemplateColumns: 'repeat(3, minmax(0, 190px))'});
});

it('keeps tiles at a fixed width when the available width changes', () => {
  let resize!: () => void;
  const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 1200, height: 100, top: 0 } as DOMRect);
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { resize = callback; }
    observe() {} disconnect() {}
  });
  try {
    const props = { models: [makeModelFile({tags: []})], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(),
      onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(),
      displayPreference: 'thumbnail' as const };
    const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} /></UiDensityProvider></LanguageProvider>);
    act(() => resize());
    const grid = () => container.querySelector('.grid.gap-3\\.5') as HTMLElement;
    expect(grid()).toHaveStyle({ gridTemplateColumns: 'repeat(5, minmax(0, 190px))', justifyContent: 'start' });
    rect.mockReturnValue({ width: 900, height: 100, top: 0 } as DOMRect);
    act(() => resize());
    expect(grid()).toHaveStyle({ gridTemplateColumns: 'repeat(4, minmax(0, 190px))' });
    rect.mockReturnValue({ width: 400, height: 100, top: 0 } as DOMRect);
    act(() => resize());
    expect(grid()).toHaveStyle({ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))' });
  } finally { rect.mockRestore(); vi.unstubAllGlobals(); }
});

it('keeps web tiles at a fixed width when the available width changes', () => {
  vi.stubEnv('VITE_TARGET', 'web');
  let resize!: () => void;
  const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 1200, height: 100, top: 0 } as DOMRect);
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { resize = callback; }
    observe() {} disconnect() {}
  });
  try {
    const props = { models: [makeModelFile({tags: []})], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(),
      onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(),
      displayPreference: 'thumbnail' as const };
    const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} /></UiDensityProvider></LanguageProvider>);
    act(() => resize());
    const grid = () => container.querySelector('.grid.gap-3\\.5') as HTMLElement;
    expect(grid()).toHaveStyle({ gridTemplateColumns: 'repeat(5, minmax(0, 190px))', justifyContent: 'start' });
    rect.mockReturnValue({ width: 900, height: 100, top: 0 } as DOMRect);
    act(() => resize());
    expect(grid()).toHaveStyle({ gridTemplateColumns: 'repeat(4, minmax(0, 190px))' });
    rect.mockReturnValue({ width: 400, height: 100, top: 0 } as DOMRect);
    act(() => resize());
    expect(grid()).toHaveStyle({ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))' });
  } finally { rect.mockRestore(); vi.unstubAllGlobals(); vi.unstubAllEnvs(); }
});

it.each(['compact', 'comfort'])('disables native thumbnail dragging in both card layouts (%s)', density => {
  localStorage.setItem('3mf-katalog-density', density);
  const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props}
    models={[makeModelFile({ thumbnailImage: 'data:image/png;base64,eA==' })]} /></UiDensityProvider></LanguageProvider>);
  expect(container.querySelector('[data-model-id] img')).toHaveAttribute('draggable', 'false');
});

it.each(['compact', 'comfort', 'list'])('prevents selecting card titles while dragging (%s)', view => {
  localStorage.setItem('3mf-katalog-density', view);
  const Component = view === 'list' ? ModelList : ModelGrid;
  const { getByText } = render(<LanguageProvider><UiDensityProvider><Component {...props} /></UiDensityProvider></LanguageProvider>);
  expect(getByText(props.models[0].name)).toHaveClass('select-none');
});

it.each(['compact', 'comfort', 'list'])('shows license usage and tooltip in %s', view => {
  localStorage.setItem('3mf-katalog-density', view);
  const Component = view === 'list' ? ModelList : ModelGrid;
  for (const [license, label] of [['cc-by', 'Kommerziell erlaubt'], ['personal', 'Nur privat'], ['custom', null], [null, null]] as const) {
    const { container, unmount } = render(<LanguageProvider><UiDensityProvider><Component {...props} models={[makeModelFile({ license })]} /></UiDensityProvider></LanguageProvider>);
    const model = container.querySelector('[data-model-id]')!;
    if (label) {
      expect(model).toHaveAttribute('aria-label', `Test Model, ${label}`);
      expect(model.querySelector('[title]')).not.toBeNull();
      expect(model).toHaveTextContent(label);
    } else {
      expect(model).not.toHaveTextContent('Kommerziell erlaubt');
      expect(model).not.toHaveTextContent('Nur privat');
    }
    unmount();
  }
});


it.each(['compact', 'comfort', 'list'])('exposes bidi names in text and accessible labels without changing model data (%s)', view => {
  localStorage.setItem('3mf-katalog-density', view);
  const model = Object.freeze(makeModelFile({ name: 'part\u202etxt.3mf', path: '/models/part\u202etxt.3mf' }));
  const select = vi.fn();
  const Component = view === 'list' ? ModelList : ModelGrid;
  const { container, getByText } = render(<LanguageProvider><UiDensityProvider><Component {...props} models={[model]} onSelect={select} /></UiDensityProvider></LanguageProvider>);
  const name = getByText('part[U+202E]txt.3mf');
  expect(container.textContent).not.toContain('\u202e');
  expect(name.closest('[aria-label]')?.getAttribute('aria-label')).toContain('part[U+202E]txt.3mf');
  fireEvent.click(name);
  expect(select).toHaveBeenCalledWith(model.id);
  expect(model.name).toBe('part\u202etxt.3mf');
  expect(model.path).toBe('/models/part\u202etxt.3mf');
});

it.each([false, true])('retains metadata selection and collection drag with web fileOps=%s', async fileOps => {
  const { login, authenticationFailed } = await import('../lib/webSession');
  vi.stubEnv('VITE_TARGET', 'web');
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ authenticated: true, writable: true, fileOps, csrfToken: 'csrf' })));
  try {
    await act(async () => { await login('test'); });
    const select = vi.fn();
    const drag = vi.fn();
    const { container, unmount } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} onSelect={select} onDragFileStart={drag} /></UiDensityProvider></LanguageProvider>);
    // Drag also supports DB-only collection assignment; folder drops check fileOps themselves.
    expect(container.querySelector('[data-drag-grip]')).toBeNull();
    const card = container.querySelector('[data-model-id]');
    expect(card).not.toBeNull();
    fireEvent.click(card!);
    expect(select).toHaveBeenCalledWith(props.models[0].id);
    fireEvent.mouseDown(card!, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.mouseMove(document, { buttons: 1, clientX: 20, clientY: 10 });
    expect(drag).toHaveBeenCalledWith(props.models[0].id);
    fireEvent.mouseUp(document);
    unmount();
  } finally {
    act(() => authenticationFailed(401));
    vi.unstubAllGlobals(); vi.unstubAllEnvs();
  }
});


it('shortens long tag chips in ModelList and exposes the full tooltip', () => {
  const tag = 'a'.repeat(95);
  const { getByText } = render(<LanguageProvider><ModelList {...props} models={[makeModelFile({ tags: [tag] })]} /></LanguageProvider>);
  const text = getByText('#' + 'a'.repeat(28) + '…');
  expect(text).toHaveClass('whitespace-nowrap');
  expect(text.parentElement).toHaveAttribute('title', '#' + tag);
  expect(text.parentElement).toHaveClass('shrink-0');
});

it.each(['compact', 'comfort', 'list-compact', 'list-comfort'])('has no drag handles and still starts a card gesture (%s)', view => {
  const list = view.startsWith('list-');
  localStorage.setItem('3mf-katalog-density', view.replace('list-', ''));
  const Component = list ? ModelList : ModelGrid;
  const move = vi.fn();
  const { container, getByLabelText } = render(<LanguageProvider><UiDensityProvider><Component {...props}
    models={[makeModelFile({ path: '/model.stl', license: 'cc-by' })]} onDragFileStart={move} /></UiDensityProvider></LanguageProvider>);
  expect(container.querySelector('[data-drag-grip]')).toBeNull();
  expect(container.querySelector('button[aria-hidden="true"]')).toBeNull();
  if (!list) {
    const favorite = getByLabelText('Zu Favoriten hinzufügen');
    expect(favorite).toHaveClass('min-w-6', 'min-h-6');
    expect(favorite).toHaveClass(view === 'compact' ? 'bottom-[7px]' : 'bottom-2.5');
  }
  const card = container.querySelector('[data-model-id]')!;
  fireEvent.mouseDown(card, { button: 0, clientX: 10, clientY: 10 });
  fireEvent.mouseMove(document, { buttons: 1, clientX: 15, clientY: 10 });
  expect(move).not.toHaveBeenCalled();
  fireEvent.mouseMove(document, { buttons: 1, clientX: 16, clientY: 10 });
  fireEvent.mouseMove(document, { buttons: 1, clientX: 20, clientY: 10 });
  expect(move).toHaveBeenCalledExactlyOnceWith(props.models[0].id);
  expect(card).toHaveClass('opacity-50');
  fireEvent.mouseUp(document);
  expect(card).not.toHaveClass('opacity-50');
});

it.each(['compact', 'comfort', 'list'])('moves a card onto a folder inside the window and retains undo (%s)', async view => {
  localStorage.setItem('3mf-katalog-density', view);
  const Component = view === 'list' ? ModelList : ModelGrid;
  const model = makeModelFile({ id: 'move-me', path: '/source/model.stl', folderId: 'source' });
  const models = [model];
  const folders = [makeFolder({ id: 'source', path: '/source' }), makeFolder({ id: 'target', path: '/target' })];
  vi.mocked(invoke).mockImplementation(async command => command === 'move_files_to_folder'
    ? [{ id: model.id, status: 'moved', fromFolderId: 'source', fromPath: model.path }] : undefined);
  vi.mocked(invoke).mockClear();
  vi.mocked(beginDragOut).mockClear();
  function Workspace() {
    const drag = useFolderDragAndDrop(models, folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() });
    useDragOutTransition({ active: drag.draggedFileId !== null, ids: drag.draggedFileIds, models, onHandOver: drag.cancelFileDrag });
    return <>
      <Component {...props} models={models} onDragFileStart={drag.onDragFileStart} />
      <div data-testid="folder" onMouseEnter={() => drag.handleFolderMouseEnter('target')}>Target</div>
      <output>{drag.moveToast?.to} undo:{drag.undoCount}</output>
    </>;
  }
  const { container, getByTestId, getByText } = render(<LanguageProvider><UiDensityProvider><Workspace /></UiDensityProvider></LanguageProvider>);
  fireEvent.mouseDown(container.querySelector('[data-model-id]')!, { button: 0, clientX: 100, clientY: 100 });
  fireEvent.mouseMove(document, { buttons: 1, clientX: 120, clientY: 100 });
  fireEvent.mouseEnter(getByTestId('folder'));
  await act(async () => { fireEvent.mouseUp(document); });
  expect(invoke).toHaveBeenCalledWith('move_files_to_folder', { fileIds: ['move-me'], folderId: 'target' });
  expect(getByText('/target undo:1')).toBeInTheDocument();
  expect(beginDragOut).not.toHaveBeenCalled();
});
