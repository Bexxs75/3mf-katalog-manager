import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { makeModelFile } from '../test/factories';
import { CatalogWorkspace } from './CatalogWorkspace';
vi.mock('./ModelDetailPage', () => ({ ModelDetailPage: () => <div data-testid="detail-page" /> }));
vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));
vi.mock('./Sidebar', () => ({Sidebar: () => null}));
vi.mock('./DetailPanel', () => ({DetailPanel: () => null}));
vi.mock('../lib/api/filamentCheck', () => ({checkFilament: vi.fn().mockResolvedValue([])}));
beforeEach(() => localStorage.clear());
it.each(['grid','groupedGrid','groupedList'] as const)('blocks file and folder moves in %s while keeping the import row above models', view => {
  const file = makeModelFile({id:'m1', name:'Cube.stl', folderId:'a'});
  const onDragFileStart = vi.fn(), onDragFolderStart = vi.fn();
  // Sidebar and detail have separate tests; this fixture exercises workspace routing to the real model views.
  const props = { queue:[], models:[file], filtered:[file], collectionModels:[], folders:[{id:'a',name:'Folder A',path:'/a',parentId:null,count:1}], selectedForBulk:new Set<string>(), detailModel:null, selected:null, activeCollection:null, activeTag:null, toolView:null, view, tags:[], collections:[], sidebarWidth:{width:242}, collapsedFolders:{isCollapsed:()=>false,toggle:vi.fn()}, displayPreference:'thumbnail', selectModel:vi.fn(), setContextMenu:vi.fn(), toggleFavorite:vi.fn(), onDragFileStart, onDragFolderStart, importRow:<div data-testid="import-row">Import</div> } as unknown as ComponentProps<typeof CatalogWorkspace>;
  const ui = (locked: boolean) => <LanguageProvider><UiDensityProvider><ImportLockContext.Provider value={locked}><CatalogWorkspace {...props} /></ImportLockContext.Provider></UiDensityProvider></LanguageProvider>;
  const {rerender,container}=render(ui(true));
  const cube=screen.getByText('Cube.stl');
  expect(screen.getByTestId('import-row').compareDocumentPosition(cube) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  fireEvent.mouseDown(cube,{clientX:0,clientY:0}); fireEvent.mouseMove(document,{clientX:30,clientY:30}); fireEvent.mouseUp(document);
  expect(onDragFileStart).not.toHaveBeenCalled();
  if (view !== 'grid') {fireEvent.mouseDown(screen.getByText('Folder A'),{clientX:0,clientY:0});fireEvent.mouseMove(document,{clientX:30,clientY:30});fireEvent.mouseUp(document);expect(onDragFolderStart).not.toHaveBeenCalled();}
  rerender(ui(false));
  const tile=container.querySelector('[data-model-id="m1"]') ?? screen.getByText('Cube.stl');
  fireEvent.mouseDown(tile,{clientX:0,clientY:0});fireEvent.mouseMove(document,{clientX:30,clientY:30});fireEvent.mouseUp(document);
  expect(onDragFileStart).toHaveBeenCalledWith('m1');
});

it('restores the window after detail return and resets on view, query and filter changes', async () => {
  const files = Array.from({ length: 200 }, (_, i) => makeModelFile({ id: String(i), name: `Cube ${i}`, folderId: '' }));
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function(this: HTMLElement) {
    const top = this.dataset.catalogScroller !== undefined ? 0 : -(this.closest<HTMLElement>('[data-catalog-scroller]')?.scrollTop ?? 0);
    return { top, left: 0, width: 800, height: this.dataset.modelId ? 100 : 0 } as DOMRect;
  });
  const props = { query: '', activeFolderId: 'all', queue: [], models: files, filtered: files, collectionModels: [], folders: [], selectedForBulk: new Set<string>(), detailModel: null, selected: null, activeCollection: null, activeTag: null, toolView: null, view: 'grid', tags: [], collections: [], sidebarWidth: { width: 242 }, collapsedFolders: { isCollapsed: () => false, toggle: vi.fn() }, displayPreference: 'thumbnail', selectModel: vi.fn(), setContextMenu: vi.fn(), toggleFavorite: vi.fn() } as unknown as ComponentProps<typeof CatalogWorkspace>;
  const ui = (extra: Partial<ComponentProps<typeof CatalogWorkspace>> = {}) => <LanguageProvider><UiDensityProvider><CatalogWorkspace {...props} {...extra} /></UiDensityProvider></LanguageProvider>;
  const { container, rerender } = render(ui());
  await waitFor(() => expect(container.querySelectorAll('[data-model-id]')).toHaveLength(24));
  let scroller = container.querySelector<HTMLElement>('[data-catalog-scroller]')!;
  await act(async () => { scroller.scrollTop = 1140; fireEvent.scroll(scroller); await new Promise(resolve => requestAnimationFrame(resolve)); });
  expect(container.querySelector('[data-model-id="0"]')).toBeNull();
  rerender(ui({ detailModel: files[40] }));
  expect(screen.getByTestId('detail-page')).toBeInTheDocument();
  rerender(ui({ filtered: files.map(file => ({ ...file, lastViewedAt: '2026-10-04' })) }));
  scroller = container.querySelector<HTMLElement>('[data-catalog-scroller]')!;
  expect(scroller.scrollTop).toBe(1140);
  await waitFor(() => expect(container.querySelector('[data-model-id="32"]')).toBeInTheDocument());
  expect(container.querySelector('[data-model-id="0"]')).toBeNull();
  rerender(ui({ query: 'Cube' }));
  expect(scroller.scrollTop).toBe(0);
  scroller.scrollTop = 1000; fireEvent.scroll(scroller);
  rerender(ui({ query: 'Cube', activeFolderId: 'none' }));
  expect(scroller.scrollTop).toBe(0);
  scroller.scrollTop = 1000; fireEvent.scroll(scroller);
  rerender(ui({ query: 'Cube', activeFolderId: 'none', view: 'groupedList' }));
  expect(container.querySelector<HTMLElement>('[data-catalog-scroller]')?.scrollTop).toBe(0);
  vi.restoreAllMocks();
});
