import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
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
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

const file = (id: string, hash: string | null, day: number) =>
  makeModelFile({ id, name: `${id}.3mf`, path: `/catalog/${id}.3mf`, contentHash: hash, importedAt: `2026-02-${String(day).padStart(2, '0')}T10:00:00Z` });
let files = [file('orig', 'h1', 1), file('copy', 'h1', 2), file('other', 'h2', 3)];

beforeEach(() => {
  files = [file('orig', 'h1', 1), file('copy', 'h1', 2), file('other', 'h2', 3)];
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_file_summaries') return files.map(makeModelFileSummary);
    if (cmd === 'list_files_by_ids') return files.filter(m => (args as { ids: string[] }).ids.includes(m.id));
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'delete_files') {
      const gone = new Set((args as { fileIds: string[] }).fileIds);
      files = files.filter(m => !gone.has(m.id));
      return null;
    }
    return [];
  });
});

it('shows the duplicates in place of the catalog list, without the filter bar, and trashes only the deselected copy', async () => {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('orig.3mf');
  expect(screen.getByRole('region', { name: de.filterBarAria })).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: new RegExp(`^${de.toolDuplicates}`) }));
  const view = await waitFor(() => { const el = document.querySelector<HTMLElement>('[data-duplicates-view]'); expect(el).not.toBeNull(); return el!; });
  expect(screen.queryByRole('region', { name: de.filterBarAria })).toBeNull();
  expect(within(view).getAllByRole('radio')).toHaveLength(2);
  expect(within(view).queryByText(/other\.3mf/)).toBeNull();

  fireEvent.click(within(view).getByRole('button', { name: de.dvApply }));
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: de.dvConfirmRemove }));
  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith('delete_files', { fileIds: ['copy'] }));
  await waitFor(() => expect(screen.getByText(de.dvEmpty)).toBeInTheDocument());
  expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'delete_files')).toHaveLength(1);
  expect(screen.getByRole('status')).toHaveTextContent('1 Modell in den Papierkorb gelegt');
});
