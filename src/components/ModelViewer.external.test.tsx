import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';

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

it('asks for the geometry of an external source by its id and never by a catalog file id', async () => {
  render(<LanguageProvider><ModelViewer fileId="ext-1" externalSourceId="ext-1" needsSnapshot={false} onSnapshotCaptured={vi.fn()} /></LanguageProvider>);
  await screen.findByRole('alert');
  expect(geometry).toHaveBeenCalledWith('get_external_geometry', { sourceId: 'ext-1' });
  expect(geometry).not.toHaveBeenCalledWith('get_model_geometry', expect.anything());
});

it('keeps asking the catalog command for a cataloged file', async () => {
  render(<LanguageProvider><ModelViewer fileId="9" needsSnapshot={false} onSnapshotCaptured={vi.fn()} /></LanguageProvider>);
  await screen.findByRole('alert');
  expect(geometry).toHaveBeenCalledWith('get_model_geometry', { fileId: '9' });
  expect(geometry).not.toHaveBeenCalledWith('get_external_geometry', expect.anything());
});
