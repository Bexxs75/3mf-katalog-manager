import type { Translations } from '../i18n/types';
import { authenticationFailed, canUpload, getSnapshot, readResponse, subscribe } from './webSession';

export type UploadTextKey = { [K in keyof Translations]: Translations[K] extends string ? K : never }[keyof Translations];

export const MODEL_UPLOAD_ACCEPT = '.3mf,.stl,.obj,.stp,.step';
export interface WebUpload { uploadId: string; name: string; size: number; time: number; state: 'uploaded' | 'duplicate' | 'invalid'; result: unknown }
export interface WebImportFile { uploadId: string; name: string; state: string; fileId: string | null; existingFileId: string | null; kind: string | null }
export interface WebImportJob { jobId: string; state: string; current: string | null; total: number; done: number; files: WebImportFile[]; kind: string | null }
export interface UploadRow { id: number; name: string; percent: number; state: 'queued' | 'uploading' | 'retrying' | 'uploaded' | 'cancelled' | 'error'; error?: UploadTextKey }
interface View { queue: UploadRow[]; uploads: WebUpload[]; jobs: WebImportJob[]; connection: 'connecting' | 'connected' | 'disconnected'; error: UploadTextKey | null; busy: boolean; result: WebImportJob | null }
interface Failure { status?: number; kind?: string; code?: string; retryAfter?: string | null }
export const importFinished = (job: WebImportJob) => ['completed', 'cancelled', 'failed'].includes(job.state);
export function uploadErrorKey(error: unknown): UploadTextKey {
  const e = (error ?? {}) as Failure;
  if (e.kind === 'archive_desktop_only') return 'webUploadArchives';
  if (e.kind === 'folders') return 'webUploadFolders';
  if (e.kind === 'cancelled') return 'webUploadCancelled';
  if (e.status === 503 && !e.retryAfter) return 'webUploadRestart';
  return ({ 400: 'webUploadInvalid', 401: 'webUploadSessionEnded', 403: 'webUploadForbidden', 404: 'webUploadMissing', 408: 'webUploadOffline', 409: 'webUploadConflict', 411: 'webUploadLength', 413: 'webUploadTooLarge', 415: 'webUploadType', 429: 'webUploadBusy', 503: 'webUploadBusy', 507: 'webUploadNoSpace' } as Record<number, UploadTextKey>)[e.status ?? 0] ?? (e.status ? 'webUploadFailed' : 'webUploadOffline');
}
function retryDelay(e: Failure): number | null {
  if (![429, 503].includes(e.status ?? 0) || !e.retryAfter) return null;
  const seconds = Number(e.retryAfter);
  const delay = Number.isFinite(seconds) ? seconds * 1000 : Date.parse(e.retryAfter) - Date.now();
  return Number.isFinite(delay) ? Math.max(0, Math.min(delay, 2147483647)) : null;
}
function cancelled() { return { kind: 'cancelled' }; }
function wait(delay: number, signal: AbortSignal) {
  return new Promise<void>((resolve, reject) => {
    const abort = () => { clearTimeout(timer); reject(cancelled()); };
    const timer = setTimeout(() => { signal.removeEventListener('abort', abort); resolve(); }, delay);
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted) abort();
  });
}
async function request<T>(path: string, signal: AbortSignal, args?: Record<string, unknown>): Promise<T> {
  if (!canUpload() || signal.aborted) throw cancelled();
  const csrf = getSnapshot().csrfToken;
  for (let attempt = 0; ; attempt++) {
    if (!canUpload() || signal.aborted || csrf !== getSnapshot().csrfToken) throw cancelled();
    const response = await fetch('/api/' + path, {
      method: args ? 'POST' : 'GET', credentials: 'same-origin', signal,
      ...(args ? { headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf ?? '' }, body: JSON.stringify(args) } : {}),
    });
    try { return await readResponse<T>(response); }
    catch (error) {
      const failure: Failure = { ...(typeof error === 'object' ? error : {}), status: response.status, retryAfter: response.headers.get('Retry-After') };
      const delay = retryDelay(failure);
      if (attempt === 0 && delay !== null) { await wait(delay, signal); continue; }
      throw failure;
    }
  }
}
function upload(file: File, signal: AbortSignal, progress: (percent: number) => void): Promise<{ uploadId: string }> {
  return new Promise((resolve, reject) => {
    if (!canUpload() || signal.aborted) { reject(cancelled()); return; }
    const xhr = new XMLHttpRequest();
    const abort = () => { xhr.abort(); };
    const finish = (error?: unknown, value?: { uploadId: string }) => {
      signal.removeEventListener('abort', abort);
      if (error) reject(error); else resolve(value!);
    };
    xhr.open('POST', '/api/upload');
    xhr.setRequestHeader('Content-Type', 'application/octet-stream');
    xhr.setRequestHeader('X-CSRF-Token', getSnapshot().csrfToken ?? '');
    xhr.setRequestHeader('X-Upload-Name', encodeURIComponent(file.name));
    xhr.upload.onprogress = event => { if (event.lengthComputable) progress(Math.round(event.loaded / event.total * 100)); };
    xhr.onerror = () => finish({ status: 0 });
    xhr.onabort = () => finish(cancelled());
    xhr.onload = () => {
      let value: { uploadId?: string; code?: string; kind?: string } = {};
      try { value = JSON.parse(xhr.responseText); } catch { /* Proxy errors may be HTML. */ }
      if (xhr.status >= 200 && xhr.status < 300 && typeof value.uploadId === 'string') finish(undefined, { uploadId: value.uploadId });
      else {
        // Settle first: auth subscribers synchronously abort all other work.
        finish({ ...value, status: xhr.status, retryAfter: xhr.getResponseHeader('Retry-After') });
        authenticationFailed(xhr.status, value.code);
      }
    };
    signal.addEventListener('abort', abort, { once: true });
    // The browser supplies Content-Length for File bodies. Never buffer or stream it ourselves.
    xhr.send(file);
  });
}

