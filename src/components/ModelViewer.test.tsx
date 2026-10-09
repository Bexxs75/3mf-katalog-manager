import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Mimics WebKit without WebGL: the renderer constructor throws.
const rendererCtor = vi.fn(() => {
  throw new TypeError("null is not an object (evaluating 'e.getShaderPrecisionFormat(e.VERTEX_SHADER,e.HIGH_FLOAT).precision')");
});
vi.mock('three', async (importOriginal) => {
  const actual = await importOriginal<typeof import('three')>();
  return {
    ...actual,
    WebGLRenderer: function WebGLRenderer() {
      return rendererCtor();
    },
  };
});

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(new ArrayBuffer(0));
  localStorage.setItem('3mf-katalog-language', 'de');
});

function renderViewer(fileId: string, onError: () => void) {
  return render(
    <LanguageProvider>
      <ModelViewer fileId={fileId} needsSnapshot onSnapshotCaptured={() => {}} onError={onError} />
    </LanguageProvider>,
  );
}

describe('ModelViewer without WebGL', () => {
  it('shows a WebGL explanation instead of crashing the app', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const onError = vi.fn();
    renderViewer('a', onError);
    expect(onError).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText('Die 3D-Ansicht wird auf diesem Rechner nicht unterstützt')).toBeInTheDocument());
    expect(onError).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalledWith('get_model_geometry', expect.anything());
  });

  it('does not try a new WebGL context for every further model', async () => {
    const calls = rendererCtor.mock.calls.length;
    const onError = vi.fn();
    renderViewer('b', onError);
    await waitFor(() => expect(onError).toHaveBeenCalledTimes(1));
    expect(rendererCtor.mock.calls.length).toBe(calls);
  });
});

it('measures and remeasures the surface even when WebGL is unavailable', () => {
  let resize!: () => void;
  let surface!: HTMLElement;
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { resize = callback; }
    observe(element: HTMLElement) { surface = element; }
    disconnect() {}
  });
  const { container } = renderViewer('resize-error', vi.fn());
  expect(surface).toBeDefined();
  for (const [width, height, compact] of [[350, 265, true], [800, 600, false], [500, 500, true]] as const) {
    Object.defineProperties(surface, {clientWidth: {value: width, configurable: true}, clientHeight: {value: height, configurable: true}});
    act(() => resize());
    expect(container.querySelector('[data-viewer-surface]')).toHaveAttribute('data-compact', String(compact));
    expect(screen.getByRole('alert').classList.contains('viewer-error-compact')).toBe(compact);
  }
});


it('cancels a deferred WebGL error when the viewer unmounts', async () => {
  const onError = vi.fn();
  const view = renderViewer('unmounted', onError);
  view.unmount();
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });
  expect(onError).not.toHaveBeenCalled();
});
