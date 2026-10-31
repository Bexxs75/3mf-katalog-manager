import { newRequestId } from './transport';
import { authenticationFailed, canManageFiles, getSnapshot } from './webSession';
import { blockFileOperations } from './webSession';
import { fileOpsError } from './fileOpsErrors';

/** The types the server accepts; the server decides by extension and content anyway. */
export const COMPANION_ACCEPT = '.png,.jpg,.jpeg,.gif,.webp,.pdf,.txt,.md';
const ALLOWED_NAME = /\.(png|jpe?g|gif|webp|pdf|txt|md)$/i;
export const isAllowedCompanionName = (name: string): boolean => ALLOWED_NAME.test(name);

/**
 * What is wrong with the file's size before a single byte is sent. The server answers an
 * oversized or empty file before it has read the body, and a browser that is still sending
 * then reports a broken connection instead of the answer, so these are caught here.
 */
export function companionSizeProblem(file: File): 'empty' | 'too_large' | null {
  if (file.size === 0) return 'empty';
  const max = getSnapshot().companionMaxBytes;
  return max !== undefined && file.size > max ? 'too_large' : null;
}

export interface UploadedCompanion { id: string; fileId: string; name: string; kind: string; sizeBytes: number }

// What the server answers when the body has no `kind` (a proxy in front of it, for instance).
const KIND_BY_STATUS: Record<number, string> = { 400: 'invalid', 408: 'upload_timeout', 413: 'companion_too_large', 415: 'companion_type', 422: 'companion_content', 503: 'busy', 507: 'insufficient_storage' };

/**
 * Sends one file as the raw body of `POST /api/companion-upload`, with progress. The browser
 * supplies Content-Length for a File; nothing is buffered here. One request id per call.
 */
export function uploadCompanion(fileId: string, file: File, onProgress: (percent: number) => void, signal?: AbortSignal): Promise<UploadedCompanion> {
  return new Promise((resolve, reject) => {
    if (!canManageFiles()) { reject(fileOpsError({ kind: 'revoked' })); return; }
    if (signal?.aborted) { reject(fileOpsError({ kind: 'cancelled' })); return; }
    const problem = companionSizeProblem(file);
    if (problem) { reject(fileOpsError({ kind: problem === 'empty' ? 'companion_content' : 'companion_too_large' })); return; }
    const csrf = getSnapshot().csrfToken ?? '';
    const xhr = new XMLHttpRequest();
    const abort = () => xhr.abort();
    const done = (settle: () => void) => { signal?.removeEventListener('abort', abort); settle(); };
    xhr.open('POST', '/api/companion-upload');
    xhr.setRequestHeader('Content-Type', 'application/octet-stream');
    xhr.setRequestHeader('X-CSRF-Token', csrf);
    xhr.setRequestHeader('X-File-Id', fileId);
    xhr.setRequestHeader('X-Upload-Name', encodeURIComponent(file.name));
    xhr.setRequestHeader('X-Request-Id', newRequestId());
    xhr.upload.onprogress = (event) => { if (event.lengthComputable && event.total > 0) onProgress(Math.round(event.loaded / event.total * 100)); };
    // No answer arrived: the connection broke, or the server turned the file away early.
    xhr.onerror = () => done(() => reject(fileOpsError({ kind: 'upload_interrupted' })));
    xhr.onabort = () => done(() => reject(fileOpsError({ kind: 'cancelled' })));
    xhr.onload = () => {
      let body: { id?: unknown; kind?: unknown; code?: unknown } = {};
      try { body = JSON.parse(xhr.responseText); } catch { /* A proxy may answer with HTML. */ }
      if (xhr.status >= 200 && xhr.status < 300 && typeof body.id === 'string') {
        done(() => resolve(body as unknown as UploadedCompanion));
        return;
      }
      const kind = typeof body.kind === 'string' ? body.kind : KIND_BY_STATUS[xhr.status] ?? 'generic';
      const failure = fileOpsError({ kind, retryAfter: xhr.getResponseHeader('Retry-After') });
      done(() => reject(failure));
      if (kind === 'restart_required') blockFileOperations();
      authenticationFailed(xhr.status, typeof body.code === 'string' ? body.code : undefined);
    };
    signal?.addEventListener('abort', abort, { once: true });
    xhr.send(file);
  });
}
