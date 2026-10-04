import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { makeModelFile } from '../test/factories';
import { CatalogWorkspace } from './CatalogWorkspace';
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
