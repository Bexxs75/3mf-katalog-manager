import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor, fireEvent } from '@testing-library/react';
import * as THREE from 'three';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { ModelViewer } from './ModelViewer';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../diagnostics/ReportProblemLink', () => ({ ReportProblemLink: () => <button>Problem melden</button> }));
vi.mock('../hooks/useModelImages', () => ({ useModelImages: () => new Map() }));
let scene: THREE.Scene;
const forceContextLoss = vi.fn();
const lutDispose = vi.fn();
const lut = { dispose: lutDispose };
const materialProperties = vi.fn((_material: THREE.Material) => ({ uniforms: { dfgLUT: { value: lut } } }));
vi.mock('three', async (importOriginal) => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: function () {
    return { domElement: document.createElement('canvas'), setPixelRatio: vi.fn(), setSize: vi.fn(),
      properties: { get: (material: THREE.Material) => materialProperties(material) },
      render: (s: THREE.Scene) => { scene = s; }, forceContextLoss, dispose: vi.fn() };
  } };
});
function buffer(colors = true, extra: Record<string, unknown> = {}) {
  const header = { meshes: [{ vertexCount: 3, hasNormal: false, indexCount: 3,
    objectName: 'Topf', ...(colors ? { groups: [{ start: 0, count: 3, colorIndex: 0 }] } : {}) }],
    palette: colors ? [{ name: 'Terrakotta', color: '#c8643c' }] : [], ...extra };
  const json = new TextEncoder().encode(JSON.stringify(header));
  const length = Math.ceil(json.length / 4) * 4;
  const result = new ArrayBuffer(4 + length + 48);
  new DataView(result).setUint32(0, length, true);
  new Uint8Array(result, 4, length).fill(32);
  new Uint8Array(result, 4, json.length).set(json);
  new Float32Array(result, 4 + length, 9).set([0, 0, 0, 1, 0, 0, 0, 1, 0]);
  new Uint32Array(result, 4 + length + 36, 3).set([0, 1, 2]);
  return result;
}
function modelMesh() {
  const object = scene.children.find(child => child instanceof THREE.Group)!;
  return object.children[0] as THREE.Mesh<THREE.BufferGeometry, THREE.MeshStandardMaterial | THREE.MeshStandardMaterial[]>;
}
function page(detail = true, id = 'a') {
  return <LanguageProvider><div data-app="light"><ModelViewer fileId={id} showRotationControls={detail}
    needsSnapshot={false} onSnapshotCaptured={vi.fn()} /></div></LanguageProvider>;
}
beforeEach(() => {
  materialProperties.mockReset().mockReturnValue({ uniforms: { dfgLUT: { value: lut } } });
  vi.mocked(invoke).mockReset().mockResolvedValue(buffer());
  vi.spyOn(console, 'error').mockImplementation(() => {});
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1));
  vi.stubGlobal('cancelAnimationFrame', vi.fn());
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe('viewer colors and lifecycle', () => {
  it.each([false, true])('persists an attempt before native geometry and clears it on completion (reject=%s)', async (reject) => {
    const id = `attempt-${reject}`;
    const key = `3mf-katalog-preview-attempt:${id}`;
    vi.mocked(invoke).mockImplementation(() => {
      expect(localStorage.getItem(key)).toBe('1');
      return reject ? Promise.reject(new Error('geometry failed')) : Promise.resolve(buffer());
    });
    render(page(true, id));
    await waitFor(() => expect(localStorage.getItem(key)).toBeNull());
  });

  it('switches actual mesh materials without fetching or recreating geometry', async () => {
    const view = render(page());
    await screen.findByRole('button', { name: 'Einfarbig' });
    const mesh = modelMesh();
    const geometry = mesh.geometry;
    expect(Array.isArray(mesh.material)).toBe(true);
    expect((mesh.material as THREE.MeshStandardMaterial[])[1].color.getHexString()).toBe('c8643c');
    expect((mesh.material as THREE.MeshStandardMaterial[])[1].flatShading).toBe(true);
    expect(geometry.getAttribute('normal')).toBeUndefined();
    expect(geometry.groups).toEqual([{ start: 0, count: 3, materialIndex: 1 }]);
    expect(screen.getByText('Topf')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Einfarbig' }));
    expect((mesh.material as THREE.MeshStandardMaterial).color.getHexString()).toBe('d0603f');
    expect(screen.queryByText('Terrakotta')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Dateifarben' }));
    expect(Array.isArray(mesh.material)).toBe(true);
    expect(mesh.geometry).toBe(geometry);
    expect(invoke).toHaveBeenCalledTimes(1);
    const disposed = vi.spyOn(geometry, 'dispose');
    view.unmount();
    expect(disposed).toHaveBeenCalledTimes(1);
    expect(forceContextLoss).toHaveBeenCalled();
  });
  it('uses file colors in the side panel without switch or legend', async () => {
    render(page(false));
    await waitFor(() => expect(Array.isArray(modelMesh().material)).toBe(true));
    expect(screen.queryByRole('button', { name: 'Dateifarben' })).not.toBeInTheDocument();
    expect(screen.queryByText('Terrakotta')).not.toBeInTheDocument();
  });
  it('has no switch or legend without file colors', async () => {
    vi.mocked(invoke).mockResolvedValue(buffer(false));
    render(page());
    await screen.findByRole('button', { name: 'Einpassen' });
    expect(screen.queryByRole('button', { name: 'Dateifarben' })).not.toBeInTheDocument();
    expect(screen.queryByText('Farben der Datei')).not.toBeInTheDocument();
  });
  it('names slicer filaments by slot and notes simplified painting', async () => {
    vi.mocked(invoke).mockResolvedValue(buffer(true, { palette: [{ name: 'PLA', color: '#f88343', filament: 2 }], paintSimplified: true }));
    render(page());
    expect(await screen.findByText('Filament 2 · PLA')).toBeInTheDocument();
    expect(screen.getByText(/Bemalung vereinfacht/)).toBeInTheDocument();
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Einfarbig' }));
    expect(screen.queryByText(/Bemalung vereinfacht/)).not.toBeInTheDocument();
  });
  it.each([true, false])('explains unread file colors in text (palette=%s)', async (colors) => {
    vi.mocked(invoke).mockResolvedValue(buffer(colors, { colorsUnresolved: true }));
    render(page());
    const note = await screen.findByRole('note');
    expect(note).toHaveTextContent('Nicht alle Farbangaben dieser Datei können angezeigt werden.');
    if (colors) {
      // Stays visible in single-color mode, where the legend is hidden.
      fireEvent.click(screen.getByRole('button', { name: 'Einfarbig' }));
      expect(screen.getByRole('note')).toBeInTheDocument();
    }
  });
  it('shows no color note for single-color models or in the side panel', async () => {
    vi.mocked(invoke).mockResolvedValue(buffer(false));
    const view = render(page());
    await screen.findByRole('button', { name: 'Einpassen' });
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
    view.unmount();
    vi.mocked(invoke).mockResolvedValue(buffer(false, { colorsUnresolved: true }));
    render(page(false));
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });
  it('resets colors and disposes geometry on a model change while keeping the scene', async () => {
    const view = render(page());
    await screen.findByRole('button', { name: 'Einfarbig' });
    fireEvent.click(screen.getByRole('button', { name: 'Einfarbig' }));
    const oldScene = scene;
    const dispose = vi.spyOn(modelMesh().geometry, 'dispose');
    view.rerender(page(true, 'b'));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Dateifarben' })).toHaveAttribute('aria-pressed', 'true'));
    expect(scene).toBe(oldScene);
    expect(dispose).toHaveBeenCalledOnce();
  });
  it('updates the grid token on theme changes without reloading geometry', async () => {
    const view = render(page());
    await screen.findByRole('button', { name: 'Einpassen' });
    const floor = scene.children.find(c => c instanceof THREE.Mesh) as THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>;
    const color = floor.material.uniforms.gridColor.value;
    const copy = vi.spyOn(color, 'copy');
    view.container.querySelector('[data-app]')!.setAttribute('data-app', 'dark');
    await waitFor(() => expect(copy).toHaveBeenCalledOnce());
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(floor.visible).toBe(true);
  });
});

describe('viewer backend errors', () => {
  it.each([
    ['notFound', 'Die Datei wurde nicht gefunden'], ['unreadable', 'Die Datei lässt sich nicht lesen'],
    ['tooLarge', 'Das Modell ist für die 3D-Ansicht zu groß'], ['unsupported', 'Dieser Dateityp unterstützt keine 3D-Vorschau'],
  ])('shows the %s explanation', async (code, title) => {
    vi.mocked(invoke).mockRejectedValue({ code, message: 'backend', expected: code !== 'unreadable' });
    render(page());
    expect(await screen.findByRole('alert')).toHaveTextContent(title);
    expect(screen.queryByRole('button', { name: 'Problem melden' }) !== null).toBe(code === 'unreadable');
  });
  it('announces loading', () => {
    vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
    render(page());
    expect(screen.getByRole('status')).toHaveTextContent('3D-Ansicht wird geladen');
  });
});

it('disconnects controls while attached and releases the shared DFG LUT once', async () => {
  lutDispose.mockClear();
  const roots: boolean[] = [];
  const original = OrbitControls.prototype.dispose;
  vi.spyOn(OrbitControls.prototype, 'dispose').mockImplementation(function (this: OrbitControls) {
    roots.push(this.domElement!.isConnected);
    original.call(this);
  });
  const add = vi.spyOn(document, 'addEventListener');
  const remove = vi.spyOn(document, 'removeEventListener');
  const view = render(page());
  await screen.findByRole('button', { name: 'Einpassen' });
  view.unmount();
  expect(roots[0]).toBe(true);
  const keyListeners = add.mock.calls.filter(([type, , options]) => type === 'keydown' && typeof options === 'object' && options.capture);
  expect(keyListeners.length).toBeGreaterThan(0);
  for (const [, listener] of keyListeners) {
    expect(remove).toHaveBeenCalledWith('keydown', listener, { capture: true });
  }
  expect(lutDispose).toHaveBeenCalledTimes(1);
});

it('retains the LUT from color-only materials across file changes until renderer disposal', async () => {
  lutDispose.mockClear();
  materialProperties.mockImplementation(material => (material as THREE.MeshStandardMaterial).color.getHexString() === 'c8643c'
    ? { uniforms: { dfgLUT: { value: lut } } } : {} as ReturnType<typeof materialProperties>);
  const view = render(page());
  await screen.findByRole('button', { name: 'Einpassen' });
  vi.mocked(invoke).mockResolvedValue(buffer(false));
  view.rerender(page(true, 'uncolored'));
  await waitFor(() => expect(modelMesh().material).not.toBeInstanceOf(Array));
  expect(lutDispose).not.toHaveBeenCalled();
  view.unmount();
  expect(lutDispose).toHaveBeenCalledTimes(1);
});

it('does not discard visible-viewer geometry on context loss without a recovery callback', async () => {
  let resolve!: (value: ArrayBuffer) => void;
  vi.mocked(invoke).mockReturnValue(new Promise<ArrayBuffer>(done => { resolve = done; }));
  const add = vi.spyOn(HTMLCanvasElement.prototype, 'addEventListener');
  const view = render(page());
  expect(add.mock.calls.filter(([type]) => type === 'webglcontextlost')).toEqual([]);
  fireEvent(view.container.querySelector('canvas')!, new Event('webglcontextlost'));
  resolve(buffer());
  await screen.findByRole('button', { name: 'Einpassen' });
});

it('does not request context recovery after delivering a capture', async () => {
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => { frames.push(cb); return frames.length; });
  vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue('data:image/png;base64,c25hcHNob3Q=');
  const onContextLost = vi.fn();
  const onSnapshotCaptured = vi.fn(() => {
    document.querySelector('canvas')!.dispatchEvent(new Event('webglcontextlost'));
  });
  render(<LanguageProvider><ModelViewer fileId="captured" needsSnapshot showRotationControls
    onSnapshotCaptured={onSnapshotCaptured} onContextLost={onContextLost} /></LanguageProvider>);
  await screen.findByRole('button', { name: 'Einpassen' });
  for (let i = 0; i < 2; i++) {
    await act(async () => { frames.splice(0).forEach(cb => cb(0)); });
  }
  expect(onSnapshotCaptured).toHaveBeenCalledTimes(1);
  expect(onContextLost).not.toHaveBeenCalled();
});
