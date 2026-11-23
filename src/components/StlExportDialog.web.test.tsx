import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { StlExportDialog } from './StlExportDialog';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { de } from '../i18n/de';
import * as stlApi from '../lib/api/stl';
import * as filesApi from '../lib/api/files';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/stl', () => ({ exportModelAsStl: vi.fn(), downloadModelAsStl: vi.fn(), showFolderInFileManager: vi.fn() }));
vi.mock('../lib/api/files', () => ({ readPlateOverview: vi.fn() }));
const download = vi.mocked(stlApi.downloadModelAsStl);
const desktopSave = vi.mocked(stlApi.exportModelAsStl);
const readPlateOverview = vi.mocked(filesApi.readPlateOverview);

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  download.mockReset(); desktopSave.mockReset();
  readPlateOverview.mockReset().mockResolvedValue({ plates: [{ plateIndex: 1 }, { plateIndex: 2 }] } as never);
});
const open = () => render(<LanguageProvider><StlExportDialog model={{ id: '7', name: 'Box.3mf', fileType: '3mf' }} onClose={vi.fn()} /></LanguageProvider>);

it('offers a download instead of the save dialog and confirms it without a folder button', async () => {
  download.mockResolvedValue({ name: 'Box.stl', folder: '', triangles: 12, fileCount: 1, inCatalogFolder: false });
  open();
  fireEvent.click(screen.getByRole('button', { name: de.stDownload }));
  expect(await screen.findByText(de.stDownloaded)).toBeVisible();
  expect(download).toHaveBeenCalledWith('7', 'Box.3mf', { kind: 'all' });
  expect(desktopSave).not.toHaveBeenCalled();
  expect(screen.queryByRole('button', { name: de.showInFileManager })).toBeNull();
  expect(screen.getByRole('button', { name: de.stClose, hidden: false })).toBeVisible();
});

it('sends the chosen plate', async () => {
  download.mockResolvedValue({ name: 'Box_Platte2.stl', folder: '', triangles: 1, fileCount: 1, inCatalogFolder: false });
  open();
  await screen.findByText(de.stScopePlate);
  fireEvent.click(screen.getByLabelText(new RegExp(de.stScopePlate)));
  fireEvent.click(screen.getByRole('button', { name: de.stDownload }));
  await waitFor(() => expect(download).toHaveBeenCalled());
  expect(download.mock.calls[0][2]).toMatchObject({ kind: 'plate' });
});

it('shows a refusal in an alert and keeps the dialog open for another try', async () => {
  download.mockRejectedValue({ message: 'stlTooLarge', expected: true, params: ['2000000', '1000000'] });
  open();
  fireEvent.click(screen.getByRole('button', { name: de.stDownload }));
  expect(await screen.findByRole('alert')).toBeVisible();
  expect(screen.getByRole('button', { name: de.stDownload })).toBeEnabled();
});