/** Owns one mounted browser import surface, its queue and its SSE connection. */
export class WebImportController {
  private view: View = { queue: [], uploads: [], jobs: [], connection: 'connecting', error: null, busy: false, result: null };
  private listeners = new Set<() => void>();
  private lifetime = new AbortController();
  private unsubscribe?: () => void;
  private events?: EventSource;
  private reconnect?: ReturnType<typeof setTimeout>;
  private pending = new Map<number, { file: File; abort: AbortController }>();
  private serial = 0;
  private running = false;
  private revisions = new Map<string, number>();
  private uploadRevision = 0;
  private completed = new Set<string>();
  onImported?: () => void | Promise<unknown>;
  getSnapshot = () => this.view;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private patch(value: Partial<View>) { this.view = { ...this.view, ...value }; this.listeners.forEach(listener => listener()); }
  private row(id: number, value: Partial<UploadRow>) { this.patch({ queue: this.view.queue.map(row => row.id === id ? { ...row, ...value } : row) }); }
  clearError = () => this.patch({ error: null });
  report(error: unknown) { if (!this.lifetime.signal.aborted) this.patch({ error: uploadErrorKey(error) }); }
  connect() {
    if (!canUpload()) return;
    this.unsubscribe = subscribe(() => { if (!canUpload()) this.dispose(); });
    this.openEvents();
    void this.reconcile();
  }
  dispose() {
    this.unsubscribe?.(); this.unsubscribe = undefined;
    this.lifetime.abort(); this.events?.close(); clearTimeout(this.reconnect);
    for (const [id, task] of this.pending) { task.abort.abort(); this.row(id, { state: 'cancelled', error: 'webUploadSessionEnded' }); }
    this.pending.clear();
  }
  private openEvents() {
    if (this.lifetime.signal.aborted || !canUpload()) return;
    const events = new EventSource('/api/events'); this.events = events;
    events.onopen = () => { this.patch({ connection: 'connected' }); void this.reconcile(); };
    events.onerror = () => {
      if (this.lifetime.signal.aborted) return;
      this.patch({ connection: 'disconnected' });
      // EventSource does not expose HTTP 401. A guarded read performs auth recovery.
      void this.reconcile();
      if (events.readyState === EventSource.CLOSED) {
        events.close(); clearTimeout(this.reconnect);
        this.reconnect = setTimeout(() => this.openEvents(), 3000);
      }
    };
    const receive = (event: MessageEvent) => {
      if (this.lifetime.signal.aborted) return;
      try {
        const value = JSON.parse(event.data);
        if (Array.isArray(value.uploads)) { this.uploadRevision++; this.patch({ uploads: value.uploads }); }
        if (Array.isArray(value.jobs)) {
          if (value.snapshot) this.patch({ jobs: this.view.jobs.filter(j => importFinished(j) || value.jobs.some((v: WebImportJob) => v.jobId === j.jobId)) });
          value.jobs.forEach((job: WebImportJob) => this.receiveJob(job, !value.snapshot || this.view.jobs.some(previous => previous.jobId === job.jobId && !importFinished(previous))));
        } else if (value.jobId) this.receiveJob(value);
      } catch { this.patch({ error: 'webUploadFailed' }); }
    };
    events.addEventListener('import-progress', receive);
    events.addEventListener('import-finished', receive);
  }
  private receiveJob(job: WebImportJob, notify = true) {
    if (this.lifetime.signal.aborted || this.completed.has(job.jobId)) return;
    this.revisions.set(job.jobId, (this.revisions.get(job.jobId) ?? 0) + 1);
    this.patch({ jobs: [...this.view.jobs.filter(j => j.jobId !== job.jobId), job] });
    if (importFinished(job)) {
      this.completed.add(job.jobId);
      if (notify) {
        this.patch({ result: job });
        void Promise.resolve().then(() => this.onImported?.()).catch(e => this.report(e));
        void this.refreshUploads();
      }
    }
  }
  private async refreshUploads() {
    const revision = this.uploadRevision;
    try {
      const uploads = await request<WebUpload[]>('uploads', this.lifetime.signal);
      if (!this.lifetime.signal.aborted && revision === this.uploadRevision) { this.uploadRevision++; this.patch({ uploads }); }
      return this.lifetime.signal.aborted ? null : uploads;
    } catch (e) { this.report(e); return null; }
  }
  private async recover(jobId: string) {
    const revision = this.revisions.get(jobId);
    try {
      let job = await request<WebImportJob>('invoke/get_import_state', this.lifetime.signal, { jobId });
      if (importFinished(job)) job = await request<WebImportJob>('invoke/get_import_result', this.lifetime.signal, { jobId });
      if (revision === this.revisions.get(jobId)) this.receiveJob(job);
    } catch (e) {
      if ((e as Failure)?.status === 404 && revision === this.revisions.get(jobId)) this.patch({ jobs: this.view.jobs.filter(j => j.jobId !== jobId) });
      else this.report(e);
    }
  }
  reconcile = async () => { await Promise.all([this.refreshUploads(), ...this.view.jobs.filter(j => !importFinished(j)).map(j => this.recover(j.jobId))]); };
  addFiles(files: File[]) {
    if (!canUpload() || this.lifetime.signal.aborted) return;
    for (const file of files) {
      const id = ++this.serial;
      const error: UploadTextKey | undefined = file.webkitRelativePath ? 'webUploadFolders'
        : /\.(zip|7z|rar)$/i.test(file.name) ? 'webUploadArchives'
        : !/\.(3mf|stl|obj|stp|step)$/i.test(file.name) ? 'webUploadType' : undefined;
      this.patch({ queue: [...this.view.queue, { id, name: file.name, percent: 0, state: error ? 'error' : 'queued', error }] });
      if (!error) this.pending.set(id, { file, abort: new AbortController() });
    }
    void this.drain();
  }
  cancelUpload(id: number) {
    this.pending.get(id)?.abort.abort(); this.pending.delete(id); this.row(id, { state: 'cancelled', error: undefined });
  }
  private async drain() {
    if (this.running) return;
    this.running = true;
    const successful: string[] = [];
    try {
      while (this.pending.size && !this.lifetime.signal.aborted) {
        const [id, task] = this.pending.entries().next().value!;
        try {
          for (let attempt = 0; ; attempt++) {
            this.row(id, { state: 'uploading', percent: 0 });
            try {
              const result = await upload(task.file, task.abort.signal, percent => this.row(id, { percent }));
              successful.push(result.uploadId); this.row(id, { state: 'uploaded', percent: 100 }); break;
            } catch (e) {
              const delay = retryDelay(e as Failure);
              if (attempt === 0 && delay !== null) {
                this.row(id, { state: 'retrying' }); await wait(delay, task.abort.signal); continue;
              }
              throw e;
            }
          }
        } catch (e) {
          if (task.abort.signal.aborted || this.lifetime.signal.aborted) this.row(id, { state: 'cancelled' });
          else this.row(id, { state: 'error', error: uploadErrorKey(e) });
        } finally { this.pending.delete(id); }
      }
      if (!this.lifetime.signal.aborted && successful.length) {
        const inventory = await this.refreshUploads();
        if (inventory) {
          // Use this authoritative read even if delayed SSE changed the displayed inventory.
          // Manual actions or another session may already have consumed an earlier upload.
          const ready = new Set(inventory.filter(u => u.state === 'uploaded').map(u => u.uploadId));
          // Remaining uploads beyond this server limit stay available in the journal.
          await this.start(successful.filter(id => ready.has(id)).slice(0, 200));
        }
      }
    } finally { this.running = false; if (this.pending.size) void this.drain(); }
  }
  start = async (uploadIds: string[]) => {
    if (!uploadIds.length || this.view.busy || !canUpload() || this.lifetime.signal.aborted) return;
    this.patch({ busy: true, error: null });
    try {
      const result = await request<{ jobId: string }>('invoke/start_upload_import', this.lifetime.signal, { uploadIds });
      await this.recover(result.jobId); await this.refreshUploads();
    } catch (e) { this.report(e); await this.refreshUploads(); }
    finally { if (!this.lifetime.signal.aborted) this.patch({ busy: false }); }
  };
  discard = async (uploadId: string) => {
    try { await request('invoke/discard_upload', this.lifetime.signal, { uploadId }); await this.refreshUploads(); }
    catch (e) { this.report(e); }
  };
  cancelImport = async (jobId: string) => {
    try { this.receiveJob(await request<WebImportJob>('invoke/cancel_import', this.lifetime.signal, { jobId })); }
    catch (e) { this.report(e); }
  };
}
