import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import * as session from './webSession';
import { WebImportController, uploadErrorKey } from './webImport';

class Xhr {
  static requests: Xhr[] = [];
  upload = { onprogress: null as ((event: { loaded: number; total: number; lengthComputable: boolean }) => void) | null };
  onload: (() => void) | null = null; onerror: (() => void) | null = null; onabort: (() => void) | null = null;
  status = 200; responseText = ''; headers: Record<string, string> = {}; retry: string | null = null; body?: File; aborted = false;
  open = vi.fn(); setRequestHeader(name: string, value: string) { this.headers[name] = value; }
  getResponseHeader() { return this.retry; }
  send(body: File) { this.body = body; Xhr.requests.push(this); }
  abort() { this.aborted = true; this.onabort?.(); }
  respond(status = 200, value: unknown = { uploadId: 'u1', name: this.body?.name, size: 1 }, retry: string | null = null) {
    if (status === 200 && typeof value === 'object' && value && 'uploadId' in value) uploads.push({ ...value, state: 'uploaded', time: 1, result: null });
    this.status = status; this.responseText = JSON.stringify(value); this.retry = retry; this.onload?.();
  }
}
class Events {
  static instances: Events[] = [];
  handlers = new Map<string, (event: MessageEvent) => void>();
  onopen: (() => void) | null = null; onerror: (() => void) | null = null; closed = false;
  constructor(public url: string) { Events.instances.push(this); }
  addEventListener(name: string, handler: (event: MessageEvent) => void) { this.handlers.set(name, handler); }
  close() { this.closed = true; }
  emit(value: unknown, name = 'import-progress') { this.handlers.get(name)?.({ data: JSON.stringify(value) } as MessageEvent); }
}
const job = (state = 'importing') => ({ jobId: 'j1', state, current: 'one.stl', total: 2, done: 1, files: [{ uploadId: 'u1', name: 'one.stl', state: 'imported', fileId: '1', existingFileId: null, kind: null }], kind: null });
let controller: WebImportController;
let calls: { url: string; args: Record<string, unknown> }[];
let uploads: unknown[];
let state: ReturnType<typeof job>;
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
beforeEach(async () => {
  vi.stubGlobal('XMLHttpRequest', Xhr); vi.stubGlobal('EventSource', Events);
  Xhr.requests = []; Events.instances = []; calls = []; uploads = []; state = job();
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    const args = JSON.parse(String(options?.body ?? '{}')); calls.push({ url, args });
    return Response.json(url === '/api/session' ? { authenticated: true, writable: true, uploadable: true, csrfToken: 'csrf', mustChangePassword: false }
      : url === '/api/uploads' ? uploads : url.endsWith('start_upload_import') ? { jobId: 'j1' } : state);
  }));
  await session.getSession();
  controller = new WebImportController(); controller.connect(); await flush(); calls = [];
});
afterEach(() => { controller.dispose(); session.authenticationFailed(401); vi.useRealTimers(); vi.unstubAllGlobals(); });
it('sends files sequentially as File bodies, encodes the name and starts only successful upload IDs', async () => {
  controller.addFiles([new File(['a'], 'ä #.stl'), new File(['b'], 'two.obj')]);
  expect(Xhr.requests).toHaveLength(1);
  expect(Xhr.requests[0].headers).toEqual({ 'Content-Type': 'application/octet-stream', 'X-CSRF-Token': 'csrf', 'X-Upload-Name': '%C3%A4%20%23.stl' });
  expect(Xhr.requests[0].body).toBeInstanceOf(File);
  Xhr.requests[0].upload.onprogress?.({ loaded: 1, total: 2, lengthComputable: true });
  expect(controller.getSnapshot().queue[0].percent).toBe(50);
  Xhr.requests[0].respond(); await flush(); expect(Xhr.requests).toHaveLength(2);
  Xhr.requests[1].respond(400, { kind: 'invalid_signature' }); await flush();
  expect(controller.getSnapshot().queue[1].error).toBe('webUploadInvalid');
  expect(calls.find(c => c.url.endsWith('start_upload_import'))?.args).toEqual({ uploadIds: ['u1'] });
});
it('rejects archives, wrong extensions and directory selections before sending', async () => {
  const folderFile = new File(['a'], 'nested.stl'); Object.defineProperty(folderFile, 'webkitRelativePath', { value: 'folder/nested.stl' });
  controller.addFiles([new File(['a'], 'file.zip'), new File(['a'], 'x.3mf.exe'), folderFile]); await flush();
  expect(Xhr.requests).toHaveLength(0);
  expect(controller.getSnapshot().queue.map(r => r.error)).toEqual(['webUploadArchives', 'webUploadType', 'webUploadFolders']);
});
it('cancels an active file and a queued file without uploading the latter', async () => {
  controller.addFiles([new File(['a'], 'one.stl'), new File(['b'], 'two.stl')]);
  const rows = controller.getSnapshot().queue;
  controller.cancelUpload(rows[1].id); controller.cancelUpload(rows[0].id); await flush();
  expect(Xhr.requests[0].aborted).toBe(true); expect(Xhr.requests).toHaveLength(1);
  expect(controller.getSnapshot().queue.map(r => r.state)).toEqual(['cancelled', 'cancelled']);
});
it.each([429, 503])('retries %s once only when Retry-After is present', async status => {
  vi.useFakeTimers(); controller.addFiles([new File(['a'], 'one.stl')]);
  Xhr.requests[0].respond(status, {}, '2'); await flush();
  await vi.advanceTimersByTimeAsync(1999); expect(Xhr.requests).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(1); expect(Xhr.requests).toHaveLength(2);
  Xhr.requests[1].respond(status, {}, '2'); await flush();
  await vi.advanceTimersByTimeAsync(2000); expect(Xhr.requests).toHaveLength(2);
  expect(controller.getSnapshot().queue[0].state).toBe('error');
});
it('does not retry a permanent 503, including a non-JSON proxy response', async () => {
  controller.addFiles([new File(['a'], 'one.stl')]); Xhr.requests[0].respond(503, '<html>unavailable</html>'); await flush();
  expect(Xhr.requests).toHaveLength(1); expect(controller.getSnapshot().queue[0].error).toBe('webUploadRestart');
});
it('401 aborts the queue and closes SSE; reauthentication recovers server-owned uploads and jobs', async () => {
  controller.addFiles([new File(['a'], 'one.stl'), new File(['b'], 'two.stl')]);
  Xhr.requests[0].respond(401, { code: 'session_expired' }); await flush();
  expect(session.canUpload()).toBe(false); expect(Xhr.requests).toHaveLength(1); expect(Events.instances[0].closed).toBe(true);
  expect(controller.getSnapshot().queue.every(r => r.state === 'cancelled' || r.error === 'webUploadSessionEnded')).toBe(true);
  controller.dispose(); await session.getSession(); controller = new WebImportController(); controller.connect();
  Events.instances[1].emit({ snapshot: true, jobs: [job()], uploads: [{ uploadId: 'saved', name: 'saved.stl', state: 'uploaded', size: 1, time: 1, result: null }] });
  expect(controller.getSnapshot().jobs[0].jobId).toBe('j1'); expect(controller.getSnapshot().uploads[0].uploadId).toBe('saved');
});
it('reconciles on reconnect and keeps finished events idempotent despite stale HTTP responses', async () => {
  const refresh = vi.fn(); controller.onImported = refresh;
  Events.instances[0].emit(job()); Events.instances[0].onerror?.(); await flush();
  expect(controller.getSnapshot().connection).toBe('disconnected');
  state = job('completed'); Events.instances[0].onopen?.(); await flush();
  expect(calls.some(c => c.url.endsWith('get_import_state'))).toBe(true);
  expect(calls.some(c => c.url.endsWith('get_import_result'))).toBe(true);
  expect(calls.some(c => c.url === '/api/uploads')).toBe(true);
  Events.instances[0].emit(job('completed'), 'import-finished'); Events.instances[0].emit(job());
  expect(controller.getSnapshot().jobs[0].state).toBe('completed'); expect(refresh).toHaveBeenCalledTimes(1);
});
it('discards open uploads and submits an import without any target folder', async () => {
  await controller.discard('u1'); await controller.start(['u2']);
  expect(calls.find(c => c.url.endsWith('discard_upload'))?.args).toEqual({ uploadId: 'u1' });
  expect(calls.find(c => c.url.endsWith('start_upload_import'))?.args).toEqual({ uploadIds: ['u2'] });
});
it.each([[400,'webUploadInvalid'],[409,'webUploadConflict'],[413,'webUploadTooLarge'],[415,'webUploadType'],[507,'webUploadNoSpace'],[0,'webUploadOffline'],[503,'webUploadRestart']] as const)('maps HTTP %s to a user-facing error', (status, key) => {
  expect(uploadErrorKey({ status })).toBe(key);
});
it('cannot upload when the server has not granted uploadable', async () => {
  session.authenticationFailed(401); controller.addFiles([new File(['a'], 'one.stl')]); await flush();
  expect(Xhr.requests).toHaveLength(0);
});
it('hydrates historical snapshot results without refreshing the catalog for every old job', async () => {
  const refresh = vi.fn(); controller.onImported = refresh;
  Events.instances[0].emit({ snapshot: true, jobs: [job('completed'), { ...job('completed'), jobId: 'j2' }], uploads: [] }); await flush();
  expect(refresh).not.toHaveBeenCalled(); expect(calls).toEqual([]);
  expect(controller.getSnapshot().jobs).toHaveLength(2);
});
it('does not start discarded or already imported IDs when the remaining queue finishes', async () => {
  controller.addFiles([new File(['a'], 'one.stl'), new File(['b'], 'two.stl')]);
  Xhr.requests[0].respond(); await flush();
  uploads = [{ uploadId: 'u2', name: 'two.stl', state: 'uploaded', size: 1, time: 1, result: null }];
  Xhr.requests[1].respond(200, { uploadId: 'u2', name: 'two.stl', size: 1 }); await flush();
  expect(calls.find(c => c.url.endsWith('start_upload_import'))?.args).toEqual({ uploadIds: ['u2'] });
});
it('cancels Retry-After waiting without sending another request', async () => {
  vi.useFakeTimers(); controller.addFiles([new File(['a'], 'one.stl')]);
  Xhr.requests[0].respond(429, {}, '2'); await flush();
  controller.cancelUpload(controller.getSnapshot().queue[0].id); await vi.advanceTimersByTimeAsync(2000);
  expect(Xhr.requests).toHaveLength(1); expect(controller.getSnapshot().queue[0].state).toBe('cancelled');
});
it('detects session expiry when EventSource disconnects and closes the stream', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ code: 'session_expired' }, { status: 401 })));
  Events.instances[0].onerror?.(); await flush();
  expect(session.canUpload()).toBe(false); expect(Events.instances[0].closed).toBe(true);
});

