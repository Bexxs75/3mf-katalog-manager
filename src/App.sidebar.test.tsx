import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeFolder, makeModelFileSummary } from './test/factories';

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
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update') return null;
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
