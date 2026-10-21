import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeModelFile, makeModelFileSummary } from './test/factories';

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


async function setup() {
  return await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
}
it('opens help from ?, the header button and the settings Info tab', async () => {
  await setup();
  fireEvent.keyDown(document.body, { key: '?', shiftKey: true }); expect(screen.getByRole('dialog', { name: de.keyboardTipsTitle })).toBeVisible();
  fireEvent.keyDown(screen.getByRole('tab', { name: de.keyboardTab }), { key: 'Escape' }); expect(screen.queryByRole('dialog')).toBeNull();
  const headerButton = within(screen.getByRole('banner')).getByRole('button', { name: de.keyboardTipsTitle }); headerButton.focus(); fireEvent.click(headerButton);
  fireEvent.keyDown(screen.getByRole('tab', { name: de.keyboardTab }), { key: 'Escape' }); expect(headerButton).toHaveFocus();
  fireEvent.click(screen.getByRole('button', { name: de.settingsTitle })); fireEvent.click(screen.getByRole('tab', { name: de.settingsTabInfo }));
  const infoButton = within(screen.getByRole('complementary', { name: de.settingsTitle })).getByRole('button', { name: de.keyboardTipsTitle }); infoButton.focus(); fireEvent.click(infoButton);
  expect(screen.getByRole('dialog', { name: de.keyboardTipsTitle })).toBeVisible(); fireEvent.keyDown(screen.getByRole('tab', { name: de.keyboardTab }), { key: 'Escape' }); expect(infoButton).toHaveFocus();
});
it('persists the General single-key switch and disables help', async () => {
  const result = await setup(); fireEvent.click(screen.getByRole('button', { name: de.settingsTitle }));
  const toggle = screen.getByRole('switch', { name: de.singleKeyShortcutsTitle }); expect(toggle).toHaveAttribute('aria-checked', 'true'); fireEvent.click(toggle);
  expect(localStorage.getItem('3mf-katalog-single-key-shortcuts')).toBe('false');
  fireEvent.keyDown(document.body, { key: '?' }); expect(screen.queryByRole('dialog')).toBeNull(); result.unmount(); await setup();
  fireEvent.click(screen.getByRole('button', { name: de.settingsTitle })); expect(screen.getByRole('switch', { name: de.singleKeyShortcutsTitle })).toHaveAttribute('aria-checked', 'false');
});
it('shows hints only when the entire catalog is empty', async () => {
  const result = await setup(); expect(screen.getByRole('heading', { name: de.emptyCatalogTitle })).toBeVisible(); result.unmount();
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => cmd === 'list_file_summaries' ? [makeModelFileSummary()] : fallback(cmd, args));
  await setup(); expect(screen.queryByRole('heading', { name: de.emptyCatalogTitle })).toBeNull();
});
it.each(['grid', 'groupedGrid', 'groupedList'])('Ctrl+A selects the visible models in %s', async view => {
  localStorage.setItem('3mf-katalog-view', view);
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => cmd === 'list_file_summaries' ? [makeModelFileSummary({ id: 'a', name: 'A.stl' }), makeModelFileSummary({ id: 'b', name: 'B.stl' })] : fallback(cmd, args));
  await setup();
  const label = view === 'grid' ? de.viewGrid : view === 'groupedGrid' ? de.viewFolder : de.viewList;
  fireEvent.click(within(screen.getByRole('banner')).getByRole('button', { name: label }));
  const event = new KeyboardEvent('keydown', { key: 'a', ctrlKey: true, bubbles: true, cancelable: true });
  await act(async () => { document.body.dispatchEvent(event); });
  expect(event.defaultPrevented).toBe(true); expect(screen.getByRole('button', { name: de.clearSelectionLabel })).toBeVisible();
  expect(screen.getByText(de.bulkSelectedCount.replace('{count}', '2'))).toBeVisible();
});

