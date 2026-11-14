import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import App from '../App';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
vi.mock('./CatalogWorkspace', () => ({ CatalogWorkspace: () => null }));
import { WebBackgroundSnapshots } from './WebBackgroundSnapshots';
import { authenticationFailed, login } from '../lib/webSession';
import { lostAnswerRetry } from '../lib/transport';
import { useCatalogStore } from '../hooks/useCatalogStore';
import { makeModelFile, makeModelFileSummary } from '../test/factories';
vi.mock('./BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: (p: { fileId: string; onSnapshotCaptured: (id: string, b64: string) => void; onError: (id: string) => void; onContextLost: (id: string) => void }) => <div data-testid="renderer"><button onClick={() => p.onSnapshotCaptured(p.fileId, 'cG5n')}>{p.fileId}</button><button onClick={() => p.onError(p.fileId)}>error</button><button onClick={() => p.onContextLost(p.fileId)}>lost</button></div> }));
let writable: boolean;
let status: number;
let saves: { fileId: string; imageBase64: string }[];
let release: (() => void) | undefined;
let hold: boolean;
let noWebGL: boolean;
function Harness() {
  const store = useCatalogStore();
  return <WebBackgroundSnapshots ids={store.pendingSnapshotIds} noWebGL={noWebGL} capture={store.captureRenderSnapshot} skip={store.skipSnapshot} />;
}
const flush = async () => { await act(async () => { await vi.advanceTimersByTimeAsync(0); }); };
const next = async () => { await act(async () => { await vi.advanceTimersByTimeAsync(250); }); };
beforeEach(async () => {
  vi.useFakeTimers(); lostAnswerRetry.delaysMs = []; writable = true; status = 200; saves = []; hold = false; release = undefined; noWebGL = false;
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.stubGlobal('fetch', vi.fn(async (url: string, opts?: RequestInit) => {
    const cmd = url.split('/').pop();
    if (cmd === 'login') return Response.json({ authenticated: true, writable, csrfToken: 't', mustChangePassword: false });
    const models = ['1','2'].map(id => makeModelFile({ id, hasRenderSnapshot: false }));
    if (cmd === 'list_file_summaries') return Response.json(models.map(makeModelFileSummary));
    if (cmd === 'list_all_file_tags') return Response.json({});
    if (cmd === 'list_files_by_ids') return Response.json(models);
    if (cmd === 'set_render_snapshot') {
      saves.push(JSON.parse(opts!.body as string));
      if (hold) await new Promise<void>(resolve => { release = resolve; });
      if (status === -1) throw new TypeError('network');
      return Response.json(status === 200 ? null : { message: 'failed', expected: true }, { status, headers: [429,503].includes(status) ? { 'Retry-After': '1' } : {} });
    }
    return Response.json([]);
  }));
  await login('pw');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it('does not render or save without write capability or WebGL', async () => {
  writable = false; await login('pw'); render(<Harness />); await flush();
  expect(screen.queryByTestId('renderer')).toBeNull(); expect(saves).toEqual([]);
  cleanup(); writable = true; await login('pw'); noWebGL = true; render(<Harness />); await flush();
  expect(screen.queryByTestId('renderer')).toBeNull(); expect(saves).toEqual([]);
});
it('saves renderer bytes once per file, serially, waiting for save and pause', async () => {
  hold = true; render(<Harness />); await flush();
  fireEvent.click(screen.getByText('1')); await flush();
  expect(saves).toEqual([{ fileId: '1', imageBase64: 'cG5n' }]);
  await next(); expect(screen.queryByTestId('renderer')).toBeNull();
  hold = false; await act(async () => release!());
  expect(screen.queryByTestId('renderer')).toBeNull(); await next();
  fireEvent.click(screen.getByText('2')); await flush(); await next();
  expect(saves.map(s => s.fileId)).toEqual(['1','2']); expect(screen.queryByTestId('renderer')).toBeNull();
});
it.each([404, 413, -1])('skips failed save %s and continues with the next file', async failure => {
  status = failure; render(<Harness />); await flush();
  fireEvent.click(screen.getByText('1')); await flush(); await next();
  expect(screen.getByText('2')).toBeInTheDocument();
  status = 200; fireEvent.click(screen.getByText('2')); await flush(); await next();
  expect(saves.map(s => s.fileId)).toEqual(['1','2']); expect(screen.queryByTestId('renderer')).toBeNull();
});
it.each([429,503])('retries %s once after Retry-After, then skips', async failure => {
  status = failure; render(<Harness />); await flush(); fireEvent.click(screen.getByText('1')); await flush();
  expect(saves).toHaveLength(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(999); }); expect(saves).toHaveLength(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(1); }); expect(saves).toHaveLength(2);
  await next(); expect(screen.getByText('2')).toBeInTheDocument();
});
it('stops on 401 and resumes after login', async () => {
  status = 401; render(<Harness />); await flush(); fireEvent.click(screen.getByText('1')); await flush(); await next();
  expect(screen.queryByTestId('renderer')).toBeNull(); expect(saves).toHaveLength(1);
  status = 200; await act(async () => login('pw')); await flush(); expect(screen.getByText('2')).toBeInTheDocument();
});
it('pauses while hidden and resumes when visible', async () => {
  const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
  render(<Harness />); await flush(); expect(screen.queryByTestId('renderer')).toBeNull();
  visibility.mockReturnValue('visible'); fireEvent(document, new Event('visibilitychange')); await flush();
  expect(screen.getByText('1')).toBeInTheDocument();
  visibility.mockReturnValue('hidden'); fireEvent(document, new Event('visibilitychange')); expect(screen.queryByTestId('renderer')).toBeNull(); expect(saves).toEqual([]);
});
it.each(['error','lost'])('skips renderer %s without looping', async event => {
  render(<Harness />); await flush(); fireEvent.click(screen.getByText(event)); await next();
  expect(screen.getByText('2')).toBeInTheDocument(); expect(saves).toEqual([]);
});

it('App wires the web queue to the real store and transport', async () => {
  localStorage.setItem('3mf-katalog-display-preference', 'render');
  render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>);
  await flush();
  fireEvent.click(screen.getByText('1')); await flush(); await next();
  expect(saves).toEqual([{ fileId: '1', imageBase64: 'cG5n' }]);
  expect(screen.getByText('2')).toBeInTheDocument();
});
