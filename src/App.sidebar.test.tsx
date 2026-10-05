import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeFolder, makeModelFile, makeModelFileSummary } from './test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
// WebGL rendering is unrelated to catalog actions and unavailable in jsdom.
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'reset_catalog') return { modelCount: 0, folderCount: 0 };
    return [];
  });
});

it('shares both toggles between Header, Sidebar and grouped content while retaining selection', async () => {
  const folders = [makeFolder({ id: 'parent', name: 'Parent' }),
    makeFolder({ id: 'child', name: 'Child', parentId: 'parent' })];
  const model = makeModelFileSummary({ name: 'Nested.stl', folderId: 'child' });
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_folders') return folders;
    if (cmd === 'list_file_summaries') return [model];
    if (cmd === 'list_all_file_tags') return {};
    return fallback(cmd, args);
  });
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  const sidebar = within(screen.getByRole('separator').closest('aside')!);
  const header = within(screen.getByRole('banner'));
  const content = within(screen.getByRole('main'));
  fireEvent.click(sidebar.getByText('Parent'));
  fireEvent.click(header.getByRole('button', { name: de.viewFolder }));
  expect(content.getByText('Nested.stl')).toBeVisible();
  fireEvent.click(header.getByRole('button', { name: de.collapseAllFolders }));
  expect(content.queryByText('Nested.stl')).toBeNull();
  expect(sidebar.queryByText('Child')).toBeNull();
  expect(header.getByRole('button', { name: de.expandAllFolders })).toBeVisible();
  expect(sidebar.getByText('Parent').parentElement).toHaveClass('font-semibold');
  fireEvent.click(sidebar.getByRole('button', { name: de.expandAllFolders }));
  expect(sidebar.getByText('Child')).toBeVisible();
  expect(content.getByText('Nested.stl')).toBeVisible();
  expect(JSON.parse(localStorage.getItem('3mf-katalog-sidebar-expanded')!)).toEqual(['parent']);
  fireEvent.click(header.getByRole('button', { name: de.viewList }));
  fireEvent.click(sidebar.getByRole('button', { name: de.collapseAllFolders }));
  expect(content.queryByText('Nested.stl')).toBeNull();
  fireEvent.click(header.getByRole('button', { name: de.expandAllFolders }));
  expect(content.getByText('Nested.stl')).toBeVisible();
  expect(sidebar.getByText('Parent').parentElement).toHaveClass('font-semibold');
});

it('uses the visible sidebar width for the panel and zero in trash and Material Manager', async () => {
  const { container } = await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
  const root = container.firstElementChild as HTMLElement;
  const handle = screen.getByRole('separator');
  fireEvent.keyDown(handle, { key: 'End' });
  expect(root.style.getPropertyValue('--sidebar-width')).toBe('420px');
  for (const name of [de.trashHeading, de.railFilament]) {
    await act(async () => fireEvent.click(screen.getByRole('button', { name })));
    expect(root.style.getPropertyValue('--sidebar-width')).toBe('0px');
    expect(screen.queryByRole('separator')).toBeNull();
  }
  fireEvent.click(screen.getByRole('button', { name: de.railCatalog }));
  expect(root.style.getPropertyValue('--sidebar-width')).toBe('420px');
});

it('assigns the shared printer master width to the settings offset in Printer Manager', async () => {
  const { container } = await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
  await act(async () => fireEvent.click(screen.getByRole('button', {name: de.railPrinters})));
  expect((container.firstElementChild as HTMLElement).style.getPropertyValue('--sidebar-width')).toBe('var(--pm-master-width)');
  expect(container.querySelector('[data-list-open]')).toHaveClass('grid-cols-[var(--pm-master-width)_minmax(0,1fr)]');
});

it.each([false, true])('deletes through the sidebar hook, preserves models and retains gallery=%s', async (galleryOpen) => {
  let collections = [{id: 'c1', name: 'Kitchen', modelCount: 1}];
  const model = makeModelFile({name: 'Kept.stl'});
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_collections') return collections;
    if (cmd === 'list_file_summaries' || cmd === 'list_collection_files') return [model];
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'delete_collection') { collections = []; return; }
    return fallback(cmd, args);
  });
  await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
  const sidebar = within(screen.getByRole('separator').closest('aside')!);
  await act(async () => fireEvent.click(sidebar.getByRole('button', {name: 'Kitchen'})));
  if (galleryOpen) fireEvent.click(sidebar.getByText(de.viewAllCollectionsLabel));
  fireEvent.contextMenu(sidebar.getByRole('button', {name: 'Kitchen'}));
  fireEvent.click(screen.getByRole('button', {name: de.delete}));
  await act(async () => fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', {name: de.delete})));
  expect(invoke).toHaveBeenCalledWith('delete_collection', {collectionId: 'c1'});
  expect(sidebar.queryByRole('button', {name: 'Kitchen'})).toBeNull();
  if (galleryOpen) expect(screen.getByText(de.noCollectionsEmptyState)).toBeVisible();
  else expect(screen.getByRole('main')).toHaveTextContent('Kept.stl');
  expect(invoke).not.toHaveBeenCalledWith('delete_file', expect.anything());
});