it('shares and persists detail mode between settings and the header', async () => {
  await setup();
  const toggle = screen.getByRole('button', { name: de.detailPanelPin });
  expect(toggle).toHaveAttribute('aria-pressed', 'false');
  fireEvent.click(screen.getByRole('button', { name: de.settingsTitle }));
  const pinned = screen.getByRole('radio', { name: de.detailPanelPinned });
  expect(screen.getByRole('radio', { name: de.detailPanelAuto })).toBeChecked();
  fireEvent.click(pinned);
  expect(localStorage.getItem('3mf-katalog-detail-panel')).toBe('pinned');
  expect(toggle).toHaveAttribute('aria-pressed', 'true');
  fireEvent.click(toggle);
  expect(localStorage.getItem('3mf-katalog-detail-panel')).toBe('auto');
  expect(screen.getByRole('radio', { name: de.detailPanelAuto })).toBeChecked();
});

it('Escape leaves search without clearing the query; the next Escape clears selection', async () => {
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => cmd === 'list_file_summaries' ? [makeModelFileSummary({id: 'one', name: 'cube.3mf'})] : fallback(cmd, args));
  await setup();
  const search = screen.getByPlaceholderText(de.searchPlaceholder);
  fireEvent.change(search, {target: {value: 'cube'}});
  screen.getByRole('main').focus();
  await act(async () => { document.body.dispatchEvent(new KeyboardEvent('keydown', {key: 'a', ctrlKey: true, bubbles: true, cancelable: true})); });
  search.focus();
  fireEvent.keyDown(search, {key: 'Escape'});
  expect(search).toHaveValue('cube');
  expect(screen.getByRole('main')).toHaveFocus();
  expect(screen.getByText(de.bulkSelectedCount.replace('{count}', '1'))).toBeVisible();
  fireEvent.keyDown(screen.getByRole('main'), {key: 'Escape'});
  expect(screen.queryByText(de.bulkSelectedCount.replace('{count}', '1'))).not.toBeInTheDocument();
});

it.each(['keyboard', 'toolbar'])('collection selection via %s sends only its visible member to bulk writes', async mode => {
  const a = makeModelFile({ id: 'a', name: 'A.stl' });
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_file_summaries') return [makeModelFileSummary({ id: 'a', name: 'A.stl' }), makeModelFileSummary({ id: 'b', name: 'B.stl' })];
    if (cmd === 'list_collections') return [{ id: 'c', name: 'Only A', modelCount: 1 }];
    if (cmd === 'list_collection_files' || cmd === 'list_files_by_ids') return [a];
    return fallback(cmd, args);
  });
  await setup();
  await act(async () => { fireEvent.click(screen.getByText('Only A')); });
  fireEvent.keyDown(document.body, { key: 'a', ctrlKey: true });
  if (mode === 'toolbar') fireEvent.click(screen.getByRole('button', { name: de.selectAllLabel }));
  expect(screen.getByText(de.bulkSelectedCount.replace('{count}', '1'))).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: de.licenseSet }));
  fireEvent.change(screen.getByRole('combobox', { name: de.licenseHeading }), { target: { value: 'cc0' } });
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: de.licenseSave })); });
  expect(invoke).toHaveBeenCalledWith('set_license', { fileIds: ['a'], license: 'cc0' });
  fireEvent.click(screen.getByRole('button', { name: de.delete }));
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: de.delete })); });
  expect(invoke).toHaveBeenCalledWith('delete_files', { fileIds: ['a'] });
});

it('uses the displayed collection for detail boundaries, position and arrow navigation', async () => {
  const a = makeModelFile({id: 'a', name: 'A.stl'});
  const fallback = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'list_file_summaries') return [makeModelFileSummary(a), makeModelFileSummary({id: 'b', name: 'B.stl'})];
    if (cmd === 'list_collections') return [{id: 'c', name: 'Only A', modelCount: 1}];
    if (cmd === 'list_collection_files' || cmd === 'list_files_by_ids') return [a];
    return fallback(cmd, args);
  });
  await setup();
  await act(async () => { fireEvent.click(screen.getByText('Only A')); });
  await act(async () => { fireEvent.doubleClick(document.querySelector('[data-model-id="a"]')!); });
  expect(screen.getByText('1 von 1')).toBeVisible();
  expect(screen.getByRole('button', {name: de.detailPrevious})).toBeDisabled();
  expect(screen.getByRole('button', {name: de.detailNext})).toBeDisabled();
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(screen.getByText('1 von 1')).toBeVisible();
  expect(invoke).not.toHaveBeenCalledWith('list_files_by_ids', {ids: ['b']});
});
