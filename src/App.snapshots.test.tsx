import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, render } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { makeModelFile, makeModelFileSummary } from './test/factories';
import type { useCatalogStore } from './hooks/useCatalogStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
// Keep the real App queue wiring and store, without rendering unrelated catalog tiles.
vi.mock('./components/CatalogWorkspace', () => ({ CatalogWorkspace: () => null }));
let store: ReturnType<typeof useCatalogStore>;
const skippedSizes: number[] = [];
vi.mock('./hooks/useCatalogStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./hooks/useCatalogStore')>();
  return { useCatalogStore: (...args: Parameters<typeof useCatalogStore>) => {
    store = actual.useCatalogStore(...args);
    if (skippedSizes[skippedSizes.length - 1] !== store.skippedSnapshotIds.size) skippedSizes.push(store.skippedSnapshotIds.size);
    return store;
  } };
});
const rendererCtor = vi.fn();
vi.mock('three', async (importOriginal) => ({
  ...await importOriginal<typeof import('three')>(),
  WebGLRenderer: function () { return rendererCtor(); },
}));

let viewerMounts = 0;
let models: ReturnType<typeof makeModelFile>[];
let frames: Map<number, FrameRequestCallback>;
let nextFrame: number;
const saved: string[] = [];

beforeEach(() => {
  skippedSizes.length = 0;
  saved.length = 0;
  viewerMounts = 0;
  frames = new Map();
  nextFrame = 0;
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/test');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  localStorage.setItem('3mf-katalog-display-preference', 'render');
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(300);
  vi.stubGlobal('ResizeObserver', class {
    observe(element: HTMLElement) { if (element.parentElement?.hasAttribute('data-viewer-surface')) viewerMounts++; }
    disconnect() {}
  });
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => { frames.set(++nextFrame, cb); return nextFrame; });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
  rendererCtor.mockReset().mockImplementation(() => {
    const canvas = document.createElement('canvas');
    canvas.toDataURL = () => 'data:image/png;base64,c25hcHNob3Q=';
    return { domElement: canvas, setPixelRatio() {}, setSize() {}, render() {}, forceContextLoss() {}, dispose() {} };
  });
  vi.mocked(invoke).mockReset().mockImplementation(async (cmd, args) => {
    if (cmd === 'list_file_summaries') return models.map(makeModelFileSummary);
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'list_files_by_ids') return models.filter(m => (args as { ids: string[] }).ids.includes(m.id));
    if (cmd === 'get_model_geometry') {
      const header = new TextEncoder().encode('{"meshes":[],"palette":[]}');
      const buffer = new ArrayBuffer(4 + header.length);
      new DataView(buffer).setUint32(0, header.length, true);
      new Uint8Array(buffer, 4).set(header);
      return buffer;
    }
    if (cmd === 'set_render_snapshot') {
      const { fileId, imageBase64 } = args as { fileId: string; imageBase64: string };
      saved.push(fileId);
      models = models.map(m => m.id === fileId ? { ...m, renderSnapshotImage: `data:image/png;base64,${imageBase64}` } : m);
      return;
    }
    if (cmd === 'get_app_version') return '0.16.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    return [];
  });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

async function mountQueue(count: number) {
  models = Array.from({ length: count }, (_, i) => makeModelFile({ id: `m${i}`, path: `/test/${i}.stl` }));
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
}
async function frame() {
  await act(async () => {
    const callbacks = [...frames.values()];
    frames.clear();
    callbacks.forEach(cb => cb(0));
  });
}

it('stores a snapshot and mounts the next file with working WebGL', async () => {
  await mountQueue(2);
  expect(store.pendingSnapshotIds).toHaveLength(2);
  await frame();
  await frame();
  expect(saved).toEqual(['m0']);
  expect(store.models[0].renderSnapshotImage).toBe('data:image/png;base64,c25hcHNob3Q=');
  expect(store.pendingSnapshotIds).toEqual(['m1']);
  await frame();
  await frame();
  expect(saved).toEqual(['m0', 'm1']);
  expect(store.pendingSnapshotIds).toEqual([]);
  expect(viewerMounts).toBe(2);
});

it('drains 600 snapshots in 3D display mode without nested updates with working WebGL', async () => {
  await mountQueue(600);
  for (let i = 0; i < 1200; i++) await frame();
  expect(saved).toEqual(models.map(m => m.id));
  expect(store.pendingSnapshotIds).toEqual([]);
  expect(store.skippedSnapshotIds.size).toBe(0);
  expect(viewerMounts).toBe(600);
}, 30000);

it.each(['geometry', 'snapshot'])('skips only the failing file after a %s error', async (failure) => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  if (failure === 'geometry') {
    const fallback = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'get_model_geometry' && (args as { fileId: string }).fileId === 'm0') throw new Error('Bad geometry');
      return fallback(cmd, args);
    });
  } else {
    const fallback = rendererCtor.getMockImplementation()!;
    rendererCtor.mockImplementationOnce(() => {
      const renderer = fallback();
      renderer.domElement.toDataURL = () => { throw new Error('Snapshot failed'); };
      return renderer;
    });
  }
  await mountQueue(2);
  if (failure === 'snapshot') { await frame(); await frame(); }
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });
  expect([...store.skippedSnapshotIds]).toEqual(['m0']);
  await frame();
  await frame();
  expect(saved).toEqual(['m1']);
  expect(store.pendingSnapshotIds).toEqual([]);
});

it('skips 200 pending files in one update without remounting when WebGL creation throws', async () => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  rendererCtor.mockImplementation(() => { throw new Error('No WebGL'); });
  await mountQueue(200);
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });
  expect(store.pendingSnapshotIds).toEqual([]);
  expect(store.skippedSnapshotIds.size).toBe(200);
  expect(skippedSizes).toEqual([0, 200]);
  expect(viewerMounts).toBe(1);
  expect(rendererCtor).toHaveBeenCalledTimes(1);

  // The session-level failure must also cover later imports and refreshes.
  await act(async () => {
    store.setModels(prev => [...prev, makeModelFile({ id: 'later', path: '/test/later.stl' })]);
  });
  expect(store.pendingSnapshotIds).toEqual([]);
  expect(store.skippedSnapshotIds.size).toBe(201);
  expect(viewerMounts).toBe(1);
  await act(async () => { await store.refreshFiles(); });
  expect(store.pendingSnapshotIds).toEqual([]);
  expect(viewerMounts).toBe(1);
});
