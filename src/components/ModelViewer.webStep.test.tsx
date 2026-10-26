import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';
import { makeModelFile } from '../test/factories';

const geometry = vi.hoisted(() => vi.fn());
vi.mock('../lib/transport', () => ({ invoke: geometry }));
vi.mock('../hooks/useModelImages', () => ({ useModelImages: () => new Map() }));
vi.mock('three', async (importOriginal) => {
  const actual = await importOriginal<typeof import('three')>();
  return { ...actual, WebGLRenderer: function () {
    return { domElement: document.createElement('canvas'), setPixelRatio: vi.fn(), setSize: vi.fn(),
      properties: { get: () => ({ uniforms: {} }) }, render: vi.fn(), forceContextLoss: vi.fn(), dispose: vi.fn() };
  } };
});
beforeEach(() => {
  geometry.mockReset().mockRejectedValue({ code: 'notFound', message: 'x', expected: true });
  vi.spyOn(console, 'error').mockImplementation(() => {});
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1));
  vi.stubGlobal('cancelAnimationFrame', vi.fn());
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it('explains a STEP file in the web build without asking the server for geometry', async () => {
  const model = makeModelFile({ id: '9', path: '/models/würfel.STEP', name: 'würfel.STEP' });
  render(<LanguageProvider><ModelViewer fileId="9" model={model} needsSnapshot={false} onSnapshotCaptured={vi.fn()} /></LanguageProvider>);
  expect(await screen.findByRole('alert')).toHaveTextContent('Dieser Dateityp unterstützt keine 3D-Vorschau');
  expect(geometry).not.toHaveBeenCalled();
  expect(console.error).not.toHaveBeenCalledWith('[ModelViewer] loading failed:', expect.anything());
});
it('still loads geometry for other formats in the web build', async () => {
  const model = makeModelFile({ id: '9', path: '/models/a.stl', name: 'a.stl' });
  render(<LanguageProvider><ModelViewer fileId="9" model={model} needsSnapshot={false} onSnapshotCaptured={vi.fn()} /></LanguageProvider>);
  await screen.findByRole('alert');
  expect(geometry).toHaveBeenCalledWith('get_model_geometry', { fileId: '9' });
});
