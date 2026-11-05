import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';
import { makeModelFile } from '../test/factories';

const runtime = vi.hoisted(() => ({ scene: null as import('three').Scene | null }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../hooks/useModelImages', () => ({ useModelImages: () => new Map() }));
vi.mock('three', async importOriginal => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: class {
    domElement = document.createElement('canvas');
    setPixelRatio() {} setSize() {} forceContextLoss() {} dispose() {}
    render(scene: import('three').Scene) { runtime.scene = scene; }
  } };
});
vi.mock('three/examples/jsm/controls/OrbitControls.js', async () => {
  const { Vector3 } = await import('three');
  return { OrbitControls: class {
    target = new Vector3();
    constructor(private camera: import('three').PerspectiveCamera) {}
    update() { this.camera.lookAt(this.target); this.camera.updateMatrixWorld(); }
    dispose() {}
  } };
});

/** `count` single-triangle meshes, side by side; the last one is the collective part when `rest` is set. */
function buffer(count: number, rest = false) {
  const meshes = Array.from({ length: count }, (_, i) => ({ vertexCount: 3, indexCount: 3, hasNormal: false,
    ...(rest && i === count - 1 ? { objectName: 'step:rest' } : {}) }));
  const header = JSON.stringify({ meshes, palette: [], plates: [] });
  const bytes = new TextEncoder().encode(header + ' '.repeat((4 - new TextEncoder().encode(header).length % 4) % 4));
  const result = new ArrayBuffer(4 + bytes.length + count * 48);
  new DataView(result).setUint32(0, bytes.length, true);
  new Uint8Array(result, 4, bytes.length).set(bytes);
  let offset = 4 + bytes.length;
  for (let i = 0; i < count; i++) {
    const x = i * 40;
    new Float32Array(result, offset, 9).set([x, 0, 0, x + 10, 0, 0, x, 20, 0]); offset += 36;
    new Uint32Array(result, offset, 3).set([0, 1, 2]); offset += 12;
  }
  return result;
}

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(invoke).mockReset(); runtime.scene = null;
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1));
  vi.stubGlobal('cancelAnimationFrame', vi.fn());
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
});

const viewer = (path: string, controls = true) => <LanguageProvider><ModelViewer fileId="9" showRotationControls={controls}
  model={makeModelFile({ id: '9', path, name: path.split('/').pop()! })} needsSnapshot={false} onSnapshotCaptured={() => {}} /></LanguageProvider>;
const meshes = () => (runtime.scene!.children.find(child => child instanceof THREE.Group) as THREE.Group).children as THREE.Mesh[];

it('lists the bodies of a STEP file and hides and shows them in the picture', async () => {
  vi.mocked(invoke).mockResolvedValue(buffer(3));
  render(viewer('/models/Getriebe.step'));
  await screen.findByRole('group', { name: 'Teile' });
  expect(screen.getAllByRole('checkbox')).toHaveLength(3);
  expect(meshes().map(mesh => mesh.visible)).toEqual([true, true, true]);
  fireEvent.click(screen.getByRole('checkbox', { name: 'Teil 2 von 3, sichtbar' }));
  expect(meshes().map(mesh => mesh.visible)).toEqual([true, false, true]);
  expect(screen.getByRole('checkbox', { name: 'Teil 2 von 3, ausgeblendet' })).not.toBeChecked();
  fireEvent.click(screen.getByRole('button', { name: 'Alle zeigen' }));
  expect(meshes().map(mesh => mesh.visible)).toEqual([true, true, true]);
});

it('highlights the hovered part with its own material and restores it afterwards', async () => {
  vi.mocked(invoke).mockResolvedValue(buffer(2));
  render(viewer('/models/Getriebe.stp'));
  await screen.findByRole('group', { name: 'Teile' });
  const base = meshes()[0].material;
  const row = screen.getByRole('checkbox', { name: 'Teil 1 von 2, sichtbar' }).closest('[data-part-row]')!;
  fireEvent.mouseEnter(row);
  expect(meshes()[0].material).not.toBe(base);
  expect(meshes()[1].material).toBe(base);
  fireEvent.mouseLeave(row);
  expect(meshes()[0].material).toBe(base);
});

it('shows the size of the chosen part and labels the collective part', async () => {
  vi.mocked(invoke).mockResolvedValue(buffer(3, true));
  render(viewer('/models/Getriebe.step'));
  await screen.findByRole('group', { name: 'Teile' });
  fireEvent.click(screen.getByRole('button', { name: 'Weitere Teile' }));
  expect(screen.getByRole('status')).toHaveTextContent('Maße: 10 × 20 × 0 mm');
  expect(screen.getByRole('button', { name: 'Weitere Teile als STL speichern' })).toBeEnabled();
});

it('opens the STL dialog with the chosen part index', async () => {
  vi.mocked(invoke).mockResolvedValue(buffer(3));
  render(viewer('/models/Getriebe.step'));
  await screen.findByRole('group', { name: 'Teile' });
  fireEvent.click(screen.getByRole('button', { name: 'Teil 3' }));
  fireEvent.click(screen.getByRole('button', { name: 'Teil 3 als STL speichern' }));
  expect(await screen.findByRole('radio', { name: 'Gewählte Teile (1)' })).toBeChecked();
});

it.each([
  ['a STEP file with a single body', '/models/Würfel.step', 1, true],
  ['a file that is not STEP, even with several meshes', '/models/Box.3mf', 3, true],
  ['the small preview without controls', '/models/Getriebe.step', 3, false],
])('shows no parts list for %s', async (_, path, count, controls) => {
  vi.mocked(invoke).mockResolvedValue(buffer(count));
  render(viewer(path, controls));
  await waitFor(() => expect(runtime.scene && meshes().length).toBe(count));
  expect(screen.queryByRole('group', { name: 'Teile' })).not.toBeInTheDocument();
});
