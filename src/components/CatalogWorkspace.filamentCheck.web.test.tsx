import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { render, waitFor } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { CatalogWorkspace } from './CatalogWorkspace';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { checkFilament } from '../lib/api/filamentCheck';
vi.mock('./Sidebar', () => ({ Sidebar: () => null }));
vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../lib/api/filamentCheck', () => ({ checkFilament: vi.fn().mockResolvedValue([]) }));
vi.mock('../lib/api/companions', () => ({ listWebCompanions: vi.fn().mockResolvedValue([]), companionImageData: vi.fn(), companionDownloadUrl: (id: string) => id }));
vi.mock('../lib/api/lastPrinter', () => ({ getLastPrinterForFile: vi.fn().mockResolvedValue(null) }));
beforeEach(() => {
  vi.stubEnv('VITE_TARGET', 'web');
  vi.mocked(checkFilament).mockClear();
  localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de');
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}),
  });
});
afterEach(() => vi.unstubAllEnvs());
function mount(selected: boolean) {
  const file = makeModelFile({ id: 'one', name: 'Cube' });
  const queued = makeModelFile({ id: 'two', name: 'Queued' });
  const props = { query: '', activeFolderId: 'all', slicers: [], queue: [queued], models: [file, queued], filtered: [file],
    collectionModels: [], folders: [], selectedForBulk: new Set<string>(), detailModel: null, selected: selected ? file : null,
    selectedId: selected ? 'one' : null, selectModel: vi.fn(), onCloseDetails: vi.fn(), activeCollection: null,
    activeTag: null, toolView: null, view: 'grid', sort: 'name', tags: [], collections: [],
    sidebarWidth: { width: 242 }, collapsedFolders: { isCollapsed: () => false, toggle: vi.fn() },
    displayPreference: 'thumbnail', toggleFavorite: vi.fn(), setContextMenu: vi.fn(),
    clearBulkSelection: vi.fn(), toggleBulkSelect: vi.fn(), onAddTag: vi.fn(), onRemoveTag: vi.fn(), addTag: vi.fn(), removeTag: vi.fn(),
  } as unknown as ComponentProps<typeof CatalogWorkspace>;
  return render(<LanguageProvider><UiDensityProvider><CatalogWorkspace {...props} /></UiDensityProvider></LanguageProvider>);
}
it('checks the filament of the queue in the web build', async () => {
  mount(false);
  await waitFor(() => expect(checkFilament).toHaveBeenCalledWith(['two']));
});
it('checks the filament of the selected model in the web build', async () => {
  mount(true);
  await waitFor(() => expect(checkFilament).toHaveBeenCalledWith(['one']));
});
