import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import * as THREE from 'three';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../diagnostics/ReportProblemLink', () => ({ ReportProblemLink: () => <button>Problem melden</button> }));
vi.mock('../hooks/useModelImages', () => ({ useModelImages: () => new Map() }));
let scene: THREE.Scene;
const forceContextLoss = vi.fn();
vi.mock('three', async (importOriginal) => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: function () {
    return { domElement: document.createElement('canvas'), setPixelRatio: vi.fn(), setSize: vi.fn(),
      render: (s: THREE.Scene) => { scene = s; }, forceContextLoss, dispose: vi.fn() };
  } };
});
function buffer(colors = true) {
  const header = { meshes: [{ vertexCount: 3, hasNormal: false, indexCount: 3,
    objectName: 'Topf', ...(colors ? { groups: [{ start: 0, count: 3, colorIndex: 0 }] } : {}) }],
    palette: colors ? [{ name: 'Terrakotta', color: '#c8643c' }] : [] };
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
