import { act, render, screen, waitFor, fireEvent } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useCatalogStore } from '../hooks/useCatalogStore';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ModelGrid } from './ModelGrid';
import { ModelDetailPage } from './ModelDetailPage';
import { makeModelFile, makeModelFileSummary } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (s: string) => s }));
vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => ({}) }));
let store: ReturnType<typeof useCatalogStore>;
let model = makeModelFile({ id: 'snapshot' });
const noop = () => {};
function Consumers() {
  store = useCatalogStore({ preselectFirst: false });
  const current = store.models[0];
  if (!current) return null;
  return <>
    <ModelGrid models={store.models} windowed={false} selectedId={null} selectedForBulk={new Set()}
      onSelect={noop} onOpenDetail={noop} onContextMenu={noop} onToggleFavorite={noop}
      onToggleBulkSelect={noop} displayPreference="thumbnail" />
    <ModelDetailPage hasPrevious={false} hasNext={false} model={current} allTags={[]} onClose={noop} onAddTag={noop} onRemoveTag={noop}
      onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop} onToggleQueue={noop}
      onUploadImage={async () => {}} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
      onRescanMetadata={noop} onAddToCollection={noop} collections={[]} slicers={[]} slicerError={null}
      rescanError={null} rescanSuccess={false} displayPreference="thumbnail" />
  </>;
}
beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  model = makeModelFile({ id: 'snapshot' });
  vi.mocked(invoke).mockReset().mockImplementation(async (cmd, args) => {
    if (cmd === 'list_file_summaries') return [makeModelFileSummary(model)];
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'list_files_by_ids') return [model];
    if (cmd === 'list_file_images') return [model];
    if (cmd === 'set_render_snapshot') {
      model = { ...model, renderSnapshotImage: `data:image/png;base64,${(args as {imageBase64: string}).imageBase64}` };
      return;
    }
    return [];
  });
});
it('reloads grid and already open detail images after capture without storing the data URL', async () => {
  const { container } = render(<LanguageProvider><UiDensityProvider><Consumers /></UiDensityProvider></LanguageProvider>);
  await waitFor(() => expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === 'list_file_images')).toBe(true));
  await act(async () => { store.ensureFullModel('snapshot'); });
  const fullReads = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'list_files_by_ids').length;
  await act(async () => { store.captureRenderSnapshot('snapshot', 'c25hcHNob3Q='); });
  const src = 'data:image/png;base64,c25hcHNob3Q=';
  await waitFor(() => expect(container.querySelector(`img[src="${src}"]`)).not.toBeNull());
  fireEvent.click(await screen.findByRole('button', { name: 'Bild' }));
  expect(container.querySelectorAll(`img[src="${src}"]`)).toHaveLength(2);
  expect(store.models[0].renderSnapshotImage).toBeNull();
  expect(store.models[0].hasRenderSnapshot).toBe(true);
  expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'list_files_by_ids')).toHaveLength(fullReads);
});
