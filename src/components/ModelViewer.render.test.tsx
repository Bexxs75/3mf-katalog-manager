import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';

const runtime = vi.hoisted(() => ({ render: vi.fn(), capture: vi.fn(), resize: () => {} }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('three', async importOriginal => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: class {
    domElement = document.createElement('canvas');
    setPixelRatio = vi.fn();
    setSize = vi.fn();
    forceContextLoss = vi.fn();
    dispose = vi.fn();
    render = runtime.render;
  } };
});

function buffer() {
  const header = JSON.stringify({ meshes: [{ vertexCount: 3, indexCount: 3, hasNormal: false,
    groups: [{ start: 0, count: 3, colorIndex: 0 }] }], palette: [{ name: 'Red', color: '#ff0000' }] });
  const bytes = new TextEncoder().encode(header);
  const length = Math.ceil(bytes.length / 4) * 4;
  const result = new ArrayBuffer(4 + length + 48);
  new DataView(result).setUint32(0, length, true);
  new Uint8Array(result, 4, length).fill(32);
  new Uint8Array(result, 4, bytes.length).set(bytes);
  new Float32Array(result, 4 + length, 9).set([0, 0, 0, 10, 0, 0, 0, 10, 0]);
  new Uint32Array(result, 4 + length + 36, 3).set([0, 1, 2]);
  return result;
}
const page = (snapshot = false, id = 'render') => <LanguageProvider><div data-app="light">
  <ModelViewer fileId={id} needsSnapshot={snapshot} onSnapshotCaptured={runtime.capture} showRotationControls />
</div></LanguageProvider>;
const tick = (n = 1) => act(() => { vi.advanceTimersByTime(n * 16); });
async function load(snapshot = false) {
  const view = render(page(snapshot));
  await act(async () => {});
  expect(screen.getByRole('button', { name: 'Einpassen' })).toBeInTheDocument();
  return view;
}
beforeEach(() => {
  vi.useFakeTimers();
  runtime.render.mockClear(); runtime.capture.mockClear();
  vi.mocked(invoke).mockResolvedValue(buffer());
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => setTimeout(() => cb(performance.now()), 16));
  vi.stubGlobal('cancelAnimationFrame', (id: number) => clearTimeout(id));
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { runtime.resize = callback; }
    observe() {} disconnect() {}
  });
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(500);
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
});
afterEach(() => {
  cleanup(); vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals();
});

it('renders the loaded model and then leaves 100 idle ticks without rendering', async () => {
  await load();
  tick(100);
  const scene = runtime.render.mock.lastCall![0] as import('three').Scene;
  expect(scene.children.some(child => child.type === 'Group' && child.children.length === 1)).toBe(true);
  runtime.render.mockClear();
  tick(100);
  expect(runtime.render).not.toHaveBeenCalled();
});

it('renders synchronously immediately before capturing the snapshot', async () => {
  const events: string[] = [];
  runtime.render.mockImplementation(() => events.push('render'));
  vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockImplementation(() => {
    events.push('capture'); return 'data:image/png;base64,c25hcHNob3Q=';
  });
  await load(true);
  tick();
  events.length = 0;
  tick();
  // One scheduled frame, then the explicit snapshot frame, then toDataURL.
  expect(events).toEqual(['render', 'render', 'capture']);
  expect(runtime.capture).toHaveBeenCalledWith('c25hcHNob3Q=');
  runtime.render.mockReset();
});

it('invalidates for change events even when the next update reports no movement and detaches on unmount', async () => {
  const add = vi.spyOn(OrbitControls.prototype, 'addEventListener');
  const remove = vi.spyOn(OrbitControls.prototype, 'removeEventListener');
  const view = await load();
  tick(100);
  const index = add.mock.calls.findIndex(([type]) => type === 'change');
  expect(index).toBeGreaterThanOrEqual(0);
  const controls = add.mock.contexts[index] as OrbitControls;
  vi.spyOn(controls, 'update').mockReturnValue(false);
  runtime.render.mockClear();
  controls.dispatchEvent({ type: 'change' });
  tick();
  expect(runtime.render).toHaveBeenCalledTimes(1);
  view.unmount();
  expect(remove).toHaveBeenCalledWith('change', add.mock.calls[index][1]);
  runtime.render.mockClear(); tick(100);
  expect(runtime.render).not.toHaveBeenCalled();
});

it('redraws after idle for colors, theme, resize and restored context', async () => {
  const view = await load();
  tick(100);
  const check = () => { tick(); expect(runtime.render).toHaveBeenCalledTimes(1); tick(100); runtime.render.mockClear(); };
  runtime.render.mockClear();
  fireEvent.click(screen.getByRole('button', { name: 'Einfarbig' })); check();
  await act(async () => { view.container.querySelector('[data-app]')!.setAttribute('data-app', 'dark'); }); check();
  act(() => runtime.resize()); check();
  fireEvent(view.container.querySelector('canvas')!, new Event('webglcontextrestored')); check();
});

it('redraws the cleared scene while a replacement model is still loading', async () => {
  const view = await load();
  tick(100); runtime.render.mockClear();
  vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
  view.rerender(page(false, 'replacement'));
  tick();
  expect(runtime.render).toHaveBeenCalledTimes(1);
  const scene = runtime.render.mock.lastCall![0] as import('three').Scene;
  expect(scene.children.some(child => child.type === 'Group')).toBe(false);
});
