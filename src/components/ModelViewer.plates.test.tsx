import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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
    target = new Vector3(); update() {} dispose() {}
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
  await screen.findByRole('radio', {name: 'Platte 1'});
  const {group, floor} = objects();
  const originalMeshes = [...group.children];
  const allSize = floor.scale.x;
  const allDistance = runtime.camera!.position.length();
  expect(group.children.map(mesh => mesh.visible)).toEqual([true, true, true]);
  expect(screen.getByText('Red')).toBeVisible(); expect(screen.getByText('Blue')).toBeVisible();
  fireEvent.click(screen.getByRole('radio', {name: 'Platte 1'}));
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
  fireEvent.click(screen.getByRole('radio', {name: 'Alle'}));
  expect(group.children.map(mesh => mesh.visible)).toEqual([true, true, true]);
  expect(floor.scale.x).toBeCloseTo(allSize);
  fireEvent.click(screen.getByRole('radio', {name: 'Platte 2'}));
  result.rerender(viewer('second'));
  await waitFor(() => expect(screen.getByRole('radio', {name: 'Alle'})).toHaveAttribute('aria-checked', 'true'));
  expect(objects().group.children.map(mesh => mesh.visible)).toEqual([true, true, true]);
  expect(invoke).toHaveBeenCalledTimes(2);
});
it.each([0, 1])('hides plate selection for %s plate files', async count => {
  vi.mocked(invoke).mockResolvedValue(buffer(count));
  render(viewer('single'));
  await screen.findByRole('button', {name: 'Einpassen'});
  expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument();
});
