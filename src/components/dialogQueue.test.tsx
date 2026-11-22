import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StlExportDialog } from './StlExportDialog';
import { PlateOverviewDialog } from './PlateOverviewDialog';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { makeModelFile } from '../test/factories';
import { de } from '../i18n/de';
import * as stlApi from '../lib/api/stl';
import * as filesApi from '../lib/api/files';
import * as stepApi from '../lib/api/stepPreview';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/stl', () => ({ exportModelAsStl: vi.fn(), showFolderInFileManager: vi.fn() }));
vi.mock('../lib/api/files', () => ({ readPlateOverview: vi.fn() }));
vi.mock('../lib/api/stepPreview', () => ({ hasStepPreview: vi.fn() }));

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(filesApi.readPlateOverview).mockReset().mockResolvedValue({ plates: [{ plateIndex: 1 }] });
  vi.mocked(stepApi.hasStepPreview).mockReset().mockResolvedValue(true);
  vi.mocked(stlApi.exportModelAsStl).mockReset().mockResolvedValue({ name: 'Box.stl', folder: '/lib', triangles: 1, fileCount: 1, inCatalogFolder: false });
});

describe('multi-selection dialog queues', () => {
  it('STL export: cancel and Escape abort the queue, only a finished save continues it', async () => {
    const onClose = vi.fn(); const onFinished = vi.fn();
    render(<LanguageProvider><StlExportDialog model={{ id: '7', name: 'Box', fileType: '3mf' }} onClose={onClose} onFinished={onFinished} /></LanguageProvider>);
    fireEvent.click(screen.getByRole('button', { name: de.cancel }));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onFinished).not.toHaveBeenCalled();
    await waitFor(() => expect(filesApi.readPlateOverview).toHaveBeenCalled());
    fireEvent.click(screen.getByRole('button', { name: de.stNext }));
    fireEvent.click(await screen.findByRole('button', { name: de.stClose }));
    expect(onFinished).toHaveBeenCalledTimes(1);
  });

  it('plate overview: Escape aborts the queue, the close button continues it', async () => {
    const onClose = vi.fn(); const onFinished = vi.fn();
    const model = makeModelFile({ id: 'm1', name: 'Box', path: '/lib/box.3mf' });
    render(<LanguageProvider><PlateOverviewDialog model={model} onClose={onClose} onFinished={onFinished} /></LanguageProvider>);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onFinished).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: de.poClose }));
    expect(onFinished).toHaveBeenCalledTimes(1);
  });
});
