import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import App from './App';
import { WEB_READ_COMMANDS } from './lib/readOnly';
import { de } from './i18n/de';
import { RuntimeEnvironmentProvider } from './hooks/useRuntimeEnvironment';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { makeFolder, makeModelFile, makeModelFileSummary } from './test/factories';
// Only the GPU renderer is replaced; viewer, binary decoding and HTTP remain real.
vi.mock('three', async importOriginal => ({
  ...await importOriginal<typeof import('three')>(),
  WebGLRenderer: function () {
    return { domElement: document.createElement('canvas'), setPixelRatio() {}, setSize() {}, render() {}, forceContextLoss() {}, dispose() {} };
  },
}));
it('starts and opens model details using only allowed read commands', async () => {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 800, height: 400, top: 0, left: 0, right: 800, bottom: 400, x: 0, y: 0, toJSON: () => ({}) });
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(400);
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  const models = [makeModelFile({ id: '1', name: 'Web model.stl', folderId: 'root', tags: ['web-tag'] })];
  const commands: string[] = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const command = url.split('/').slice(-1)[0]; commands.push(command);
    if (command === 'get_model_geometry') {
      const header = new TextEncoder().encode('{"meshes":[],"palette":[]}');
      const buffer = new ArrayBuffer(4 + header.length);
      new DataView(buffer).setUint32(0, header.length, true);
      new Uint8Array(buffer, 4).set(header);
      return new Response(buffer, { headers: { 'Content-Type': 'application/octet-stream' } });
    }
    const value = command === 'list_file_summaries' ? models.map(makeModelFileSummary)
      : command === 'list_files_by_ids' ? models : command === 'list_all_file_tags' ? { '1': ['web-tag'] }
      : command === 'list_folders' ? [makeFolder({ id: 'root', name: 'Web folder' })]
      : command === 'list_tag_counts' ? [{ label: 'web-tag', count: 2, colorHue: 20 }]
      : command === 'list_collections' ? [{ id: '1', name: 'Web collection', modelCount: 1 }]
      : command === 'list_collection_files' ? models
      : command === 'get_app_version' ? '0.1.0' : ['has_step_preview', 'is_preview_build'].includes(command) ? false : [];
    return Response.json(value);
  }));
  await act(async () => { render(<RuntimeEnvironmentProvider><LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics></RuntimeEnvironmentProvider>); });
  console.info('WEB_START_COMMANDS', [...new Set(commands)].sort().join(', '));
  const item = within(screen.getByRole('main')).getByText('Web model.stl');
  await act(async () => { fireEvent.click(item); fireEvent.doubleClick(item); });
  expect(commands).toContain('list_files_by_ids');
  expect(commands).toContain('list_print_log_entries');
  expect(commands).toContain('get_model_geometry');
  await act(async () => fireEvent.click(screen.getByTitle(de.backToCatalog)));
  const header = within(screen.getByRole('banner'));
  for (const view of [de.viewList, de.viewFolder, de.viewGrid]) {
    await act(async () => fireEvent.click(header.getByRole('button', { name: view })));
  }
  const sidebar = within(screen.getByRole('separator').closest('aside')!);
  await act(async () => fireEvent.click(sidebar.getByText('Web folder')));
  fireEvent.click(sidebar.getByText(de.tagsHeading));
  await act(async () => fireEvent.click(sidebar.getByRole('button', { name: /#web-tag/ })));
  await act(async () => fireEvent.click(sidebar.getByRole('button', { name: 'Web collection' })));
  expect(commands).toContain('list_collection_files');
  await act(async () => fireEvent.click(screen.getByRole('button', { name: de.trashHeading })));
  await act(async () => fireEvent.click(screen.getByRole('button', { name: de.settingsTitle })));
  await act(async () => fireEvent.click(screen.getByRole('tab', { name: de.settingsTabInfo })));
  expect(screen.getByRole('button', { name: de.railFilament })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: de.railPrinters })).toBeInTheDocument();
  const allowed = new Set<string>(WEB_READ_COMMANDS);
  expect(commands.filter(command => !allowed.has(command))).toEqual([]);
  console.info('WEB_COMMANDS', [...new Set(commands)].sort().join(', '));
});
