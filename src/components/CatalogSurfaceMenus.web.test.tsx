import type { ComponentProps } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/transport', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock('./DetailPanel', () => ({DetailPanel: () => null}));
vi.mock('./ModelDetailPage', () => ({ModelDetailPage: () => null}));
import { CatalogWorkspace } from './CatalogWorkspace';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { authenticationFailed, login } from '../lib/webSession';
import { de } from '../i18n/de';

beforeEach(() => { vi.stubEnv('VITE_TARGET', 'web'); localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.unstubAllEnvs(); });
function setup() {
  const onImport = vi.fn(), onCreateFolder = vi.fn();
  const props = {query: '', activeFolderId: 'a', slicers: [], queue: [], models: [], filtered: [], collectionModels: [],
    folders: [{id: 'a', name: 'Folder', path: '/a', parentId: null, count: 0}], selectedForBulk: new Set(), detailModel: null, selected: null,
    activeCollection: null, activeTag: null, toolView: null, view: 'grid', sort: 'name', tags: [{label: 'custom', count: 2, colorHue: 0}], collections: [],
    sidebarWidth: {width: 242}, expansion: {expanded: new Set(), isExpanded: () => false},
    collapsedFolders: {isCollapsed: () => false}, displayPreference: 'thumbnail',
    onCreateFolder, onRenameTag: vi.fn(), onMergeTags: vi.fn(), onDeleteTag: vi.fn(), selectAllVisible: vi.fn(), surfaceActions: {onImport, onViewChange: vi.fn(), onSortChange: vi.fn()},
  } as unknown as ComponentProps<typeof CatalogWorkspace>;
  render(<LanguageProvider><UiDensityProvider><CatalogWorkspace {...props} /></UiDensityProvider></LanguageProvider>);
  return {onImport, onCreateFolder};
}
it('only exposes view/sort and tag filtering to read-only web sessions', () => {
  setup(); fireEvent.contextMenu(document.querySelector('[data-catalog-scroller]')!);
  expect(screen.getAllByRole('menuitem').map(item => item.textContent)).toEqual([de.emptyMenuView, de.sortLabel]);
  fireEvent.keyDown(document.activeElement!, {key: 'Escape'});
  fireEvent.click(screen.getByRole('button', {name: de.tagsHeading}));
  fireEvent.contextMenu(screen.getByTitle('#custom'));
  expect(screen.getAllByRole('menuitem').map(item => item.textContent)).toEqual([de.tagMenuFilter]);
});
it('offers upload, folder creation and tag deletion only with the respective capabilities', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({authenticated: true, writable: true, uploadable: true, fileOps: true, csrfToken: 'csrf'})));
  await login('test'); const {onImport, onCreateFolder} = setup();
  const open = () => fireEvent.contextMenu(document.querySelector('[data-catalog-scroller]')!);
  open(); expect(screen.queryByRole('menuitem', {name: /Mit Festplatte/})).toBeNull();
  fireEvent.click(screen.getByRole('menuitem', {name: de.emptyMenuImport})); expect(onImport).toHaveBeenCalledOnce();
  open(); fireEvent.click(screen.getByRole('menuitem', {name: de.emptyMenuNewFolder}));
  const input = screen.getByPlaceholderText(de.newFolderPlaceholder); expect(input).toHaveFocus();
  fireEvent.change(input, {target: {value: 'New'}}); fireEvent.keyDown(input, {key: 'Enter'});
  expect(onCreateFolder).toHaveBeenCalledWith('a', 'New');
  fireEvent.click(screen.getByRole('button', {name: de.tagsHeading}));
  fireEvent.contextMenu(screen.getByTitle('#custom')); expect(screen.getByRole('menuitem', {name: de.tagMenuDelete})).toBeVisible();
  expect(screen.getByRole('menuitem', {name: de.tagMenuRename})).toBeVisible();
  expect(screen.getByRole('menuitem', {name: de.tagMenuMerge})).toBeVisible();
  await act(async () => authenticationFailed(401));
  expect(screen.queryByRole('menuitem', {name: de.tagMenuDelete})).toBeNull();
});
