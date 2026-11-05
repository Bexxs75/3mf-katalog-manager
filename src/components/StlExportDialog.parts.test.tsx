import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StlExportDialog } from './StlExportDialog';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { de } from '../i18n/de';
import * as stlApi from '../lib/api/stl';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/stl', () => ({ exportModelAsStl: vi.fn(), showFolderInFileManager: vi.fn() }));
vi.mock('../lib/api/files', () => ({ readPlateOverview: vi.fn() }));
const exportModelAsStl = vi.mocked(stlApi.exportModelAsStl);

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); exportModelAsStl.mockReset(); });

function open(parts: { indices: number[]; initial: 'parts' | 'eachPart' }) {
  render(<LanguageProvider><StlExportDialog model={{ id: '7', name: 'Getriebe', fileType: 'step' }} parts={parts} onClose={vi.fn()} /></LanguageProvider>);
}

describe('StlExportDialog with a parts choice', () => {
  it('saves the chosen parts with the parts scope and no plate choice', async () => {
    exportModelAsStl.mockResolvedValue({ name: 'Getriebe.stl', folder: '/lib', triangles: 24, fileCount: 1, inCatalogFolder: false });
    open({ indices: [1, 3], initial: 'parts' });
    expect(screen.getByRole('radio', { name: 'Gewählte Teile (2)' })).toBeChecked();
    expect(screen.queryByRole('radio', { name: de.stScopeAll })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: de.stNext }));
    await waitFor(() => expect(exportModelAsStl).toHaveBeenCalledWith('7', { kind: 'parts', indices: [1, 3] }));
    expect(await screen.findByText('Getriebe.stl')).toBeInTheDocument();
  });

  it('saves one file per part and reports how many files were written', async () => {
    exportModelAsStl.mockResolvedValue({ name: 'Getriebe_Teil*.stl', folder: '/lib', triangles: 120, fileCount: 12, inCatalogFolder: false });
    open({ indices: [0, 1], initial: 'eachPart' });
    expect(screen.getByRole('radio', { name: de.stScopeEach })).toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: de.stNext }));
    await waitFor(() => expect(exportModelAsStl).toHaveBeenCalledWith('7', { kind: 'eachPart' }));
    expect(await screen.findByText('12 Dateien gespeichert')).toBeInTheDocument();
  });

  it('switches between the two choices and disables "chosen parts" when none is left', () => {
    open({ indices: [], initial: 'eachPart' });
    expect(screen.getByRole('radio', { name: 'Gewählte Teile (0)' })).toBeDisabled();
    expect(screen.getByRole('radio', { name: de.stScopeEach })).toBeEnabled();
  });
});
