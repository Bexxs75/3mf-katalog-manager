import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';

const runtime = vi.hoisted(() => ({ scene: null as import('three').Scene | null, camera: null as import('three').PerspectiveCamera | null }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('three', async importOriginal => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: class {
    domElement = document.createElement('canvas');
    setPixelRatio() {} setSize() {} forceContextLoss() {} dispose() {}
    render(scene: import('three').Scene, camera: import('three').PerspectiveCamera) { runtime.scene = scene; runtime.camera = camera; }
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

function buffer(plateCount = 2) {
  const meshes = [1, 2, null].map(plate => ({vertexCount: 3, indexCount: 3, hasNormal: false,
    plate, objectName: 'Same', groups: [{start: 0, count: 3, colorIndex: plate === 1 ? 0 : 1}]}));
  const header = JSON.stringify({meshes, palette: [{name: 'Red', color: '#ff0000'}, {name: 'Blue', color: '#0000ff'}],
    plates: Array.from({length: plateCount}, (_, i) => ({number: i + 1, name: i === 1 ? 'Regalplatte' : null}))});
  const bytes = new TextEncoder().encode(header + ' '.repeat((4 - new TextEncoder().encode(header).length % 4) % 4));
  const result = new ArrayBuffer(4 + bytes.length + meshes.length * 48);
  new DataView(result).setUint32(0, bytes.length, true);
  new Uint8Array(result, 4, bytes.length).set(bytes);
  let offset = 4 + bytes.length;
  meshes.forEach((_, i) => {
    const x = i * 400;
    new Float32Array(result, offset, 9).set([x, 0, 0, x + 10, 0, 0, x, 10, 0]); offset += 36;
    new Uint32Array(result, offset, 3).set([0, 1, 2]); offset += 12;
  });
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
const viewer = (id: string) => <LanguageProvider><ModelViewer fileId={id} showRotationControls
  needsSnapshot={false} onSnapshotCaptured={() => {}} /></LanguageProvider>;
function objects() {
  const scene = runtime.scene!;
  const group = scene.children.find(child => child instanceof THREE.Group)!;
  const floor = scene.children.find(child => child instanceof THREE.Mesh) as THREE.Mesh;
  return {group, floor};
}
it('filters existing meshes and legend, refits floor and resets only on model change', async () => {
  vi.mocked(invoke).mockResolvedValue(buffer());
  const result = render(viewer('first'));
  await screen.findByRole('button', {name: 'Nächste Platte'});
  const {group, floor} = objects();
  const originalMeshes = [...group.children];
  const allSize = floor.scale.x;
  const allDistance = runtime.camera!.position.length();
  expect(group.children.map(mesh => mesh.visible)).toEqual([true, true, true]);
  expect(screen.getByText('Red')).toBeVisible(); expect(screen.getByText('Blue')).toBeVisible();
  fireEvent.click(screen.getByRole('button', {name: 'Nächste Platte'}));
  expect(group.children).toEqual(originalMeshes);
  expect(group.children.map(mesh => mesh.visible)).toEqual([true, false, false]);
  expect(floor.scale.x).toBeLessThan(allSize / 10);
  expect(runtime.camera!.position.length()).toBeLessThan(allDistance / 10);
  expect(screen.queryByText('Blue')).not.toBeInTheDocument();
  expect(invoke).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', {name: 'Einfarbig'}));
  expect(screen.queryByText('Red')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name: 'Dateifarben'}));
  expect(screen.getByText('Red')).toBeVisible();
  fireEvent.click(screen.getByRole('button', {name: 'Ansicht zurücksetzen'}));
  expect(group.children.map(mesh => mesh.visible)).toEqual([true, false, false]);
  fireEvent.click(screen.getByRole('button', {name: 'Alle'}));
  expect(group.children.map(mesh => mesh.visible)).toEqual([true, true, true]);
  expect(floor.scale.x).toBeCloseTo(allSize);
  fireEvent.click(screen.getByRole('button', {name: 'Platte wählen'}));
  fireEvent.click(screen.getByRole('option', {name: /^Platte 2/}));
  result.rerender(viewer('second'));
  await waitFor(() => expect(screen.getByRole('button', {name: 'Alle'})).toHaveAttribute('aria-pressed', 'true'));
  expect(objects().group.children.map(mesh => mesh.visible)).toEqual([true, true, true]);
  expect(invoke).toHaveBeenCalledTimes(2);
});
it.each([0, 1])('hides plate selection for %s plate files', async count => {
  vi.mocked(invoke).mockResolvedValue(buffer(count));
  render(viewer('single'));
  await screen.findByRole('button', {name: 'Einpassen'});
  expect(screen.queryByRole('group', {name: 'Druckplatte'})).not.toBeInTheDocument();
});

it('moves plates outside the measured surface and keeps them in the compact panel', async () => {
  vi.mocked(invoke).mockResolvedValue(buffer());
  render(<LanguageProvider><ModelViewer fileId="panel" needsSnapshot={false} onSnapshotCaptured={() => {}} /></LanguageProvider>);
  const plates = await screen.findByRole('group', {name: 'Druckplatte'});
  expect(plates.closest('[data-viewer-surface]')).toBeNull();
  expect(plates).toHaveClass('w-full', 'flex-nowrap');
});
it('adapts controls and legend to measured container size', async () => {
  let resize!: () => void;
  let surface!: HTMLElement;
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { resize = callback; }
    observe(element: HTMLElement) { surface = element; }
    disconnect() {}
  });
  vi.mocked(invoke).mockResolvedValue(buffer());
  render(viewer('resize'));
  await screen.findByRole('button', {name: 'Einpassen'});
  Object.defineProperties(surface, {clientWidth: {value: 350, configurable: true}, clientHeight: {value: 260, configurable: true}});
  act(() => resize());
  const reset = screen.getByRole('button', {name: 'Ansicht zurücksetzen'});
  expect(reset).toHaveAttribute('title', 'Ansicht zurücksetzen');
  expect(reset).toHaveTextContent('');
  expect(reset.querySelector('svg')).not.toBeNull();
  expect(screen.queryByText('Red')).not.toBeInTheDocument();
  const legend = screen.getByRole('button', {name: 'Legende'});
  expect(legend).toHaveAttribute('aria-expanded', 'false');
  fireEvent.click(legend);
  expect(screen.getByText('Red')).toBeVisible();
  fireEvent.click(legend);
  expect(screen.queryByText('Red')).not.toBeInTheDocument();
  expect(screen.getByRole('button', {name: 'Dateifarben'}).parentElement).toHaveClass('viewer-toggle-compact', 'top-14');
  Object.defineProperties(surface, {clientWidth: {value: 800, configurable: true}, clientHeight: {value: 600, configurable: true}});
  act(() => resize());
  expect(reset).toHaveTextContent('Ansicht zurücksetzen');
  expect(screen.getByText('Red')).toBeVisible();
  expect(screen.queryByRole('button', {name: 'Legende'})).not.toBeInTheDocument();
  expect(screen.getByRole('button', {name: 'Dateifarben'}).parentElement).not.toHaveClass('viewer-toggle-compact');
  Object.defineProperties(surface, {clientWidth: {value: 350, configurable: true}, clientHeight: {value: 265, configurable: true}});
  act(() => resize());
  expect(reset).toHaveTextContent('');
  expect(screen.getByRole('button', {name: 'Dateifarben'}).parentElement).toHaveClass('viewer-toggle-compact');
});

function boxesBuffer(boxes: { plate: number; min: number[]; max: number[] }[]) {
  const header = JSON.stringify({palette: [], meshes: boxes.map(box => ({vertexCount: 3, indexCount: 3, hasNormal: false, plate: box.plate})),
    plates: [{number: 1}, {number: 2}]});
  const bytes = new TextEncoder().encode(header + ' '.repeat((4 - new TextEncoder().encode(header).length % 4) % 4));
  const buffer = new ArrayBuffer(4 + bytes.length + boxes.length * 48);
  new DataView(buffer).setUint32(0, bytes.length, true);
  new Uint8Array(buffer, 4, bytes.length).set(bytes);
  boxes.forEach((box, index) => {
    const offset = 4 + bytes.length + index * 48;
    new Float32Array(buffer, offset, 9).set([...box.min, ...box.max, box.min[0], box.max[1], box.min[2]]);
    new Uint32Array(buffer, offset + 36, 3).set([0, 1, 2]);
  });
  return buffer;
}

it.each([350 / 265, 0.5, 2])('frames all transformed meshes of a plate at aspect %s, including distant objects', async aspect => {
  const boxes = [
    {plate: 1, min: [-2000, -1000, 0], max: [-1980, -980, 20]},
    {plate: 2, min: [364, 100, 0], max: [384, 120, 20]},
    {plate: 2, min: [100, 210, 0], max: [114, 215, 10]},
    {plate: 2, min: [580, -250, 0], max: [594, -245, 10]},
  ];
  vi.mocked(invoke).mockResolvedValue(boxesBuffer(boxes));
  render(viewer('distant'));
  await screen.findByRole('button', {name: 'Nächste Platte'});
  runtime.camera!.aspect = aspect;
  fireEvent.click(screen.getByRole('button', {name: 'Platte wählen'}));
  fireEvent.click(screen.getByRole('option', {name: /^Platte 2/}));
  const { group } = objects();
  const bounds = new THREE.Box3();
  group.updateWorldMatrix(true, true);
  group.traverseVisible(child => {
    if (child instanceof THREE.Mesh) bounds.union(new THREE.Box3().setFromObject(child));
  });
  expect(group.children.filter(child => child.visible)).toHaveLength(3);
  expect(bounds.getCenter(new THREE.Vector3()).length()).toBeCloseTo(0);
  bounds.getSize(new THREE.Vector3()).toArray().forEach((value, index) => expect(value).toBeCloseTo([494, 20, 465][index]));
  const camera = runtime.camera!;
  expect(camera.getWorldDirection(new THREE.Vector3()).dot(camera.position.clone().negate().normalize())).toBeCloseTo(1);
  for (const x of [bounds.min.x, bounds.max.x]) for (const y of [bounds.min.y, bounds.max.y]) for (const z of [bounds.min.z, bounds.max.z]) {
    const projected = new THREE.Vector3(x, y, z).project(camera);
    expect(Math.abs(projected.x)).toBeLessThan(0.92);
    expect(Math.abs(projected.y)).toBeLessThan(0.92);
    expect(Math.abs(projected.z)).toBeLessThan(1);
  }
});

it('includes both overlapping clips from the Creality fixture in the world-space frame', async () => {
  // Extents from the fixture's component vertices plus its build translations.
  vi.mocked(invoke).mockResolvedValue(boxesBuffer([
    {plate: 1, min: [100, 100, 0], max: [120, 120, 20]},
    {plate: 2, min: [364, 100, 0], max: [384, 120, 20]},
    {plate: 2, min: [365, 102.625, 0], max: [383, 120.625, 10]},
    {plate: 2, min: [367, 99.375, 0], max: [381, 102.875, 10]},
  ]));
  render(viewer('overlap'));
  await screen.findByRole('button', {name: 'Nächste Platte'});
  fireEvent.click(screen.getByRole('button', {name: 'Platte wählen'}));
  fireEvent.click(screen.getByRole('option', {name: /^Platte 2/}));
  const { group } = objects();
  const cube = new THREE.Box3().setFromObject(group.children[1]);
  const firstClip = new THREE.Box3().setFromObject(group.children[2]);
  const outerClip = new THREE.Box3().setFromObject(group.children[3]);
  expect(cube.intersectsBox(firstClip)).toBe(true);
  expect(firstClip.min.z).toBeCloseTo(cube.min.z - 0.625);
  expect(cube.intersectsBox(outerClip)).toBe(true);
  expect(outerClip.max.z).toBeCloseTo(cube.max.z + 0.625);
  expect(group.children.map(mesh => mesh.visible)).toEqual([false, true, true, true]);
});
