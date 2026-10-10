import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { invoke } from '@tauri-apps/api/core';
import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { DetailPanel } from './DetailPanel';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

it.each(['compact', 'comfort'])('shows dismissible unexpected upload errors in the %s detail panel', async (density) => {
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.setItem('3mf-katalog-density', density);
  const noop = vi.fn();
  render(<LanguageProvider><UiDensityProvider><DetailPanel model={makeModelFile()} allTags={[]}
    onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop}
    onToggleFavorite={noop} onToggleQueue={noop} onUploadImage={() => Promise.reject({message: 'database failed', expected: false})}
    onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop} slicerError={null}
  /></UiDensityProvider></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Bild hochladen' }));
  expect(await screen.findByText('database failed')).toBeVisible();
  expect(screen.getByRole('button', {name: 'Problem melden'})).toBeVisible();
  fireEvent.click(screen.getByRole('button', {name: 'Hinweis schließen'}));
  expect(screen.queryByRole('alert')).toBeNull();
});

it.each(['compact', 'comfort'])('reveals the selected model in the %s detail panel', density => {
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.setItem('3mf-katalog-density', density);
  const noop = vi.fn(); const model = makeModelFile();
  render(<LanguageProvider><UiDensityProvider><DetailPanel model={model} allTags={[]}
    onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop}
    onToggleFavorite={noop} onToggleQueue={noop} onUploadImage={noop}
    onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop} slicerError={null}
  /></UiDensityProvider></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', {name: 'Im Dateimanager anzeigen'}));
  expect(invoke).toHaveBeenCalledWith('reveal_in_file_manager', {fileId: model.id});
});

it.each(['compact', 'comfort'])('offers a copyable source URL in the %s container panel', density => {
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.setItem('3mf-katalog-density', density);
  const noop = vi.fn(); const sourceUrl = 'https://example.com/model';
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: true }}><UiDensityProvider>
    <DetailPanel model={makeModelFile({ sourceUrl })} allTags={[]}
      onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop}
      onToggleFavorite={noop} onToggleQueue={noop} onSnapshotCaptured={noop} onUploadImage={noop}
      onSetSourceUrl={noop} onOpenInSlicer={noop} slicerError={null} />
  </UiDensityProvider></RuntimeEnvironmentProvider></LanguageProvider>);
  expect(screen.queryByRole('link', { name: sourceUrl })).not.toBeInTheDocument();
  expect(screen.getByText(sourceUrl)).toBeVisible();
  expect(screen.getByRole('button', { name: `Link kopieren: ${sourceUrl}` })).toBeVisible();
});