it('ignores a stale HTTP progress response that arrives after a finished SSE event', async () => {
  Events.instances[0].emit(job());
  let resolve!: (response: Response) => void;
  const original = globalThis.fetch;
  vi.stubGlobal('fetch', vi.fn((url: string, options?: RequestInit) => url.endsWith('get_import_state')
    ? new Promise<Response>(done => { resolve = done; }) : original(url, options)));
  const checking = controller.reconcile(); await flush();
  Events.instances[0].emit(job('completed'), 'import-finished');
  resolve(Response.json(job())); await checking;
  expect(controller.getSnapshot().jobs[0].state).toBe('completed');
});
it.each([400, 409, 413, 415, 507])('keeps a failed HTTP %s upload out of the import command', async status => {
  controller.addFiles([new File(['a'], 'bad.stl')]); Xhr.requests[0].respond(status, {}); await flush();
  expect(controller.getSnapshot().queue[0].state).toBe('error');
  expect(calls.some(c => c.url.endsWith('start_upload_import'))).toBe(false);
});
it('does not retry an upload after a lost network response', async () => {
  controller.addFiles([new File(['a'], 'one.stl')]); Xhr.requests[0].onerror?.(); await flush();
  expect(Xhr.requests).toHaveLength(1); expect(controller.getSnapshot().queue[0].error).toBe('webUploadOffline');
});
it('starts all successful uploads from the authoritative GET despite a delayed older SSE inventory', async () => {
  controller.addFiles([new File(['a'], 'one.stl'), new File(['b'], 'two.stl')]);
  Xhr.requests[0].respond(); await flush();
  const original = globalThis.fetch;
  let resolve!: (response: Response) => void;
  let held = false;
  vi.stubGlobal('fetch', vi.fn((url: string, options?: RequestInit) => {
    if (url === '/api/uploads' && !held) { held = true; return new Promise<Response>(done => { resolve = done; }); }
    return original(url, options);
  }));
  Xhr.requests[1].respond(200, { uploadId: 'u2', name: 'two.stl', size: 1 }); await flush();
  Events.instances[0].emit({ uploads: [{ uploadId: 'u1', name: 'one.stl', state: 'uploaded', size: 1, time: 1, result: null }] });
  resolve(Response.json(uploads)); await flush();
  expect(calls.find(c => c.url.endsWith('start_upload_import'))?.args).toEqual({ uploadIds: ['u1', 'u2'] });
});
