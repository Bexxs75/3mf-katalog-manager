import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StlExportDialog } from './StlExportDialog';
import { SaveAsStlButton } from './SaveAsStlButton';
import { ContextMenu } from './ContextMenu';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import * as stlApi from '../lib/api/stl';
import * as filesApi from '../lib/api/files';
import * as stepApi from '../lib/api/stepPreview';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/stl', () => ({ exportModelAsStl: vi.fn(), showFolderInFileManager: vi.fn() }));
vi.mock('../lib/api/files', () => ({ readPlateOverview: vi.fn() }));
vi.mock('../lib/api/stepPreview', () => ({ hasStepPreview: vi.fn() }));
const exportModelAsStl = vi.mocked(stlApi.exportModelAsStl);
const showFolder = vi.mocked(stlApi.showFolderInFileManager);
const readPlateOverview = vi.mocked(filesApi.readPlateOverview);
const hasStepPreview = vi.mocked(stepApi.hasStepPreview);

const saved = { name: 'Box.stl', folder: '/lib', triangles: 12, inCatalogFolder: false };

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  exportModelAsStl.mockReset();
  showFolder.mockReset().mockResolvedValue(undefined);
  readPlateOverview.mockReset().mockResolvedValue({ plates: [{ plateIndex: 1 }] });
  hasStepPreview.mockReset().mockResolvedValue(true);
});

function open(fileType: string, onClose = vi.fn()) {
  render(<LanguageProvider><StlExportDialog model={{ id: '7', name: 'Box', fileType }} onClose={onClose} /></LanguageProvider>);
  return onClose;
}
const next = () => screen.getByRole('button', { name: de.stNext });

describe('StlExportDialog', () => {
  it('offers no range for a single plate, notes the lost colors and saves everything', async () => {
    exportModelAsStl.mockResolvedValue(saved);
    open('3mf');
    await waitFor(() => expect(readPlateOverview).toHaveBeenCalledWith('7', false));
    expect(screen.queryByRole('group', { name: de.stScope })).not.toBeInTheDocument();
    expect(screen.getByText(de.stNoteColors)).toBeVisible();
    expect(screen.queryByText(de.stNotePlates)).not.toBeInTheDocument();
    fireEvent.click(next());
    await waitFor(() => expect(exportModelAsStl).toHaveBeenCalledWith('7', { kind: 'all' }));
    expect(await screen.findByText(de.stDone)).toBeVisible();
    expect(screen.queryByText(de.stDoneInCatalog)).not.toBeInTheDocument();
  });

  it('asks for the range of a 3MF with several plates and sends the chosen plate', async () => {
    readPlateOverview.mockResolvedValue({ plates: [{ plateIndex: 2 }, { plateIndex: 1 }, { plateIndex: 3 }] });
    exportModelAsStl.mockResolvedValue(saved);
    open('3mf');
    const range = await screen.findByRole('group', { name: de.stScope });
    expect(screen.getByText(de.stNotePlates)).toBeVisible();
    expect(within(range).getByRole('radio', { name: de.stScopeAll })).toBeChecked();
    fireEvent.click(within(range).getByRole('radio', { name: new RegExp(de.stScopePlate) }));
    expect(screen.queryByText(de.stNotePlates)).not.toBeInTheDocument();
    const plate = within(range).getByRole('combobox', { name: de.stPlate });
    expect(within(plate).getAllByRole('option').map(o => o.textContent)).toEqual(['Platte 1', 'Platte 2', 'Platte 3']);
    fireEvent.change(plate, { target: { value: '3' } });
    fireEvent.click(next());
    await waitFor(() => expect(exportModelAsStl).toHaveBeenCalledWith('7', { kind: 'plate', number: 3 }));
  });

  it('shows no range and no color note for an OBJ', async () => {
    open('obj');
    expect(screen.queryByText(de.stNoteColors)).not.toBeInTheDocument();
    expect(readPlateOverview).not.toHaveBeenCalled();
  });

  it('goes back to the settings when the system dialog is cancelled', async () => {
    exportModelAsStl.mockResolvedValue(null);
    open('3mf');
    fireEvent.click(next());
    await waitFor(() => expect(exportModelAsStl).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(next()).toBeEnabled());
    expect(screen.queryByText(de.stDone)).not.toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('keeps the dialog busy while saving, then shows the result and where the file is', async () => {
    let finish: (value: stlApi.StlExportResult) => void = () => {};
    exportModelAsStl.mockReturnValue(new Promise(resolve => { finish = resolve; }));
    const onClose = open('3mf');
    fireEvent.click(next());
    expect(await screen.findByText(de.stSaving)).toBeVisible();
    expect(next()).toBeDisabled();
    expect(screen.getByRole('button', { name: de.cancel })).toBeDisabled();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    finish({ ...saved, inCatalogFolder: true });
    expect(await screen.findByText(de.stDone)).toBeVisible();
    expect(screen.getByText('Box.stl')).toBeVisible();
    expect(screen.getByText(de.stDoneInCatalog)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: de.showInFileManager }));
    expect(showFolder).toHaveBeenCalledWith('/lib');
    const closeButtons = screen.getAllByRole('button', { name: de.stClose });
    fireEvent.click(closeButtons[closeButtons.length - 1]);
    expect(onClose).toHaveBeenCalled();
  });

  it('shows a registered backend code in the chosen language, with its parameters', async () => {
    exportModelAsStl.mockRejectedValue({ message: 'stlTargetExists', expected: true, params: ['Box.stl'] });
    open('3mf');
    fireEvent.click(next());
    expect(await screen.findByRole('alert')).toHaveTextContent(de.errStlTargetExists.replace('{0}', 'Box.stl'));
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
    expect(next()).toBeEnabled();
  });

  it('closes on Cancel without calling the backend', () => {
    const onClose = open('3mf');
    fireEvent.click(screen.getByRole('button', { name: de.cancel }));
    expect(onClose).toHaveBeenCalled();
    expect(exportModelAsStl).not.toHaveBeenCalled();
  });
});

