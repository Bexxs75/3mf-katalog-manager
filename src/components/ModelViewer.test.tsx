import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
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