it('clears folder, tag, search and tool filters through App setters while retaining view and sorting', async () => {
  const folders = [makeFolder({id: 'a', name: 'Folder A'})];
  const files = [makeModelFileSummary({id: 'one', name: 'Cube', folderId: 'a', favorite: true}),
    makeModelFileSummary({id: 'two', name: 'Sphere', folderId: 'a'})];
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_folders') return folders;
    if (cmd === 'list_file_summaries') return files;
    if (cmd === 'list_all_file_tags') return {one: ['test'], two: ['test']};
    if (cmd === 'list_tag_counts') return [{label: 'test', count: 2, colorHue: 20}];
    return fallback(cmd, args);
  });
  await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
  const sidebar = within(screen.getByRole('separator').closest('aside')!);
  const header = within(screen.getByRole('banner'));
  fireEvent.click(header.getByRole('button', {name: de.viewList}));
  fireEvent.click(header.getByRole('button', {name: /↑/}));
  fireEvent.click(header.getByRole('menuitemradio', {name: de.sortSize}));
  fireEvent.keyDown(document, {key: 'Escape'});
  const sortBefore = header.getByRole('button', {name: /↓/}).textContent;
  fireEvent.click(sidebar.getByRole('button', {name: /Folder A/}));
  fireEvent.click(sidebar.getByText(de.tagsHeading));
  fireEvent.click(sidebar.getByRole('button', {name: /#test/}));
  fireEvent.change(sidebar.getByPlaceholderText(de.searchPlaceholder), {target: {value: 'Cube'}});
  fireEvent.click(sidebar.getByRole('button', {name: /Favoriten/}));
  expect(screen.getByRole('region', {name: de.filterBarAria}).querySelectorAll('[data-filter-kind]')).toHaveLength(4);
  fireEvent.click(screen.getByRole('button', {name: de.filterBarClearAll}));
  expect(screen.queryByRole('region', {name: de.filterBarAria})).toBeNull();
  expect(sidebar.getByPlaceholderText(de.searchPlaceholder)).toHaveValue('');
  expect(sidebar.getByRole('button', {name: /Alle Modelle/})).toHaveClass('font-semibold');
  expect(sidebar.getByRole('button', {name: /Favoriten/})).toHaveAttribute('aria-pressed', 'false');
  expect(sidebar.getByRole('button', {name: /#test/})).not.toHaveClass('text-[var(--accent)]');
  expect(header.getByRole('button', {name: de.viewList})).toHaveClass('bg-[var(--accent)]');
  expect(header.getByRole('button', {name: /↓/})).toHaveTextContent(sortBefore!);
  expect(screen.getByRole('main')).toHaveTextContent('Sphere');
  expect(invoke).not.toHaveBeenCalledWith('reset_catalog');
});

it('clears an active collection without resetting the catalog', async () => {
  const model = makeModelFileSummary({name: 'Cube'});
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_collections') return [{id: 'c1', name: 'Kitchen', modelCount: 1}];
    if (cmd === 'list_file_summaries') return [model];
    if (cmd === 'list_collection_files') return [makeModelFile({name: 'Cube'})];
    if (cmd === 'list_all_file_tags') return {};
    return fallback(cmd, args);
  });
  await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
  const sidebar = within(screen.getByRole('separator').closest('aside')!);
  await act(async () => fireEvent.click(sidebar.getByRole('button', {name: 'Kitchen'})));
  fireEvent.click(screen.getByRole('button', {name: de.filterBarClearAll}));
  expect(screen.queryByRole('region', {name: de.filterBarAria})).toBeNull();
  expect(sidebar.getByRole('button', {name: 'Kitchen'})).not.toHaveClass('font-semibold');
  expect(screen.getByRole('main')).toHaveTextContent('Cube');
  expect(invoke).not.toHaveBeenCalledWith('reset_catalog');
});