describe('SaveAsStlButton', () => {
  const render3 = (fileType: string | undefined, path: string, runtime: { container: boolean } = { container: false }) =>
    render(<LanguageProvider><RuntimeEnvironmentProvider value={runtime}>
      <SaveAsStlButton model={{ id: '7', name: 'Box', fileType, path }} className="x" />
    </RuntimeEnvironmentProvider></LanguageProvider>);

  it('opens the dialog for a 3MF', async () => {
    render3('3mf', '/lib/box.3mf');
    fireEvent.click(screen.getByRole('button', { name: de.stSaveAsStl }));
    expect(await screen.findByRole('dialog', { name: de.stDialogTitle })).toBeVisible();
    expect(hasStepPreview).not.toHaveBeenCalled();
  });

  it('is not there for STL files or inside a container', () => {
    render3('stl', '/lib/box.stl');
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    render3('3mf', '/lib/box.3mf', { container: true });
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('is disabled with a hint for a STEP file in a build without STEP preview', async () => {
    hasStepPreview.mockResolvedValue(false);
    render3(undefined, '/lib/box.step');
    const button = await screen.findByRole('button', { name: de.stSaveAsStl });
    await waitFor(() => expect(button).toBeDisabled());
    expect(button).toHaveAccessibleDescription(de.stStepNoPreview);
  });
});

describe('ContextMenu entry', () => {
  const menu = (props: Partial<React.ComponentProps<typeof ContextMenu>>, runtime = { container: false }) => {
    const onSaveAsStl = vi.fn();
    const onClose = vi.fn();
    render(<LanguageProvider><RuntimeEnvironmentProvider value={runtime}>
      <ContextMenu x={0} y={0} onClose={onClose} onOpenInSlicer={vi.fn()} onDelete={vi.fn()} inQueue={false} onToggleQueue={vi.fn()}
        printed={false} onTogglePrintStatus={vi.fn()} fileId="7" currentName="Box" onRename={vi.fn()}
        onSaveAsStl={onSaveAsStl} stlModel={{ fileType: '3mf', name: 'Box', path: '/lib/box.3mf' }} {...props} />
    </RuntimeEnvironmentProvider></LanguageProvider>);
    return { onSaveAsStl, onClose };
  };

  it('hands the click to the caller (the menu closes, the dialog lives elsewhere)', () => {
    const { onSaveAsStl, onClose } = menu({});
    fireEvent.click(screen.getByRole('button', { name: de.stSaveAsStl }));
    expect(onSaveAsStl).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalled();
  });

  it('has no entry for a STL file', () => {
    menu({ stlModel: { fileType: 'stl', name: 'Box', path: '/lib/box.stl' } });
    expect(screen.queryByText(de.stSaveAsStl)).not.toBeInTheDocument();
  });
  it('has no entry without a callback', () => {
    menu({ onSaveAsStl: undefined });
    expect(screen.queryByText(de.stSaveAsStl)).not.toBeInTheDocument();
  });
  it('has no entry in a container', () => {
    menu({}, { container: true });
    expect(screen.queryByText(de.stSaveAsStl)).not.toBeInTheDocument();
  });
});

describe('texts', () => {
  it('exist in German and English', () => {
    expect(de.stSaveAsStl).not.toBe(en.stSaveAsStl);
  });
});
